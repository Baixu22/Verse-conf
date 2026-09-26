use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::ast::Ast;
use crate::engine::IncrementalParser;

/// 文件变更事件
#[derive(Debug, Clone)]
pub enum FileChangeEvent {
    Modified(PathBuf),
    Created(PathBuf),
    Removed(PathBuf),
}

/// 一次重载的结果，供调用方（例如 `verseconf watch`）作出反应。
///
/// 关键在于 **解析失败也要报出来**：之前 `handle_event` 里写的是
/// `if let Ok(ast) = ...`，解析失败时静默保留旧 AST——于是"文件现在是坏的"
/// 这件事永远不会被任何人知道。这正是本项目最反对的那种失败方式。
#[derive(Debug, Clone)]
pub enum ReloadEvent {
    /// 文件内容变化后的重新解析结果；`error` 为 `None` 表示解析成功
    Changed {
        path: PathBuf,
        error: Option<String>,
    },
    /// 被监视的文件已删除
    Removed { path: PathBuf },
}

impl ReloadEvent {
    /// 事件对应的文件路径
    pub fn path(&self) -> &Path {
        match self {
            ReloadEvent::Changed { path, .. } | ReloadEvent::Removed { path } => path,
        }
    }
}

/// 热重载状态
#[derive(Debug)]
struct HotReloadState {
    parser: IncrementalParser,
    watched_files: HashMap<PathBuf, Ast>,
}

/// 热重载管理器
pub struct HotReloader {
    state: Arc<Mutex<HotReloadState>>,
    _watcher: Option<RecommendedWatcher>,
    /// 有订阅者时，每次重载的结果都会发到这里
    events: Option<Sender<ReloadEvent>>,
}

impl HotReloader {
    pub fn new(cache_size: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(HotReloadState {
                parser: IncrementalParser::new(cache_size),
                watched_files: HashMap::new(),
            })),
            _watcher: None,
            events: None,
        }
    }

    /// 构造一个会把重载结果发到通道里的实例。
    ///
    /// 调用方拿到 `Receiver` 后即可对每次变化作出反应（`verseconf watch` 用它
    /// 打印每次改动后的校验结果）。
    pub fn with_events(cache_size: usize) -> (Self, Receiver<ReloadEvent>) {
        let (tx, rx) = channel();
        let mut reloader = Self::new(cache_size);
        reloader.events = Some(tx);
        (reloader, rx)
    }

    pub fn watch(&mut self, path: &Path) -> Result<(), String> {
        let ast = {
            let mut state = self.state.lock().unwrap();
            state.parser.parse_file(path)?.clone()
        };

        {
            let mut state = self.state.lock().unwrap();
            state.watched_files.insert(path.to_path_buf(), ast);
        }

        if self._watcher.is_none() {
            self.setup_watcher()?;
        }

        if let Some(watcher) = &mut self._watcher {
            let path_to_watch = if path.is_dir() {
                path.to_path_buf()
            } else {
                path.parent().unwrap_or(path).to_path_buf()
            };

            watcher
                .watch(&path_to_watch, RecursiveMode::NonRecursive)
                .map_err(|e| format!("Failed to watch path: {}", e))?;
        }

        Ok(())
    }

    pub fn unwatch(&mut self, path: &Path) {
        let mut state = self.state.lock().unwrap();
        state.watched_files.remove(path);
    }

    pub fn get_ast(&self, path: &Path) -> Option<Ast> {
        let state = self.state.lock().unwrap();
        state.watched_files.get(path).cloned()
    }

    pub fn reload(&mut self, path: &Path) -> Result<(), String> {
        let needs_reparse = {
            let state = self.state.lock().unwrap();
            state.parser.needs_reparse(path)
        };

        if needs_reparse {
            let ast = {
                let mut state = self.state.lock().unwrap();
                state.parser.parse_file(path)?.clone()
            };
            let mut state = self.state.lock().unwrap();
            state.watched_files.insert(path.to_path_buf(), ast);
        }
        Ok(())
    }

    pub fn reload_all(&mut self) -> Result<usize, String> {
        let paths: Vec<PathBuf> = {
            let state = self.state.lock().unwrap();
            state.watched_files.keys().cloned().collect()
        };

        let mut reloaded = 0;
        for path in paths {
            self.reload(&path)?;
            reloaded += 1;
        }

        Ok(reloaded)
    }

    pub fn stats(&self) -> HotReloadStats {
        let state = self.state.lock().unwrap();
        HotReloadStats {
            watched_files: state.watched_files.len(),
            cache_stats: state.parser.cache_stats(),
        }
    }

    fn setup_watcher(&mut self) -> Result<(), String> {
        let state = Arc::clone(&self.state);
        let events = self.events.clone();
        let (tx, rx) = channel();

        let watcher = RecommendedWatcher::new(tx, notify::Config::default())
            .map_err(|e| format!("Failed to create watcher: {}", e))?;

        std::thread::spawn(move || loop {
            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(Ok(event)) => {
                    handle_event(&event, &state, events.as_ref());
                }
                Ok(Err(e)) => {
                    eprintln!("Watch error: {}", e);
                }
                Err(_) => {
                    continue;
                }
            }
        });

        self._watcher = Some(watcher);
        Ok(())
    }
}

fn handle_event(
    event: &Event,
    state: &Arc<Mutex<HotReloadState>>,
    events: Option<&Sender<ReloadEvent>>,
) {
    for path in &event.paths {
        let change_event = match event.kind {
            notify::EventKind::Modify(_) => FileChangeEvent::Modified(path.clone()),
            notify::EventKind::Create(_) => FileChangeEvent::Created(path.clone()),
            notify::EventKind::Remove(_) => FileChangeEvent::Removed(path.clone()),
            _ => continue,
        };

        match &change_event {
            FileChangeEvent::Modified(p) | FileChangeEvent::Created(p) => {
                // 解析成功就更新缓存；失败也必须把错误发出去。
                // 这里以前是 `if let Ok(ast) = ...`：解析失败时静默保留旧 AST，
                // 于是"文件现在是坏的"这件事永远不会被任何人知道。
                let error = {
                    let mut state = state.lock().unwrap();
                    match state.parser.parse_file(p) {
                        Ok(ast) => {
                            state.watched_files.insert(p.clone(), ast);
                            None
                        }
                        Err(error) => Some(error),
                    }
                };

                if let Some(sender) = events {
                    let _ = sender.send(ReloadEvent::Changed {
                        path: p.clone(),
                        error,
                    });
                }
            }
            FileChangeEvent::Removed(p) => {
                {
                    let mut state = state.lock().unwrap();
                    state.watched_files.remove(p);
                }
                if let Some(sender) = events {
                    let _ = sender.send(ReloadEvent::Removed { path: p.clone() });
                }
            }
        }
    }
}

/// 热重载统计
#[derive(Debug)]
pub struct HotReloadStats {
    pub watched_files: usize,
    pub cache_stats: crate::engine::ParseCacheStats,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    /// 回归：解析失败必须作为事件报出去。
    ///
    /// 之前 `handle_event` 里是 `if let Ok(ast) = ...`：解析失败时静默保留旧 AST，
    /// 于是"文件现在是坏的"这件事永远不会被任何人知道——`verseconf watch`
    /// 会一直显示上一次的成功结果。
    #[test]
    fn test_parse_failure_is_reported_not_swallowed() {
        let test_dir = std::env::temp_dir().join("verseconf_hotreload_failure");
        let _ = std::fs::remove_dir_all(&test_dir);
        std::fs::create_dir_all(&test_dir).unwrap();

        let test_file = test_dir.join("broken.vcf");
        std::fs::write(&test_file, "ok = 1\n").unwrap();

        let (mut reloader, events) = HotReloader::with_events(10);
        reloader.watch(&test_file).unwrap();

        // 写入无法解析的内容
        std::fs::write(&test_file, "this is not valid {{{\n").unwrap();

        // 不同平台/编辑器的文件系统事件数量不一致，所以循环等到"确实报出失败"为止
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut reported_failure = false;
        while std::time::Instant::now() < deadline {
            if let Ok(ReloadEvent::Changed { error, .. }) =
                events.recv_timeout(Duration::from_millis(500))
            {
                if error.is_some() {
                    reported_failure = true;
                    break;
                }
            }
        }

        assert!(
            reported_failure,
            "解析失败必须产生带 error 的事件，而不是静默保留旧 AST"
        );

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    /// 成功解析时事件里的 error 必须是 None，避免把正常改动报成错误
    #[test]
    fn test_successful_reload_reports_no_error() {
        let test_dir = std::env::temp_dir().join("verseconf_hotreload_success");
        let _ = std::fs::remove_dir_all(&test_dir);
        std::fs::create_dir_all(&test_dir).unwrap();

        let test_file = test_dir.join("ok.vcf");
        std::fs::write(&test_file, "ok = 1\n").unwrap();

        let (mut reloader, events) = HotReloader::with_events(10);
        reloader.watch(&test_file).unwrap();

        std::fs::write(&test_file, "ok = 2\n").unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut saw_success = false;
        while std::time::Instant::now() < deadline {
            if let Ok(ReloadEvent::Changed { error, .. }) =
                events.recv_timeout(Duration::from_millis(500))
            {
                if error.is_none() {
                    saw_success = true;
                    break;
                }
            }
        }

        assert!(saw_success, "正常改动必须产生 error 为 None 的事件");

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_hot_reload_basic() {
        let test_dir = std::env::temp_dir().join("verseconf_hotreload_test");
        let _ = std::fs::remove_dir_all(&test_dir);
        std::fs::create_dir_all(&test_dir).unwrap();

        let test_file = test_dir.join("test.vcf");
        let mut file = File::create(&test_file).unwrap();
        file.write_all(b"name = \"test\"").unwrap();
        drop(file);

        let mut reloader = HotReloader::new(10);
        reloader.watch(&test_file).unwrap();

        assert!(reloader.get_ast(&test_file).is_some());

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_hot_reload_stats() {
        let test_dir = std::env::temp_dir().join("verseconf_hotreload_test2");
        let _ = std::fs::remove_dir_all(&test_dir);
        std::fs::create_dir_all(&test_dir).unwrap();

        let test_file = test_dir.join("test.vcf");
        let mut file = File::create(&test_file).unwrap();
        file.write_all(b"name = \"test\"").unwrap();
        drop(file);

        let mut reloader = HotReloader::new(10);
        reloader.watch(&test_file).unwrap();

        let stats = reloader.stats();
        assert_eq!(stats.watched_files, 1);

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_hot_reload_multiple_files() {
        let test_dir = std::env::temp_dir().join("verseconf_hotreload_test3");
        let _ = std::fs::remove_dir_all(&test_dir);
        std::fs::create_dir_all(&test_dir).unwrap();

        let file1 = test_dir.join("test1.vcf");
        let file2 = test_dir.join("test2.vcf");

        let mut f1 = File::create(&file1).unwrap();
        f1.write_all(b"name = \"test1\"").unwrap();
        drop(f1);

        let mut f2 = File::create(&file2).unwrap();
        f2.write_all(b"name = \"test2\"").unwrap();
        drop(f2);

        let mut reloader = HotReloader::new(10);
        reloader.watch(&file1).unwrap();
        reloader.watch(&file2).unwrap();

        let stats = reloader.stats();
        assert_eq!(stats.watched_files, 2);

        let _ = std::fs::remove_dir_all(&test_dir);
    }
}

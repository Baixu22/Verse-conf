use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
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
}

impl HotReloader {
    pub fn new(cache_size: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(HotReloadState {
                parser: IncrementalParser::new(cache_size),
                watched_files: HashMap::new(),
            })),
            _watcher: None,
        }
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
            
            watcher.watch(&path_to_watch, RecursiveMode::NonRecursive)
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
        let (tx, rx) = channel();
        
        let watcher = RecommendedWatcher::new(tx, notify::Config::default())
            .map_err(|e| format!("Failed to create watcher: {}", e))?;
        
        std::thread::spawn(move || {
            loop {
                match rx.recv_timeout(Duration::from_secs(1)) {
                    Ok(Ok(event)) => {
                        handle_event(&event, &state);
                    }
                    Ok(Err(e)) => {
                        eprintln!("Watch error: {}", e);
                    }
                    Err(_) => {
                        continue;
                    }
                }
            }
        });
        
        self._watcher = Some(watcher);
        Ok(())
    }
}

fn handle_event(event: &Event, state: &Arc<Mutex<HotReloadState>>) {
    for path in &event.paths {
        let change_event = match event.kind {
            notify::EventKind::Modify(_) => FileChangeEvent::Modified(path.clone()),
            notify::EventKind::Create(_) => FileChangeEvent::Created(path.clone()),
            notify::EventKind::Remove(_) => FileChangeEvent::Removed(path.clone()),
            _ => continue,
        };

        match &change_event {
            FileChangeEvent::Modified(p) | FileChangeEvent::Created(p) => {
                let mut state = state.lock().unwrap();
                if let Ok(ast) = state.parser.parse_file(p) {
                    state.watched_files.insert(p.clone(), ast);
                }
            }
            FileChangeEvent::Removed(p) => {
                let mut state = state.lock().unwrap();
                state.watched_files.remove(p);
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

use anyhow::Result;
use std::path::{Path, PathBuf};
use verseconf_core::{parse_and_validate, AuditEngine, ErrorReport, HotReloader, ReloadEvent};

/// 监视一个配置文件，每次改动后重新校验并报告结果。
///
/// 这是 `HotReloader` 的命令行入口：在此之前它只有库内实现、没有任何用户入口，
/// 属于"能力只存在于代码里"的状态。
///
/// 与编辑器里的实时诊断（LSP）互补：LSP 覆盖"在编辑器里改"，`watch` 覆盖
/// 无编辑器的场景——Agent 在后台改文件、脚本生成配置、CI 里等待一次保存。
///
/// 改动后**无论成功还是失败都会输出**：解析失败时如实报出错误，而不是静默保留
/// 上一次的结果（那正是这个模块以前的行为）。
pub fn execute(file: &str, max_events: usize) -> Result<()> {
    let path = PathBuf::from(file);

    let (mut reloader, events) = HotReloader::with_events(16);
    reloader
        .watch(&path)
        .map_err(|error| anyhow::anyhow!("无法监视 {}：{}", file, error))?;

    println!("[watch] 监视 {}（改动后自动重新校验；Ctrl-C 退出）", file);

    let mut last: Option<String> = None;
    let mut seen = 0usize;

    print_if_changed(&mut last, &check(&path));

    for event in events.iter() {
        match &event {
            ReloadEvent::Changed { path, .. } => {
                // 重新读文件渲染，保证与初次检查的格式一致
                // （事件里也带了 error 字段，供库的使用者直接消费）
                print_if_changed(&mut last, &check(path));
            }
            ReloadEvent::Removed { path } => {
                println!("[watch] {}  已删除：{}", stamp(), path.display());
                last = None;
            }
        }

        seen += 1;
        if max_events > 0 && seen >= max_events {
            break;
        }
    }

    Ok(())
}

/// 只在结论与上一次不同时输出。
///
/// 编辑器与操作系统一次保存常常触发多个文件系统事件，不去重的话
/// "改了一次"会打印两三行一模一样的结果，反而看不出真实变化。
fn print_if_changed(last: &mut Option<String>, line: &str) {
    if last.as_deref() != Some(line) {
        println!("{line}");
        *last = Some(line.to_string());
    }
}

/// 读文件 → 校验（含 schema）→ 顺带报一句安全审计结论
fn check(path: &Path) -> String {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            return format!("[watch] {}  读不到 {}：{}", stamp(), path.display(), error);
        }
    };

    match parse_and_validate(&source) {
        Ok(ast) => {
            let findings = AuditEngine::new().audit_source(&source).findings.len();
            let audit = if findings == 0 {
                "安全审计无发现".to_string()
            } else {
                format!("安全审计 {} 项发现", findings)
            };
            format!(
                "[watch] {}  通过：{} 个根条目，{}",
                stamp(),
                ast.root.entries.len(),
                audit
            )
        }
        Err(error) => {
            // 用与其它子命令一致的 `文件:行:列: 说明` 形式，只取第一行
            let rendered =
                ErrorReport::with_path(error, source.clone(), path.display().to_string()).format();
            let first = rendered.lines().next().unwrap_or("").trim();
            // 文件路径已经在标题里出现过了，这里去掉前缀避免一行太长
            let prefix = format!("{}:", path.display());
            let first = first.strip_prefix(&prefix).unwrap_or(first).trim();
            format!("[watch] {}  {}", stamp(), first)
        }
    }
}

fn stamp() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

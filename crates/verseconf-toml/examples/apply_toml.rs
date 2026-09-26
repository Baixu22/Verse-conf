//! 手动验证与实验用的小入口：把一个编辑计划应用到一份 TOML 文件上。
//!
//! ```text
//! cargo run -p verseconf-toml --example apply_toml -- <文件> <计划.json> [选项]
//!
//! 选项：
//!   --write            覆盖原文件（默认把结果写到 stdout）
//!   --schema <文件>    按与 .vcf 同源的 schema 文本做写入前校验
//!   --mechanism-only   只保留编辑机制，关掉 schema 校验与安全审计
//! ```
//!
//! `--mechanism-only` 只有一个正当用途：消融实验（TF-0079）需要「编辑机制」与
//! 「编辑机制 + 校验层」两组，而两组必须用**同一个可执行文件、同一套输入协议**，
//! 只让校验层这一个变量不同。真实写入不要加这个开关。
//!
//! 拒绝时打印稳定错误码并以非零码退出。

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use verseconf_core::EditPlan;
use verseconf_toml::{apply_toml_edit, apply_toml_edit_with, TomlGuard};

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let write = args.iter().any(|arg| arg == "--write");
    let mechanism_only = args.iter().any(|arg| arg == "--mechanism-only");
    args.retain(|arg| arg != "--write" && arg != "--mechanism-only");

    let schema_path = match args.iter().position(|arg| arg == "--schema") {
        Some(index) => {
            let path = args.get(index + 1).cloned();
            args.drain(index..=index + 1);
            match path {
                Some(path) => Some(path),
                None => {
                    eprintln!("--schema 后面要跟一个文件路径");
                    return ExitCode::from(2);
                }
            }
        }
        None => None,
    };

    let (Some(file), Some(plan_path)) = (args.first(), args.get(1)) else {
        eprintln!(
            "用法：apply_toml <文件> <计划.json> [--write] [--schema <文件>] [--mechanism-only]"
        );
        return ExitCode::from(2);
    };

    let source = match fs::read_to_string(file) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("读不到 {file}：{error}");
            return ExitCode::FAILURE;
        }
    };
    let plan_text = match fs::read_to_string(plan_path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("读不到 {plan_path}：{error}");
            return ExitCode::FAILURE;
        }
    };
    let schema_text = match &schema_path {
        Some(path) => match fs::read_to_string(path) {
            Ok(text) => Some(text),
            Err(error) => {
                eprintln!("读不到 schema {path}：{error}");
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };
    let plan = match EditPlan::from_json(&plan_text) {
        Ok(plan) => plan,
        Err(violations) => {
            let detail: Vec<String> = violations.iter().map(ToString::to_string).collect();
            eprintln!("invalid_plan: {}", detail.join("; "));
            return ExitCode::FAILURE;
        }
    };

    let outcome = if mechanism_only {
        apply_toml_edit_with(&source, &plan, &TomlGuard::edit_mechanism_only())
    } else {
        match schema_text.as_deref() {
            Some(schema) => apply_toml_edit_with(&source, &plan, &TomlGuard::with_schema(schema)),
            None => apply_toml_edit(&source, &plan),
        }
    };

    match outcome {
        Ok(outcome) => {
            for edit in &outcome.applied {
                eprintln!(
                    "{} {}: {} → {}",
                    edit.op.as_str(),
                    edit.path,
                    edit.before,
                    edit.after
                );
            }
            if write {
                if let Err(error) = fs::write(Path::new(file), &outcome.source) {
                    eprintln!("写不回去：{error}");
                    return ExitCode::FAILURE;
                }
                eprintln!("已写入 {file}");
            } else {
                print!("{}", outcome.source);
            }
            ExitCode::SUCCESS
        }
        Err(refusal) => {
            eprintln!("{}: {}", refusal.code(), refusal);
            ExitCode::FAILURE
        }
    }
}

//! 对照实验用的第二个基线：**与 `toml_set` 完全相同的协议**（`{path, value}`），
//! 但用 `span()` 做字节级替换，而不是 `DocumentMut → to_string()`。
//!
//! ```text
//! cargo run -p verseconf-toml --example toml_set_span -- <文件> <点号路径> <值> [--write]
//! ```
//!
//! 存在的理由：第一轮实验里的 `toml_set` 走序列化路径，会在 CRLF 文件上改写换行符。
//! 但那只能说明「这个具体实现」有缺陷，不能说明「成熟库路线不行」。
//! 这个臂回答的问题是：**把写入方式换成 span 替换之后，收益还剩多少？**
//!
//! 它同样**不是产品**：不做写入前校验、不返回结构化拒绝码、不管并发。

use std::fs;
use std::ops::Range;
use std::path::Path;
use std::process::ExitCode;

use toml_edit::{ImDocument, Item, Value as TomlValue};

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let write = args.iter().any(|arg| arg == "--write");
    args.retain(|arg| arg != "--write");

    let (Some(file), Some(dotted), Some(raw)) = (args.first(), args.get(1), args.get(2)) else {
        eprintln!("用法：toml_set_span <文件> <点号路径> <值> [--write]");
        return ExitCode::from(2);
    };

    let source = match fs::read_to_string(file) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("读不到 {file}：{error}");
            return ExitCode::FAILURE;
        }
    };
    let document = match ImDocument::parse(source.clone()) {
        Ok(document) => document,
        Err(error) => {
            eprintln!("解析失败：{error}");
            return ExitCode::FAILURE;
        }
    };

    let mut item: &Item = document.as_item();
    for segment in dotted.split('.') {
        match item.get(segment) {
            Some(next) => item = next,
            None => {
                eprintln!("找不到 {dotted}");
                return ExitCode::FAILURE;
            }
        }
    }

    let Some(span) = item.span() else {
        eprintln!("拿不到 {dotted} 的字节区间");
        return ExitCode::FAILURE;
    };
    let rendered = render_scalar(raw);

    let result = splice(&source, span, &rendered);
    if write {
        if let Err(error) = fs::write(Path::new(file), &result) {
            eprintln!("写不回去：{error}");
            return ExitCode::FAILURE;
        }
    } else {
        print!("{result}");
    }
    ExitCode::SUCCESS
}

/// 只替换那一段字节，其余原样
fn splice(source: &str, span: Range<usize>, replacement: &str) -> String {
    let mut out = String::with_capacity(source.len());
    out.push_str(&source[..span.start]);
    out.push_str(replacement);
    out.push_str(&source[span.end..]);
    out
}

/// 值按最朴素的方式猜类型：布尔、整数、浮点，其余当字符串
fn render_scalar(raw: &str) -> String {
    if let Ok(flag) = raw.parse::<bool>() {
        return TomlValue::from(flag).to_string();
    }
    if let Ok(number) = raw.parse::<i64>() {
        return TomlValue::from(number).to_string();
    }
    if let Ok(number) = raw.parse::<f64>() {
        return TomlValue::from(number).to_string();
    }
    TomlValue::from(raw).to_string()
}

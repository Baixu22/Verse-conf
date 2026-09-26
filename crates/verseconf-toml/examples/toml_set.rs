//! 对照实验用的「成熟保格式库的最薄语义封装」。
//!
//! ```text
//! cargo run -p verseconf-toml --example toml_set -- <文件> <点号路径> <值> [--write]
//! ```
//!
//! 这是**对照组的基线，不是产品的一部分**：它演示「用成熟库改一个值」最少要写多少代码。
//! 刻意不做写入前校验、不返回结构化拒绝码、不管并发，也不检查结果是否仍然合法——
//! 那些正是被测对象要提供的东西，放进基线里就没法比较了。

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use toml_edit::{value, DocumentMut, Item, Table};

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let write = args.iter().any(|arg| arg == "--write");
    args.retain(|arg| arg != "--write");

    let (Some(file), Some(dotted)) = (args.first(), args.get(1)) else {
        eprintln!("用法：toml_set <文件> <点号路径> <值> [--write]");
        eprintln!("      路径给 `-` 时只做「解析 → 序列化」往返，用于单独度量库本身的保真度");
        return ExitCode::from(2);
    };
    let round_trip = dotted == "-";
    let raw = args.get(2);
    if !round_trip && raw.is_none() {
        eprintln!("用法：toml_set <文件> <点号路径> <值> [--write]");
        return ExitCode::from(2);
    }
    let raw = raw.map(String::as_str).unwrap_or("");

    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("读不到 {file}：{error}");
            return ExitCode::FAILURE;
        }
    };
    let mut document: DocumentMut = match text.parse() {
        Ok(document) => document,
        Err(error) => {
            eprintln!("解析失败：{error}");
            return ExitCode::FAILURE;
        }
    };

    let segments: Vec<&str> = dotted.split('.').collect();
    // 路径给 `-` 时不改任何东西，只做「解析 → 序列化」往返，用来单独度量库本身的保真度
    if round_trip {
        let result = document.to_string();
        if write {
            let _ = fs::write(Path::new(file), &result);
        } else {
            print!("{result}");
        }
        return ExitCode::SUCCESS;
    }
    let (key, tables) = segments.split_last().expect("路径至少有一段");
    let Some(table) = table_at_mut(&mut document, tables) else {
        eprintln!("找不到表：{dotted}");
        return ExitCode::FAILURE;
    };
    // 已有的键就地改值（最自然的「薄封装」写法，尽量不动装饰）；没有才插入
    match table.get_mut(key) {
        Some(existing) => *existing = parse_scalar(raw),
        None => {
            table.insert(key, parse_scalar(raw));
        }
    }

    let result = document.to_string();
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

fn table_at_mut<'a>(document: &'a mut DocumentMut, path: &[&str]) -> Option<&'a mut Table> {
    let mut table = document.as_table_mut();
    for segment in path {
        table = table.get_mut(segment)?.as_table_mut()?;
    }
    Some(table)
}

/// 值按最朴素的方式猜类型：布尔、整数、浮点，其余当字符串
fn parse_scalar(raw: &str) -> Item {
    if let Ok(flag) = raw.parse::<bool>() {
        return value(flag);
    }
    if let Ok(number) = raw.parse::<i64>() {
        return value(number);
    }
    if let Ok(number) = raw.parse::<f64>() {
        return value(number);
    }
    value(raw)
}

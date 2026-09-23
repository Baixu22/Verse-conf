use std::io::{BufRead, Write};

use verseconf_mcp::{call_tool, tools_list_value, McpServer};

const USAGE: &str = "\
verseconf-mcp - 把 VerseConf 能力暴露给 Agent 宿主的工具协议服务

用法：
  verseconf-mcp                      以 stdio 逐行 JSON-RPC 方式运行（宿主默认接入方式）
  verseconf-mcp --list-tools         打印四个工具的描述与输入契约
  verseconf-mcp --call <工具名> [JSON 参数]
                                     单次调用工具，便于冒烟测试与排错
  verseconf-mcp --help               显示本帮助

示例：
  verseconf-mcp --call verseconf_validate '{\"source\":\"port = 8080\\n\"}'
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("--help") | Some("-h") => print!("{}", USAGE),
        Some("--list-tools") => list_tools(),
        Some("--call") => call_once(&args),
        _ => serve_stdio(),
    }
}

fn list_tools() {
    println!(
        "{}",
        serde_json::to_string_pretty(&tools_list_value()).unwrap_or_default()
    );
}

fn call_once(args: &[String]) {
    let Some(name) = args.get(1) else {
        eprintln!("--call 需要工具名，例如 --call verseconf_validate");
        std::process::exit(2);
    };
    let raw = args.get(2).map(String::as_str).unwrap_or("{}");
    let arguments: serde_json::Value = match serde_json::from_str(raw) {
        Ok(arguments) => arguments,
        Err(error) => {
            eprintln!("参数不是合法 JSON：{}", error);
            std::process::exit(2);
        }
    };

    match call_tool(name, &arguments) {
        Ok(outcome) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&outcome.structured).unwrap_or_default()
            );
        }
        Err(failure) => {
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&failure.to_json()).unwrap_or_default()
            );
            std::process::exit(1);
        }
    }
}

fn serve_stdio() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut server = McpServer::new();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => break,
        };

        if let Some(response) = server.handle_line(&line) {
            if writeln!(out, "{}", response).is_err() {
                break;
            }
            let _ = out.flush();
        }
    }
}

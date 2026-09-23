//! 真实二进制层面的验收：宿主能否发现并调用四个工具、失败是否返回结构化原因。
//!
//! 这里刻意走子进程 + stdio，而不是直接调用库函数，以证明「宿主可接入」成立。

use std::io::Write;
use std::process::{Command, Stdio};

fn run_session(lines: &[&str]) -> (Vec<serde_json::Value>, String, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_verseconf-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("应当能启动 MCP 服务");

    {
        let stdin = child.stdin.as_mut().expect("stdin 可用");
        for line in lines {
            writeln!(stdin, "{}", line).expect("应当能写入请求");
        }
    }
    // 关闭 stdin：服务读到 EOF 后自行退出
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("应当能收集输出");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let responses = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("每行必须是合法 JSON"))
        .collect();

    (responses, stderr, output.status.code().unwrap_or(-1))
}

#[test]
fn host_can_discover_and_call_all_four_tools_over_stdio() {
    let source = "#@schema {\n  server {\n    type = \"table\"\n    port {\n      type = \"integer\"\n    }\n  }\n}\n\nserver {\n  port = 8080 #@ range(1..65535)\n}\n";

    let requests = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"test-host","version":"1.0"}}}"#.to_string(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#.to_string(),
        format!(
            r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{{"name":"verseconf_validate","arguments":{{"source":{}}}}}}}"#,
            serde_json::to_string(source).unwrap()
        ),
        r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"verseconf_audit","arguments":{"source":"db_password = \"secret\"\n"}}}"#.to_string(),
        format!(
            r#"{{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{{"name":"verseconf_apply_edit","arguments":{{"source":{},"plan":{{"version":"1.0","edits":[{{"op":"set","path":["server","port"],"value":9090,"expect":{{"value":8080}}}}]}}}}}}}}"#,
            serde_json::to_string(source).unwrap()
        ),
        r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"verseconf_edit_range","arguments":{"source":"port = 8080\n","start":7,"end":11,"replacement":"9090"}}}"#.to_string(),
    ];
    let request_refs: Vec<&str> = requests.iter().map(String::as_str).collect();

    let (responses, stderr, status) = run_session(&request_refs);
    assert_eq!(status, 0, "服务应当正常退出，stderr: {}", stderr);

    // 通知不产生响应：7 条请求里 6 条有 id
    assert_eq!(responses.len(), 6, "响应条数应与带 id 的请求一致");
    let ids: Vec<i64> = responses
        .iter()
        .map(|response| response["id"].as_i64().expect("响应必须带 id"))
        .collect();
    assert_eq!(ids, vec![1, 2, 3, 4, 5, 6]);

    // 握手
    assert!(responses[0]["result"]["capabilities"]["tools"].is_object());

    // 工具发现
    let tools = responses[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 4);
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    for expected in [
        "verseconf_validate",
        "verseconf_audit",
        "verseconf_apply_edit",
        "verseconf_edit_range",
    ] {
        assert!(names.contains(&expected), "缺少工具 {}", expected);
    }

    // 四个工具都能被调用并返回结构化结果
    assert_eq!(responses[2]["result"]["isError"], serde_json::json!(false));
    assert_eq!(
        responses[2]["result"]["structuredContent"]["valid"],
        serde_json::json!(true)
    );

    assert_eq!(responses[3]["result"]["isError"], serde_json::json!(false));
    assert!(!responses[3]["result"]["structuredContent"]["findings"]
        .as_array()
        .unwrap()
        .is_empty());

    let updated = responses[4]["result"]["structuredContent"]["source"]
        .as_str()
        .unwrap();
    assert!(updated.contains("  port = 9090 #@ range(1..65535)"));
    assert!(updated.contains("#@schema {"));

    assert_eq!(
        responses[5]["result"]["structuredContent"]["source"],
        serde_json::json!("port = 9090\n")
    );
}

#[test]
fn tool_failures_return_structured_reasons_not_just_text() {
    let refused = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"verseconf_apply_edit","arguments":{"source":"port = 8080\n","plan":{"version":"1.0","edits":[{"op":"set","path":["missing"],"value":1}]}}}}"#;
    let unknown = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"verseconf_nope","arguments":{}}}"#;

    let (responses, stderr, status) = run_session(&[refused, unknown]);
    assert_eq!(status, 0, "stderr: {}", stderr);

    assert_eq!(responses[0]["result"]["isError"], serde_json::json!(true));
    assert_eq!(
        responses[0]["result"]["structuredContent"]["code"],
        serde_json::json!("target_not_found")
    );
    assert_eq!(
        responses[0]["result"]["structuredContent"]["details"]["path"],
        serde_json::json!("missing")
    );

    assert_eq!(responses[1]["result"]["isError"], serde_json::json!(true));
    assert_eq!(
        responses[1]["result"]["structuredContent"]["code"],
        serde_json::json!("unknown_tool")
    );
}

#[test]
fn protocol_level_problems_use_json_rpc_errors() {
    let (responses, stderr, status) = run_session(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"resources/list","params":{}}"#,
        "{ this is not json",
    ]);
    assert_eq!(status, 0, "stderr: {}", stderr);

    assert_eq!(responses[0]["error"]["code"], serde_json::json!(-32601));
    assert_eq!(
        responses[0]["error"]["data"]["code"],
        serde_json::json!("method_not_found")
    );
    assert_eq!(responses[1]["error"]["code"], serde_json::json!(-32700));
}

#[test]
fn list_tools_flag_prints_the_contract_for_offline_inspection() {
    let output = Command::new(env!("CARGO_BIN_EXE_verseconf-mcp"))
        .arg("--list-tools")
        .output()
        .expect("应当能运行 --list-tools");
    assert_eq!(output.status.code(), Some(0));

    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("输出必须是合法 JSON");
    let tools = payload["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 4);
    for tool in tools {
        assert!(tool["inputSchema"]["properties"].is_object());
    }
}

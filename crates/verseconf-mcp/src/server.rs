//! 极简 MCP 风格 JSON-RPC 服务端：握手、工具发现、工具调用。
//!
//! 传输层是逐行 JSON（stdio），因此不需要任何原生依赖即可被宿主拉起。
//! 工具自身的失败通过 `isError` 结果返回结构化原因；协议层问题才用 JSON-RPC error。

use serde_json::{json, Value};

use crate::tools::{tool_result_value, tools_list_value};

/// 服务端名称
pub const SERVER_NAME: &str = "verseconf";
/// 服务端版本
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
/// 默认协议版本
pub const PROTOCOL_VERSION: &str = "2024-11-05";

const INSTRUCTIONS: &str =
    "VerseConf 工具集：校验、安全审计、按编辑意图做确定性最小改动、区间编辑。\
所有编辑工具都只返回改动后的文本，不写文件；目标无法唯一定位或改动后不合法时一律拒绝。";

/// JSON-RPC / MCP 会话状态
#[derive(Debug, Default)]
pub struct McpServer {
    initialized: bool,
}

impl McpServer {
    pub fn new() -> Self {
        Self::default()
    }

    /// 宿主是否已经完成 `initialize` 握手
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// 处理一行输入；通知或空行返回 None
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        let message: Value = match serde_json::from_str(trimmed) {
            Ok(message) => message,
            Err(error) => {
                return Some(
                    error_response(
                        Value::Null,
                        -32700,
                        format!("JSON 解析失败：{}", error),
                        json!({ "code": "parse_error" }),
                    )
                    .to_string(),
                )
            }
        };

        self.handle(&message).map(|response| response.to_string())
    }

    /// 处理一条已解析的消息
    pub fn handle(&mut self, message: &Value) -> Option<Value> {
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let params = message.get("params").cloned().unwrap_or(Value::Null);

        // 通知没有 id，不产生响应
        let Some(id) = message.get("id").cloned() else {
            if method == "notifications/initialized" {
                self.initialized = true;
            }
            return None;
        };

        match method.as_str() {
            "initialize" => {
                self.initialized = true;
                let requested = params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .unwrap_or(PROTOCOL_VERSION);

                Some(success(
                    id,
                    json!({
                        "protocolVersion": requested,
                        "capabilities": { "tools": { "listChanged": false } },
                        "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
                        "instructions": INSTRUCTIONS,
                    }),
                ))
            }
            "ping" => Some(success(id, json!({}))),
            "tools/list" => Some(success(id, tools_list_value())),
            "tools/call" => {
                let name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if name.is_empty() {
                    return Some(error_response(
                        id,
                        -32602,
                        "tools/call 缺少工具名 'name'",
                        json!({ "code": "invalid_params" }),
                    ));
                }
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
                Some(success(id, tool_result_value(&name, &arguments)))
            }
            other => Some(error_response(
                id,
                -32601,
                format!("不支持的方法：{}", other),
                json!({ "code": "method_not_found", "method": other }),
            )),
        }
    }
}

fn success(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: impl Into<String>, data: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message.into(), "data": data }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: i64, method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
    }

    #[test]
    fn handshake_advertises_tool_capability() {
        let mut server = McpServer::new();
        assert!(!server.is_initialized());

        let response = server
            .handle(&request(
                1,
                "initialize",
                json!({ "protocolVersion": "2024-11-05", "clientInfo": { "name": "test" } }),
            ))
            .expect("initialize 必须返回响应");

        assert_eq!(response["jsonrpc"], json!("2.0"));
        assert_eq!(response["id"], json!(1));
        assert_eq!(response["result"]["protocolVersion"], json!("2024-11-05"));
        assert!(response["result"]["capabilities"]["tools"].is_object());
        assert_eq!(response["result"]["serverInfo"]["name"], json!("verseconf"));
        assert!(server.is_initialized());
    }

    #[test]
    fn initialized_notification_produces_no_response() {
        let mut server = McpServer::new();
        let notification = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(server.handle(&notification).is_none());
        assert!(server.is_initialized());
    }

    #[test]
    fn tools_list_exposes_four_tools_with_schemas() {
        let mut server = McpServer::new();
        let response = server.handle(&request(2, "tools/list", json!({}))).unwrap();
        let tools = response["result"]["tools"].as_array().unwrap();

        assert_eq!(tools.len(), 4);
        let names: Vec<&str> = tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"verseconf_validate"));
        assert!(names.contains(&"verseconf_audit"));
        assert!(names.contains(&"verseconf_apply_edit"));
        assert!(names.contains(&"verseconf_edit_range"));
        for tool in tools {
            assert!(tool["description"].as_str().unwrap().len() > 10);
            assert_eq!(tool["inputSchema"]["type"], json!("object"));
        }
    }

    #[test]
    fn tools_call_returns_structured_success_and_failure() {
        let mut server = McpServer::new();

        let ok = server
            .handle(&request(
                3,
                "tools/call",
                json!({ "name": "verseconf_validate", "arguments": { "source": "port = 8080\n" } }),
            ))
            .unwrap();
        assert_eq!(ok["result"]["isError"], json!(false));
        assert_eq!(ok["result"]["structuredContent"]["valid"], json!(true));
        assert_eq!(ok["result"]["content"][0]["type"], json!("text"));

        let refused = server
            .handle(&request(
                4,
                "tools/call",
                json!({
                    "name": "verseconf_apply_edit",
                    "arguments": {
                        "source": "port = 8080\n",
                        "plan": { "version": "1.0", "edits": [ { "op": "set", "path": ["missing"], "value": 1 } ] }
                    }
                }),
            ))
            .unwrap();
        assert_eq!(refused["result"]["isError"], json!(true));
        assert_eq!(
            refused["result"]["structuredContent"]["code"],
            json!("target_not_found")
        );
    }

    #[test]
    fn protocol_errors_are_separate_from_tool_errors() {
        let mut server = McpServer::new();

        let unknown = server
            .handle(&request(5, "resources/list", json!({})))
            .unwrap();
        assert_eq!(unknown["error"]["code"], json!(-32601));
        assert_eq!(unknown["error"]["data"]["code"], json!("method_not_found"));

        let missing_name = server
            .handle(&request(6, "tools/call", json!({ "arguments": {} })))
            .unwrap();
        assert_eq!(missing_name["error"]["code"], json!(-32602));

        let malformed = server.handle_line("{ not json").unwrap();
        assert!(malformed.contains("-32700"));
    }

    #[test]
    fn blank_lines_are_ignored() {
        let mut server = McpServer::new();
        assert!(server.handle_line("   ").is_none());
        assert!(server.handle_line("").is_none());
    }
}

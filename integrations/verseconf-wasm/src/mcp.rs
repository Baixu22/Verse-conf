//! WebAssembly 侧的宿主入口。
//!
//! 这一层不重新实现任何能力：它直接复用 `verseconf-mcp` 的工具与会话实现，
//! 因此同一份输入在「本机原生二进制」与「宿主零安装的 wasm 模块」上返回
//! 完全相同的 JSON。宿主只需要一个 JS 运行时（Agent 宿主本来就带），
//! 不需要 Rust 工具链，也不需要安装任何本机二进制。

use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

use verseconf_mcp::{tool_result_value, tools_list_value, McpServer, PROTOCOL_VERSION};

/// wasm 版的 MCP 会话：与原生 stdio 服务端逐行处理完全一致。
#[wasm_bindgen]
pub struct WasmMcpServer {
    inner: McpServer,
}

#[wasm_bindgen]
impl WasmMcpServer {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmMcpServer {
        WasmMcpServer {
            inner: McpServer::new(),
        }
    }

    /// 处理一行 JSON-RPC 输入。
    ///
    /// 通知（没有 `id`）与空行返回 `undefined`，调用方不需要回写任何内容。
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        self.inner.handle_line(line)
    }

    /// 宿主是否已经完成 `initialize` 握手
    pub fn is_initialized(&self) -> bool {
        self.inner.is_initialized()
    }
}

impl Default for WasmMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

/// `tools/list` 的结果，与原生命令行/stdio 服务端逐字节一致。
#[wasm_bindgen]
pub fn tools_json() -> String {
    tools_list_value().to_string()
}

/// 直接调用一个工具，返回与 `tools/call` 完全相同的结果信封。
///
/// 参数是 JSON 文本（对象）。`arguments_json` 不是合法 JSON 时返回 `Err`，
/// 因为那是调用方的序列化错误，不是工具的执行失败——与协议层把 JSON 解析
/// 失败当作 JSON-RPC error 的处理保持一致。
#[wasm_bindgen]
pub fn call_tool_json(name: &str, arguments_json: &str) -> Result<String, JsValue> {
    let arguments: Value = if arguments_json.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(arguments_json)
            .map_err(|error| JsValue::from_str(&format!("arguments 不是合法 JSON：{}", error)))?
    };

    Ok(tool_result_value(name, &arguments).to_string())
}

/// 服务端自述信息，宿主可用它做版本核对。
#[wasm_bindgen]
pub fn server_info_json() -> String {
    json!({
        "name": verseconf_mcp::SERVER_NAME,
        "version": verseconf_mcp::SERVER_VERSION,
        "protocolVersion": PROTOCOL_VERSION,
    })
    .to_string()
}

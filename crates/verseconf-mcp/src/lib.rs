//! VerseConf 的工具协议服务：把校验、安全审计、意图应用与区间编辑暴露给 Agent 宿主。
//!
//! 设计约束：
//! - 所有编辑能力都只返回改动后的文本，不写文件；
//! - 失败返回稳定的错误码与结构化细节，宿主不必解析文案；
//! - 协议层问题用 JSON-RPC error，工具执行失败用 `isError` 结果。

pub mod server;
pub mod tools;

pub use server::{McpServer, PROTOCOL_VERSION, SERVER_NAME, SERVER_VERSION};
pub use tools::{
    call_tool, tool_descriptors, tool_result_value, tools_list_value, ToolDescriptor, ToolFailure,
    ToolOutcome, TOOL_APPLY_EDIT, TOOL_AUDIT, TOOL_EDIT_RANGE, TOOL_VALIDATE,
};

pub use crate::ast::span::Span;

/// 源码信息
#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub path: Option<String>,
    pub content: String,
}

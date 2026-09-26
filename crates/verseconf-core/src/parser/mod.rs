pub mod builder;
pub mod config;
pub mod error;

pub use builder::*;
pub use config::*;
pub use error::*;

use crate::ast::*;
use crate::lexer::*;
use crate::parser::AstBuilder;
use crate::Span;
use std::fmt;

/// 解析阶段错误：词法错误或语法错误，二者都携带源码位置
#[derive(Debug, Clone)]
pub enum ParseStageError {
    Lex(LexError),
    Parse(ParseError),
}

impl ParseStageError {
    /// 错误消息（不含位置前缀）
    pub fn message(&self) -> &str {
        match self {
            ParseStageError::Lex(e) => &e.message,
            ParseStageError::Parse(e) => &e.message,
        }
    }

    /// 错误位置
    pub fn span(&self) -> Span {
        match self {
            ParseStageError::Lex(e) => e.span,
            ParseStageError::Parse(e) => e.span,
        }
    }

    /// 是否为词法错误
    pub fn is_lexical(&self) -> bool {
        matches!(self, ParseStageError::Lex(_))
    }
}

impl fmt::Display for ParseStageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseStageError::Lex(e) => write!(f, "{}", e),
            ParseStageError::Parse(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for ParseStageError {}

impl From<LexError> for ParseStageError {
    fn from(e: LexError) -> Self {
        ParseStageError::Lex(e)
    }
}

impl From<ParseError> for ParseStageError {
    fn from(e: ParseError) -> Self {
        ParseStageError::Parse(e)
    }
}

/// 解析器
pub struct Parser {
    source: String,
    config: ParseConfig,
}

impl Parser {
    /// 创建新的解析器
    pub fn new(source: String) -> Self {
        Self {
            source,
            config: ParseConfig::default(),
        }
    }

    /// 创建带配置的解析器
    pub fn with_config(source: String, config: ParseConfig) -> Self {
        Self { source, config }
    }

    /// 解析源码返回 AST
    pub fn parse(&self) -> Result<Ast, ParseStageError> {
        let mut lexer = Lexer::new(&self.source);
        let tokens = lexer.tokenize_all()?;

        let mut builder = AstBuilder::new(self.source.clone());
        let ast = builder.build(&tokens)?;

        Ok(ast)
    }

    /// 解析源码返回 AST 和 warnings（tolerant 模式）
    pub fn parse_with_warnings(&self) -> Result<ParseResult<Ast>, ParseStageError> {
        let mut lexer = Lexer::new(&self.source);
        let tokens = lexer.tokenize_all()?;

        let mut builder = AstBuilder::new(self.source.clone());
        let ast = builder.build(&tokens)?;

        let mut result = ParseResult::new(ast);

        if self.config.tolerant {
            result = Self::apply_tolerant_fixes(result, &self.source);
        }

        Ok(result)
    }

    /// 递归收集重复键警告。
    ///
    /// 重复键在严格模式下由校验器拒绝；宽容模式下这里只**记录**事实，
    /// 由调用方决定如何呈现，不静默丢弃任何一个值。
    fn collect_duplicate_key_warnings(
        table: &TableBlock,
        prefix: &str,
        seen: &mut std::collections::BTreeMap<String, Span>,
        warnings: &mut Vec<ParseWarning>,
    ) {
        for entry in &table.entries {
            match entry {
                TableEntry::KeyValue(kv) => {
                    let name = kv.key.as_str();
                    let path = if prefix.is_empty() {
                        name.to_string()
                    } else {
                        format!("{}.{}", prefix, name)
                    };
                    if let Some(previous) = seen.get(&path) {
                        warnings.push(make_warning(
                            &format!(
                                "重复的键 '{}'：本次出现在 {}，此前出现在 {}，后者将覆盖前者",
                                path, kv.span, previous
                            ),
                            kv.span,
                            WarningCategory::Other,
                        ));
                    } else {
                        seen.insert(path, kv.span);
                    }
                }
                TableEntry::TableBlock(child) => {
                    let path = match &child.name {
                        Some(name) if !prefix.is_empty() => format!("{}.{}", prefix, name),
                        Some(name) => name.clone(),
                        None => prefix.to_string(),
                    };
                    Self::collect_duplicate_key_warnings(child, &path, seen, warnings);
                }
                _ => {}
            }
        }
    }

    /// Apply tolerant mode fixes and collect warnings.
    ///
    /// 这里只做**能证明其正确性的**宽容处理，不做语法改写：
    /// 之前这个函数原样返回入参，于是 `--tolerant` 是个静默空操作——
    /// 同样的坏文件带不带该标志都返回同一个错误，而文档却把它列为一项能力。
    ///
    /// 当前实现覆盖两类可确定的宽容：
    /// 1. 重复键：后一个值覆盖前一个（与 TOML 之外多数配置格式的直觉一致），
    ///    并产生一条警告说明被覆盖的位置，而不是让文件解析失败；
    /// 2. 空文档：产生一条警告说明文件为空，仍返回空 AST 而不是报错。
    ///
    /// 词法错误和结构性语法错误**不**在这里被"修好"——猜测用户意图并改写
    /// 语法正是本项目反对的做法，这类错误仍然照常拒绝。
    fn apply_tolerant_fixes(result: ParseResult<Ast>, source: &str) -> ParseResult<Ast> {
        let mut result = result;

        if source.trim().is_empty() {
            result.warnings.push(make_warning(
                "文件为空，没有任何配置项",
                Span::unknown(),
                WarningCategory::Other,
            ));
        }

        let mut seen: std::collections::BTreeMap<String, Span> = std::collections::BTreeMap::new();
        Self::collect_duplicate_key_warnings(
            &result.value.root,
            "",
            &mut seen,
            &mut result.warnings,
        );

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_basic() {
        let source = r#"
name = "test"
version = "1.0.0"
"#;
        let parser = Parser::new(source.to_string());
        let result = parser.parse();
        assert!(result.is_ok());
    }

    #[test]
    fn test_parser_with_table() {
        let source = r#"
database {
    host = "localhost"
    port = 5432
}
"#;
        let parser = Parser::new(source.to_string());
        let result = parser.parse();
        assert!(result.is_ok());
    }

    #[test]
    fn test_parser_with_array() {
        let source = r#"
features = [
    "auth",
    "logging",
]
"#;
        let parser = Parser::new(source.to_string());
        let result = parser.parse();
        assert!(result.is_ok());
    }

    #[test]
    fn test_parser_with_metadata() {
        let source = r#"
port = 8080 #@ range(1024..65535), description="服务器端口"
"#;
        let parser = Parser::new(source.to_string());
        let result = parser.parse();
        assert!(result.is_ok());
    }
}

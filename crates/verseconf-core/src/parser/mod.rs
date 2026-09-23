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

    /// Apply tolerant mode fixes and collect warnings
    fn apply_tolerant_fixes(result: ParseResult<Ast>, _source: &str) -> ParseResult<Ast> {
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

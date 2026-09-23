use crate::lexer::LexError;
use crate::parser::{ParseError, ParseStageError};
use crate::semantic::ValidationError;
use crate::source::Span;
use std::fmt;

/// VerseConf 统一错误类型
#[derive(Debug)]
pub enum VerseconfError {
    Lex(LexError),
    Parse(ParseError),
    Semantic {
        message: String,
        span: Span,
    },
    Validation(ValidationError),
    Template(String),
    /// 文件包含（@include）解析/合并失败
    Include(String),
    Io(std::io::Error),
}

impl fmt::Display for VerseconfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerseconfError::Lex(e) => write!(f, "{}", e),
            VerseconfError::Parse(e) => write!(f, "{}", e),
            VerseconfError::Semantic { message, span } => {
                write!(f, "Semantic error at {}: {}", span, message)
            }
            VerseconfError::Validation(e) => write!(f, "{}", e),
            VerseconfError::Template(e) => write!(f, "Template error: {}", e),
            VerseconfError::Include(e) => write!(f, "Include error: {}", e),
            VerseconfError::Io(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl VerseconfError {
    /// 错误的源码位置；模板与 IO 错误没有位置
    pub fn span(&self) -> Span {
        match self {
            VerseconfError::Lex(e) => e.span,
            VerseconfError::Parse(e) => e.span,
            VerseconfError::Semantic { span, .. } => *span,
            VerseconfError::Validation(e) => e.span,
            VerseconfError::Template(_) | VerseconfError::Include(_) | VerseconfError::Io(_) => {
                Span::unknown()
            }
        }
    }

    /// 纯错误消息：不含位置前缀，也不含内部类型名
    pub fn message(&self) -> String {
        match self {
            VerseconfError::Lex(e) => e.message.clone(),
            VerseconfError::Parse(e) => e.message.clone(),
            VerseconfError::Semantic { message, .. } => message.clone(),
            VerseconfError::Validation(e) => e.message.clone(),
            VerseconfError::Template(e) => e.clone(),
            VerseconfError::Include(e) => e.clone(),
            VerseconfError::Io(e) => e.to_string(),
        }
    }

    /// 错误类别（面向用户的短标签）
    pub fn kind(&self) -> &'static str {
        match self {
            VerseconfError::Lex(_) => "lexical error",
            VerseconfError::Parse(_) => "parse error",
            VerseconfError::Semantic { .. } => "semantic error",
            VerseconfError::Validation(_) => "validation error",
            VerseconfError::Template(_) => "template error",
            VerseconfError::Include(_) => "include error",
            VerseconfError::Io(_) => "io error",
        }
    }
}

impl std::error::Error for VerseconfError {}

impl From<ParseStageError> for VerseconfError {
    fn from(e: ParseStageError) -> Self {
        match e {
            ParseStageError::Lex(e) => VerseconfError::Lex(e),
            ParseStageError::Parse(e) => VerseconfError::Parse(e),
        }
    }
}

impl From<LexError> for VerseconfError {
    fn from(e: LexError) -> Self {
        VerseconfError::Lex(e)
    }
}

impl From<ParseError> for VerseconfError {
    fn from(e: ParseError) -> Self {
        VerseconfError::Parse(e)
    }
}

impl From<ValidationError> for VerseconfError {
    fn from(e: ValidationError) -> Self {
        VerseconfError::Validation(e)
    }
}

impl From<std::io::Error> for VerseconfError {
    fn from(e: std::io::Error) -> Self {
        VerseconfError::Io(e)
    }
}

/// 错误报告（带源码高亮）
pub struct ErrorReport {
    pub error: VerseconfError,
    pub source: String,
    /// 出错文件路径；用于输出 文件:行:列: 说明
    pub path: Option<String>,
    pub context_lines: usize,
}

impl ErrorReport {
    pub fn new(error: VerseconfError, source: String) -> Self {
        Self {
            error,
            source,
            path: None,
            context_lines: 3,
        }
    }

    /// 带文件路径的错误报告
    pub fn with_path(
        error: VerseconfError,
        source: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            error,
            source: source.into(),
            path: Some(path.into()),
            context_lines: 3,
        }
    }

    /// 出错位置显示名：有路径时为 路径:行:列
    pub fn location(&self) -> String {
        let span = self.error.span();
        let name = self.path.as_deref().unwrap_or("<source>");
        if span.is_unknown() {
            name.to_string()
        } else {
            format!("{}:{}:{}", name, span.line, span.column)
        }
    }

    /// 格式化错误输出，首行形如 config.vcf:12:5: error: 说明
    pub fn format(&self) -> String {
        let span = self.error.span();

        let mut output = String::new();
        output.push_str(&format!(
            "{}: {}: {}\n",
            self.location(),
            self.error.kind(),
            self.error.message()
        ));

        if !span.is_unknown() && !self.source.is_empty() {
            output.push_str(&self.format_context(&span));
        }

        output
    }

    fn format_context(&self, span: &Span) -> String {
        let lines: Vec<&str> = self.source.lines().collect();
        if span.line == 0 || span.line as usize > lines.len() {
            return String::new();
        }

        let line_idx = (span.line - 1) as usize;
        let start = line_idx.saturating_sub(self.context_lines);
        let end = (line_idx + self.context_lines + 1).min(lines.len());

        let mut output = String::new();
        let name = self.path.as_deref().unwrap_or("<source>");
        output.push_str(&format!("  --> {}:{}:{}\n", name, span.line, span.column));

        for (i, line) in lines.iter().enumerate().skip(start).take(end - start) {
            let line_num = i + 1;
            output.push_str(&format!("{:>4} | {}\n", line_num, line));

            if i == line_idx {
                let caret_pos = span.column.saturating_sub(1) as usize;
                output.push_str(&format!("     | {}^\n", " ".repeat(caret_pos)));
            }
        }

        output
    }
}

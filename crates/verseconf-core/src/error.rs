use crate::lexer::LexError;
use crate::parser::ParseError;
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
            VerseconfError::Io(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for VerseconfError {}

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
    pub context_lines: usize,
}

impl ErrorReport {
    pub fn new(error: VerseconfError, source: String) -> Self {
        Self {
            error,
            source,
            context_lines: 3,
        }
    }

    /// 格式化错误输出
    pub fn format(&self) -> String {
        let span = match &self.error {
            VerseconfError::Lex(e) => e.span,
            VerseconfError::Parse(e) => e.span,
            VerseconfError::Semantic { span, .. } => *span,
            VerseconfError::Validation(e) => e.span,
            VerseconfError::Template(_) => Span::unknown(),
            VerseconfError::Io(_) => Span::unknown(),
        };

        let mut output = String::new();
        output.push_str(&format!("error: {}\n", self.error));

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
        output.push_str(&format!(
            "  --> <source>:{}:{}\n",
            span.line, span.column
        ));

        for (i, line) in lines.iter().enumerate().skip(start).take(end - start) {
            let line_num = i + 1;
            output.push_str(&format!("{:>4} | {}\n", line_num, line));

            if i == line_idx {
                let indent = "     | ";
                let caret_pos = span.column.saturating_sub(1) as usize;
                let padding = " ".repeat(caret_pos.min(indent.len()));
                output.push_str(&format!("{}^\n", padding));
            }
        }

        output
    }
}

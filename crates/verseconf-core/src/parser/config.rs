use crate::ast::Span;

/// Parse configuration options
#[derive(Debug, Clone)]
pub struct ParseConfig {
    /// Enable tolerant parsing mode (auto-fix common issues)
    pub tolerant: bool,
    /// Collect warnings during parsing
    pub collect_warnings: bool,
}

impl Default for ParseConfig {
    fn default() -> Self {
        Self {
            tolerant: false,
            collect_warnings: false,
        }
    }
}

/// Warning during parsing
#[derive(Debug, Clone)]
pub struct ParseWarning {
    /// Warning message
    pub message: String,
    /// Source location
    pub span: Span,
    /// Warning category
    pub category: WarningCategory,
}

/// Warning categories
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WarningCategory {
    BooleanCase,
    QuoteStyle,
    Whitespace,
    Deprecated,
    Other,
}

impl std::fmt::Display for WarningCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WarningCategory::BooleanCase => write!(f, "BooleanCase"),
            WarningCategory::QuoteStyle => write!(f, "QuoteStyle"),
            WarningCategory::Whitespace => write!(f, "Whitespace"),
            WarningCategory::Deprecated => write!(f, "Deprecated"),
            WarningCategory::Other => write!(f, "Other"),
        }
    }
}

impl std::fmt::Display for ParseWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}] {}", self.category, self.message)
    }
}

/// Parse result with warnings
#[derive(Debug)]
pub struct ParseResult<T> {
    /// Parsed AST
    pub value: T,
    /// Warnings collected during parsing
    pub warnings: Vec<ParseWarning>,
}

impl<T> ParseResult<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            warnings: Vec::new(),
        }
    }

    pub fn with_warning(mut self, warning: ParseWarning) -> Self {
        self.warnings.push(warning);
        self
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

/// Helper to create a warning
pub fn make_warning(message: &str, span: Span, category: WarningCategory) -> ParseWarning {
    ParseWarning {
        message: message.to_string(),
        span,
        category,
    }
}

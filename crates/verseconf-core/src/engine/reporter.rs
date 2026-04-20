use std::fmt;

/// ANSI color codes for terminal output
#[derive(Debug, Clone, Copy)]
pub struct Colors {
    pub red: &'static str,
    pub green: &'static str,
    pub yellow: &'static str,
    pub blue: &'static str,
    pub magenta: &'static str,
    pub cyan: &'static str,
    pub white: &'static str,
    pub bold: &'static str,
    pub reset: &'static str,
}

impl Colors {
    pub fn enabled() -> Self {
        Self {
            red: "\x1b[31m",
            green: "\x1b[32m",
            yellow: "\x1b[33m",
            blue: "\x1b[34m",
            magenta: "\x1b[35m",
            cyan: "\x1b[36m",
            white: "\x1b[37m",
            bold: "\x1b[1m",
            reset: "\x1b[0m",
        }
    }

    pub fn disabled() -> Self {
        Self {
            red: "",
            green: "",
            yellow: "",
            blue: "",
            magenta: "",
            cyan: "",
            white: "",
            bold: "",
            reset: "",
        }
    }

    /// Detect if colors should be enabled
    pub fn auto() -> Self {
        if atty::is(atty::Stream::Stderr) {
            Self::enabled()
        } else {
            Self::disabled()
        }
    }
}

/// Diagnostic severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
        }
    }
}

/// A single diagnostic message
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Option<String>,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub source_line: Option<String>,
    pub suggestions: Vec<String>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code: None,
            message: message.into(),
            file: None,
            line: None,
            column: None,
            source_line: None,
            suggestions: Vec::new(),
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code: None,
            message: message.into(),
            file: None,
            line: None,
            column: None,
            source_line: None,
            suggestions: Vec::new(),
        }
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn with_location(mut self, file: impl Into<String>, line: usize, column: usize) -> Self {
        self.file = Some(file.into());
        self.line = Some(line);
        self.column = Some(column);
        self
    }

    pub fn with_source_line(mut self, line: impl Into<String>) -> Self {
        self.source_line = Some(line.into());
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestions.push(suggestion.into());
        self
    }
}

/// Diagnostic reporter with colored output
pub struct Reporter {
    colors: Colors,
}

impl Reporter {
    pub fn new(colors: Colors) -> Self {
        Self { colors }
    }

    pub fn auto() -> Self {
        Self::new(Colors::auto())
    }

    /// Format a single diagnostic message
    pub fn format_diagnostic(&self, diag: &Diagnostic) -> String {
        let mut output = String::new();
        
        // Severity and message
        let (color, icon) = match diag.severity {
            Severity::Error => (self.colors.red, "✖"),
            Severity::Warning => (self.colors.yellow, "⚠"),
            Severity::Info => (self.colors.blue, "ℹ"),
        };
        
        output.push_str(&format!("{}{}{}{}{} ", 
            self.colors.bold, color, icon, self.colors.reset, self.colors.bold));
        output.push_str(&format!("{}{}:", diag.severity, self.colors.reset));
        
        if let Some(ref code) = diag.code {
            output.push_str(&format!(" [{}]", code));
        }
        
        output.push_str(&format!(" {}\n", diag.message));
        
        // Location
        if let (Some(ref file), Some(line), Some(col)) = (&diag.file, diag.line, diag.column) {
            output.push_str(&format!("  {}-->{} {}:{}:{}\n", 
                self.colors.cyan, self.colors.reset, file, line, col));
        }
        
        // Source line with highlight
        if let Some(ref source) = diag.source_line {
            output.push_str(&format!("   {}|{}\n", self.colors.cyan, self.colors.reset));
            output.push_str(&format!(" {}{}{} {}\n", 
                self.colors.cyan, 
                diag.line.map(|l| l.to_string()).unwrap_or_default(), 
                self.colors.reset, 
                source));
            output.push_str(&format!("   {}|{}\n", self.colors.cyan, self.colors.reset));
        }
        
        // Suggestions
        for suggestion in &diag.suggestions {
            output.push_str(&format!("  {}help:{} {}\n", 
                self.colors.green, self.colors.reset, suggestion));
        }
        
        output
    }

    /// Format multiple diagnostics
    pub fn format_diagnostics(&self, diagnostics: &[Diagnostic]) -> String {
        let mut output = String::new();
        for diag in diagnostics {
            output.push_str(&self.format_diagnostic(diag));
            output.push('\n');
        }
        output
    }

    /// Print diagnostics to stderr
    pub fn print_diagnostics(&self, diagnostics: &[Diagnostic]) {
        let output = self.format_diagnostics(diagnostics);
        eprint!("{}", output);
    }
}

/// Create a diagnostic from a parse error
pub fn diagnostic_from_parse(message: &str, file: Option<&str>, line: Option<usize>) -> Diagnostic {
    let mut diag = Diagnostic::error(message);
    if let Some(f) = file {
        diag = diag.with_location(f, line.unwrap_or(1), 1);
    }
    diag
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_basic() {
        let diag = Diagnostic::error("unexpected token")
            .with_code("E001")
            .with_location("test.vcf", 10, 5)
            .with_suggestion("Did you mean to use a quoted key?");
        
        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.code, Some("E001".to_string()));
        assert_eq!(diag.file, Some("test.vcf".to_string()));
        assert_eq!(diag.line, Some(10));
        assert_eq!(diag.suggestions.len(), 1);
    }

    #[test]
    fn test_reporter_format() {
        let reporter = Reporter::new(Colors::disabled());
        let diag = Diagnostic::error("test error");
        let output = reporter.format_diagnostic(&diag);
        assert!(output.contains("error"));
        assert!(output.contains("test error"));
    }
}

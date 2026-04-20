use crate::ast::*;
use std::time::Duration as StdDuration;

fn format_duration(d: &StdDuration) -> String {
    let secs = d.as_secs();
    if secs % 86400 == 0 && secs > 0 {
        format!("{}d", secs / 86400)
    } else if secs % 3600 == 0 && secs > 0 {
        format!("{}h", secs / 3600)
    } else if secs % 60 == 0 && secs > 0 {
        format!("{}m", secs / 60)
    } else {
        format!("{}s", secs)
    }
}

/// Configuration for the pretty printer
#[derive(Debug, Clone)]
pub struct PrettyPrintConfig {
    pub indent_size: usize,
    pub indent_style: IndentStyle,
    pub max_line_length: usize,
    pub preserve_comments: bool,
    pub preserve_metadata: bool,
    pub inline_short_arrays: bool,
    pub trailing_comma: bool,
    /// AI canonical mode: sorted keys, consistent formatting
    pub ai_canonical: bool,
}

impl Default for PrettyPrintConfig {
    fn default() -> Self {
        Self {
            indent_size: 2,
            indent_style: IndentStyle::Spaces,
            max_line_length: 80,
            preserve_comments: true,
            preserve_metadata: true,
            inline_short_arrays: true,
            trailing_comma: true,
            ai_canonical: false,
        }
    }
}

impl PrettyPrintConfig {
    /// Create a config optimized for AI generation
    pub fn ai_canonical() -> Self {
        Self {
            indent_size: 2,
            indent_style: IndentStyle::Spaces,
            max_line_length: 80,
            preserve_comments: true,
            preserve_metadata: true,
            inline_short_arrays: true,
            trailing_comma: true,
            ai_canonical: true,
        }
    }
}

/// Indentation style
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentStyle {
    Spaces,
    Tabs,
}

/// Enhanced pretty printer with configurable options
pub struct PrettyPrinter {
    config: PrettyPrintConfig,
    output: String,
}

impl PrettyPrinter {
    pub fn new(config: PrettyPrintConfig) -> Self {
        Self {
            config,
            output: String::new(),
        }
    }

    pub fn print(ast: &Ast) -> String {
        Self::new(PrettyPrintConfig::default()).print_ast(ast)
    }

    pub fn print_with_config(ast: &Ast, config: PrettyPrintConfig) -> String {
        Self::new(config).print_ast(ast)
    }

    fn print_ast(mut self, ast: &Ast) -> String {
        self.print_table_block(&ast.root, 0);
        self.output
    }

    fn indent(&self, level: usize) -> String {
        match self.config.indent_style {
            IndentStyle::Spaces => " ".repeat(self.config.indent_size * level),
            IndentStyle::Tabs => "\t".repeat(level),
        }
    }

    fn print_table_block(&mut self, table: &TableBlock, indent: usize) {
        let mut entries: Vec<_> = table.entries.iter().collect();
        
        if self.config.ai_canonical {
            entries.sort_by(|a, b| {
                let key_a = match a {
                    TableEntry::KeyValue(kv) => kv.key.as_str(),
                    TableEntry::TableBlock(tb) => tb.name.as_deref().unwrap_or(""),
                    TableEntry::ArrayTable(at) => at.key.as_str(),
                    TableEntry::IncludeDirective(inc) => inc.path.as_str(),
                    TableEntry::Comment(_) => "",
                };
                let key_b = match b {
                    TableEntry::KeyValue(kv) => kv.key.as_str(),
                    TableEntry::TableBlock(tb) => tb.name.as_deref().unwrap_or(""),
                    TableEntry::ArrayTable(at) => at.key.as_str(),
                    TableEntry::IncludeDirective(inc) => inc.path.as_str(),
                    TableEntry::Comment(_) => "",
                };
                key_a.cmp(key_b)
            });
        }
        
        for entry in entries {
            match entry {
                TableEntry::KeyValue(kv) => {
                    self.print_key_value(kv, indent);
                }
                TableEntry::TableBlock(tb) => {
                    if let Some(ref name) = tb.name {
                        self.output.push_str(&self.indent(indent));
                        self.output.push_str(name);
                        self.output.push_str(" {\n");
                    } else {
                        self.output.push_str(&self.indent(indent));
                        self.output.push_str("{\n");
                    }
                    self.print_table_block(tb, indent + 1);
                    self.output.push_str(&self.indent(indent));
                    self.output.push_str("}\n");
                }
                TableEntry::ArrayTable(at) => {
                    self.output.push_str(&self.indent(indent));
                    self.output.push_str(&format!("[[{}]]\n", at.key));
                    for kv in &at.entries {
                        self.print_key_value(kv, indent + 1);
                    }
                }
                TableEntry::IncludeDirective(inc) => {
                    self.output.push_str(&self.indent(indent));
                    self.output.push_str(&format!("@include \"{}\"", inc.path));
                    if inc.merge_strategy != MergeStrategy::Override {
                        self.output.push_str(&format!(" merge={}", inc.merge_strategy));
                    }
                    self.output.push('\n');
                }
                TableEntry::Comment(c) => {
                    if self.config.preserve_comments {
                        self.output.push_str(&self.indent(indent));
                        self.output.push_str(&c.to_string());
                        self.output.push('\n');
                    }
                }
            }
        }
    }

    fn print_key_value(&mut self, kv: &KeyValue, indent: usize) {
        self.output.push_str(&self.indent(indent));
        self.output.push_str(&format!("{} = ", kv.key));
        self.print_value(&kv.value, indent);
        
        if self.config.preserve_metadata {
            if let Some(metadata) = &kv.metadata {
                self.output.push_str(" #@");
                for (i, item) in metadata.items.iter().enumerate() {
                    if i > 0 {
                        self.output.push(',');
                    }
                    match item {
                        MetadataItem::Standard(s) => {
                            self.output.push_str(&format!(" {}", s));
                        }
                        MetadataItem::Custom { key, value } => {
                            if let Some(v) = value {
                                self.output.push_str(&format!(" {}={:?}", key, v));
                            } else {
                                self.output.push_str(&format!(" {}", key));
                            }
                        }
                    }
                }
            }
        }
        
        if self.config.preserve_comments {
            if let Some(comment) = &kv.comment {
                self.output.push_str(&format!(" # {}", comment.content));
            }
        }
        
        self.output.push('\n');
    }

    fn print_value(&mut self, value: &Value, indent: usize) {
        match value {
            Value::Scalar(s) => {
                match s {
                    ScalarValue::String(str) => self.output.push_str(&format!("\"{}\"", str)),
                    ScalarValue::Number(n) => self.output.push_str(&format!("{}", n)),
                    ScalarValue::Boolean(b) => self.output.push_str(&format!("{}", b)),
                    ScalarValue::DateTime(dt) => self.output.push_str(dt),
                    ScalarValue::Duration(d) => self.output.push_str(&format_duration(d)),
                }
            }
            Value::InlineTable(table) => {
                self.output.push_str("{ ");
                for (i, entry) in table.entries.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.output.push_str(&format!("{} = ", entry.key));
                    self.print_value(&entry.value, indent);
                }
                self.output.push_str(" }");
            }
            Value::Array(arr) => {
                if self.config.inline_short_arrays && arr.elements.len() <= 3 {
                    self.output.push('[');
                    for (i, elem) in arr.elements.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.print_value(elem, indent);
                    }
                    self.output.push(']');
                } else {
                    self.output.push_str("[\n");
                    for elem in &arr.elements {
                        self.output.push_str(&self.indent(indent + 1));
                        self.print_value(elem, indent + 1);
                        if self.config.trailing_comma {
                            self.output.push(',');
                        }
                        self.output.push('\n');
                    }
                    self.output.push_str(&self.indent(indent));
                    self.output.push(']');
                }
            }
            Value::TableBlock(table) => {
                self.output.push_str("{\n");
                self.print_table_block(table, indent + 1);
                self.output.push_str(&self.indent(indent));
                self.output.push('}');
            }
            Value::Expression(expr) => {
                self.print_expression(expr);
            }
        }
    }

    fn print_expression(&mut self, expr: &Expression) {
        match expr {
            Expression::Literal(scalar) => {
                match scalar {
                    ScalarValue::Number(n) => self.output.push_str(&format!("{}", n)),
                    ScalarValue::Duration(d) => self.output.push_str(&format_duration(d)),
                    ScalarValue::String(s) => self.output.push_str(&format!("\"{}\"", s)),
                    ScalarValue::Boolean(b) => self.output.push_str(&format!("{}", b)),
                    ScalarValue::DateTime(dt) => self.output.push_str(dt),
                }
            }
            Expression::BinaryOp { left, operator, right } => {
                self.print_expression(left);
                self.output.push_str(&format!(" {} ", operator));
                self.print_expression(right);
            }
            Expression::UnitValue { value, unit } => {
                let unit_str = match unit {
                    TimeUnit::Seconds => "s",
                    TimeUnit::Minutes => "m",
                    TimeUnit::Hours => "h",
                    TimeUnit::Days => "d",
                };
                self.output.push_str(&format!("{}{}", value, unit_str));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    #[test]
    fn test_pretty_print_basic() {
        let source = r#"
name = "test"
port = 8080
"#;
        let parser = Parser::new(source.to_string());
        let ast = parser.parse().unwrap();
        let output = PrettyPrinter::print(&ast);
        assert!(output.contains("name = \"test\""));
        assert!(output.contains("port = 8080"));
    }

    #[test]
    fn test_pretty_print_custom_indent() {
        let source = r#"
app {
    name = "test"
}
"#;
        let parser = Parser::new(source.to_string());
        let ast = parser.parse().unwrap();
        
        let config = PrettyPrintConfig {
            indent_size: 4,
            ..Default::default()
        };
        let output = PrettyPrinter::print_with_config(&ast, config);
        assert!(output.contains("    name = \"test\""));
    }

    #[test]
    fn test_pretty_print_array() {
        let source = r#"
items = [1, 2, 3]
"#;
        let parser = Parser::new(source.to_string());
        let ast = parser.parse().unwrap();
        let output = PrettyPrinter::print(&ast);
        assert!(output.contains("[1, 2, 3]"));
    }

    #[test]
    fn test_pretty_print_expression() {
        let source = r#"
result = 10 + 5
"#;
        let parser = Parser::new(source.to_string());
        let ast = parser.parse().unwrap();
        let output = PrettyPrinter::print(&ast);
        assert!(output.contains("10 + 5"));
    }
}

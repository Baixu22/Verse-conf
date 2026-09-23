use crate::ast::*;
use crate::lexer::{Lexer, Token};
use std::time::Duration as StdDuration;

fn quote_string(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for ch in value.chars() {
        match ch {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\t' => quoted.push_str("\\t"),
            '\r' => quoted.push_str("\\r"),
            _ => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

fn format_key(key: &Key) -> String {
    match key {
        Key::BareKey(value) => value.clone(),
        Key::QuotedKey(value) => quote_string(value),
    }
}

fn format_table_name(name: &str) -> String {
    let mut chars = name.chars();
    let is_bare = chars
        .next()
        .map(|first| first.is_ascii_alphabetic() || first == '_')
        .unwrap_or(false)
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-');

    if is_bare {
        name.to_string()
    } else {
        quote_string(name)
    }
}

fn format_metadata_value(value: &MetadataValue) -> String {
    match value {
        MetadataValue::String(value) => quote_string(value),
        MetadataValue::Number(value) => value.to_string(),
        MetadataValue::Range { min, max } => format!("({}..{})", min, max),
    }
}

fn format_standard_metadata(metadata: &StandardMetadata) -> String {
    match metadata {
        StandardMetadata::Sensitive => "sensitive".to_string(),
        StandardMetadata::Required => "required".to_string(),
        StandardMetadata::Deprecated {
            message: Some(message),
        } => {
            format!("deprecated={}", quote_string(message))
        }
        StandardMetadata::Deprecated { message: None } => "deprecated".to_string(),
        StandardMetadata::Description { text } => format!("description={}", quote_string(text)),
        StandardMetadata::Example { value } => format!("example={}", quote_string(value)),
        StandardMetadata::TypeHint { hint } => format!("type_hint={}", quote_string(hint)),
        StandardMetadata::ItemType { item_type } => {
            format!("item_type={}", quote_string(item_type))
        }
        StandardMetadata::Range { min, max } => format!("range({}..{})", min, max),
    }
}

/// 把单个值渲染成配置文本（不含键名与缩进前缀）。
///
/// 最小改动编辑用它替换目标字段的值区间，从而与格式化共用同一套转义规则。
pub fn render_value(value: &Value, indent_level: usize) -> String {
    let mut printer = PrettyPrinter::new(PrettyPrintConfig::default());
    printer.print_value(value, indent_level);
    printer.output
}

/// 把持续时间渲染成配置里的持续时间字面量
pub fn render_duration(d: &StdDuration) -> String {
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
        if ast.schema.is_some() {
            if let Some(raw_schema) = self.raw_schema_source(&ast.source.content) {
                self.output.push_str(raw_schema);
                if !raw_schema.ends_with('\n') {
                    self.output.push('\n');
                }
            }
        }
        self.print_table_block(&ast.root, 0);
        self.output
    }

    fn raw_schema_source<'a>(&self, source: &'a str) -> Option<&'a str> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize_all().ok()?;
        let mut pos = 0;

        // The parser only accepts a schema before configuration entries. Match the same
        // leading comments so none of the schema preamble is lost.
        while pos < tokens.len() {
            match &tokens[pos].0 {
                Token::Newline | Token::LineComment(_) | Token::BlockComment(_) => pos += 1,
                _ => break,
            }
        }
        if pos + 2 >= tokens.len()
            || !matches!(tokens[pos].0, Token::MetadataPrefix)
            || !matches!(&tokens[pos + 1].0, Token::BareKey(key) if key == "schema")
            || !matches!(tokens[pos + 2].0, Token::LBrace)
        {
            return None;
        }

        let mut depth = 0usize;
        for (token, span) in &tokens[pos + 2..] {
            match token {
                Token::LBrace => depth += 1,
                Token::RBrace => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        if span.end <= source.len() && source.is_char_boundary(span.end) {
                            return source.get(..span.end);
                        }
                        return None;
                    }
                }
                _ => {}
            }
        }
        None
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
                        self.output.push_str(&format_table_name(name));
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
                    self.output
                        .push_str(&format!("[[{}]]\n", format_key(&at.key)));
                    for kv in &at.entries {
                        // 数组表里的注释以占位键 `_comment` 承载（AST 结构所限），
                        // 输出时必须还原成注释而不是伪造一个键值对
                        let is_comment_placeholder = kv.key.as_str() == "_comment"
                            && kv.span.is_unknown()
                            && kv.comment.is_some();
                        if is_comment_placeholder {
                            if self.config.preserve_comments {
                                if let Some(comment) = &kv.comment {
                                    self.output.push_str(&self.indent(indent + 1));
                                    self.output.push_str(&comment.to_string());
                                    self.output.push('\n');
                                }
                            }
                            continue;
                        }
                        self.print_key_value(kv, indent + 1);
                    }
                }
                TableEntry::IncludeDirective(inc) => {
                    self.output.push_str(&self.indent(indent));
                    self.output
                        .push_str(&format!("@include {}", quote_string(&inc.path)));
                    if inc.merge_strategy != MergeStrategy::Override {
                        self.output
                            .push_str(&format!(" merge={}", inc.merge_strategy));
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
        self.output.push_str(&format!("{} = ", format_key(&kv.key)));
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
                            self.output
                                .push_str(&format!(" {}", format_standard_metadata(s)));
                        }
                        MetadataItem::Custom { key, value } => {
                            if let Some(v) = value {
                                self.output.push_str(&format!(
                                    " {}={}",
                                    key,
                                    format_metadata_value(v)
                                ));
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
                match comment.is_block {
                    true => self.output.push_str(&format!(" /*{}*/", comment.content)),
                    false => self.output.push_str(&format!(" # {}", comment.content)),
                }
            }
        }

        self.output.push('\n');
    }

    fn print_value(&mut self, value: &Value, indent: usize) {
        match value {
            Value::Scalar(s) => match s {
                ScalarValue::String(value) => self.output.push_str(&quote_string(value)),
                ScalarValue::Number(n) => self.output.push_str(&format!("{}", n)),
                ScalarValue::Boolean(b) => self.output.push_str(&format!("{}", b)),
                ScalarValue::DateTime(dt) => self.output.push_str(dt),
                ScalarValue::Duration(d) => self.output.push_str(&render_duration(d)),
            },
            Value::InlineTable(table) => {
                if table.entries.is_empty() {
                    self.output.push_str("{}");
                } else {
                    self.output.push_str("{\n");
                    for entry in &table.entries {
                        self.print_key_value(entry, indent + 1);
                    }
                    self.output.push_str(&self.indent(indent));
                    self.output.push('}');
                }
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
            Expression::Literal(scalar) => match scalar {
                ScalarValue::Number(n) => self.output.push_str(&format!("{}", n)),
                ScalarValue::Duration(d) => self.output.push_str(&render_duration(d)),
                ScalarValue::String(value) => self.output.push_str(&quote_string(value)),
                ScalarValue::Boolean(b) => self.output.push_str(&format!("{}", b)),
                ScalarValue::DateTime(dt) => self.output.push_str(dt),
            },
            Expression::BinaryOp {
                left,
                operator,
                right,
            } => {
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
    fn test_pretty_print_preserves_comments_metadata_and_schema() {
        let source = r#"# Keep this schema comment
#@schema {
  version = "1.0"
  settings {
    type = "table"
    value {
      type = "string"
      default = "ready"
    }
  }
}
# Keep this config comment
settings {
  value = "original" #@ description="A \"quoted\" value" # inline note
}
"#;
        let ast = Parser::new(source.to_string()).parse().unwrap();
        let formatted = PrettyPrinter::print(&ast);

        assert!(formatted.contains("# Keep this schema comment"));
        assert!(formatted.contains("#@schema {"));
        assert!(formatted.contains("# Keep this config comment"));
        assert!(formatted.contains("description=\"A \\\"quoted\\\" value\""));
        assert!(formatted.contains("# inline note"));

        let reparsed = Parser::new(formatted.clone()).parse().unwrap();
        let reformatted = PrettyPrinter::print(&reparsed);
        assert_eq!(formatted, reformatted);
    }

    #[test]
    fn test_pretty_print_escapes_strings_and_preserves_block_comments() {
        let source = "value = \"a \\\"quote\\\" and \\\\path\\nnext\"\n/* block note */\n";
        let ast = Parser::new(source.to_string()).parse().unwrap();
        let formatted = PrettyPrinter::print(&ast);
        assert!(formatted.contains("value = \"a \\\"quote\\\" and \\\\path\\nnext\""));
        assert!(formatted.contains("/* block note */"));

        let reparsed = Parser::new(formatted.clone()).parse().unwrap();
        assert_eq!(formatted, PrettyPrinter::print(&reparsed));
    }

    #[test]
    fn test_pretty_print_inline_table_preserves_comments_and_metadata() {
        let source = "settings = { port = 8080 #@ required, description=\"service port\" # stable port\n }\n";
        let ast = Parser::new(source.to_string()).parse().unwrap();
        let formatted = PrettyPrinter::print(&ast);
        assert!(formatted.contains("#@ required, description=\"service port\""));
        assert!(formatted.contains("# stable port"));

        let reparsed = Parser::new(formatted.clone()).parse().unwrap();
        assert_eq!(formatted, PrettyPrinter::print(&reparsed));
    }

    #[test]
    fn test_pretty_print_array_table_real_comment_key() {
        let source = "[[servers]]\n_comment = \"application data\"\n";
        let ast = Parser::new(source.to_string()).parse().unwrap();
        let formatted = PrettyPrinter::print(&ast);
        assert!(formatted.contains("_comment = \"application data\""));
    }

    #[test]
    fn test_pretty_print_array_table_comment_placeholder() {
        let source = "[[servers]]\n# server note\nname = \"primary\"\n";
        let ast = Parser::new(source.to_string()).parse().unwrap();
        let formatted = PrettyPrinter::print(&ast);
        assert!(formatted.contains("# server note"));
        assert!(!formatted.contains("_comment ="));
    }
}

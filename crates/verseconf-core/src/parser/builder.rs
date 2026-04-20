use crate::ast::*;
use crate::lexer::*;
use crate::parser::ParseError;
use crate::Span;

/// AST 构建器
pub struct AstBuilder {
    source: String,
    #[allow(dead_code)]
    errors: Vec<ParseError>,
}

impl AstBuilder {
    pub fn new(source: String) -> Self {
        Self {
            source,
            errors: Vec::new(),
        }
    }

    /// 从 tokens 构建 AST
    pub fn build(&mut self, tokens: &[(Token, Span)]) -> Result<Ast, ParseError> {
        let mut pos = 0;
        
        // Check for schema block at the beginning
        let schema = self.try_parse_schema(tokens, &mut pos)?;
        
        let root = self.parse_table_block(tokens, &mut pos)?;

        Ok(Ast {
            root,
            schema,
            source: SourceInfo {
                path: None,
                content: self.source.clone(),
            },
        })
    }

    /// Try to parse schema block if present
    fn try_parse_schema(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<Option<SchemaDefinition>, ParseError> {
        // Skip newlines and comments at the beginning
        while *pos < tokens.len() {
            match &tokens[*pos].0 {
                Token::Newline | Token::LineComment(_) | Token::BlockComment(_) => {
                    *pos += 1;
                }
                Token::MetadataPrefix => {
                    // Check if followed by 'schema' and '{'
                    if *pos + 2 < tokens.len() {
                        if let Token::BareKey(key) = &tokens[*pos + 1].0 {
                            if key == "schema" && matches!(tokens[*pos + 2].0, Token::LBrace) {
                                *pos += 3; // skip '#@', 'schema', '{'
                                let schema = self.parse_schema_block(tokens, pos)?;
                                return Ok(Some(schema));
                            }
                        }
                    }
                    // Not a schema block, stop skipping
                    break;
                }
                _ => break,
            }
        }
        Ok(None)
    }

    /// Parse schema block content
    fn parse_schema_block(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<SchemaDefinition, ParseError> {
        let start = tokens[*pos].1;
        let mut version = None;
        let mut description = None;
        let mut strict = false;
        let mut fields = Vec::new();

        while *pos < tokens.len() {
            if matches!(tokens[*pos].0, Token::RBrace) {
                *pos += 1;
                break;
            }

            if matches!(tokens[*pos].0, Token::Newline | Token::LineComment(_) | Token::BlockComment(_)) {
                *pos += 1;
                continue;
            }

            // Parse schema field
            if let Token::BareKey(key) = &tokens[*pos].0 {
                match key.as_str() {
                    "version" => {
                        *pos += 1;
                        // Expect '='
                        if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                            *pos += 1;
                        }
                        if *pos < tokens.len() {
                            if let Token::StringLiteral(s) = &tokens[*pos].0 {
                                version = Some(s.clone());
                                *pos += 1;
                            }
                        }
                    }
                    "description" => {
                        *pos += 1;
                        // Expect '='
                        if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                            *pos += 1;
                        }
                        if *pos < tokens.len() {
                            if let Token::StringLiteral(s) = &tokens[*pos].0 {
                                description = Some(s.clone());
                                *pos += 1;
                            }
                        }
                    }
                    "strict" => {
                        *pos += 1;
                        // Expect '='
                        if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                            *pos += 1;
                        }
                        if *pos < tokens.len() {
                            if let Token::BooleanLiteral(b) = &tokens[*pos].0 {
                                strict = *b;
                                *pos += 1;
                            }
                        }
                    }
                    _ => {
                        // This is a field definition
                        let field = self.parse_schema_field(tokens, pos)?;
                        fields.push(field);
                    }
                }
            } else {
                *pos += 1;
            }
        }

        let end = if *pos > 0 && *pos <= tokens.len() {
            tokens[*pos - 1].1
        } else {
            start
        };

        Ok(SchemaDefinition {
            version,
            description,
            strict,
            fields,
            span: start.merge(&end),
        })
    }

    /// Parse a single schema field
    fn parse_schema_field(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<SchemaField, ParseError> {
        let start = tokens[*pos].1;
        
        // Get field name
        let name = if let Token::BareKey(key) = &tokens[*pos].0 {
            let name = key.clone();
            *pos += 1;
            name
        } else {
            return Err(ParseError::new(
                "expected field name in schema",
                tokens[*pos].1,
            ));
        };

        // Check if followed by '{' (nested table field)
        if *pos < tokens.len() && matches!(tokens[*pos].0, Token::LBrace) {
            *pos += 1;
            // Parse nested schema fields
            let mut nested_fields = Vec::new();
            let mut field_type = SchemaType::Table;
            let mut required = false;
            let mut default = None;
            let mut range = None;
            let mut pattern = None;
            let mut enum_values = None;
            let mut desc = None;
            let mut example = None;
            let mut llm_hint = None;
            let mut sensitive = false;

            while *pos < tokens.len() {
                if matches!(tokens[*pos].0, Token::RBrace) {
                    *pos += 1;
                    break;
                }

                if matches!(tokens[*pos].0, Token::Newline | Token::LineComment(_) | Token::BlockComment(_)) {
                    *pos += 1;
                    continue;
                }

                // Parse field property or nested field
                if let Token::BareKey(key) = &tokens[*pos].0 {
                    match key.as_str() {
                        "type" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            if *pos < tokens.len() {
                                if let Token::StringLiteral(s) = &tokens[*pos].0 {
                                    field_type = self.parse_schema_type(s)?;
                                    *pos += 1;
                                }
                            }
                        }
                        "required" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            if *pos < tokens.len() {
                                if let Token::BooleanLiteral(b) = &tokens[*pos].0 {
                                    required = *b;
                                    *pos += 1;
                                }
                            }
                        }
                        "default" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            let value = self.parse_value(tokens, pos)?;
                            default = Some(value);
                        }
                        "range" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            range = self.parse_schema_range(tokens, pos)?;
                        }
                        "pattern" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            if *pos < tokens.len() {
                                if let Token::StringLiteral(s) = &tokens[*pos].0 {
                                    pattern = Some(s.clone());
                                    *pos += 1;
                                }
                            }
                        }
                        "enum" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            enum_values = self.parse_schema_enum(tokens, pos)?;
                        }
                        "desc" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            if *pos < tokens.len() {
                                if let Token::StringLiteral(s) = &tokens[*pos].0 {
                                    desc = Some(s.clone());
                                    *pos += 1;
                                }
                            }
                        }
                        "example" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            if *pos < tokens.len() {
                                if let Token::StringLiteral(s) = &tokens[*pos].0 {
                                    example = Some(s.clone());
                                    *pos += 1;
                                }
                            }
                        }
                        "llm_hint" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            if *pos < tokens.len() {
                                if let Token::StringLiteral(s) = &tokens[*pos].0 {
                                    llm_hint = Some(s.clone());
                                    *pos += 1;
                                }
                            }
                        }
                        "sensitive" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            if *pos < tokens.len() {
                                if let Token::BooleanLiteral(b) = &tokens[*pos].0 {
                                    sensitive = *b;
                                    *pos += 1;
                                }
                            }
                        }
                        _ => {
                            // This is a nested field
                            let nested = self.parse_schema_field(tokens, pos)?;
                            nested_fields.push(nested);
                        }
                    }
                } else {
                    *pos += 1;
                }
            }

            let end = if *pos > 0 && *pos <= tokens.len() {
                tokens[*pos - 1].1
            } else {
                start
            };

            Ok(SchemaField {
                name,
                field_type,
                required,
                default,
                range,
                pattern,
                enum_values,
                desc,
                example,
                llm_hint,
                sensitive,
                nested_fields,
                span: start.merge(&end),
            })
        } else {
            // Simple field with key = value properties
            let mut field_type = SchemaType::String;
            let mut required = false;
            let mut default = None;
            let mut range = None;
            let mut pattern = None;
            let mut enum_values = None;
            let mut desc = None;
            let mut example = None;
            let mut llm_hint = None;
            let mut sensitive = false;

            // Parse field properties on same line or next lines
            while *pos < tokens.len() {
                if matches!(tokens[*pos].0, Token::Newline) {
                    *pos += 1;
                    // Check if next line continues with field properties
                    if *pos < tokens.len() && matches!(tokens[*pos].0, Token::BareKey(_)) {
                        continue;
                    }
                    break;
                }

                if matches!(tokens[*pos].0, Token::LineComment(_) | Token::BlockComment(_)) {
                    *pos += 1;
                    continue;
                }

                if matches!(tokens[*pos].0, Token::Comma) {
                    *pos += 1;
                    continue;
                }

                if let Token::BareKey(key) = &tokens[*pos].0 {
                    match key.as_str() {
                        "type" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            if *pos < tokens.len() {
                                if let Token::StringLiteral(s) = &tokens[*pos].0 {
                                    field_type = self.parse_schema_type(s)?;
                                    *pos += 1;
                                }
                            }
                        }
                        "required" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            if *pos < tokens.len() {
                                if let Token::BooleanLiteral(b) = &tokens[*pos].0 {
                                    required = *b;
                                    *pos += 1;
                                }
                            }
                        }
                        "default" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            let value = self.parse_value(tokens, pos)?;
                            default = Some(value);
                        }
                        "range" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            range = self.parse_schema_range(tokens, pos)?;
                        }
                        "pattern" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            if *pos < tokens.len() {
                                if let Token::StringLiteral(s) = &tokens[*pos].0 {
                                    pattern = Some(s.clone());
                                    *pos += 1;
                                }
                            }
                        }
                        "enum" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            enum_values = self.parse_schema_enum(tokens, pos)?;
                        }
                        "desc" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            if *pos < tokens.len() {
                                if let Token::StringLiteral(s) = &tokens[*pos].0 {
                                    desc = Some(s.clone());
                                    *pos += 1;
                                }
                            }
                        }
                        "example" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            if *pos < tokens.len() {
                                if let Token::StringLiteral(s) = &tokens[*pos].0 {
                                    example = Some(s.clone());
                                    *pos += 1;
                                }
                            }
                        }
                        "llm_hint" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            if *pos < tokens.len() {
                                if let Token::StringLiteral(s) = &tokens[*pos].0 {
                                    llm_hint = Some(s.clone());
                                    *pos += 1;
                                }
                            }
                        }
                        "sensitive" => {
                            *pos += 1;
                            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                                *pos += 1;
                            }
                            if *pos < tokens.len() {
                                if let Token::BooleanLiteral(b) = &tokens[*pos].0 {
                                    sensitive = *b;
                                    *pos += 1;
                                }
                            }
                        }
                        _ => {
                            // Unknown property, skip
                            *pos += 1;
                        }
                    }
                } else {
                    break;
                }
            }

            let end = if *pos > 0 && *pos <= tokens.len() {
                tokens[*pos - 1].1
            } else {
                start
            };

            Ok(SchemaField {
                name,
                field_type,
                required,
                default,
                range,
                pattern,
                enum_values,
                desc,
                example,
                llm_hint,
                sensitive,
                nested_fields: Vec::new(),
                span: start.merge(&end),
            })
        }
    }

    /// Parse schema type from string
    fn parse_schema_type(&self, s: &str) -> Result<SchemaType, ParseError> {
        match s {
            "string" => Ok(SchemaType::String),
            "integer" | "int" => Ok(SchemaType::Integer),
            "float" | "number" => Ok(SchemaType::Float),
            "bool" | "boolean" => Ok(SchemaType::Boolean),
            "datetime" => Ok(SchemaType::DateTime),
            "duration" => Ok(SchemaType::Duration),
            "table" => Ok(SchemaType::Table),
            "array" => Ok(SchemaType::Array(Box::new(SchemaType::String))), // Default to array of strings
            _ => {
                // Check for array<type> syntax
                if s.starts_with("array<") && s.ends_with(">") {
                    let inner = &s[6..s.len()-1];
                    let inner_type = self.parse_schema_type(inner)?;
                    Ok(SchemaType::Array(Box::new(inner_type)))
                } else {
                    Ok(SchemaType::String) // Default to string for unknown types
                }
            }
        }
    }

    /// Parse range constraint
    fn parse_schema_range(
        &self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<Option<RangeConstraint>, ParseError> {
        // Expect '('
        if *pos < tokens.len() && matches!(tokens[*pos].0, Token::LParen) {
            *pos += 1;
        }

        let min = if *pos < tokens.len() {
            if let Token::NumberLiteral(n) = &tokens[*pos].0 {
                let val = n.parse::<i64>().unwrap_or(0);
                *pos += 1;
                Some(val)
            } else {
                None
            }
        } else {
            None
        };

        // Expect '..'
        if *pos < tokens.len() && matches!(tokens[*pos].0, Token::RangeOp) {
            *pos += 1;
        }

        let max = if *pos < tokens.len() {
            if let Token::NumberLiteral(n) = &tokens[*pos].0 {
                let val = n.parse::<i64>().unwrap_or(0);
                *pos += 1;
                Some(val)
            } else {
                None
            }
        } else {
            None
        };

        // Expect ')'
        if *pos < tokens.len() && matches!(tokens[*pos].0, Token::RParen) {
            *pos += 1;
        }

        Ok(Some(RangeConstraint { min, max }))
    }

    /// Parse enum constraint
    fn parse_schema_enum(
        &self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<Option<Vec<ScalarValue>>, ParseError> {
        // Expect '['
        if *pos < tokens.len() && matches!(tokens[*pos].0, Token::LBracket) {
            *pos += 1;
        }

        let mut values = Vec::new();
        while *pos < tokens.len() {
            if matches!(tokens[*pos].0, Token::RBracket) {
                *pos += 1;
                break;
            }

            if matches!(tokens[*pos].0, Token::Comma) {
                *pos += 1;
                continue;
            }

            if let Token::StringLiteral(s) = &tokens[*pos].0 {
                values.push(ScalarValue::String(s.clone()));
                *pos += 1;
            } else {
                *pos += 1;
            }
        }

        if values.is_empty() {
            Ok(None)
        } else {
            Ok(Some(values))
        }
    }

    fn parse_table_block(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<TableBlock, ParseError> {
        let start = if *pos < tokens.len() {
            tokens[*pos].1
        } else {
            Span::unknown()
        };

        let mut entries = Vec::new();

        while *pos < tokens.len() {
            let (token, span) = &tokens[*pos];

            match token {
                Token::RBrace => {
                    *pos += 1;
                    break;
                }
                Token::Eof => {
                    break;
                }
                Token::BareKey(_) | Token::QuotedKey(_) => {
                    // Check if this is a table block definition (key followed by '{')
                    if *pos + 1 < tokens.len() {
                        if matches!(tokens[*pos + 1].0, Token::LBrace) {
                            // Extract the table name before consuming tokens
                            let table_name = match &tokens[*pos].0 {
                                Token::BareKey(k) => Some(k.clone()),
                                Token::QuotedKey(k) => Some(k.clone()),
                                _ => None,
                            };
                            *pos += 2; // skip the key name and '{'
                            let mut table = self.parse_table_block(tokens, pos)?;
                            table.name = table_name;
                            entries.push(TableEntry::TableBlock(table));
                            continue;
                        }
                        // Check if this is an array table definition (key followed by '[[')
                        if matches!(tokens[*pos + 1].0, Token::LDoubleBracket) {
                            *pos += 1; // skip the key name
                            let at = self.parse_array_table(tokens, pos)?;
                            entries.push(TableEntry::ArrayTable(at));
                            continue;
                        }
                    }
                    let kv = self.parse_key_value(tokens, pos)?;
                    entries.push(TableEntry::KeyValue(kv));
                }
                Token::LDoubleBracket => {
                    let at = self.parse_array_table(tokens, pos)?;
                    entries.push(TableEntry::ArrayTable(at));
                }
                Token::Include => {
                    let inc = self.parse_include(tokens, pos)?;
                    entries.push(TableEntry::IncludeDirective(inc));
                }
                Token::LineComment(_) | Token::BlockComment(_) => {
                    let comment = self.parse_comment(tokens, pos)?;
                    entries.push(TableEntry::Comment(comment));
                }
                Token::Newline => {
                    *pos += 1;
                }
                _ => {
                    return Err(ParseError::new(
                        format!("unexpected token in table block: {}", token),
                        *span,
                    ))
                }
            }
        }

        let end = if *pos > 0 && *pos <= tokens.len() {
            tokens[*pos - 1].1
        } else {
            start
        };

        Ok(TableBlock {
            name: None,
            entries,
            span: start.merge(&end),
        })
    }

    fn parse_key_value(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<KeyValue, ParseError> {
        let start = tokens[*pos].1;
        let key = self.parse_key(tokens, pos)?;

        // Expect ASSIGN
        if *pos >= tokens.len() {
            return Err(ParseError::new("expected '=' or ':' after key", start));
        }
        match &tokens[*pos].0 {
            Token::Assign => {
                *pos += 1;
            }
            other => {
                return Err(ParseError::new(
                    format!("expected '=' or ':', found {}", other),
                    tokens[*pos].1,
                ))
            }
        }

        let value = self.parse_value(tokens, pos)?;

        let metadata = if *pos < tokens.len() && matches!(tokens[*pos].0, Token::MetadataPrefix) {
            Some(self.parse_metadata(tokens, pos)?)
        } else {
            None
        };

        let comment = if *pos < tokens.len()
            && matches!(tokens[*pos].0, Token::LineComment(_) | Token::BlockComment(_))
        {
            Some(self.parse_comment(tokens, pos)?)
        } else {
            None
        };

        let end = tokens[*pos - 1].1;

        Ok(KeyValue {
            key,
            value,
            metadata,
            comment,
            span: start.merge(&end),
        })
    }

    fn parse_key(&mut self, tokens: &[(Token, Span)], pos: &mut usize) -> Result<Key, ParseError> {
        if *pos >= tokens.len() {
            return Err(ParseError::new("expected key", Span::unknown()));
        }

        let (token, span) = &tokens[*pos];
        *pos += 1;

        match token {
            Token::BareKey(k) => Ok(Key::BareKey(k.clone())),
            Token::QuotedKey(k) => Ok(Key::QuotedKey(k.clone())),
            Token::StringLiteral(k) => Ok(Key::QuotedKey(k.clone())),
            _ => Err(ParseError::new(
                format!("expected key, found {}", token),
                *span,
            )),
        }
    }

    fn parse_value(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<Value, ParseError> {
        if *pos >= tokens.len() {
            return Err(ParseError::new("expected value", Span::unknown()));
        }

        let (token, _) = &tokens[*pos];

        match token {
            Token::StringLiteral(_)
            | Token::NumberLiteral(_)
            | Token::BooleanLiteral(_)
            | Token::DurationLiteral(_)
            | Token::DateTimeLiteral(_) => {
                let expr = self.parse_expression(tokens, pos)?;
                Ok(Value::Expression(expr))
            }
            Token::LBrace => {
                // Could be inline_table or table_block
                self.parse_inline_or_table(tokens, pos)
            }
            Token::LBracket => {
                let arr = self.parse_array(tokens, pos)?;
                Ok(Value::Array(arr))
            }
            _ => Err(ParseError::new(
                format!("expected value, found {}", token),
                tokens[*pos].1,
            )),
        }
    }

    fn parse_expression(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<Expression, ParseError> {
        let left = self.parse_expression_primary(tokens, pos)?;
        self.parse_expression_rest(tokens, pos, left)
    }

    fn parse_expression_primary(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<Expression, ParseError> {
        if *pos >= tokens.len() {
            return Err(ParseError::new("expected expression value", Span::unknown()));
        }

        let (token, span) = &tokens[*pos];

        match token {
            Token::NumberLiteral(n) => {
                *pos += 1;
                // Check if followed by a time unit
                if *pos < tokens.len() {
                    if let Token::BareKey(unit) = &tokens[*pos].0 {
                        let time_unit = match unit.as_str() {
                            "s" => Some(TimeUnit::Seconds),
                            "m" => Some(TimeUnit::Minutes),
                            "h" => Some(TimeUnit::Hours),
                            "d" => Some(TimeUnit::Days),
                            _ => None,
                        };
                        if let Some(tu) = time_unit {
                            *pos += 1;
                            let value = n.parse::<f64>().unwrap_or(0.0);
                            return Ok(Expression::UnitValue {
                                value,
                                unit: tu,
                            });
                        }
                    }
                }
                // Regular number
                let scalar = self.parse_number_scalar(n, *span)?;
                Ok(Expression::Literal(scalar))
            }
            Token::StringLiteral(s) => {
                *pos += 1;
                Ok(Expression::Literal(ScalarValue::String(s.clone())))
            }
            Token::BooleanLiteral(b) => {
                *pos += 1;
                Ok(Expression::Literal(ScalarValue::Boolean(*b)))
            }
            Token::DurationLiteral(d) => {
                *pos += 1;
                let (num, unit) = self.parse_duration_literal(d)?;
                Ok(Expression::UnitValue {
                    value: num,
                    unit,
                })
            }
            Token::DateTimeLiteral(dt) => {
                *pos += 1;
                Ok(Expression::Literal(ScalarValue::DateTime(dt.clone())))
            }
            _ => Err(ParseError::new(
                format!("expected expression value, found {}", token),
                *span,
            )),
        }
    }

    fn parse_expression_rest(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
        left: Expression,
    ) -> Result<Expression, ParseError> {
        if *pos >= tokens.len() {
            return Ok(left);
        }

        let (token, _) = &tokens[*pos];

        if let Token::Operator(op) = token {
            *pos += 1;
            let right = self.parse_expression_primary(tokens, pos)?;
            let binary_op = match op {
                Operator::Add => BinaryOperator::Add,
                Operator::Subtract => BinaryOperator::Subtract,
                Operator::Multiply => BinaryOperator::Multiply,
                Operator::Divide => BinaryOperator::Divide,
            };
            let expr = Expression::BinaryOp {
                left: Box::new(left),
                operator: binary_op,
                right: Box::new(right),
            };
            // Continue parsing rest of expression (left-associative)
            self.parse_expression_rest(tokens, pos, expr)
        } else {
            Ok(left)
        }
    }

    fn parse_number_scalar(&self, n: &str, span: Span) -> Result<ScalarValue, ParseError> {
        if n.contains('.') {
            n.parse::<f64>()
                .map(NumberValue::Float)
                .map(ScalarValue::Number)
                .map_err(|_| ParseError::new("invalid float number", span))
        } else {
            n.parse::<i64>()
                .map(NumberValue::Integer)
                .map(ScalarValue::Number)
                .map_err(|_| ParseError::new("invalid integer number", span))
        }
    }

    fn parse_inline_or_table(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<Value, ParseError> {
        // Simplified: treat as inline_table for now
        self.parse_inline_table(tokens, pos)
    }

    #[allow(dead_code)]
    fn parse_scalar(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<ScalarValue, ParseError> {
        if *pos >= tokens.len() {
            return Err(ParseError::new("expected scalar", Span::unknown()));
        }

        let (token, span) = &tokens[*pos];
        *pos += 1;

        match token {
            Token::StringLiteral(s) => Ok(ScalarValue::String(s.clone())),
            Token::NumberLiteral(n) => {
                if n.contains('.') {
                    n.parse::<f64>()
                        .map(NumberValue::Float)
                        .map(ScalarValue::Number)
                        .map_err(|_| ParseError::new("invalid float number", *span))
                } else {
                    n.parse::<i64>()
                        .map(NumberValue::Integer)
                        .map(ScalarValue::Number)
                        .map_err(|_| ParseError::new("invalid integer number", *span))
                }
            }
            Token::BooleanLiteral(b) => Ok(ScalarValue::Boolean(*b)),
            Token::DurationLiteral(d) => {
                // Parse duration like "60s", "5m", "1h", "1d"
                let (num, unit) = self.parse_duration_literal(d)?;
                let seconds = (num as u64) * unit.to_seconds();
                Ok(ScalarValue::Duration(std::time::Duration::from_secs(seconds)))
            }
            Token::DateTimeLiteral(dt) => Ok(ScalarValue::DateTime(dt.clone())),
            _ => Err(ParseError::new(
                format!("expected scalar, found {}", token),
                *span,
            )),
        }
    }

    fn parse_duration_literal(&self, s: &str) -> Result<(f64, TimeUnit), ParseError> {
        if let Some(stripped) = s.strip_suffix('s') {
            let num = stripped
                .parse::<f64>()
                .map_err(|_| ParseError::new("invalid duration", Span::unknown()))?;
            Ok((num, TimeUnit::Seconds))
        } else if let Some(stripped) = s.strip_suffix('m') {
            let num = stripped
                .parse::<f64>()
                .map_err(|_| ParseError::new("invalid duration", Span::unknown()))?;
            Ok((num, TimeUnit::Minutes))
        } else if let Some(stripped) = s.strip_suffix('h') {
            let num = stripped
                .parse::<f64>()
                .map_err(|_| ParseError::new("invalid duration", Span::unknown()))?;
            Ok((num, TimeUnit::Hours))
        } else if let Some(stripped) = s.strip_suffix('d') {
            let num = stripped
                .parse::<f64>()
                .map_err(|_| ParseError::new("invalid duration", Span::unknown()))?;
            Ok((num, TimeUnit::Days))
        } else {
            Err(ParseError::new("invalid duration format", Span::unknown()))
        }
    }

    fn parse_inline_table(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<Value, ParseError> {
        let start = tokens[*pos].1;
        *pos += 1; // skip '{'

        let mut entries = Vec::new();

        while *pos < tokens.len() {
            if matches!(tokens[*pos].0, Token::RBrace) {
                *pos += 1;
                break;
            }

            if matches!(tokens[*pos].0, Token::Comma | Token::Newline) {
                *pos += 1;
                continue;
            }

            let kv = self.parse_key_value(tokens, pos)?;
            entries.push(kv);
        }

        let end = tokens[*pos - 1].1;

        Ok(Value::InlineTable(InlineTable {
            entries,
            span: start.merge(&end),
        }))
    }

    fn parse_array(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<ArrayValue, ParseError> {
        let start = tokens[*pos].1;
        *pos += 1; // skip '['

        let mut elements = Vec::new();

        while *pos < tokens.len() {
            if matches!(tokens[*pos].0, Token::RBracket) {
                *pos += 1;
                break;
            }

            if matches!(tokens[*pos].0, Token::Comma | Token::Newline) {
                *pos += 1;
                continue;
            }

            let value = self.parse_value(tokens, pos)?;
            elements.push(value);
        }

        let end = tokens[*pos - 1].1;

        Ok(ArrayValue {
            elements,
            span: start.merge(&end),
        })
    }

    fn parse_array_table(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<ArrayTable, ParseError> {
        let start = tokens[*pos].1;
        *pos += 1; // skip '[['

        let key = self.parse_key(tokens, pos)?;

        // Expect ']]'
        if *pos >= tokens.len() || !matches!(tokens[*pos].0, Token::RDoubleBracket) {
            return Err(ParseError::new("expected ']]'", start));
        }
        *pos += 1;

        let mut entries = Vec::new();

        while *pos < tokens.len() {
            match &tokens[*pos].0 {
                Token::BareKey(_) | Token::QuotedKey(_) => {
                    let kv = self.parse_key_value(tokens, pos)?;
                    entries.push(kv);
                }
                Token::LineComment(_) | Token::BlockComment(_) => {
                    let comment = self.parse_comment(tokens, pos)?;
                    entries.push(KeyValue {
                        key: Key::BareKey("_comment".to_string()),
                        value: Value::Scalar(ScalarValue::String(comment.content.clone())),
                        metadata: None,
                        comment: Some(comment),
                        span: Span::unknown(),
                    });
                }
                Token::Newline => {
                    *pos += 1;
                }
                _ => break,
            }
        }

        let end = tokens[*pos - 1].1;

        Ok(ArrayTable {
            key,
            entries,
            span: start.merge(&end),
        })
    }

    fn parse_include(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<IncludeDirective, ParseError> {
        let start = tokens[*pos].1;
        *pos += 1; // skip '@include'

        // Expect string path
        if *pos >= tokens.len() {
            return Err(ParseError::new("expected file path", start));
        }

        let path = match &tokens[*pos].0 {
            Token::StringLiteral(s) => s.clone(),
            _ => {
                return Err(ParseError::new(
                    "expected string path after @include",
                    tokens[*pos].1,
                ))
            }
        };
        *pos += 1;

        let mut merge_strategy = MergeStrategy::Override;

        // Check for merge strategy
        if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Merge) {
            *pos += 1;

            // Expect '='
            if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                *pos += 1;
            }

            // Parse strategy
            if *pos < tokens.len() {
                merge_strategy = match &tokens[*pos].0 {
                    Token::BareKey(s) => match s.as_str() {
                        "override" => MergeStrategy::Override,
                        "append" => MergeStrategy::Append,
                        "merge" => MergeStrategy::Merge,
                        "deep_merge" => MergeStrategy::DeepMerge,
                        _ => {
                            return Err(ParseError::new(
                                format!("unknown merge strategy: {}", s),
                                tokens[*pos].1,
                            ))
                        }
                    },
                    _ => {
                        return Err(ParseError::new(
                            "expected merge strategy",
                            tokens[*pos].1,
                        ))
                    }
                };
                *pos += 1;
            }
        }

        let end = tokens[*pos - 1].1;

        Ok(IncludeDirective {
            path,
            merge_strategy,
            span: start.merge(&end),
        })
    }

    fn parse_metadata(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<MetadataList, ParseError> {
        let start = tokens[*pos].1;
        *pos += 1; // skip '#@'

        let mut items = Vec::new();

        while *pos < tokens.len() {
            if matches!(tokens[*pos].0, Token::LineComment(_) | Token::Newline) {
                break;
            }

            if matches!(tokens[*pos].0, Token::Comma) {
                *pos += 1;
                continue;
            }

            let item = self.parse_metadata_item(tokens, pos)?;
            items.push(item);
        }

        let end = if *pos > 0 {
            tokens[*pos - 1].1
        } else {
            start
        };

        Ok(MetadataList {
            items,
            span: start.merge(&end),
        })
    }

    fn parse_metadata_item(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<MetadataItem, ParseError> {
        if *pos >= tokens.len() {
            return Err(ParseError::new("expected metadata item", Span::unknown()));
        }

        let (token, _) = &tokens[*pos];

        match token {
            Token::BareKey(key) => {
                let key_clone = key.clone();
                *pos += 1;

                // Check if it's a function-style metadata like range(...)
                if *pos < tokens.len() && matches!(tokens[*pos].0, Token::LParen) {
                    return self.parse_function_metadata(&key_clone, tokens, pos);
                }

                // Check if it has a value
                if *pos < tokens.len() && matches!(tokens[*pos].0, Token::Assign) {
                    *pos += 1;

                    if *pos >= tokens.len() {
                        return Err(ParseError::new(
                            "expected value after '='",
                            Span::unknown(),
                        ));
                    }

                    let value = match &tokens[*pos].0 {
                        Token::StringLiteral(s) => MetadataValue::String(s.clone()),
                        Token::NumberLiteral(n) => {
                            MetadataValue::Number(n.parse().unwrap_or(0.0))
                        }
                        _ => {
                            return Err(ParseError::new(
                                "expected string or number in metadata",
                                tokens[*pos].1,
                            ))
                        }
                    };
                    *pos += 1;

                    // Check if it's a standard metadata
                    Ok(self.create_standard_metadata(&key_clone, Some(value)))
                } else {
                    // Boolean metadata (no value)
                    Ok(self.create_standard_metadata(&key_clone, None))
                }
            }
            _ => Err(ParseError::new(
                format!("expected metadata key, found {}", token),
                tokens[*pos].1,
            )),
        }
    }

    fn parse_function_metadata(
        &mut self,
        key: &str,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<MetadataItem, ParseError> {
        // Skip '('
        *pos += 1;

        let result = match key {
            "range" => {
                // Parse min..max
                let min = match &tokens[*pos].0 {
                    Token::NumberLiteral(n) => n.parse::<f64>().map_err(|_| {
                        ParseError::new("invalid range min value", tokens[*pos].1)
                    })?,
                    _ => {
                        return Err(ParseError::new(
                            "expected number in range",
                            tokens[*pos].1,
                        ))
                    }
                };
                *pos += 1;

                // Expect '..'
                if !matches!(tokens[*pos].0, Token::RangeOp | Token::Dot) {
                    return Err(ParseError::new(
                        "expected '..' in range",
                        tokens[*pos].1,
                    ));
                }
                *pos += 1;
                // If it was a single dot, check for another
                if matches!(tokens[*pos - 1].0, Token::Dot) {
                    if !matches!(tokens[*pos].0, Token::Dot) {
                        return Err(ParseError::new(
                            "expected '..' in range",
                            tokens[*pos].1,
                        ));
                    }
                    *pos += 1;
                }

                let max = match &tokens[*pos].0 {
                    Token::NumberLiteral(n) => n.parse::<f64>().map_err(|_| {
                        ParseError::new("invalid range max value", tokens[*pos].1)
                    })?,
                    _ => {
                        return Err(ParseError::new(
                            "expected number in range",
                            tokens[*pos].1,
                        ))
                    }
                };
                *pos += 1;

                MetadataItem::Standard(StandardMetadata::Range { min, max })
            }
            _ => {
                return Err(ParseError::new(
                    format!("unknown function metadata: {}", key),
                    Span::unknown(),
                ))
            }
        };

        // Expect ')'
        if !matches!(tokens[*pos].0, Token::RParen) {
            return Err(ParseError::new(
                "expected ')' to close function call",
                tokens[*pos].1,
            ));
        }
        *pos += 1;

        Ok(result)
    }

    fn create_standard_metadata(&self, key: &str, value: Option<MetadataValue>) -> MetadataItem {
        match key {
            "sensitive" => MetadataItem::Standard(StandardMetadata::Sensitive),
            "required" => MetadataItem::Standard(StandardMetadata::Required),
            "deprecated" => MetadataItem::Standard(StandardMetadata::Deprecated {
                message: value.and_then(|v| match v {
                    MetadataValue::String(s) => Some(s),
                    _ => None,
                }),
            }),
            "description" => MetadataItem::Standard(StandardMetadata::Description {
                text: value
                    .and_then(|v| match v {
                        MetadataValue::String(s) => Some(s),
                        _ => None,
                    })
                    .unwrap_or_default(),
            }),
            "example" => MetadataItem::Standard(StandardMetadata::Example {
                value: value
                    .and_then(|v| match v {
                        MetadataValue::String(s) => Some(s),
                        _ => None,
                    })
                    .unwrap_or_default(),
            }),
            "type_hint" => MetadataItem::Standard(StandardMetadata::TypeHint {
                hint: value
                    .and_then(|v| match v {
                        MetadataValue::String(s) => Some(s),
                        _ => None,
                    })
                    .unwrap_or_default(),
            }),
            "item_type" => MetadataItem::Standard(StandardMetadata::ItemType {
                item_type: value
                    .and_then(|v| match v {
                        MetadataValue::String(s) => Some(s),
                        _ => None,
                    })
                    .unwrap_or_default(),
            }),
            _ => MetadataItem::Custom {
                key: key.to_string(),
                value,
            },
        }
    }

    fn parse_comment(
        &mut self,
        tokens: &[(Token, Span)],
        pos: &mut usize,
    ) -> Result<Comment, ParseError> {
        if *pos >= tokens.len() {
            return Err(ParseError::new("expected comment", Span::unknown()));
        }

        let (token, span) = &tokens[*pos];
        *pos += 1;

        match token {
            Token::LineComment(c) => Ok(Comment {
                content: c.clone(),
                is_block: false,
                span: *span,
            }),
            Token::BlockComment(c) => Ok(Comment {
                content: c.clone(),
                is_block: true,
                span: *span,
            }),
            _ => Err(ParseError::new(
                format!("expected comment, found {}", token),
                *span,
            )),
        }
    }
}

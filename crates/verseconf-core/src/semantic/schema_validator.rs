use crate::ast::*;
use crate::semantic::validator::{ValidationError, ValidationWarning};

/// Schema 校验器
pub struct SchemaValidator {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

impl SchemaValidator {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// 根据 Schema 校验配置数据
    pub fn validate_with_schema(
        &mut self,
        ast: &Ast,
    ) -> Result<(), ValidationError> {
        if let Some(schema) = &ast.schema {
            self.validate_table_against_schema(&ast.root, &schema.fields, schema.strict)?;
        }
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors[0].clone())
        }
    }

    /// 校验 TableBlock 是否符合 Schema 定义
    fn validate_table_against_schema(
        &mut self,
        table: &TableBlock,
        schema_fields: &[SchemaField],
        strict: bool,
    ) -> Result<(), ValidationError> {
        // 收集 table 中的所有 key
        let table_keys: Vec<&str> = table
            .entries
            .iter()
            .filter_map(|entry| match entry {
                TableEntry::KeyValue(kv) => Some(kv.key.as_str()),
                TableEntry::TableBlock(tb) => tb.name.as_deref(),
                _ => None,
            })
            .collect();

        // 收集 schema 中声明的字段名
        let schema_field_names: Vec<&str> = schema_fields
            .iter()
            .map(|f| f.name.as_str())
            .collect();

        // 严格模式：检查未声明字段
        if strict {
            for &key in &table_keys {
                if !schema_field_names.contains(&key) {
                    // 找到该 key 的 span
                    let span = table.entries.iter().find_map(|entry| match entry {
                        TableEntry::KeyValue(kv) if kv.key.as_str() == key => Some(kv.span),
                        TableEntry::TableBlock(tb) if tb.name.as_deref() == Some(key) => Some(tb.span),
                        _ => None,
                    }).unwrap_or(Span::unknown());
                    
                    self.errors.push(ValidationError::new(
                        format!("undeclared field '{}' in strict mode", key),
                        span,
                    ));
                }
            }
        }

        // 校验每个 schema field
        for field in schema_fields {
            let has_key = table_keys.iter().any(|&k| k == field.name);

            // 检查 required
            if field.required && !has_key {
                self.errors.push(ValidationError::new(
                    format!("missing required field '{}'", field.name),
                    field.span,
                ));
                continue;
            }

            // 如果字段存在，校验类型和约束
            if has_key {
                // Try KeyValue first
                if let Some(kv) = table.entries.iter().find_map(|entry| match entry {
                    TableEntry::KeyValue(kv) if kv.key.as_str() == field.name => Some(kv),
                    _ => None,
                }) {
                    self.validate_field_value(&kv.value, field)?;
                } else if let Some(tb) = table.entries.iter().find_map(|entry| match entry {
                    TableEntry::TableBlock(tb) if tb.name.as_deref() == Some(field.name.as_str()) => Some(tb),
                    _ => None,
                }) {
                    // Handle TableBlock entry
                    if !field.nested_fields.is_empty() {
                        self.validate_table_against_schema(tb, &field.nested_fields, true)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// 校验字段值是否符合 Schema 约束
    fn validate_field_value(
        &mut self,
        value: &Value,
        field: &SchemaField,
    ) -> Result<(), ValidationError> {
        // 校验类型
        self.validate_type(value, &field.field_type, field)?;

        // 校验范围约束
        if let Some(range) = &field.range {
            self.validate_range_constraint(value, range, field)?;
        }

        // 校验 pattern 约束
        if let Some(pattern) = &field.pattern {
            self.validate_pattern_constraint(value, pattern, field)?;
        }

        // 校验 enum 约束
        if let Some(enum_values) = &field.enum_values {
            self.validate_enum_constraint(value, enum_values, field)?;
        }

        // 递归校验嵌套字段
        if let Value::TableBlock(table) = value {
            if !field.nested_fields.is_empty() {
                self.validate_table_against_schema(table, &field.nested_fields, true)?;
            }
        }

        Ok(())
    }

    /// 校验类型匹配
    fn validate_type(
        &mut self,
        value: &Value,
        expected_type: &SchemaType,
        field: &SchemaField,
    ) -> Result<(), ValidationError> {
        let matches = match (value, expected_type) {
            (Value::Scalar(ScalarValue::String(_)), SchemaType::String) => true,
            (Value::Scalar(ScalarValue::Number(NumberValue::Integer(_))), SchemaType::Integer) => true,
            (Value::Scalar(ScalarValue::Number(NumberValue::Float(_))), SchemaType::Float) => true,
            (Value::Scalar(ScalarValue::Number(_)), SchemaType::Float) => true,
            (Value::Scalar(ScalarValue::Boolean(_)), SchemaType::Boolean) => true,
            (Value::Scalar(ScalarValue::DateTime(_)), SchemaType::DateTime) => true,
            (Value::Scalar(ScalarValue::Duration(_)), SchemaType::Duration) => true,
            (Value::TableBlock(_), SchemaType::Table) => true,
            (Value::Array(_), SchemaType::Array(_)) => true,
            // 表达式类型，需要评估后校验
            (Value::Expression(expr), _) => {
                match expr.evaluate() {
                    Ok(evaluated) => {
                        // 递归校验评估后的值
                        return self.validate_type(&Value::Scalar(evaluated), expected_type, field);
                    }
                    Err(_) => true, // 评估失败则跳过校验
                }
            }
            _ => false,
        };

        if !matches {
            self.errors.push(ValidationError::new(
                format!(
                    "type mismatch for field '{}': expected {:?}, found {:?}",
                    field.name, expected_type, value
                ),
                field.span,
            ));
        }

        Ok(())
    }

    /// 校验范围约束
    fn validate_range_constraint(
        &mut self,
        value: &Value,
        range: &RangeConstraint,
        field: &SchemaField,
    ) -> Result<(), ValidationError> {
        let num_value = match value {
            Value::Scalar(ScalarValue::Number(NumberValue::Integer(n))) => *n as f64,
            Value::Scalar(ScalarValue::Number(NumberValue::Float(n))) => *n,
            Value::Expression(expr) => {
                match expr.evaluate() {
                    Ok(ScalarValue::Number(NumberValue::Integer(n))) => n as f64,
                    Ok(ScalarValue::Number(NumberValue::Float(n))) => n,
                    _ => return Ok(()),
                }
            }
            _ => return Ok(()),
        };

        if let Some(min) = range.min {
            if num_value < min as f64 {
                self.errors.push(ValidationError::new(
                    format!(
                        "value {} for field '{}' is below minimum {}",
                        num_value, field.name, min
                    ),
                    field.span,
                ));
            }
        }

        if let Some(max) = range.max {
            if num_value > max as f64 {
                self.errors.push(ValidationError::new(
                    format!(
                        "value {} for field '{}' exceeds maximum {}",
                        num_value, field.name, max
                    ),
                    field.span,
                ));
            }
        }

        Ok(())
    }

    /// 校验 pattern 约束
    fn validate_pattern_constraint(
        &mut self,
        value: &Value,
        pattern: &str,
        field: &SchemaField,
    ) -> Result<(), ValidationError> {
        let str_value = match value {
            Value::Scalar(ScalarValue::String(s)) => s,
            _ => return Ok(()),
        };

        // 简单实现：使用基本的字符串匹配
        // 完整实现应使用 regex crate
        if !self.simple_pattern_match(str_value, pattern) {
            self.warnings.push(ValidationWarning::new(
                format!(
                    "value '{}' for field '{}' may not match pattern '{}'",
                    str_value, field.name, pattern
                ),
                field.span,
            ));
        }

        Ok(())
    }

    /// 简单的 pattern 匹配（基础实现）
    fn simple_pattern_match(&self, value: &str, pattern: &str) -> bool {
        // 这里只是基础实现，完整实现需要 regex crate
        // 目前只支持简单的通配符
        if pattern.is_empty() {
            return true;
        }
        // 简单检查：如果 pattern 包含在 value 中，认为匹配
        value.contains(pattern) || pattern == "*"
    }

    /// 校验 enum 约束
    fn validate_enum_constraint(
        &mut self,
        value: &Value,
        enum_values: &[ScalarValue],
        field: &SchemaField,
    ) -> Result<(), ValidationError> {
        let scalar = match value {
            Value::Scalar(s) => s,
            Value::Expression(expr) => {
                match expr.evaluate() {
                    Ok(evaluated) => {
                        let matches = enum_values.iter().any(|ev| ev == &evaluated);
                        if !matches {
                            self.errors.push(ValidationError::new(
                                format!(
                                    "value {:?} for field '{}' is not in allowed values: {:?}",
                                    evaluated, field.name, enum_values
                                ),
                                field.span,
                            ));
                        }
                        return Ok(());
                    }
                    Err(_) => return Ok(()),
                }
            }
            _ => return Ok(()),
        };

        let matches = enum_values.iter().any(|ev| ev == scalar);

        if !matches {
            self.errors.push(ValidationError::new(
                format!(
                    "value {:?} for field '{}' is not in allowed values: {:?}",
                    scalar, field.name, enum_values
                ),
                field.span,
            ));
        }

        Ok(())
    }
}

impl Default for SchemaValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_and_validate;

    #[test]
    fn test_schema_validator_new() {
        let validator = SchemaValidator::new();
        assert!(validator.errors.is_empty());
        assert!(validator.warnings.is_empty());
    }

    #[test]
    fn test_simple_pattern_match() {
        let validator = SchemaValidator::new();
        assert!(validator.simple_pattern_match("hello", "ell"));
        assert!(validator.simple_pattern_match("hello", "*"));
        assert!(!validator.simple_pattern_match("hello", "xyz"));
    }

    #[test]
    fn test_parse_schema_basic() {
        let source = r#"#@schema {
    version = "1.0"
    description = "Test schema"
    
    name {
        type = "string"
        required = true
        desc = "User name"
    }
}

name = "test"
"#;
        let result = parse_and_validate(source);
        assert!(result.is_ok());
        let ast = result.unwrap();
        assert!(ast.schema.is_some());
        let schema = ast.schema.as_ref().unwrap();
        assert_eq!(schema.version, Some("1.0".to_string()));
        assert_eq!(schema.fields.len(), 1);
        assert_eq!(schema.fields[0].name, "name");
        assert!(schema.fields[0].required);
    }

    #[test]
    fn test_schema_required_field_missing() {
        let source = r#"#@schema {
    name {
        type = "string"
        required = true
    }
}
"#;
        let result = parse_and_validate(source);
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_type_mismatch() {
        let source = r#"#@schema {
    count {
        type = "integer"
    }
}

count = "not a number"
"#;
        let result = parse_and_validate(source);
        // Debug: check if schema exists
        if let Ok(ref ast) = result {
            eprintln!("Schema exists: {}", ast.schema.is_some());
            if let Some(ref schema) = ast.schema {
                eprintln!("Schema fields: {}", schema.fields.len());
                for field in &schema.fields {
                    eprintln!("  Field: {} type={:?}", field.name, field.field_type);
                }
            }
        }
        assert!(result.is_err(), "Expected type mismatch error");
    }

    #[test]
    fn test_schema_range_constraint_valid() {
        let source = r#"#@schema {
    port {
        type = "integer"
        range = (1024..65535)
    }
}

port = 8080
"#;
        let result = parse_and_validate(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_schema_range_constraint_violation() {
        let source = r#"#@schema {
    port {
        type = "integer"
        range = (1024..65535)
    }
}

port = 80
"#;
        let result = parse_and_validate(source);
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_enum_constraint_valid() {
        let source = r#"#@schema {
    log_level {
        type = "string"
        enum = ["debug", "info", "warn", "error"]
    }
}

log_level = "info"
"#;
        let result = parse_and_validate(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_schema_enum_constraint_violation() {
        let source = r#"#@schema {
    log_level {
        type = "string"
        enum = ["debug", "info", "warn", "error"]
    }
}

log_level = "trace"
"#;
        let result = parse_and_validate(source);
        assert!(result.is_err());
    }

    #[test]
    fn test_schema_nested_fields() {
        let source = r#"#@schema {
    database {
        type = "table"
        
        host {
            type = "string"
            required = true
        }
        
        port {
            type = "integer"
            default = 5432
        }
    }
}

database {
    host = "localhost"
    port = 5432
}
"#;
        let result = parse_and_validate(source);
        assert!(result.is_ok());
        let ast = result.unwrap();
        assert!(ast.schema.is_some());
        let schema = ast.schema.as_ref().unwrap();
        assert_eq!(schema.fields.len(), 1);
        assert_eq!(schema.fields[0].nested_fields.len(), 2);
    }

    #[test]
    fn test_schema_without_schema_file() {
        let source = r#"name = "test"
port = 8080
"#;
        let result = parse_and_validate(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_schema_with_llm_hint() {
        let source = r#"#@schema {
    api_key {
        type = "string"
        required = true
        sensitive = true
        llm_hint = "This is a secret API key, do not expose in logs"
    }
}

api_key = "secret123"
"#;
        let result = parse_and_validate(source);
        assert!(result.is_ok());
        let ast = result.unwrap();
        let schema = ast.schema.as_ref().unwrap();
        assert!(schema.fields[0].sensitive);
        assert!(schema.fields[0].llm_hint.is_some());
    }

    #[test]
    fn test_strict_mode_undeclared_field() {
        let source = r#"#@schema {
    strict = true
    name {
        type = "string"
        required = true
    }
}

name = "test"
extra_field = "should fail"
"#;
        let result = parse_and_validate(source);
        assert!(result.is_err());
    }

    #[test]
    fn test_non_strict_mode_allows_extra_fields() {
        let source = r#"#@schema {
    strict = false
    name {
        type = "string"
        required = true
    }
}

name = "test"
extra_field = "should pass"
"#;
        let result = parse_and_validate(source);
        assert!(result.is_ok());
    }

    #[test]
    fn test_strict_mode_nested_table() {
        let source = r#"#@schema {
    strict = true
    database {
        type = "table"
        host {
            type = "string"
            required = true
        }
    }
}

database {
    host = "localhost"
    unknown = "should fail"
}
"#;
        let result = parse_and_validate(source);
        assert!(result.is_err());
    }

    #[test]
    fn test_strict_parsing() {
        let source = r#"#@schema {
    strict = true
    port {
        type = "integer"
    }
}

port = 8080
"#;
        let result = parse_and_validate(source);
        assert!(result.is_ok());
        let ast = result.unwrap();
        let schema = ast.schema.as_ref().unwrap();
        assert!(schema.strict);
    }
}

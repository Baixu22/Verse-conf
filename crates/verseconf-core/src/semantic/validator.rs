use crate::ast::*;
use crate::Span;
use std::fmt;

/// 校验错误
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub message: String,
    pub span: Span,
}

impl ValidationError {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Validation error at {}: {}", self.span, self.message)
    }
}

/// 校验警告
#[derive(Debug, Clone)]
pub struct ValidationWarning {
    pub message: String,
    pub span: Span,
}

impl ValidationWarning {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }
}

impl fmt::Display for ValidationWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Validation warning at {}: {}", self.span, self.message)
    }
}

/// 校验结果
pub type ValidationResult = Result<(), ValidationError>;

/// 校验器
pub struct Validator {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

impl Validator {
    /// 创建新的校验器
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// 校验 AST
    pub fn validate(&mut self, ast: &Ast) -> ValidationResult {
        self.validate_table_block(&ast.root)?;
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors[0].clone())
        }
    }

    /// 校验表块
    fn validate_table_block(&mut self, table: &TableBlock) -> ValidationResult {
        for entry in &table.entries {
            match entry {
                TableEntry::KeyValue(kv) => {
                    self.validate_key_value(kv)?;
                }
                TableEntry::ArrayTable(at) => {
                    for kv in &at.entries {
                        self.validate_key_value(kv)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// 校验键值对
    fn validate_key_value(&mut self, kv: &KeyValue) -> ValidationResult {
        // 校验元数据
        if let Some(metadata) = &kv.metadata {
            self.validate_metadata(&kv.value, metadata)?;
        }

        // 递归校验嵌套表
        if let Value::TableBlock(table) = &kv.value {
            self.validate_table_block(table)?;
        }

        // 校验内联表
        if let Value::InlineTable(table) = &kv.value {
            for entry in &table.entries {
                self.validate_key_value(entry)?;
            }
        }

        // 校验数组
        if let Value::Array(arr) = &kv.value {
            for elem in &arr.elements {
                if let Value::Scalar(_scalar) = elem {
                    // 数组元素校验在 validate_metadata 中处理
                }
            }
        }

        Ok(())
    }

    /// 校验元数据
    fn validate_metadata(
        &mut self,
        value: &Value,
        metadata: &MetadataList,
    ) -> ValidationResult {
        // 校验 range
        if let Some((min, max)) = metadata.get_range() {
            self.validate_range(value, min, max)?;
        }

        // 校验 type_hint
        if let Some(hint) = metadata.get_type_hint() {
            self.validate_type_hint(value, hint)?;
        }

        // 校验 sensitive
        if metadata.has_sensitive() {
            self.warn_sensitive(value);
        }

        // 校验 required
        if metadata.has_required() {
            self.validate_required(value)?;
        }

        Ok(())
    }

    /// 校验 range
    fn validate_range(&mut self, value: &Value, min: f64, max: f64) -> ValidationResult {
        let num_value = match value {
            Value::Scalar(ScalarValue::Number(num)) => num.as_f64(),
            Value::Expression(expr) => {
                match expr.evaluate() {
                    Ok(ScalarValue::Number(num)) => num.as_f64(),
                    Ok(_) => return Ok(()),
                    Err(_) => return Ok(()),
                }
            }
            _ => return Ok(()),
        };
        
        if num_value < min || num_value > max {
            return Err(ValidationError::new(
                format!("value {} is out of range [{}..{}]", num_value, min, max),
                Span::unknown(),
            ));
        }
        Ok(())
    }

    /// 校验 type_hint
    fn validate_type_hint(&mut self, value: &Value, hint: &str) -> ValidationResult {
        match hint {
            "int" => {
                if let Value::Scalar(ScalarValue::Number(NumberValue::Integer(_))) = value {
                    return Ok(());
                }
                Err(ValidationError::new(
                    format!("expected integer, found {:?}", value),
                    Span::unknown(),
                ))
            }
            "float" => {
                if let Value::Scalar(ScalarValue::Number(_)) = value {
                    return Ok(());
                }
                Err(ValidationError::new(
                    format!("expected float, found {:?}", value),
                    Span::unknown(),
                ))
            }
            "string" => {
                if let Value::Scalar(ScalarValue::String(_)) = value {
                    return Ok(());
                }
                Err(ValidationError::new(
                    format!("expected string, found {:?}", value),
                    Span::unknown(),
                ))
            }
            "bool" => {
                if let Value::Scalar(ScalarValue::Boolean(_)) = value {
                    return Ok(());
                }
                Err(ValidationError::new(
                    format!("expected boolean, found {:?}", value),
                    Span::unknown(),
                ))
            }
            _ => {
                self.warnings.push(ValidationWarning::new(
                    format!("unknown type_hint: {}", hint),
                    Span::unknown(),
                ));
                Ok(())
            }
        }
    }

    /// 校验 required
    fn validate_required(&mut self, value: &Value) -> ValidationResult {
        // 如果值为空字符串或未定义，则报错
        if let Value::Scalar(ScalarValue::String(s)) = value {
            if s.is_empty() {
                return Err(ValidationError::new(
                    "required value is empty",
                    Span::unknown(),
                ));
            }
        }
        Ok(())
    }

    /// 警告敏感值
    fn warn_sensitive(&mut self, value: &Value) {
        if let Value::Scalar(ScalarValue::String(s)) = value {
            if !s.is_empty() {
                self.warnings.push(ValidationWarning::new(
                    "sensitive value detected, will be masked in output",
                    Span::unknown(),
                ));
            }
        }
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_range_valid() {
        let mut validator = Validator::new();
        let value = Value::Scalar(ScalarValue::Number(NumberValue::Integer(8080)));
        let result = validator.validate_range(&value, 1024.0, 65535.0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_range_invalid() {
        let mut validator = Validator::new();
        let value = Value::Scalar(ScalarValue::Number(NumberValue::Integer(80)));
        let result = validator.validate_range(&value, 1024.0, 65535.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_type_hint_int() {
        let mut validator = Validator::new();
        let value = Value::Scalar(ScalarValue::Number(NumberValue::Integer(42)));
        let result = validator.validate_type_hint(&value, "int");
        assert!(result.is_ok());
    }
}

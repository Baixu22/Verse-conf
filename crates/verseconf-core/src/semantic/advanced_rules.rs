use serde::{Deserialize, Serialize};

use crate::ast::*;
use crate::semantic::validator::ValidationError;

/// 高级验证规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdvancedRule {
    /// 正则表达式验证
    Regex {
        field: String,
        pattern: String,
        error_message: Option<String>,
    },
    /// 字段依赖关系验证（如果 field 存在，则 required_fields 也必须存在）
    Dependency {
        field: String,
        required_fields: Vec<String>,
        error_message: Option<String>,
    },
    /// 交叉验证（字段间的关系）
    CrossField {
        field_a: String,
        field_b: String,
        relation: FieldRelation,
        error_message: Option<String>,
    },
    /// 条件验证（当 condition_field 满足某值时，target_field 必须满足规则）
    Conditional {
        condition_field: String,
        condition_value: String,
        target_field: String,
        rule: Box<AdvancedRule>,
    },
}

/// 字段间关系
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FieldRelation {
    /// field_a 必须大于 field_b
    GreaterThan,
    /// field_a 必须小于 field_b
    LessThan,
    /// field_a 必须大于等于 field_b
    GreaterThanOrEqual,
    /// field_a 必须小于等于 field_b
    LessThanOrEqual,
    /// field_a 必须等于 field_b
    Equal,
    /// field_a 必须不等于 field_b
    NotEqual,
    /// 如果 field_a 存在，则 field_b 也必须存在
    Implies,
}

/// 高级验证规则集
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedValidationRules {
    pub rules: Vec<AdvancedRule>,
}

impl AdvancedValidationRules {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn add_rule(&mut self, rule: AdvancedRule) {
        self.rules.push(rule);
    }

    /// 验证 AST 是否符合高级规则
    pub fn validate(&self, ast: &Ast) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        
        for rule in &self.rules {
            if let Err(e) = self.validate_rule(rule, ast) {
                errors.push(e);
            }
        }
        
        errors
    }

    fn validate_rule(&self, rule: &AdvancedRule, ast: &Ast) -> Result<(), ValidationError> {
        match rule {
            AdvancedRule::Regex { field, pattern, error_message } => {
                self.validate_regex(field, pattern, error_message.as_deref(), ast)
            }
            AdvancedRule::Dependency { field, required_fields, error_message } => {
                self.validate_dependency(field, required_fields, error_message.as_deref(), ast)
            }
            AdvancedRule::CrossField { field_a, field_b, relation, error_message } => {
                self.validate_cross_field(field_a, field_b, relation, error_message.as_deref(), ast)
            }
            AdvancedRule::Conditional { condition_field, condition_value, target_field, rule } => {
                self.validate_conditional(condition_field, condition_value, target_field, rule, ast)
            }
        }
    }

    fn validate_regex(&self, field: &str, pattern: &str, error_msg: Option<&str>, ast: &Ast) -> Result<(), ValidationError> {
        let value = self.extract_field_value(field, ast);
        
        let str_value = match &value {
            Some(Value::Scalar(ScalarValue::String(s))) => s.clone(),
            Some(Value::Expression(expr)) => {
                match expr.evaluate() {
                    Ok(ScalarValue::String(s)) => s,
                    _ => return Ok(()),
                }
            }
            _ => return Ok(()),
        };
        
        let re = regex::Regex::new(pattern).map_err(|e| {
            ValidationError::new(
                format!("invalid regex pattern for field '{}': {}", field, e),
                Span::unknown(),
            )
        })?;
        
        if !re.is_match(&str_value) {
            return Err(ValidationError::new(
                error_msg.unwrap_or(&format!(
                    "value '{}' for field '{}' does not match pattern '{}'",
                    str_value, field, pattern
                )),
                Span::unknown(),
            ));
        }
        
        Ok(())
    }

    fn validate_dependency(&self, field: &str, required_fields: &[String], error_msg: Option<&str>, ast: &Ast) -> Result<(), ValidationError> {
        let field_value = self.extract_field_value(field, ast);
        
        if field_value.is_some() {
            for required_field in required_fields {
                if self.extract_field_value(required_field, ast).is_none() {
                    return Err(ValidationError::new(
                        error_msg.unwrap_or(&format!(
                            "field '{}' requires field '{}' to be present",
                            field, required_field
                        )),
                        Span::unknown(),
                    ));
                }
            }
        }
        
        Ok(())
    }

    fn validate_cross_field(&self, field_a: &str, field_b: &str, relation: &FieldRelation, error_msg: Option<&str>, ast: &Ast) -> Result<(), ValidationError> {
        let value_a = self.extract_field_value(field_a, ast);
        let value_b = self.extract_field_value(field_b, ast);
        
        if let (Some(val_a), Some(val_b)) = (value_a, value_b) {
            match relation {
                FieldRelation::GreaterThan => {
                    if !self.compare_values(&val_a, &val_b, |a, b| a > b) {
                        return Err(ValidationError::new(
                            error_msg.unwrap_or(&format!(
                                "field '{}' must be greater than field '{}'",
                                field_a, field_b
                            )),
                            Span::unknown(),
                        ));
                    }
                }
                FieldRelation::LessThan => {
                    if !self.compare_values(&val_a, &val_b, |a, b| a < b) {
                        return Err(ValidationError::new(
                            error_msg.unwrap_or(&format!(
                                "field '{}' must be less than field '{}'",
                                field_a, field_b
                            )),
                            Span::unknown(),
                        ));
                    }
                }
                FieldRelation::GreaterThanOrEqual => {
                    if !self.compare_values(&val_a, &val_b, |a, b| a >= b) {
                        return Err(ValidationError::new(
                            error_msg.unwrap_or(&format!(
                                "field '{}' must be greater than or equal to field '{}'",
                                field_a, field_b
                            )),
                            Span::unknown(),
                        ));
                    }
                }
                FieldRelation::LessThanOrEqual => {
                    if !self.compare_values(&val_a, &val_b, |a, b| a <= b) {
                        return Err(ValidationError::new(
                            error_msg.unwrap_or(&format!(
                                "field '{}' must be less than or equal to field '{}'",
                                field_a, field_b
                            )),
                            Span::unknown(),
                        ));
                    }
                }
                FieldRelation::Equal => {
                    if !self.values_equal(&val_a, &val_b) {
                        return Err(ValidationError::new(
                            error_msg.unwrap_or(&format!(
                                "field '{}' must be equal to field '{}'",
                                field_a, field_b
                            )),
                            Span::unknown(),
                        ));
                    }
                }
                FieldRelation::NotEqual => {
                    if self.values_equal(&val_a, &val_b) {
                        return Err(ValidationError::new(
                            error_msg.unwrap_or(&format!(
                                "field '{}' must not be equal to field '{}'",
                                field_a, field_b
                            )),
                            Span::unknown(),
                        ));
                    }
                }
                FieldRelation::Implies => {
                    // If field_a exists, field_b must also exist (already checked above)
                }
            }
        }
        
        Ok(())
    }

    fn validate_conditional(&self, condition_field: &str, condition_value: &str, _target_field: &str, rule: &AdvancedRule, ast: &Ast) -> Result<(), ValidationError> {
        let cond_val = self.extract_field_value(condition_field, ast);
        
        if let Some(Value::Scalar(ScalarValue::String(s))) = cond_val {
            if s == condition_value {
                return self.validate_rule(rule, ast);
            }
        }
        
        Ok(())
    }

    fn extract_field_value(&self, path: &str, ast: &Ast) -> Option<Value> {
        let parts: Vec<&str> = path.split('.').collect();
        self.navigate_table(&ast.root, &parts)
    }

    fn navigate_table(&self, table: &TableBlock, parts: &[&str]) -> Option<Value> {
        if parts.is_empty() {
            return None;
        }
        
        let key = parts[0];
        
        for entry in &table.entries {
            match entry {
                TableEntry::KeyValue(kv) if kv.key.as_str() == key => {
                    if parts.len() == 1 {
                        return Some(kv.value.clone());
                    } else {
                        if let Value::TableBlock(inner_table) = &kv.value {
                            return self.navigate_table(inner_table, &parts[1..]);
                        }
                        return None;
                    }
                }
                TableEntry::TableBlock(tb) if tb.name.as_deref() == Some(key) => {
                    return self.navigate_table(tb, parts);
                }
                _ => {}
            }
        }
        
        None
    }

    fn compare_values<F>(&self, val_a: &Value, val_b: &Value, cmp: F) -> bool
    where
        F: Fn(f64, f64) -> bool,
    {
        let num_a = self.extract_number(val_a);
        let num_b = self.extract_number(val_b);
        
        if let (Some(a), Some(b)) = (num_a, num_b) {
            cmp(a, b)
        } else {
            false
        }
    }

    fn extract_number(&self, value: &Value) -> Option<f64> {
        match value {
            Value::Scalar(ScalarValue::Number(NumberValue::Integer(n))) => Some(*n as f64),
            Value::Scalar(ScalarValue::Number(NumberValue::Float(n))) => Some(*n),
            Value::Expression(expr) => {
                match expr.evaluate() {
                    Ok(ScalarValue::Number(NumberValue::Integer(n))) => Some(n as f64),
                    Ok(ScalarValue::Number(NumberValue::Float(n))) => Some(n),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn values_equal(&self, val_a: &Value, val_b: &Value) -> bool {
        match (val_a, val_b) {
            (Value::Scalar(a), Value::Scalar(b)) => a == b,
            (Value::Expression(expr_a), Value::Expression(expr_b)) => {
                match (expr_a.evaluate(), expr_b.evaluate()) {
                    (Ok(a), Ok(b)) => a == b,
                    _ => false,
                }
            }
            (Value::Scalar(s), Value::Expression(expr)) | (Value::Expression(expr), Value::Scalar(s)) => {
                match expr.evaluate() {
                    Ok(evaluated) => *s == evaluated,
                    _ => false,
                }
            }
            _ => false,
        }
    }
}

impl Default for AdvancedValidationRules {
    fn default() -> Self {
        Self::new()
    }
}

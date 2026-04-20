use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// 模板变量定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateVariable {
    /// 变量名称
    pub name: String,
    /// 变量类型
    pub var_type: VariableType,
    /// 默认值
    pub default: Option<String>,
    /// 变量描述
    pub description: Option<String>,
    /// 是否必需
    pub required: bool,
    /// 可选值（用于枚举类型）
    pub choices: Option<Vec<String>>,
}

/// 变量类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum VariableType {
    String,
    Integer,
    Float,
    Boolean,
    Enum,
    Password,
    Hostname,
    Port,
    Path,
    Url,
}

impl fmt::Display for VariableType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VariableType::String => write!(f, "string"),
            VariableType::Integer => write!(f, "integer"),
            VariableType::Float => write!(f, "float"),
            VariableType::Boolean => write!(f, "boolean"),
            VariableType::Enum => write!(f, "enum"),
            VariableType::Password => write!(f, "password"),
            VariableType::Hostname => write!(f, "hostname"),
            VariableType::Port => write!(f, "port"),
            VariableType::Path => write!(f, "path"),
            VariableType::Url => write!(f, "url"),
        }
    }
}

/// 模板定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    /// 模板名称
    pub name: String,
    /// 模板描述
    pub description: Option<String>,
    /// 模板版本
    pub version: String,
    /// 变量定义
    pub variables: Vec<TemplateVariable>,
    /// 模板内容（VerseConf 格式，包含变量占位符）
    pub content: String,
}

/// 变量值（用户提供的实际值）
#[derive(Debug, Clone)]
pub struct VariableValue {
    pub name: String,
    pub value: String,
}

/// 模板渲染上下文
#[derive(Debug, Clone)]
pub struct RenderContext {
    /// 变量值映射
    pub values: HashMap<String, String>,
    /// 是否严格模式（未定义变量报错）
    pub strict: bool,
}

impl RenderContext {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            strict: true,
        }
    }

    pub fn with_value(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.values.insert(name.into(), value.into());
        self
    }

    pub fn with_values(mut self, values: HashMap<String, String>) -> Self {
        self.values.extend(values);
        self
    }

    pub fn strict_mode(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }
}

impl Default for RenderContext {
    fn default() -> Self {
        Self::new()
    }
}

/// 模板渲染错误
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("undefined variable: {0}")]
    UndefinedVariable(String),
    #[error("invalid variable value: {0}")]
    InvalidValue(String),
    #[error("template parse error: {0}")]
    ParseError(String),
    #[error("missing required variable: {0}")]
    MissingRequired(String),
}

/// 验证变量值是否符合模板定义
pub fn validate_variables(
    template: &Template,
    values: &HashMap<String, String>,
) -> Result<(), TemplateError> {
    for var in &template.variables {
        if var.required && !values.contains_key(&var.name) {
            if var.default.is_none() {
                return Err(TemplateError::MissingRequired(var.name.clone()));
            }
        }

        if let Some(value) = values.get(&var.name) {
            validate_variable_value(var, value)?;
        }
    }
    Ok(())
}

fn validate_variable_value(var: &TemplateVariable, value: &str) -> Result<(), TemplateError> {
    match var.var_type {
        VariableType::Integer => {
            if value.parse::<i64>().is_err() {
                return Err(TemplateError::InvalidValue(format!(
                    "'{}' is not a valid integer",
                    value
                )));
            }
        }
        VariableType::Float => {
            if value.parse::<f64>().is_err() {
                return Err(TemplateError::InvalidValue(format!(
                    "'{}' is not a valid float",
                    value
                )));
            }
        }
        VariableType::Boolean => {
            if !matches!(value.to_lowercase().as_str(), "true" | "false" | "1" | "0") {
                return Err(TemplateError::InvalidValue(format!(
                    "'{}' is not a valid boolean",
                    value
                )));
            }
        }
        VariableType::Port => {
            if let Ok(port) = value.parse::<u16>() {
                if port == 0 {
                    return Err(TemplateError::InvalidValue(format!(
                        "port {} is out of range (1-65535)",
                        port
                    )));
                }
            } else {
                return Err(TemplateError::InvalidValue(format!(
                    "'{}' is not a valid port number",
                    value
                )));
            }
        }
        VariableType::Enum => {
            if let Some(choices) = &var.choices {
                if !choices.contains(&value.to_string()) {
                    return Err(TemplateError::InvalidValue(format!(
                        "'{}' is not one of the allowed values: {:?}",
                        value, choices
                    )));
                }
            }
        }
        _ => {}
    }
    Ok(())
}

use regex::Regex;
use std::collections::HashMap;
use std::env;

/// 环境变量插值错误
#[derive(Debug, thiserror::Error)]
pub enum InterpError {
    #[error("undefined environment variable: {0}")]
    UndefinedVariable(String),
    #[error("invalid interpolation syntax: {0}")]
    InvalidSyntax(String),
}

/// 环境变量插值器
pub struct EnvInterpolator {
    env_vars: HashMap<String, String>,
}

impl EnvInterpolator {
    /// 创建新的插值器
    pub fn new() -> Self {
        Self {
            env_vars: HashMap::new(),
        }
    }

    /// 从系统环境加载
    pub fn load_from_env(&mut self) {
        self.env_vars = env::vars().collect();
    }

    /// 设置环境变量
    pub fn set_var(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.env_vars.insert(key.into(), value.into());
    }

    /// 插值字符串: "${DB_HOST|localhost}" -> "localhost"
    pub fn interpolate(&self, s: &str) -> Result<String, InterpError> {
        let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
        let mut result = s.to_string();

        for cap in re.captures_iter(s) {
            let full_match = cap.get(0).unwrap().as_str();
            let content = cap.get(1).unwrap().as_str();

            let replacement = self.resolve_variable(content)?;
            result = result.replace(full_match, &replacement);
        }

        Ok(result)
    }

    /// 解析单个变量
    fn resolve_variable(&self, content: &str) -> Result<String, InterpError> {
        // 检查是否有默认值
        if let Some((var_name, default_value)) = content.split_once('|') {
            // 如果有值则返回，否则返回默认值
            if let Some(value) = self.env_vars.get(var_name.trim()) {
                Ok(value.clone())
            } else {
                Ok(default_value.to_string())
            }
        } else {
            // 没有默认值，必须存在
            self.env_vars
                .get(content)
                .cloned()
                .ok_or_else(|| InterpError::UndefinedVariable(content.to_string()))
        }
    }
}

impl Default for EnvInterpolator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interpolate_with_default() {
        let interp = EnvInterpolator::new();
        let result = interp.interpolate("${DB_HOST|localhost}").unwrap();
        assert_eq!(result, "localhost");
    }

    #[test]
    fn test_interpolate_with_env() {
        let mut interp = EnvInterpolator::new();
        interp.set_var("DB_HOST", "production.db.com");
        let result = interp.interpolate("${DB_HOST|localhost}").unwrap();
        assert_eq!(result, "production.db.com");
    }

    #[test]
    fn test_interpolate_without_default() {
        let mut interp = EnvInterpolator::new();
        interp.set_var("TOKEN", "secret123");
        let result = interp.interpolate("${TOKEN}").unwrap();
        assert_eq!(result, "secret123");
    }

    #[test]
    fn test_interpolate_undefined() {
        let interp = EnvInterpolator::new();
        let result = interp.interpolate("${UNDEFINED}");
        assert!(result.is_err());
    }
}

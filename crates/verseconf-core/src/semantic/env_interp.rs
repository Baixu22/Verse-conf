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

    /// 解析单个变量。
    ///
    /// 默认值分隔符以 `|` 为准（`${VAR|default}`）；为兼容历史示例同时接受
    /// ` or ` 与 `:`，但文档只推荐一种写法。
    fn resolve_variable(&self, content: &str) -> Result<String, InterpError> {
        if let Some((var_name, default_value)) = Self::split_default(content) {
            match self.env_vars.get(var_name.trim()) {
                Some(value) => Ok(value.clone()),
                None => Ok(default_value.trim().to_string()),
            }
        } else {
            self.env_vars
                .get(content.trim())
                .cloned()
                .ok_or_else(|| InterpError::UndefinedVariable(content.trim().to_string()))
        }
    }

    /// 拆分 `变量|默认值`（并兼容 ` or ` 与 `:`）
    fn split_default(content: &str) -> Option<(&str, &str)> {
        if let Some(parts) = content.split_once('|') {
            return Some(parts);
        }
        if let Some(parts) = content.split_once(" or ") {
            return Some(parts);
        }
        if let Some(parts) = content.split_once(':') {
            let name = parts.0;
            let is_identifier =
                !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if is_identifier {
                return Some(parts);
            }
        }
        None
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
    fn test_interpolate_legacy_separators() {
        let interp = EnvInterpolator::new();
        assert_eq!(
            interp.interpolate("${DB_HOST:localhost}").unwrap(),
            "localhost"
        );
        assert_eq!(
            interp.interpolate("${DB_HOST or localhost}").unwrap(),
            "localhost"
        );
        assert_eq!(
            interp.interpolate("${DB_HOST|localhost}").unwrap(),
            "localhost"
        );
    }

    #[test]
    fn test_interpolate_env_wins_over_default() {
        let mut interp = EnvInterpolator::new();
        interp.set_var("DB_HOST", "prod.db");
        assert_eq!(
            interp.interpolate("${DB_HOST:localhost}").unwrap(),
            "prod.db"
        );
    }

    #[test]
    fn test_interpolate_undefined() {
        let interp = EnvInterpolator::new();
        let result = interp.interpolate("${UNDEFINED}");
        assert!(result.is_err());
    }
}

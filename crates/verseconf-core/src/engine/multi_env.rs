use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ast::Ast;
use crate::parse;

/// 环境配置
#[derive(Debug, Clone)]
pub struct EnvironmentConfig {
    pub name: String,
    pub extends: Option<String>,
    pub content: String,
    pub ast: Option<Ast>,
}

/// 环境配置管理器
pub struct EnvironmentManager {
    environments: HashMap<String, EnvironmentConfig>,
    #[allow(dead_code)]
    base_dir: PathBuf,
}

impl EnvironmentManager {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            environments: HashMap::new(),
            base_dir,
        }
    }

    pub fn add_environment(&mut self, name: &str, content: &str, extends: Option<&str>) -> Result<(), String> {
        let ast = parse(content).ok();
        
        let config = EnvironmentConfig {
            name: name.to_string(),
            extends: extends.map(String::from),
            content: content.to_string(),
            ast,
        };
        
        self.environments.insert(name.to_string(), config);
        Ok(())
    }

    pub fn load_environment_from_file(&mut self, name: &str, file_path: &Path) -> Result<(), String> {
        let content = std::fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read file: {}", e))?;
        
        let ast = parse(&content).ok();
        
        let config = EnvironmentConfig {
            name: name.to_string(),
            extends: None,
            content,
            ast,
        };
        
        self.environments.insert(name.to_string(), config);
        Ok(())
    }

    pub fn resolve_environment(&self, name: &str) -> Result<String, String> {
        let config = self.environments.get(name)
            .ok_or_else(|| format!("Environment '{}' not found", name))?;
        
        let resolved = if let Some(ref parent_name) = config.extends {
            let parent_content = self.resolve_environment(parent_name)?;
            self.merge_configs(&parent_content, &config.content)
        } else {
            config.content.clone()
        };
        
        Ok(resolved)
    }

    pub fn resolve_environment_ast(&self, name: &str) -> Result<Ast, String> {
        let resolved_content = self.resolve_environment(name)?;
        parse(&resolved_content).map_err(|e| format!("Failed to parse resolved config: {}", e))
    }

    pub fn get_environment(&self, name: &str) -> Option<&EnvironmentConfig> {
        self.environments.get(name)
    }

    pub fn list_environments(&self) -> Vec<&str> {
        self.environments.keys().map(|k| k.as_str()).collect()
    }

    fn merge_configs(&self, base: &str, override_config: &str) -> String {
        let base_lines: Vec<&str> = base.lines().collect();
        let override_lines: Vec<&str> = override_config.lines().collect();
        
        let mut result_lines = base_lines.clone();
        
        for override_line in &override_lines {
            let override_line = override_line.trim();
            if override_line.is_empty() || override_line.starts_with('#') {
                continue;
            }
            
            let key = self.extract_key(override_line);
            if let Some(k) = key {
                let mut found = false;
                for (i, base_line) in base_lines.iter().enumerate() {
                    let base_key = self.extract_key(base_line.trim());
                    if let Some(bk) = base_key {
                        if bk == k {
                            result_lines[i] = override_line;
                            found = true;
                            break;
                        }
                    }
                }
                
                if !found {
                    result_lines.push(override_line);
                }
            } else {
                result_lines.push(override_line);
            }
        }
        
        result_lines.join("\n")
    }

    fn extract_key(&self, line: &str) -> Option<String> {
        if line.contains('=') {
            Some(line.split('=').next()?.trim().to_string())
        } else if line.contains('{') {
            Some(line.split('{').next()?.trim().to_string())
        } else {
            None
        }
    }
}

/// 多环境配置构建器
pub struct MultiEnvBuilder {
    base_config: String,
    environments: HashMap<String, String>,
    extends_map: HashMap<String, String>,
}

impl MultiEnvBuilder {
    pub fn new() -> Self {
        Self {
            base_config: String::new(),
            environments: HashMap::new(),
            extends_map: HashMap::new(),
        }
    }

    pub fn with_base(mut self, config: &str) -> Self {
        self.base_config = config.to_string();
        self
    }

    pub fn with_environment(mut self, name: &str, config: &str) -> Self {
        self.environments.insert(name.to_string(), config.to_string());
        self
    }

    pub fn extends(mut self, env: &str, parent: &str) -> Self {
        self.extends_map.insert(env.to_string(), parent.to_string());
        self
    }

    pub fn build(self, base_dir: PathBuf) -> Result<EnvironmentManager, String> {
        let mut manager = EnvironmentManager::new(base_dir);
        
        if !self.base_config.is_empty() {
            manager.add_environment("base", &self.base_config, None)?;
        }
        
        for (name, config) in &self.environments {
            let extends = self.extends_map.get(name).map(|s| s.as_str());
            manager.add_environment(name, config, extends)?;
        }
        
        Ok(manager)
    }
}

impl Default for MultiEnvBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_environment() {
        let base_config = r#"
app_name = "myapp"
port = 8080
debug = false
"#;
        
        let mut manager = EnvironmentManager::new(PathBuf::from("."));
        manager.add_environment("base", base_config, None).unwrap();
        
        let resolved = manager.resolve_environment("base").unwrap();
        assert!(resolved.contains("app_name = \"myapp\""));
        assert!(resolved.contains("port = 8080"));
    }

    #[test]
    fn test_environment_with_override() {
        let base_config = r#"
app_name = "myapp"
port = 8080
debug = false
"#;
        
        let dev_config = r#"
port = 3000
debug = true
"#;
        
        let mut manager = EnvironmentManager::new(PathBuf::from("."));
        manager.add_environment("base", base_config, None).unwrap();
        manager.add_environment("dev", dev_config, Some("base")).unwrap();
        
        let resolved = manager.resolve_environment("dev").unwrap();
        assert!(resolved.contains("app_name = \"myapp\""));
        assert!(resolved.contains("port = 3000"));
        assert!(resolved.contains("debug = true"));
    }

    #[test]
    fn test_multi_level_inheritance() {
        let base_config = r#"
app_name = "myapp"
port = 8080
debug = false
log_level = "info"
"#;
        
        let staging_config = r#"
port = 9090
log_level = "warn"
"#;
        
        let prod_config = r#"
port = 443
log_level = "error"
"#;
        
        let mut manager = EnvironmentManager::new(PathBuf::from("."));
        manager.add_environment("base", base_config, None).unwrap();
        manager.add_environment("staging", staging_config, Some("base")).unwrap();
        manager.add_environment("prod", prod_config, Some("staging")).unwrap();
        
        let resolved_prod = manager.resolve_environment("prod").unwrap();
        assert!(resolved_prod.contains("app_name = \"myapp\""));
        assert!(resolved_prod.contains("port = 443"));
        assert!(resolved_prod.contains("log_level = \"error\""));
        assert!(resolved_prod.contains("debug = false"));
    }

    #[test]
    fn test_environment_not_found() {
        let manager = EnvironmentManager::new(PathBuf::from("."));
        let result = manager.resolve_environment("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_environments() {
        let mut manager = EnvironmentManager::new(PathBuf::from("."));
        manager.add_environment("base", "key = \"value\"", None).unwrap();
        manager.add_environment("dev", "key = \"dev\"", Some("base")).unwrap();
        manager.add_environment("prod", "key = \"prod\"", Some("base")).unwrap();
        
        let envs = manager.list_environments();
        assert_eq!(envs.len(), 3);
        assert!(envs.contains(&"base"));
        assert!(envs.contains(&"dev"));
        assert!(envs.contains(&"prod"));
    }

    #[test]
    fn test_multi_env_builder() {
        let builder = MultiEnvBuilder::new()
            .with_base(r#"
app_name = "myapp"
port = 8080
"#)
            .with_environment("dev", r#"
port = 3000
debug = true
"#)
            .with_environment("prod", r#"
port = 443
debug = false
"#)
            .extends("dev", "base")
            .extends("prod", "base");
        
        let manager = builder.build(PathBuf::from(".")).unwrap();
        
        let dev_resolved = manager.resolve_environment("dev").unwrap();
        assert!(dev_resolved.contains("app_name = \"myapp\""));
        assert!(dev_resolved.contains("port = 3000"));
        assert!(dev_resolved.contains("debug = true"));
        
        let prod_resolved = manager.resolve_environment("prod").unwrap();
        assert!(prod_resolved.contains("app_name = \"myapp\""));
        assert!(prod_resolved.contains("port = 443"));
        assert!(prod_resolved.contains("debug = false"));
    }

    #[test]
    fn test_environment_with_new_fields() {
        let base_config = r#"
app_name = "myapp"
port = 8080
"#;
        
        let dev_config = r#"
port = 3000
debug = true
log_file = "/tmp/dev.log"
"#;
        
        let mut manager = EnvironmentManager::new(PathBuf::from("."));
        manager.add_environment("base", base_config, None).unwrap();
        manager.add_environment("dev", dev_config, Some("base")).unwrap();
        
        let resolved = manager.resolve_environment("dev").unwrap();
        assert!(resolved.contains("app_name = \"myapp\""));
        assert!(resolved.contains("port = 3000"));
        assert!(resolved.contains("debug = true"));
        assert!(resolved.contains("log_file = \"/tmp/dev.log\""));
    }

    #[test]
    fn test_resolve_environment_ast() {
        let base_config = r#"
app_name = "myapp"
port = 8080
"#;
        
        let dev_config = r#"
port = 3000
"#;
        
        let mut manager = EnvironmentManager::new(PathBuf::from("."));
        manager.add_environment("base", base_config, None).unwrap();
        manager.add_environment("dev", dev_config, Some("base")).unwrap();
        
        let ast = manager.resolve_environment_ast("dev").unwrap();
        assert!(!ast.root.entries.is_empty());
    }
}

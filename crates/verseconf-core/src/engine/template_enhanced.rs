use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::engine::template::{RenderContext, Template, TemplateError};

/// 模板块定义
#[derive(Debug, Clone)]
pub struct TemplateBlock {
    pub name: String,
    pub content: String,
}

/// 模板继承关系
#[derive(Debug, Clone)]
pub struct TemplateInheritance {
    pub base_template: String,
    pub blocks: HashMap<String, TemplateBlock>,
}

/// 增强模板（支持继承和组合）
#[derive(Debug, Clone)]
pub struct EnhancedTemplate {
    pub template: Template,
    pub inheritance: Option<TemplateInheritance>,
    pub includes: Vec<String>,
    pub blocks: HashMap<String, TemplateBlock>,
}

impl EnhancedTemplate {
    pub fn new(template: Template) -> Self {
        Self {
            template,
            inheritance: None,
            includes: Vec::new(),
            blocks: HashMap::new(),
        }
    }

    pub fn extends(mut self, base_template: impl Into<String>) -> Self {
        self.inheritance = Some(TemplateInheritance {
            base_template: base_template.into(),
            blocks: HashMap::new(),
        });
        self
    }

    pub fn add_block(mut self, name: impl Into<String>, content: impl Into<String>) -> Self {
        let block = TemplateBlock {
            name: name.into(),
            content: content.into(),
        };
        
        if let Some(ref mut inheritance) = self.inheritance {
            inheritance.blocks.insert(block.name.clone(), block);
        } else {
            self.blocks.insert(block.name.clone(), block);
        }
        
        self
    }

    pub fn include(mut self, template_path: impl Into<String>) -> Self {
        self.includes.push(template_path.into());
        self
    }
}

/// 模板注册表
pub struct TemplateRegistry {
    templates: HashMap<String, Template>,
    enhanced_templates: HashMap<String, EnhancedTemplate>,
    template_paths: HashMap<String, PathBuf>,
}

impl TemplateRegistry {
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
            enhanced_templates: HashMap::new(),
            template_paths: HashMap::new(),
        }
    }

    pub fn register(&mut self, template: Template) {
        self.templates.insert(template.name.clone(), template);
    }

    pub fn register_enhanced(&mut self, template: EnhancedTemplate) {
        self.enhanced_templates
            .insert(template.template.name.clone(), template);
    }

    pub fn register_from_file(&mut self, path: &Path) -> Result<(), String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read template file: {}", e))?;
        
        let file_name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        
        let template_name = if file_name.ends_with(".vcf") {
            file_name.trim_end_matches(".vcf").to_string()
        } else {
            file_name
        };
        
        let template = Template {
            name: template_name,
            description: None,
            version: "1.0".to_string(),
            variables: Vec::new(),
            content,
        };
        
        self.template_paths
            .insert(template.name.clone(), path.to_path_buf());
        self.register(template);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Template> {
        self.templates.get(name)
    }

    pub fn get_enhanced(&self, name: &str) -> Option<&EnhancedTemplate> {
        self.enhanced_templates.get(name)
    }

    pub fn render_enhanced(
        &self,
        name: &str,
        context: &RenderContext,
    ) -> Result<String, TemplateError> {
        if let Some(enhanced) = self.enhanced_templates.get(name) {
            self.render_enhanced_template(enhanced, context)
        } else if let Some(template) = self.templates.get(name) {
            crate::engine::template_renderer::render_template(template, context)
        } else {
            Err(TemplateError::ParseError(format!(
                "Template '{}' not found",
                name
            )))
        }
    }

    fn render_enhanced_template(
        &self,
        enhanced: &EnhancedTemplate,
        context: &RenderContext,
    ) -> Result<String, TemplateError> {
        let mut result = if let Some(ref inheritance) = enhanced.inheritance {
            let base = self
                .templates
                .get(&inheritance.base_template)
                .ok_or_else(|| {
                    TemplateError::ParseError(format!(
                        "Base template '{}' not found",
                        inheritance.base_template
                    ))
                })?;
            
            let mut base_content = base.content.clone();
            
            for (block_name, block) in &inheritance.blocks {
                let placeholder = format!("{{% block {} %}}", block_name);
                base_content = base_content.replace(&placeholder, &block.content);
            }
            
            base_content
        } else {
            enhanced.template.content.clone()
        };
        
        for include_path in &enhanced.includes {
            if let Some(include_template) = self.templates.get(include_path) {
                let include_content =
                    crate::engine::template_renderer::render_template(include_template, context)?;
                let placeholder = format!("{{% include {} %}}", include_path);
                result = result.replace(&placeholder, &include_content);
            }
        }
        
        let re = regex::Regex::new(r"\{\{([^}]+)\}\}").unwrap();
        let replacements: Vec<_> = re
            .captures_iter(&result)
            .map(|cap| {
                let full_match = cap.get(0).unwrap().as_str().to_string();
                let var_name = cap.get(1).unwrap().as_str().trim().to_string();
                (full_match, var_name)
            })
            .collect();
        
        for (full_match, var_name) in replacements {
            let replacement = if let Some(value) = context.values.get(&var_name) {
                value.clone()
            } else if context.strict {
                return Err(TemplateError::UndefinedVariable(var_name));
            } else {
                format!("{{{{{}}}}}", var_name)
            };
            result = result.replace(&full_match, &replacement);
        }
        
        Ok(result)
    }
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::template::{TemplateVariable, VariableType};

    fn create_base_template() -> Template {
        Template {
            name: "base".to_string(),
            description: Some("Base template".to_string()),
            version: "1.0".to_string(),
            variables: vec![TemplateVariable {
                name: "app_name".to_string(),
                var_type: VariableType::String,
                default: Some("MyApp".to_string()),
                description: None,
                required: false,
                choices: None,
            }],
            content: r#"app_name = "{{app_name}}"

{% block server %}
host = "localhost"
port = 8080
{% endblock %}

{% block database %}
db_host = "localhost"
db_port = 5432
{% endblock %}"#
            .to_string(),
        }
    }

    fn create_server_template() -> Template {
        Template {
            name: "server_config".to_string(),
            description: None,
            version: "1.0".to_string(),
            variables: Vec::new(),
            content: r#"server {
    host = "{{host}}"
    port = {{port}}
}"#
            .to_string(),
        }
    }

    #[test]
    fn test_enhanced_template_inheritance() {
        let mut registry = TemplateRegistry::new();
        registry.register(create_base_template());
        
        let base = create_base_template();
        let enhanced = EnhancedTemplate::new(Template {
            name: "production".to_string(),
            description: None,
            version: "1.0".to_string(),
            variables: Vec::new(),
            content: String::new(),
        })
        .extends("base")
        .add_block(
            "server",
            r#"host = "prod.example.com"
port = 443"#,
        )
        .add_block(
            "database",
            r#"db_host = "db.prod.example.com"
db_port = 5432"#,
        );
        
        registry.register_enhanced(enhanced);
        
        let context = RenderContext::new().with_value("app_name", "ProductionApp");
        let result = registry.render_enhanced("production", &context).unwrap();
        
        assert!(result.contains("app_name = \"ProductionApp\""));
        assert!(result.contains("host = \"prod.example.com\""));
        assert!(result.contains("port = 443"));
        assert!(result.contains("db_host = \"db.prod.example.com\""));
    }

    #[test]
    fn test_enhanced_template_include() {
        let mut registry = TemplateRegistry::new();
        registry.register(create_server_template());
        
        let main_template = Template {
            name: "main".to_string(),
            description: None,
            version: "1.0".to_string(),
            variables: Vec::new(),
            content: r#"# Main configuration
{% include server_config %}

logging {
    level = "{{log_level}}"
}"#
            .to_string(),
        };
        
        let enhanced = EnhancedTemplate::new(main_template).include("server_config");
        registry.register_enhanced(enhanced);
        
        let context = RenderContext::new()
            .with_value("host", "example.com")
            .with_value("port", "8080")
            .with_value("log_level", "info");
        
        let result = registry.render_enhanced("main", &context).unwrap();
        
        assert!(result.contains("host = \"example.com\""));
        assert!(result.contains("port = 8080"));
        assert!(result.contains("level = \"info\""));
    }

    #[test]
    fn test_template_registry_from_file() {
        let test_dir = std::env::temp_dir().join("verseconf_template_test");
        let _ = std::fs::remove_dir_all(&test_dir);
        std::fs::create_dir_all(&test_dir).unwrap();
        
        let template_file = test_dir.join("test.vcf.tpl");
        std::fs::write(&template_file, "name = \"{{name}}\"\nversion = \"{{version}}\"").unwrap();
        
        let mut registry = TemplateRegistry::new();
        registry.register_from_file(&template_file).unwrap();
        
        assert!(registry.get("test").is_some());
        
        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_enhanced_template_missing_base() {
        let registry = TemplateRegistry::new();
        
        let enhanced = EnhancedTemplate::new(Template {
            name: "child".to_string(),
            description: None,
            version: "1.0".to_string(),
            variables: Vec::new(),
            content: String::new(),
        })
        .extends("nonexistent_base");
        
        let mut registry = registry;
        registry.register_enhanced(enhanced);
        
        let context = RenderContext::new();
        let result = registry.render_enhanced("child", &context);
        
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_includes() {
        let mut registry = TemplateRegistry::new();
        
        let db_template = Template {
            name: "database".to_string(),
            description: None,
            version: "1.0".to_string(),
            variables: Vec::new(),
            content: r#"database {
    host = "{{db_host}}"
    port = {{db_port}}
}"#
            .to_string(),
        };
        
        let cache_template = Template {
            name: "cache".to_string(),
            description: None,
            version: "1.0".to_string(),
            variables: Vec::new(),
            content: r#"cache {
    host = "{{cache_host}}"
    port = {{cache_port}}
}"#
            .to_string(),
        };
        
        registry.register(db_template);
        registry.register(cache_template);
        
        let main_template = Template {
            name: "full_config".to_string(),
            description: None,
            version: "1.0".to_string(),
            variables: Vec::new(),
            content: r#"{% include database %}
{% include cache %}"#
                .to_string(),
        };
        
        let enhanced = EnhancedTemplate::new(main_template)
            .include("database")
            .include("cache");
        
        registry.register_enhanced(enhanced);
        
        let context = RenderContext::new()
            .with_value("db_host", "db.example.com")
            .with_value("db_port", "5432")
            .with_value("cache_host", "cache.example.com")
            .with_value("cache_port", "6379");
        
        let result = registry.render_enhanced("full_config", &context).unwrap();
        
        assert!(result.contains("host = \"db.example.com\""));
        assert!(result.contains("port = 5432"));
        assert!(result.contains("host = \"cache.example.com\""));
        assert!(result.contains("port = 6379"));
    }
}

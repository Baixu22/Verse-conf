use regex::Regex;

use super::template::{RenderContext, Template, TemplateError};

/// 渲染模板
pub fn render_template(template: &Template, context: &RenderContext) -> Result<String, TemplateError> {
    let mut result = template.content.clone();
    
    let re = Regex::new(r"\{\{([^}]+)\}\}").unwrap();
    
    let replacements: Vec<_> = re.captures_iter(&result)
        .map(|cap| {
            let full_match = cap.get(0).unwrap().as_str().to_string();
            let var_name = cap.get(1).unwrap().as_str().trim().to_string();
            (full_match, var_name)
        })
        .collect();
    
    for (full_match, var_name) in replacements {
        let replacement = resolve_variable(&var_name, context)?;
        result = result.replace(&full_match, &replacement);
    }
    
    Ok(result)
}

/// 解析变量值
fn resolve_variable(var_name: &str, context: &RenderContext) -> Result<String, TemplateError> {
    if let Some(value) = context.values.get(var_name) {
        Ok(value.clone())
    } else if context.strict {
        Err(TemplateError::UndefinedVariable(var_name.to_string()))
    } else {
        Ok(format!("{{{{{}}}}}", var_name))
    }
}

/// 从模板内容中提取变量引用
pub fn extract_variables(content: &str) -> Vec<String> {
    let re = Regex::new(r"\{\{([^}]+)\}\}").unwrap();
    re.captures_iter(content)
        .map(|cap| cap.get(1).unwrap().as_str().trim().to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect()
}

/// 合并多个上下文
pub fn merge_contexts(contexts: &[RenderContext]) -> RenderContext {
    let mut merged = RenderContext::new();
    for ctx in contexts {
        merged.values.extend(ctx.values.clone());
        merged.strict = ctx.strict;
    }
    merged
}

/// 从环境变量填充上下文
pub fn context_from_env(vars: &[(&str, &str)]) -> RenderContext {
    let mut ctx = RenderContext::new();
    for (key, value) in vars {
        if let Ok(val) = std::env::var(key) {
            ctx.values.insert(value.to_string(), val);
        }
    }
    ctx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::template::{TemplateVariable, VariableType};

    fn create_test_template() -> Template {
        Template {
            name: "test".to_string(),
            description: Some("Test template".to_string()),
            version: "1.0".to_string(),
            variables: vec![
                TemplateVariable {
                    name: "host".to_string(),
                    var_type: VariableType::Hostname,
                    default: Some("localhost".to_string()),
                    description: Some("Server hostname".to_string()),
                    required: false,
                    choices: None,
                },
                TemplateVariable {
                    name: "port".to_string(),
                    var_type: VariableType::Port,
                    default: Some("8080".to_string()),
                    description: Some("Server port".to_string()),
                    required: true,
                    choices: None,
                },
            ],
            content: r#"server {
    host = "{{host}}"
    port = {{port}}
}"#
            .to_string(),
        }
    }

    #[test]
    fn test_render_template_basic() {
        let template = create_test_template();
        let context = RenderContext::new()
            .with_value("host", "example.com")
            .with_value("port", "3000");
        
        let result = render_template(&template, &context).unwrap();
        assert!(result.contains("host = \"example.com\""));
        assert!(result.contains("port = 3000"));
    }

    #[test]
    fn test_render_template_undefined_strict() {
        let template = create_test_template();
        let context = RenderContext::new();
        
        let result = render_template(&template, &context);
        assert!(result.is_err());
    }

    #[test]
    fn test_render_template_non_strict() {
        let template = create_test_template();
        let context = RenderContext::new().strict_mode(false);
        
        let result = render_template(&template, &context).unwrap();
        assert!(result.contains("{{host}}"));
        assert!(result.contains("{{port}}"));
    }

    #[test]
    fn test_extract_variables() {
        let content = r#"host = "{{host}}"
port = {{port}}
name = "{{name}}""#;
        
        let vars = extract_variables(content);
        assert_eq!(vars.len(), 3);
        assert!(vars.contains(&"host".to_string()));
        assert!(vars.contains(&"port".to_string()));
        assert!(vars.contains(&"name".to_string()));
    }

    #[test]
    fn test_merge_contexts() {
        let ctx1 = RenderContext::new().with_value("a", "1");
        let ctx2 = RenderContext::new().with_value("b", "2");
        
        let merged = merge_contexts(&[ctx1, ctx2]);
        assert_eq!(merged.values.get("a").unwrap(), "1");
        assert_eq!(merged.values.get("b").unwrap(), "2");
    }
}

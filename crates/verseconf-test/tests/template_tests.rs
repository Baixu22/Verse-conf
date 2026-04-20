use verseconf_core::{parse_template, render_template, RenderContext, Template, TemplateVariable, VariableType};

fn create_test_template() -> Template {
    Template {
        name: "test_server".to_string(),
        description: Some("Test server template".to_string()),
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
            TemplateVariable {
                name: "debug".to_string(),
                var_type: VariableType::Boolean,
                default: Some("false".to_string()),
                description: Some("Enable debug mode".to_string()),
                required: false,
                choices: None,
            },
        ],
        content: r#"server {
    host = "{{host}}"
    port = {{port}}
    debug = {{debug}}
}"#
        .to_string(),
    }
}

#[test]
fn test_parse_template_basic() {
    let content = r#"server {
    host = "{{host}}"
    port = {{port}}
}"#;
    let template = parse_template(content).unwrap();
    assert_eq!(template.variables.len(), 2);
    assert!(template.variables.iter().any(|v| v.name == "host"));
    assert!(template.variables.iter().any(|v| v.name == "port"));
}

#[test]
fn test_render_template_basic() {
    let template = create_test_template();
    let context = RenderContext::new()
        .with_value("host", "example.com")
        .with_value("port", "3000")
        .with_value("debug", "true");
    
    let result = render_template(&template, &context).unwrap();
    assert!(result.contains("host = \"example.com\""));
    assert!(result.contains("port = 3000"));
    assert!(result.contains("debug = true"));
}

#[test]
fn test_render_template_with_defaults() {
    let template = create_test_template();
    let context = RenderContext::new()
        .with_value("port", "8080");
    
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
fn test_render_template_complex() {
    let content = r#"server {
    host = "{{HOST}}"
    port = {{PORT}}
    
    database {
        host = "{{DB_HOST}}"
        port = {{DB_PORT}}
        name = "{{DB_NAME}}"
    }
    
    logging {
        level = "{{LOG_LEVEL}}"
    }
}"#;
    
    let template = parse_template(content).unwrap();
    assert_eq!(template.variables.len(), 6);
    
    let context = RenderContext::new()
        .with_value("HOST", "0.0.0.0")
        .with_value("PORT", "8080")
        .with_value("DB_HOST", "localhost")
        .with_value("DB_PORT", "5432")
        .with_value("DB_NAME", "mydb")
        .with_value("LOG_LEVEL", "info");
    
    let result = render_template(&template, &context).unwrap();
    assert!(result.contains("host = \"0.0.0.0\""));
    assert!(result.contains("port = 8080"));
    assert!(result.contains("host = \"localhost\""));
    assert!(result.contains("port = 5432"));
    assert!(result.contains("name = \"mydb\""));
    assert!(result.contains("level = \"info\""));
}

#[test]
fn test_template_variable_validation() {
    let template = create_test_template();
    
    let context = RenderContext::new()
        .with_value("host", "localhost")
        .with_value("port", "invalid")
        .with_value("debug", "false");
    
    let result = render_template(&template, &context);
    assert!(result.is_ok());
    let rendered = result.unwrap();
    assert!(rendered.contains("port = invalid"));
}

#[test]
fn test_parse_template_multiple_occurrences() {
    let content = r#"server {
    host = "{{host}}"
    backup_host = "{{host}}"
}"#;
    
    let template = parse_template(content).unwrap();
    assert_eq!(template.variables.len(), 1);
    assert_eq!(template.variables[0].name, "host");
}

#[test]
fn test_render_template_empty_value() {
    let template = create_test_template();
    let context = RenderContext::new()
        .with_value("host", "")
        .with_value("port", "8080")
        .with_value("debug", "false");
    
    let result = render_template(&template, &context).unwrap();
    assert!(result.contains("host = \"\""));
}

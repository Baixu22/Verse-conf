use verseconf_core::{parse, AdvancedRule, AdvancedValidationRules, FieldRelation};

#[test]
fn test_regex_validation_valid() {
    let source = r#"email = "test@example.com""#;
    let ast = parse(source).unwrap();
    
    let mut rules = AdvancedValidationRules::new();
    rules.add_rule(AdvancedRule::Regex {
        field: "email".to_string(),
        pattern: r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$".to_string(),
        error_message: None,
    });
    
    let errors = rules.validate(&ast);
    assert!(errors.is_empty());
}

#[test]
fn test_regex_validation_invalid() {
    let source = r#"email = "invalid-email""#;
    let ast = parse(source).unwrap();
    
    let mut rules = AdvancedValidationRules::new();
    rules.add_rule(AdvancedRule::Regex {
        field: "email".to_string(),
        pattern: r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$".to_string(),
        error_message: None,
    });
    
    let errors = rules.validate(&ast);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("does not match pattern"));
}

#[test]
fn test_dependency_validation_valid() {
    let source = r#"username = "john"
password = "secret123""#;
    let ast = parse(source).unwrap();
    
    let mut rules = AdvancedValidationRules::new();
    rules.add_rule(AdvancedRule::Dependency {
        field: "username".to_string(),
        required_fields: vec!["password".to_string()],
        error_message: None,
    });
    
    let errors = rules.validate(&ast);
    assert!(errors.is_empty());
}

#[test]
fn test_dependency_validation_invalid() {
    let source = r#"username = "john""#;
    let ast = parse(source).unwrap();
    
    let mut rules = AdvancedValidationRules::new();
    rules.add_rule(AdvancedRule::Dependency {
        field: "username".to_string(),
        required_fields: vec!["password".to_string()],
        error_message: None,
    });
    
    let errors = rules.validate(&ast);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("requires field"));
}

#[test]
fn test_cross_field_greater_than() {
    let source = r#"min_connections = 5
max_connections = 20"#;
    let ast = parse(source).unwrap();
    
    let mut rules = AdvancedValidationRules::new();
    rules.add_rule(AdvancedRule::CrossField {
        field_a: "max_connections".to_string(),
        field_b: "min_connections".to_string(),
        relation: FieldRelation::GreaterThan,
        error_message: None,
    });
    
    let errors = rules.validate(&ast);
    assert!(errors.is_empty());
}

#[test]
fn test_cross_field_less_than_invalid() {
    let source = r#"min_connections = 20
max_connections = 5"#;
    let ast = parse(source).unwrap();
    
    let mut rules = AdvancedValidationRules::new();
    rules.add_rule(AdvancedRule::CrossField {
        field_a: "max_connections".to_string(),
        field_b: "min_connections".to_string(),
        relation: FieldRelation::GreaterThan,
        error_message: None,
    });
    
    let errors = rules.validate(&ast);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("must be greater than"));
}

#[test]
fn test_cross_field_equal() {
    let source = r#"port = 8080
backup_port = 8080"#;
    let ast = parse(source).unwrap();
    
    let mut rules = AdvancedValidationRules::new();
    rules.add_rule(AdvancedRule::CrossField {
        field_a: "port".to_string(),
        field_b: "backup_port".to_string(),
        relation: FieldRelation::Equal,
        error_message: None,
    });
    
    let errors = rules.validate(&ast);
    assert!(errors.is_empty());
}

#[test]
fn test_cross_field_not_equal() {
    let source = r#"port = 8080
backup_port = 9090"#;
    let ast = parse(source).unwrap();
    
    let mut rules = AdvancedValidationRules::new();
    rules.add_rule(AdvancedRule::CrossField {
        field_a: "port".to_string(),
        field_b: "backup_port".to_string(),
        relation: FieldRelation::NotEqual,
        error_message: None,
    });
    
    let errors = rules.validate(&ast);
    assert!(errors.is_empty());
}

#[test]
fn test_nested_field_validation() {
    let source = r#"server {
    host = "localhost"
    port = 8080
}"#;
    let ast = parse(source).unwrap();
    
    let mut rules = AdvancedValidationRules::new();
    rules.add_rule(AdvancedRule::Regex {
        field: "server.host".to_string(),
        pattern: r"^[a-zA-Z0-9.-]+$".to_string(),
        error_message: None,
    });
    
    let errors = rules.validate(&ast);
    assert!(errors.is_empty());
}

#[test]
fn test_multiple_rules() {
    let source = r#"email = "test@example.com"
username = "john"
password = "secret123"
min_port = 1024
max_port = 65535"#;
    let ast = parse(source).unwrap();
    
    let mut rules = AdvancedValidationRules::new();
    rules.add_rule(AdvancedRule::Regex {
        field: "email".to_string(),
        pattern: r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$".to_string(),
        error_message: None,
    });
    rules.add_rule(AdvancedRule::Dependency {
        field: "username".to_string(),
        required_fields: vec!["password".to_string()],
        error_message: None,
    });
    rules.add_rule(AdvancedRule::CrossField {
        field_a: "max_port".to_string(),
        field_b: "min_port".to_string(),
        relation: FieldRelation::GreaterThan,
        error_message: None,
    });
    
    let errors = rules.validate(&ast);
    assert!(errors.is_empty());
}

#[test]
fn test_regex_custom_error_message() {
    let source = r#"hostname = "invalid host!""#;
    let ast = parse(source).unwrap();
    
    let mut rules = AdvancedValidationRules::new();
    rules.add_rule(AdvancedRule::Regex {
        field: "hostname".to_string(),
        pattern: r"^[a-zA-Z0-9.-]+$".to_string(),
        error_message: Some("Hostname must contain only alphanumeric characters, dots, and hyphens".to_string()),
    });
    
    let errors = rules.validate(&ast);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("Hostname must contain only"));
}

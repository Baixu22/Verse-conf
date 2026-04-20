use verseconf_core::{diff_sources, AstDiffer, DiffFormat, DiffFormatter, DiffType};

#[test]
fn test_diff_added_key() {
    let old = r#"name = "test""#;
    let new = r#"name = "test"
port = 8080"#;
    
    let diff = diff_sources(old, new).unwrap();
    assert!(diff.entries.iter().any(|e| e.path == "port" && e.diff_type == DiffType::Added));
}

#[test]
fn test_diff_removed_key() {
    let old = r#"name = "test"
port = 8080"#;
    let new = r#"name = "test""#;
    
    let diff = diff_sources(old, new).unwrap();
    assert!(diff.entries.iter().any(|e| e.path == "port" && e.diff_type == DiffType::Removed));
}

#[test]
fn test_diff_modified_value() {
    let old = r#"port = 8080"#;
    let new = r#"port = 9090"#;
    
    let diff = diff_sources(old, new).unwrap();
    assert!(diff.entries.iter().any(|e| {
        e.path == "port" && 
        e.diff_type == DiffType::Modified &&
        e.old_value == Some("8080".to_string()) &&
        e.new_value == Some("9090".to_string())
    }));
}

#[test]
fn test_diff_nested_tables() {
    let old = r#"server {
    host = "localhost"
    port = 8080
}"#;
    let new = r#"server {
    host = "0.0.0.0"
    port = 8080
}"#;
    
    let diff = diff_sources(old, new).unwrap();
    assert!(diff.entries.iter().any(|e| {
        e.path == "server.host" && 
        e.diff_type == DiffType::Modified
    }));
}

#[test]
fn test_diff_stats() {
    let old = r#"a = 1
b = 2
c = 3"#;
    let new = r#"a = 1
b = 20
d = 4"#;
    
    let diff = diff_sources(old, new).unwrap();
    let stats = diff.stats();
    
    assert_eq!(stats.added, 1);
    assert_eq!(stats.removed, 1);
    assert_eq!(stats.modified, 1);
    assert_eq!(stats.unchanged, 1);
}

#[test]
fn test_diff_format_text() {
    let old = r#"name = "test"
port = 8080"#;
    let new = r#"name = "test"
port = 9090
host = "localhost""#;
    
    let diff = diff_sources(old, new).unwrap();
    let output = DiffFormatter::format(&diff, DiffFormat::Text);
    
    assert!(output.contains("+ host:"));
    assert!(output.contains("~ port:"));
    assert!(output.contains("added"));
    assert!(output.contains("modified"));
}

#[test]
fn test_diff_format_json() {
    let old = r#"name = "test"
port = 8080"#;
    let new = r#"name = "test"
port = 9090
host = "localhost""#;
    
    let diff = diff_sources(old, new).unwrap();
    let output = DiffFormatter::format(&diff, DiffFormat::Json);
    
    assert!(output.contains("\"type\":\"added\""));
    assert!(output.contains("\"type\":\"modified\""));
    assert!(output.contains("\"path\":\"host\""));
}

#[test]
fn test_diff_format_markdown() {
    let old = r#"name = "test"
port = 8080"#;
    let new = r#"name = "test"
port = 9090
host = "localhost""#;
    
    let diff = diff_sources(old, new).unwrap();
    let output = DiffFormatter::format(&diff, DiffFormat::Markdown);
    
    assert!(output.contains("# Configuration Diff"));
    assert!(output.contains("| Type | Path |"));
    assert!(output.contains("➕ Added"));
    assert!(output.contains("✏️ Modified"));
}

#[test]
fn test_diff_empty_files() {
    let old = "";
    let new = r#"name = "test""#;
    
    let diff = diff_sources(old, new).unwrap();
    assert!(diff.entries.iter().any(|e| e.diff_type == DiffType::Added));
}

#[test]
fn test_diff_identical_files() {
    let content = r#"name = "test"
port = 8080"#;
    
    let diff = diff_sources(content, content).unwrap();
    assert!(diff.entries.iter().all(|e| e.diff_type == DiffType::Unchanged));
}

#[test]
fn test_diff_complex_structure() {
    let old = r#"server {
    host = "localhost"
    port = 8080
    
    database {
        host = "localhost"
        port = 5432
    }
}"#;
    let new = r#"server {
    host = "0.0.0.0"
    port = 9090
    
    database {
        host = "db.example.com"
        port = 5432
    }
}"#;
    
    let diff = diff_sources(old, new).unwrap();
    let stats = diff.stats();
    
    assert_eq!(stats.modified, 3);
    assert!(diff.entries.iter().any(|e| e.path == "server.host"));
    assert!(diff.entries.iter().any(|e| e.path == "server.port"));
    assert!(diff.entries.iter().any(|e| e.path == "server.database.host"));
}

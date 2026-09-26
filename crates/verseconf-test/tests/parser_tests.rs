use verseconf_core::{
    format_with_config, parse, parse_and_validate, parse_with_config, ParseConfig,
    PrettyPrintConfig,
};

#[test]
fn test_parse_basic() {
    let source = r#"
name = "MyApp"
version = "1.0.0"
"#;
    let result = parse(source);
    assert!(result.is_ok());
}

#[test]
fn test_parse_with_table() {
    let source = r#"
database {
    host = "localhost"
    port = 5432
}
"#;
    let result = parse(source);
    assert!(result.is_ok());
}

#[test]
fn test_parse_with_array() {
    let source = r#"
features = [
    "auth",
    "logging",
    "monitoring",
]
"#;
    let result = parse(source);
    assert!(result.is_ok());
}

#[test]
fn test_parse_with_metadata() {
    let source = r#"
port = 8080 #@ range(1024..65535), description="Server port"
password = "secret" #@ sensitive, required
"#;
    let result = parse(source);
    assert!(result.is_ok());
}

#[test]
fn test_parse_with_include() {
    let source = r#"
@include "common/base.vcf"

app {
    name = "test"
}
"#;
    let result = parse(source);
    if let Err(e) = &result {
        eprintln!("Parse error (include): {:?}", e);
    }
    assert!(result.is_ok());
}

#[test]
fn test_parse_with_duration() {
    let source = r#"
timeout = 60s
interval = 5m
"#;
    let result = parse(source);
    assert!(result.is_ok());
}

#[test]
fn test_parse_with_boolean() {
    let source = r#"
enabled = true
debug = false
"#;
    let result = parse(source);
    assert!(result.is_ok());
}

#[test]
fn test_parse_nested_tables() {
    let source = r#"
server {
    host = "0.0.0.0"
    port = 8080
    
    ssl {
        enabled = true
        cert = "/path/to/cert.pem"
    }
}
"#;
    let result = parse(source);
    assert!(result.is_ok());
}

#[test]
fn test_parse_array_table() {
    let source = r#"
[[servers]]
name = "primary"
ip = "10.0.0.1"

[[servers]]
name = "secondary"
ip = "10.0.0.2"
"#;
    let result = parse(source);
    if let Err(e) = &result {
        eprintln!("Parse error (array_table): {:?}", e);
    }
    assert!(result.is_ok());
}

#[test]
fn test_validate_range() {
    let source = r#"
port = 8080 #@ range(1024..65535)
"#;
    let result = parse_and_validate(source);
    assert!(result.is_ok());
}

#[test]
fn test_validate_range_invalid() {
    let source = r#"
port = 80 #@ range(1024..65535)
"#;
    let result = parse_and_validate(source);
    assert!(result.is_err());
}

#[test]
fn test_parse_expression_addition() {
    let source = r#"
result = 10 + 5
"#;
    let result = parse(source);
    assert!(result.is_ok());
}

#[test]
fn test_parse_expression_duration_math() {
    let source = r#"
timeout = 1h + 30m
interval = 5m * 2
"#;
    let result = parse(source);
    assert!(result.is_ok());
}

#[test]
fn test_parse_datetime() {
    let source = r#"
created = 2024-01-15T10:30:00Z
updated = 2024-01-15T10:30:00+08:00
local = 2024-01-15T10:30:00
"#;
    let result = parse(source);
    if let Err(e) = &result {
        eprintln!("Parse error (datetime): {:?}", e);
    }
    assert!(result.is_ok());
}

#[test]
fn test_parse_include_with_merge() {
    let source = r#"
@include "base.vcf" merge=deep_merge

app {
    name = "test"
}
"#;
    let result = parse(source);
    if let Err(e) = &result {
        eprintln!("Parse error (include with merge): {:?}", e);
    }
    assert!(result.is_ok());
}

#[test]
fn test_parse_config_tolerant_mode() {
    let source = r#"
name = "test"
port = 8080
"#;
    let config = ParseConfig {
        tolerant: true,
        collect_warnings: true,
    };
    let result = parse_with_config(source, config);
    assert!(result.is_ok());
    let parse_result = result.unwrap();
    assert_eq!(parse_result.value.root.entries.len(), 2);
}

/// 回归：`tolerant: true` 曾经是个静默空操作（apply_tolerant_fixes 原样返回入参），
/// 于是宽容模式永远产生不了任何警告，调用方无法区分它与严格模式。
#[test]
fn test_tolerant_mode_reports_duplicate_keys() {
    let source = "port = 8080
port = 9090
";
    let config = ParseConfig {
        tolerant: true,
        collect_warnings: true,
    };
    let result = parse_with_config(source, config).expect("宽容模式应能解析");
    assert!(
        result.has_warnings(),
        "重复键在宽容模式下必须产生警告，而不是静默通过"
    );
    assert!(
        result.warnings.iter().any(|w| w.message.contains("port")),
        "警告里应指出重复的键名，实际：{:?}",
        result
            .warnings
            .iter()
            .map(|w| &w.message)
            .collect::<Vec<_>>()
    );
}

/// 回归：空文档在宽容模式下应给出警告而不是悄悄返回空 AST。
#[test]
fn test_tolerant_mode_reports_empty_document() {
    let config = ParseConfig {
        tolerant: true,
        collect_warnings: true,
    };
    let result = parse_with_config(
        "   

", config,
    )
    .expect("空文档应能解析");
    assert!(result.has_warnings(), "空文档在宽容模式下必须产生警告");
}

/// 严格模式（tolerant=false）不应产生这些警告，证明两条路径确实不同。
#[test]
fn test_strict_mode_does_not_emit_tolerant_warnings() {
    let source = "port = 8080
port = 9090
";
    let result = parse_with_config(source, ParseConfig::default()).expect("应能解析");
    assert!(!result.has_warnings(), "严格模式不应产生宽容模式才有的警告");
}

#[test]
fn test_parse_config_default() {
    let source = r#"
name = "test"
"#;
    let config = ParseConfig::default();
    let result = parse_with_config(source, config);
    assert!(result.is_ok());
}

#[test]
fn test_roundtrip_basic() {
    let source = r#"name = "test"
port = 8080
database {
    host = "localhost"
    port = 5432
}
"#;
    let ast = parse(source).unwrap();
    let formatted = verseconf_core::pretty_print(&ast);
    let ast2 = parse(&formatted).unwrap();
    let formatted2 = verseconf_core::pretty_print(&ast2);
    assert_eq!(formatted, formatted2);
}

#[test]
fn test_roundtrip_with_array() {
    let source = r#"features = ["auth", "logging"]
timeout = 60s
"#;
    let ast = parse(source).unwrap();
    let formatted = verseconf_core::pretty_print(&ast);
    let ast2 = parse(&formatted);
    if let Err(ref e) = ast2 {
        eprintln!("Source:\n{}", source);
        eprintln!("Formatted:\n{}", formatted);
        eprintln!("Error: {:?}", e);
    }
    let ast2 = ast2.unwrap();
    let formatted2 = verseconf_core::pretty_print(&ast2);
    assert_eq!(formatted, formatted2);
}

#[test]
fn test_roundtrip_with_metadata() {
    let source = r#"port = 8080 #@ range(1024..65535)
name = "test" #@ description="App name"
"#;
    let ast = parse(source).unwrap();
    let formatted = verseconf_core::pretty_print(&ast);
    let ast2 = parse(&formatted).unwrap();
    let formatted2 = verseconf_core::pretty_print(&ast2);
    assert_eq!(formatted, formatted2);
}

#[test]
fn test_ai_canonical_sorts_keys() {
    let source = r#"zebra = "z"
apple = "a"
mango = "m"
"#;
    let config = PrettyPrintConfig::ai_canonical();
    let formatted = format_with_config(source, config).unwrap();
    assert!(formatted.find("apple").unwrap() < formatted.find("mango").unwrap());
    assert!(formatted.find("mango").unwrap() < formatted.find("zebra").unwrap());
}

#[test]
fn test_ai_canonical_nested_tables() {
    let source = r#"server {
    port = 8080
    host = "localhost"
}
app {
    name = "test"
}
"#;
    let config = PrettyPrintConfig::ai_canonical();
    let formatted = format_with_config(source, config).unwrap();
    assert!(formatted.find("app").unwrap() < formatted.find("server").unwrap());
}

#[test]
fn test_ai_canonical_deterministic() {
    let source = r#"z = 1
a = 2
m = 3
"#;
    let config = PrettyPrintConfig::ai_canonical();
    let formatted1 = format_with_config(source, config.clone()).unwrap();
    let formatted2 = format_with_config(source, config).unwrap();
    assert_eq!(formatted1, formatted2);
}

#[test]
fn test_non_canonical_preserves_order() {
    let source = r#"zebra = "z"
apple = "a"
mango = "m"
"#;
    let config = PrettyPrintConfig::default();
    let formatted = format_with_config(source, config).unwrap();
    assert!(formatted.find("zebra").unwrap() < formatted.find("apple").unwrap());
}

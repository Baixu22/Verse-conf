use std::collections::HashMap;

use crate::ast::*;
use super::diff::{DiffEntry, DiffResult};

/// AST 差异比较器
pub struct AstDiffer;

impl AstDiffer {
    /// 比较两个 AST 并生成差异
    pub fn diff(old_ast: &Ast, new_ast: &Ast) -> DiffResult {
        let mut result = DiffResult::new();
        
        let old_map = flatten_table(&old_ast.root, "");
        let new_map = flatten_table(&new_ast.root, "");
        
        let all_keys: std::collections::HashSet<_> = old_map.keys()
            .chain(new_map.keys())
            .cloned()
            .collect();
        
        for key in all_keys {
            match (old_map.get(&key), new_map.get(&key)) {
                (Some(old_val), Some(new_val)) => {
                    if old_val != new_val {
                        result.add_entry(DiffEntry::modified(&key, old_val, new_val));
                    } else {
                        result.add_entry(DiffEntry {
                            diff_type: super::diff::DiffType::Unchanged,
                            path: key,
                            old_value: Some(old_val.clone()),
                            new_value: Some(new_val.clone()),
                            line: None,
                        });
                    }
                }
                (Some(old_val), None) => {
                    result.add_entry(DiffEntry::removed(&key, old_val));
                }
                (None, Some(new_val)) => {
                    result.add_entry(DiffEntry::added(&key, new_val));
                }
                (None, None) => {}
            }
        }
        
        result
    }
}

/// 将 TableBlock 展平为键值对映射
fn flatten_table(table: &TableBlock, prefix: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    
    for entry in &table.entries {
        match entry {
            TableEntry::KeyValue(kv) => {
                let key = if prefix.is_empty() {
                    kv.key.to_string()
                } else {
                    format!("{}.{}", prefix, kv.key)
                };
                let value = format_value(&kv.value);
                map.insert(key, value);
            }
            TableEntry::TableBlock(tb) => {
                if let Some(name) = &tb.name {
                    let new_prefix = if prefix.is_empty() {
                        name.clone()
                    } else {
                        format!("{}.{}", prefix, name)
                    };
                    let child_map = flatten_table(tb, &new_prefix);
                    map.extend(child_map);
                }
            }
            TableEntry::ArrayTable(at) => {
                let key = if prefix.is_empty() {
                    format!("[[{}]]", at.key)
                } else {
                    format!("{}.[[{}]]", prefix, at.key)
                };
                let value = format!("[array table with {} entries]", at.entries.len());
                map.insert(key, value);
            }
            _ => {}
        }
    }
    
    map
}

/// 格式化值为字符串表示
fn format_value(value: &Value) -> String {
    match value {
        Value::Scalar(s) => format_scalar(s),
        Value::InlineTable(table) => {
            let entries: Vec<_> = table.entries.iter()
                .map(|kv| format!("{} = {}", kv.key, format_value(&kv.value)))
                .collect();
            format!("{{ {} }}", entries.join(", "))
        }
        Value::Array(arr) => {
            let elements: Vec<_> = arr.elements.iter()
                .map(|v| format_value(v))
                .collect();
            format!("[{}]", elements.join(", "))
        }
        Value::TableBlock(_) => "[table]".to_string(),
        Value::Expression(expr) => format_expression(expr),
    }
}

fn format_scalar(scalar: &ScalarValue) -> String {
    match scalar {
        ScalarValue::String(s) => format!("\"{}\"", s),
        ScalarValue::Number(n) => format!("{}", n),
        ScalarValue::Boolean(b) => format!("{}", b),
        ScalarValue::DateTime(dt) => dt.clone(),
        ScalarValue::Duration(d) => {
            let secs = d.as_secs();
            if secs % 86400 == 0 && secs > 0 {
                format!("{}d", secs / 86400)
            } else if secs % 3600 == 0 && secs > 0 {
                format!("{}h", secs / 3600)
            } else if secs % 60 == 0 && secs > 0 {
                format!("{}m", secs / 60)
            } else {
                format!("{}s", secs)
            }
        }
    }
}

fn format_expression(expr: &Expression) -> String {
    match expr {
        Expression::Literal(scalar) => format_scalar(scalar),
        Expression::BinaryOp { left, operator, right } => {
            format!("{} {} {}", format_expression(left), operator, format_expression(right))
        }
        Expression::UnitValue { value, unit } => {
            let unit_str = match unit {
                TimeUnit::Seconds => "s",
                TimeUnit::Minutes => "m",
                TimeUnit::Hours => "h",
                TimeUnit::Days => "d",
            };
            format!("{}{}", value, unit_str)
        }
    }
}

/// 便捷函数：比较两个源字符串
pub fn diff_sources(old_source: &str, new_source: &str) -> Result<DiffResult, crate::VerseconfError> {
    let old_ast = crate::parse(old_source)?;
    let new_ast = crate::parse(new_source)?;
    Ok(AstDiffer::diff(&old_ast, &new_ast))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn test_diff_added_key() {
        let old = r#"name = "test""#;
        let new = r#"name = "test"
port = 8080"#;
        
        let old_ast = parse(old).unwrap();
        let new_ast = parse(new).unwrap();
        let diff = AstDiffer::diff(&old_ast, &new_ast);
        
        assert!(diff.entries.iter().any(|e| e.path == "port" && e.diff_type == super::super::diff::DiffType::Added));
    }

    #[test]
    fn test_diff_removed_key() {
        let old = r#"name = "test"
port = 8080"#;
        let new = r#"name = "test""#;
        
        let old_ast = parse(old).unwrap();
        let new_ast = parse(new).unwrap();
        let diff = AstDiffer::diff(&old_ast, &new_ast);
        
        assert!(diff.entries.iter().any(|e| e.path == "port" && e.diff_type == super::super::diff::DiffType::Removed));
    }

    #[test]
    fn test_diff_modified_value() {
        let old = r#"port = 8080"#;
        let new = r#"port = 9090"#;
        
        let old_ast = parse(old).unwrap();
        let new_ast = parse(new).unwrap();
        let diff = AstDiffer::diff(&old_ast, &new_ast);
        
        assert!(diff.entries.iter().any(|e| {
            e.path == "port" && 
            e.diff_type == super::super::diff::DiffType::Modified &&
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
        
        let old_ast = parse(old).unwrap();
        let new_ast = parse(new).unwrap();
        let diff = AstDiffer::diff(&old_ast, &new_ast);
        
        assert!(diff.entries.iter().any(|e| {
            e.path == "server.host" && 
            e.diff_type == super::super::diff::DiffType::Modified
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
        
        let old_ast = parse(old).unwrap();
        let new_ast = parse(new).unwrap();
        let diff = AstDiffer::diff(&old_ast, &new_ast);
        let stats = diff.stats();
        
        assert_eq!(stats.added, 1);
        assert_eq!(stats.removed, 1);
        assert_eq!(stats.modified, 1);
        assert_eq!(stats.unchanged, 1);
    }
}

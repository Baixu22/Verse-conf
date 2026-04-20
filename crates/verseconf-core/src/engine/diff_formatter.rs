use super::diff::{DiffEntry, DiffResult, DiffType};

/// Diff 输出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffFormat {
    Text,
    Json,
    Markdown,
}

/// Diff 格式化器
pub struct DiffFormatter;

impl DiffFormatter {
    /// 格式化差异结果为字符串
    pub fn format(diff: &DiffResult, format: DiffFormat) -> String {
        match format {
            DiffFormat::Text => format_text(diff),
            DiffFormat::Json => format_json(diff),
            DiffFormat::Markdown => format_markdown(diff),
        }
    }
}

/// 文本格式输出
fn format_text(diff: &DiffResult) -> String {
    let mut output = String::new();
    
    if let (Some(old), Some(new)) = (&diff.old_source, &diff.new_source) {
        output.push_str(&format!("--- {}\n+++ {}\n\n", old, new));
    }
    
    for entry in &diff.entries {
        let line = format_entry(entry);
        output.push_str(&line);
        output.push('\n');
    }
    
    let stats = diff.stats();
    output.push_str(&format!("\n{}", stats));
    output.push('\n');
    
    output
}

/// JSON 格式输出
fn format_json(diff: &DiffResult) -> String {
    let mut entries_json = Vec::new();
    
    for entry in &diff.entries {
        let entry_obj = format!(
            r#"{{"type":"{}","path":"{}","old_value":{},"new_value":{}}}"#,
            diff_type_to_json(&entry.diff_type),
            escape_json_string(&entry.path),
            option_to_json(entry.old_value.as_deref()),
            option_to_json(entry.new_value.as_deref()),
        );
        entries_json.push(entry_obj);
    }
    
    let stats = diff.stats();
    format!(
        r#"{{"entries":[{}],"stats":{{"added":{},"removed":{},"modified":{},"unchanged":{},"total":{}}}}}"#,
        entries_json.join(","),
        stats.added,
        stats.removed,
        stats.modified,
        stats.unchanged,
        stats.total,
    )
}

/// Markdown 格式输出
fn format_markdown(diff: &DiffResult) -> String {
    let mut output = String::new();
    
    output.push_str("# Configuration Diff\n\n");
    
    if let (Some(old), Some(new)) = (&diff.old_source, &diff.new_source) {
        output.push_str(&format!("**Old**: {}\n", old));
        output.push_str(&format!("**New**: {}\n\n", new));
    }
    
    output.push_str("| Type | Path | Old Value | New Value |\n");
    output.push_str("|------|------|-----------|-----------|\n");
    
    for entry in &diff.entries {
        let type_str = match entry.diff_type {
            DiffType::Added => "➕ Added",
            DiffType::Removed => "➖ Removed",
            DiffType::Modified => "✏️ Modified",
            DiffType::Unchanged => "➡️ Unchanged",
        };
        
        let old_val = entry.old_value.as_deref().unwrap_or("-");
        let new_val = entry.new_value.as_deref().unwrap_or("-");
        
        output.push_str(&format!(
            "| {} | `{}` | `{}` | `{}` |\n",
            type_str, entry.path, old_val, new_val
        ));
    }
    
    let stats = diff.stats();
    output.push_str(&format!("\n**Summary**: {}\n", stats));
    
    output
}

fn format_entry(entry: &DiffEntry) -> String {
    match entry.diff_type {
        DiffType::Added => {
            format!("+ {}: {}", entry.path, entry.new_value.as_deref().unwrap_or(""))
        }
        DiffType::Removed => {
            format!("- {}: {}", entry.path, entry.old_value.as_deref().unwrap_or(""))
        }
        DiffType::Modified => {
            format!(
                "~ {}: {} -> {}",
                entry.path,
                entry.old_value.as_deref().unwrap_or(""),
                entry.new_value.as_deref().unwrap_or("")
            )
        }
        DiffType::Unchanged => {
            format!("  {}: {}", entry.path, entry.old_value.as_deref().unwrap_or(""))
        }
    }
}

fn diff_type_to_json(diff_type: &DiffType) -> &'static str {
    match diff_type {
        DiffType::Added => "added",
        DiffType::Removed => "removed",
        DiffType::Modified => "modified",
        DiffType::Unchanged => "unchanged",
    }
}

fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn option_to_json(value: Option<&str>) -> String {
    match value {
        Some(v) => format!("\"{}\"", escape_json_string(v)),
        None => "null".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use crate::engine::diff_comparator::AstDiffer;

    fn create_test_diff() -> DiffResult {
        let old = r#"name = "test"
port = 8080"#;
        let new = r#"name = "test"
port = 9090
host = "localhost""#;
        
        let old_ast = parse(old).unwrap();
        let new_ast = parse(new).unwrap();
        AstDiffer::diff(&old_ast, &new_ast)
    }

    #[test]
    fn test_format_text() {
        let diff = create_test_diff();
        let output = DiffFormatter::format(&diff, DiffFormat::Text);
        
        assert!(output.contains("+ host:"));
        assert!(output.contains("~ port:"));
        assert!(output.contains("added"));
        assert!(output.contains("modified"));
    }

    #[test]
    fn test_format_json() {
        let diff = create_test_diff();
        let output = DiffFormatter::format(&diff, DiffFormat::Json);
        
        assert!(output.contains("\"type\":\"added\""));
        assert!(output.contains("\"type\":\"modified\""));
        assert!(output.contains("\"path\":\"host\""));
    }

    #[test]
    fn test_format_markdown() {
        let diff = create_test_diff();
        let output = DiffFormatter::format(&diff, DiffFormat::Markdown);
        
        assert!(output.contains("# Configuration Diff"));
        assert!(output.contains("| Type | Path |"));
        assert!(output.contains("➕ Added"));
        assert!(output.contains("✏️ Modified"));
    }
}

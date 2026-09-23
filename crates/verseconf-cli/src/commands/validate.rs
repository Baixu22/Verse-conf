use anyhow::Result;
use std::fs;
use std::path::Path;
use verseconf_core;
use verseconf_core::ast::node::{Key, KeyValue, SchemaField, TableBlock, TableEntry};
use verseconf_core::LoadOptions;

pub fn execute(
    file: &str,
    strict: bool,
    fix: bool,
    dry_run: bool,
    no_include: bool,
    env_interp: bool,
) -> Result<()> {
    let source = fs::read_to_string(file)?;

    let options = LoadOptions {
        resolve_includes: !no_include,
        interpolate_env: env_interp,
    };

    let parsed = verseconf_core::parse_with_context(&source, Some(Path::new(file)), &options)
        .and_then(|ast| {
            verseconf_core::validate_ast(&ast)?;
            Ok(ast)
        });

    match parsed {
        Ok(ast) => {
            // Check if strict mode is requested
            if strict {
                // Force strict mode validation
                let mut validator =
                    verseconf_core::semantic::schema_validator::SchemaValidator::new();
                if let Some(schema) = &ast.schema {
                    // Create a copy with strict=true
                    let mut strict_schema = schema.clone();
                    strict_schema.strict = true;
                    let strict_ast = verseconf_core::ast::node::Ast {
                        root: ast.root.clone(),
                        schema: Some(strict_schema),
                        source: ast.source.clone(),
                    };
                    if let Err(e) = validator.validate_with_schema(&strict_ast) {
                        let error = verseconf_core::VerseconfError::Validation(e);
                        eprintln!(
                            "{}",
                            verseconf_core::ErrorReport::with_path(error, source.clone(), file)
                                .format()
                        );
                        std::process::exit(1);
                    }
                }
            }

            if fix {
                // Apply safe fixes. 只有真的产生改动时才写回文件：
                // 对本来就合法的配置执行 --fix 必须零改动。
                let fixed = apply_safe_fixes(&ast, &source)?;

                if fixed == source {
                    println!("No fixes needed.");
                    return Ok(());
                }

                if dry_run {
                    println!("--- {}", file);
                    println!("+++ {} (fixed)", file);
                    print!("{}", generate_diff(&source, &fixed));
                } else {
                    fs::write(file, &fixed)?;
                    println!("Configuration fixed and saved to {}", file);
                }
            } else {
                println!("Configuration is valid!");
            }
            Ok(())
        }
        Err(e) => {
            eprintln!(
                "{}",
                verseconf_core::ErrorReport::with_path(e, source.clone(), file).format()
            );
            std::process::exit(1);
        }
    }
}

/// Apply safe fixes to the configuration.
///
/// 当前唯一实现的安全修复，是把 schema 中声明了 default 但配置里缺失的字段补齐。
/// 没有可修复项时原样返回输入，保证 --fix 对合法文件零改动。
fn apply_safe_fixes(ast: &verseconf_core::ast::node::Ast, source: &str) -> Result<String> {
    let mut fixed_ast = ast.clone();

    let changed = match &ast.schema {
        Some(schema) => inject_defaults(&mut fixed_ast.root, &schema.fields),
        None => false,
    };

    if !changed {
        return Ok(source.to_string());
    }

    Ok(verseconf_core::pretty_print(&fixed_ast))
}

/// 递归补齐 schema 默认值，返回是否发生了改动
fn inject_defaults(table: &mut TableBlock, fields: &[SchemaField]) -> bool {
    let mut changed = false;

    for field in fields {
        let present = table.entries.iter().any(|entry| match entry {
            TableEntry::KeyValue(kv) => kv.key.as_str() == field.name,
            TableEntry::TableBlock(tb) => tb.name.as_deref() == Some(field.name.as_str()),
            _ => false,
        });

        if !present {
            if let Some(default) = &field.default {
                table.entries.push(TableEntry::KeyValue(KeyValue {
                    key: Key::BareKey(field.name.clone()),
                    value: default.clone(),
                    metadata: None,
                    comment: None,
                    span: field.span,
                }));
                changed = true;
            }
            continue;
        }

        if field.nested_fields.is_empty() {
            continue;
        }

        for entry in table.entries.iter_mut() {
            match entry {
                TableEntry::KeyValue(kv) => {
                    if kv.key.as_str() == field.name {
                        if let verseconf_core::ast::node::Value::TableBlock(tb) = &mut kv.value {
                            changed |= inject_defaults(tb, &field.nested_fields);
                        }
                    }
                }
                TableEntry::TableBlock(tb) if tb.name.as_deref() == Some(field.name.as_str()) => {
                    changed |= inject_defaults(tb, &field.nested_fields);
                }
                _ => {}
            }
        }
    }

    changed
}

/// Generate a diff between original and fixed content
fn generate_diff(original: &str, fixed: &str) -> String {
    let mut diff = String::new();
    let original_lines: Vec<&str> = original.lines().collect();
    let fixed_lines: Vec<&str> = fixed.lines().collect();

    let max_lines = original_lines.len().max(fixed_lines.len());

    for i in 0..max_lines {
        let orig_line = original_lines.get(i).copied().unwrap_or("");
        let fixed_line = fixed_lines.get(i).copied().unwrap_or("");

        if orig_line != fixed_line {
            if !orig_line.is_empty() {
                diff.push_str(&format!("- {}\n", orig_line));
            }
            if !fixed_line.is_empty() {
                diff.push_str(&format!("+ {}\n", fixed_line));
            }
        }
    }

    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixes_keep_schema_comments_and_metadata() {
        let source = r#"# schema note
#@schema {
  setting {
    type = "string"
    default = "ready"
  }
}
# config note
"#;
        let ast = verseconf_core::parse(source).unwrap();
        let fixed = apply_safe_fixes(&ast, source).unwrap();

        assert!(fixed.contains("# schema note"));
        assert!(fixed.contains("#@schema {"));
        assert!(fixed.contains("# config note"));
        assert!(fixed.contains("setting = \"ready\""));
        assert_ne!(fixed, source);

        let reparsed = verseconf_core::parse(&fixed).unwrap();
        let fixed_again = apply_safe_fixes(&reparsed, &fixed).unwrap();
        assert_eq!(fixed, fixed_again);
    }

    #[test]
    fn fixes_leave_unchanged_files_byte_identical() {
        let source = "# comment\nvalue = 1 #@ sensitive\r\n";
        let ast = verseconf_core::parse(source).unwrap();
        assert_eq!(apply_safe_fixes(&ast, source).unwrap(), source);
    }
}

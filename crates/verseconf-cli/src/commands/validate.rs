use anyhow::Result;
use std::fs;
use verseconf_core;

pub fn execute(file: &str, strict: bool, fix: bool, dry_run: bool) -> Result<()> {
    let source = fs::read_to_string(file)?;
    
    match verseconf_core::parse_and_validate(&source) {
        Ok(ast) => {
            // Check if strict mode is requested
            if strict {
                // Force strict mode validation
                let mut validator = verseconf_core::semantic::schema_validator::SchemaValidator::new();
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
                        eprintln!("Validation error (strict mode): {}", e);
                        std::process::exit(1);
                    }
                }
            }
            
            if fix {
                // Apply safe fixes
                let fixed = apply_safe_fixes(&ast, &source)?;
                
                if dry_run {
                    // Show diff
                    println!("--- {}", file);
                    println!("+++ {} (fixed)", file);
                    let diff = generate_diff(&source, &fixed);
                    if diff.is_empty() {
                        println!("No fixes needed.");
                    } else {
                        print!("{}", diff);
                    }
                } else {
                    // Write to file
                    fs::write(file, &fixed)?;
                    println!("Configuration fixed and saved to {}", file);
                }
            } else {
                println!("Configuration is valid!");
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("Validation error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Apply safe fixes to the configuration
fn apply_safe_fixes(ast: &verseconf_core::ast::node::Ast, _source: &str) -> Result<String> {
    // Use pretty printer to regenerate the configuration
    // This will apply defaults and fix formatting issues
    let fixed = verseconf_core::pretty_print(ast);
    
    // If schema exists, apply default values for missing required fields
    if let Some(schema) = &ast.schema {
        // Collect all defaults from schema
        let _defaults = collect_schema_defaults(&schema.fields);
        
        // If there are defaults to apply, we need to modify the AST
        // For now, just return the pretty-printed version
        // Future: actually inject default values
    }
    
    Ok(fixed)
}

/// Collect default values from schema fields
fn collect_schema_defaults(fields: &[verseconf_core::ast::node::SchemaField]) -> Vec<(String, String)> {
    let mut defaults = Vec::new();
    for field in fields {
        if let Some(ref default) = field.default {
            defaults.push((field.name.clone(), format!("{:?}", default)));
        }
    }
    defaults
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

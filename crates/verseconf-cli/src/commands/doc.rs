use anyhow::Result;
use std::fs;
use verseconf_core::ast::node::*;

pub fn execute(file: &str, output: Option<&str>) -> Result<()> {
    let source = fs::read_to_string(file)?;
    let ast = verseconf_core::parse_and_validate(&source)?;
    
    let doc = generate_doc(&ast);
    
    if let Some(output_path) = output {
        fs::write(output_path, &doc)?;
        println!("Documentation generated to {}", output_path);
    } else {
        print!("{}", doc);
    }
    
    Ok(())
}

fn generate_doc(ast: &verseconf_core::ast::node::Ast) -> String {
    let mut doc = String::new();
    
    // Title
    doc.push_str("# Configuration Documentation\n\n");
    
    // Schema description
    if let Some(schema) = &ast.schema {
        if let Some(ref desc) = schema.description {
            doc.push_str(&format!("**Description**: {}\n\n", desc));
        }
        if let Some(ref version) = schema.version {
            doc.push_str(&format!("**Schema Version**: {}\n\n", version));
        }
        doc.push_str(&format!("**Strict Mode**: {}\n\n", if schema.strict { "Enabled" } else { "Disabled" }));
        
        // Fields documentation
        doc.push_str("## Fields\n\n");
        for field in &schema.fields {
            generate_field_doc(&mut doc, field, 0);
        }
    } else {
        doc.push_str("*No schema defined*\n\n");
    }
    
    doc
}

fn generate_field_doc(doc: &mut String, field: &SchemaField, indent: usize) {
    let prefix = "  ".repeat(indent);
    
    // Field name and type
    doc.push_str(&format!("{}### {} (`{}`)", prefix, field.name, field.field_type));
    
    // Required badge
    if field.required {
        doc.push_str(" **required**");
    }
    doc.push_str("\n\n");
    
    // Description
    if let Some(ref desc) = field.desc {
        doc.push_str(&format!("{}**Description**: {}\n\n", prefix, desc));
    }
    
    // Default value
    if let Some(ref default) = field.default {
        doc.push_str(&format!("{}**Default**: `{:?}`\n\n", prefix, default));
    }
    
    // Range constraint
    if let Some(ref range) = field.range {
        doc.push_str(&format!("{}**Range**: ", prefix));
        match (range.min, range.max) {
            (Some(min), Some(max)) => doc.push_str(&format!("`{}..{}`\n\n", min, max)),
            (Some(min), None) => doc.push_str(&format!("`{}..`\n\n", min)),
            (None, Some(max)) => doc.push_str(&format!("`..{}`\n\n", max)),
            (None, None) => doc.push_str("unbounded\n\n"),
        }
    }
    
    // Pattern constraint
    if let Some(ref pattern) = field.pattern {
        doc.push_str(&format!("{}**Pattern**: `{}`\n\n", prefix, pattern));
    }
    
    // Enum values
    if let Some(ref enum_values) = field.enum_values {
        doc.push_str(&format!("{}**Allowed Values**: ", prefix));
        let values: Vec<String> = enum_values.iter().map(|v| format!("`{:?}`", v)).collect();
        doc.push_str(&values.join(", "));
        doc.push_str("\n\n");
    }
    
    // LLM hint
    if let Some(ref llm_hint) = field.llm_hint {
        doc.push_str(&format!("{}**AI Hint**: {}\n\n", prefix, llm_hint));
    }
    
    // Sensitive
    if field.sensitive {
        doc.push_str(&format!("{}**⚠️ Sensitive**: This field contains sensitive data\n\n", prefix));
    }
    
    // Example
    if let Some(ref example) = field.example {
        doc.push_str(&format!("{}**Example**:\n\n", prefix));
        doc.push_str(&format!("{}```\n{}\n```\n\n", prefix, example));
    }
    
    // Nested fields
    if !field.nested_fields.is_empty() {
        doc.push_str(&format!("{}**Sub-fields**:\n\n", prefix));
        for nested in &field.nested_fields {
            generate_field_doc(doc, nested, indent + 1);
        }
    }
}

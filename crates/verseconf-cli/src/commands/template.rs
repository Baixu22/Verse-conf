use std::fs;
use std::path::Path;
use verseconf_core::{parse_template, render_template, RenderContext};

/// Render a template file
pub fn run_render(
    template_file: &str,
    output_file: Option<&str>,
    variables: Vec<(String, String)>,
    strict: bool,
) -> anyhow::Result<()> {
    let template_path = Path::new(template_file);
    let content = fs::read_to_string(template_path)?;
    let template = parse_template(&content)?;
    
    let mut context = RenderContext::new().strict_mode(strict);
    for (key, value) in variables {
        context = context.with_value(key, value);
    }
    
    let rendered = render_template(&template, &context)?;
    
    match output_file {
        Some(output) => {
            fs::write(output, &rendered)?;
            println!("Template rendered to: {}", output);
        }
        None => {
            println!("{}", rendered);
        }
    }
    
    Ok(())
}

/// List variables in a template
pub fn run_list(template_file: &str) -> anyhow::Result<()> {
    let content = fs::read_to_string(template_file)?;
    let template = parse_template(&content)?;
    
    println!("Template: {}", template.name);
    if let Some(desc) = &template.description {
        println!("Description: {}", desc);
    }
    println!("Version: {}", template.version);
    println!("\nVariables ({}):", template.variables.len());
    
    for var in &template.variables {
        let _required = if var.required { "required" } else { "optional" };
        let default = var.default.as_deref().unwrap_or("none");
        let desc = var.description.as_deref().unwrap_or("no description");
        
        println!("  {} ({}): {} [default: {}]", var.name, var.var_type, desc, default);
        if let Some(choices) = &var.choices {
            println!("    choices: {:?}", choices);
        }
    }
    
    Ok(())
}

/// Validate a template file
pub fn run_validate(template_file: &str) -> anyhow::Result<()> {
    let content = fs::read_to_string(template_file)?;
    let template = parse_template(&content)?;
    
    println!("Template '{}' is valid", template.name);
    println!("Found {} variables", template.variables.len());
    
    Ok(())
}

/// Generate a template from interactive prompts
pub fn run_generate(template_file: &str, output_file: &str) -> anyhow::Result<()> {
    let content = fs::read_to_string(template_file)?;
    let template = parse_template(&content)?;
    
    println!("Template: {}", template.name);
    if let Some(desc) = &template.description {
        println!("Description: {}", desc);
    }
    println!("\nPlease provide values for variables:\n");
    
    let mut context = RenderContext::new();
    
    for var in &template.variables {
        let prompt = if var.required {
            format!("{} ({}) [required]: ", var.name, var.var_type)
        } else {
            let default = var.default.as_deref().unwrap_or("");
            format!("{} ({}) [default: {}]: ", var.name, var.var_type, default)
        };
        
        print!("{}", prompt);
        std::io::Write::flush(&mut std::io::stdout())?;
        
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim();
        
        let value = if input.is_empty() {
            var.default.clone().unwrap_or_default()
        } else {
            input.to_string()
        };
        
        context = context.with_value(&var.name, value);
    }
    
    let rendered = render_template(&template, &context)?;
    fs::write(output_file, &rendered)?;
    println!("\nConfiguration generated: {}", output_file);
    
    Ok(())
}

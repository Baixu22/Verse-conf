use std::fs;
use verseconf_core::{diff_sources, DiffFormat, DiffFormatter};

/// Run diff command
pub fn run_diff(
    old_file: &str,
    new_file: &str,
    format: &str,
    output_file: Option<&str>,
) -> anyhow::Result<()> {
    let old_content = fs::read_to_string(old_file)?;
    let new_content = fs::read_to_string(new_file)?;
    
    let diff = diff_sources(&old_content, &new_content)?;
    
    let diff_format = match format {
        "json" => DiffFormat::Json,
        "markdown" | "md" => DiffFormat::Markdown,
        _ => DiffFormat::Text,
    };
    
    let output = DiffFormatter::format(&diff, diff_format);
    
    match output_file {
        Some(out) => {
            fs::write(out, &output)?;
            println!("Diff written to: {}", out);
        }
        None => {
            println!("{}", output);
        }
    }
    
    Ok(())
}

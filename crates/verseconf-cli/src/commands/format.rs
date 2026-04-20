use anyhow::Result;
use std::fs;
use verseconf_core::{PrettyPrintConfig, format_with_config};

pub fn execute(file: &str, output: Option<&str>, ai_canonical: bool) -> Result<()> {
    let source = fs::read_to_string(file)?;
    
    let config = if ai_canonical {
        PrettyPrintConfig::ai_canonical()
    } else {
        PrettyPrintConfig::default()
    };
    
    match format_with_config(&source, config) {
        Ok(formatted) => {
            if let Some(out_file) = output {
                fs::write(out_file, formatted)?;
                println!("Formatted file written to {}", out_file);
            } else {
                println!("{}", formatted);
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("Format error: {}", e);
            std::process::exit(1);
        }
    }
}

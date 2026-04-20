use anyhow::Result;
use std::fs;
use verseconf_core::ParseConfig;

pub fn execute(file: &str, tolerant: bool) -> Result<()> {
    let source = fs::read_to_string(file)?;
    
    let config = ParseConfig {
        tolerant,
        collect_warnings: tolerant,
    };
    
    match verseconf_core::parse_with_config(&source, config) {
        Ok(result) => {
            if tolerant {
                println!("Successfully parsed (tolerant mode)!");
                println!("Root table has {} entries", result.value.root.entries.len());
                
                if result.has_warnings() {
                    println!("\nWarnings:");
                    for warning in &result.warnings {
                        println!("  [{}] {}", warning.category, warning.message);
                    }
                }
            } else {
                println!("Successfully parsed!");
                println!("Root table has {} entries", result.value.root.entries.len());
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("Parse error: {}", e);
            std::process::exit(1);
        }
    }
}

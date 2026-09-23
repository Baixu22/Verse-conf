use anyhow::Result;
use std::fs;
use std::path::Path;
use verseconf_core::{LoadOptions, ParseConfig};

pub fn execute(file: &str, tolerant: bool, no_include: bool, env_interp: bool) -> Result<()> {
    let source = fs::read_to_string(file)?;

    if tolerant {
        let config = ParseConfig {
            tolerant: true,
            collect_warnings: true,
        };

        return match verseconf_core::parse_with_config(&source, config) {
            Ok(result) => {
                println!("Successfully parsed (tolerant mode)!");
                println!("Root table has {} entries", result.value.root.entries.len());

                if result.has_warnings() {
                    println!("\nWarnings:");
                    for warning in &result.warnings {
                        println!("  [{}] {}", warning.category, warning.message);
                    }
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
        };
    }

    let options = LoadOptions {
        resolve_includes: !no_include,
        interpolate_env: env_interp,
    };

    match verseconf_core::parse_with_context(&source, Some(Path::new(file)), &options) {
        Ok(ast) => {
            println!("Successfully parsed!");
            println!("Root table has {} entries", ast.root.entries.len());
            if options.resolve_includes {
                println!("Resolved @include directives relative to the file directory");
            }
            if options.interpolate_env {
                println!("Applied environment variable interpolation");
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

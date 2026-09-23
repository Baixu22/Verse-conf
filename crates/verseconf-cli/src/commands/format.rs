use anyhow::Result;
use std::fs;
use std::path::Path;
use verseconf_core::{LoadOptions, PrettyPrintConfig, PrettyPrinter};

pub fn execute(
    file: &str,
    output: Option<&str>,
    ai_canonical: bool,
    include: bool,
    env_interp: bool,
) -> Result<()> {
    let source = fs::read_to_string(file)?;

    let config = if ai_canonical {
        PrettyPrintConfig::ai_canonical()
    } else {
        PrettyPrintConfig::default()
    };

    // 默认不合并 @include：格式化必须无损，@include 行要原样保留。
    let options = LoadOptions {
        resolve_includes: include,
        interpolate_env: env_interp,
    };

    let ast = match verseconf_core::parse_with_context(&source, Some(Path::new(file)), &options) {
        Ok(ast) => ast,
        Err(e) => {
            eprintln!(
                "{}",
                verseconf_core::ErrorReport::with_path(e, source.clone(), file).format()
            );
            std::process::exit(1);
        }
    };

    let formatted = PrettyPrinter::print_with_config(&ast, config);

    if let Some(out_file) = output {
        fs::write(out_file, formatted)?;
        println!("Formatted file written to {}", out_file);
    } else {
        println!("{}", formatted);
    }

    Ok(())
}

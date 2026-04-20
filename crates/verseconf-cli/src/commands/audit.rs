use std::fs;
use verseconf_core::{parse, AuditEngine};

/// Run audit command
pub fn run_audit(
    file_path: &str,
    output_format: &str,
) -> anyhow::Result<()> {
    let source = fs::read_to_string(file_path)
        .map_err(|e| anyhow::anyhow!("Failed to read file '{}': {}", file_path, e))?;

    let ast = parse(&source)
        .map_err(|e| anyhow::anyhow!("Failed to parse file '{}': {}", file_path, e))?;

    let engine = AuditEngine::new();
    let report = engine.audit_ast(&ast);

    match output_format {
        "json" => {
            let json_findings: Vec<_> = report.findings.iter().map(|f| {
                format!(
                    r#"{{"category":"{}","severity":"{}","rule_id":"{}","title":"{}","location":"{}","description":"{}","recommendation":"{}"}}"#,
                    f.category, f.severity, f.rule_id, f.title, f.location, f.description, f.recommendation
                )
            }).collect();
            
            println!(
                r#"{{"summary":{{"total":{},"critical":{},"high":{},"medium":{},"low":{},"info":{}}},"findings":[{}]}}"#,
                report.summary.total_findings,
                report.summary.critical_count,
                report.summary.high_count,
                report.summary.medium_count,
                report.summary.low_count,
                report.summary.info_count,
                json_findings.join(",")
            );
        }
        "text" | _ => {
            print!("{}", report);
        }
    }

    if report.summary.critical_count > 0 || report.summary.high_count > 0 {
        std::process::exit(1);
    }

    Ok(())
}

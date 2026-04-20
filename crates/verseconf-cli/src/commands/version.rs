use std::path::Path;
use verseconf_core::VersionManager;

/// Run version command
pub fn run_version(
    file: &str,
    subcommand: &str,
    version_id: Option<u64>,
    version_a: Option<u64>,
    version_b: Option<u64>,
    description: Option<&str>,
    storage_dir: Option<&str>,
) -> anyhow::Result<()> {
    let file_path = Path::new(file);
    
    let mut manager = VersionManager::new();
    
    if let Some(dir) = storage_dir {
        manager = manager.with_storage(Path::new(dir).to_path_buf())
            .map_err(|e| anyhow::anyhow!(e))?;
    }
    
    match subcommand {
        "create" => {
            let desc = description.map(String::from);
            let vid = manager.create_version(file_path, desc)
                .map_err(|e| anyhow::anyhow!(e))?;
            println!("Created version {}", vid);
        }
        "history" => {
            let history = manager.get_version_history(file_path);
            if history.is_empty() {
                println!("No versions found for {}", file);
            } else {
                println!("Version history for {}:", file);
                for info in &history {
                    let time = chrono::DateTime::from_timestamp(info.timestamp as i64, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    let desc = info.description.as_deref().unwrap_or("No description");
                    println!("  v{} - {} - {}", info.version_id, time, desc);
                }
            }
        }
        "rollback" => {
            let vid = version_id.ok_or_else(|| anyhow::anyhow!("Version ID required for rollback"))?;
            let result = manager.rollback(file_path, vid)
                .map_err(|e| anyhow::anyhow!(e))?;
            println!("{}", result);
        }
        "diff" => {
            let va = version_a.ok_or_else(|| anyhow::anyhow!("Version A required for diff"))?;
            let vb = version_b.ok_or_else(|| anyhow::anyhow!("Version B required for diff"))?;
            let diff = manager.compare_versions(file_path, va, vb)
                .map_err(|e| anyhow::anyhow!(e))?;
            println!("{}", diff);
        }
        "latest" => {
            if let Some(record) = manager.get_latest_version(file_path) {
                let time = chrono::DateTime::from_timestamp(record.info.timestamp as i64, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                println!("Latest version: v{}", record.info.version_id);
                println!("Timestamp: {}", time);
                println!("Content hash: {:x}", record.info.content_hash);
                if let Some(desc) = &record.info.description {
                    println!("Description: {}", desc);
                }
            } else {
                println!("No versions found for {}", file);
            }
        }
        _ => {
            return Err(anyhow::anyhow!("Unknown version subcommand: {}", subcommand));
        }
    }
    
    Ok(())
}

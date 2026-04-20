use std::path::PathBuf;
use verseconf_core::EnvironmentManager;

/// Run env command
pub fn run_env(
    subcommand: &str,
    base_dir: Option<&str>,
    env_name: Option<&str>,
    env_content: Option<&str>,
    extends: Option<&str>,
    output_format: &str,
) -> anyhow::Result<()> {
    let base_path = PathBuf::from(base_dir.unwrap_or("."));
    let env_dir = base_path.join("environments");
    let mut manager = EnvironmentManager::new(base_path.clone());

    match subcommand {
        "list" => {
            // Load all .vcf files from environments directory
            if env_dir.exists() {
                for entry in std::fs::read_dir(&env_dir).map_err(|e| anyhow::anyhow!("Failed to read environments dir: {}", e))? {
                    let entry = entry.map_err(|e| anyhow::anyhow!("Failed to read entry: {}", e))?;
                    let path = entry.path();
                    if path.extension().map_or(false, |ext| ext == "vcf") {
                        if let Some(name) = path.file_stem() {
                            let name_str = name.to_string_lossy();
                            // Handle .env.vcf pattern
                            let env_name = if name_str.ends_with(".env") {
                                name_str.trim_end_matches(".env").to_string()
                            } else {
                                name_str.to_string()
                            };
                            let _ = manager.load_environment_from_file(&env_name, &path);
                        }
                    }
                }
            }

            let envs = manager.list_environments();
            if envs.is_empty() {
                println!("No environments found.");
            } else {
                match output_format {
                    "json" => {
                        let json: Vec<_> = envs.iter().map(|e| {
                            if let Some(config) = manager.get_environment(e) {
                                let extends = config.extends.as_deref().unwrap_or("none");
                                format!("{{\"name\":\"{}\",\"extends\":\"{}\"}}", e, extends)
                            } else {
                                format!("{{\"name\":\"{}\"}}", e)
                            }
                        }).collect();
                        println!("[{}]", json.join(","));
                    }
                    "text" | _ => {
                        println!("Available environments:");
                        for env in &envs {
                            if let Some(config) = manager.get_environment(env) {
                                let extends = config.extends.as_deref().unwrap_or("none");
                                println!("  {} (extends: {})", env, extends);
                            } else {
                                println!("  {}", env);
                            }
                        }
                    }
                }
            }
        }
        "create" => {
            let name = env_name.ok_or_else(|| anyhow::anyhow!("Environment name required"))?;
            let content = env_content.unwrap_or("");
            let extends = extends.map(String::from);
            
            manager.add_environment(name, content, extends.as_deref())
                .map_err(|e| anyhow::anyhow!(e))?;
            println!("Created environment '{}'", name);
            if let Some(parent) = &extends {
                println!("  Extends: {}", parent);
            }
        }
        "resolve" => {
            let name = env_name.ok_or_else(|| anyhow::anyhow!("Environment name required"))?;
            
            // Load environments from directory
            let env_dir = base_path.join("environments");
            if env_dir.exists() {
                for entry in std::fs::read_dir(&env_dir).map_err(|e| anyhow::anyhow!("Failed to read environments dir: {}", e))? {
                    let entry = entry.map_err(|e| anyhow::anyhow!("Failed to read entry: {}", e))?;
                    let path = entry.path();
                    if path.extension().map_or(false, |ext| ext == "vcf") {
                        if let Some(file_name) = path.file_stem() {
                            let env_file_name = file_name.to_string_lossy();
                            let env_name_str = if env_file_name.ends_with(".env") {
                                env_file_name.trim_end_matches(".env").to_string()
                            } else {
                                env_file_name.to_string()
                            };
                            let _ = manager.load_environment_from_file(&env_name_str, &path);
                        }
                    }
                }
            }

            let resolved = manager.resolve_environment(name)
                .map_err(|e| anyhow::anyhow!(e))?;
            println!("{}", resolved);
        }
        "diff" => {
            let env_a = env_name.ok_or_else(|| anyhow::anyhow!("First environment name required"))?;
            let env_b = extends.ok_or_else(|| anyhow::anyhow!("Second environment name required"))?;
            
            // Load environments from directory
            let env_dir = base_path.join("environments");
            if env_dir.exists() {
                for entry in std::fs::read_dir(&env_dir).map_err(|e| anyhow::anyhow!("Failed to read environments dir: {}", e))? {
                    let entry = entry.map_err(|e| anyhow::anyhow!("Failed to read entry: {}", e))?;
                    let path = entry.path();
                    if path.extension().map_or(false, |ext| ext == "vcf") {
                        if let Some(file_name) = path.file_stem() {
                            let env_file_name = file_name.to_string_lossy();
                            let env_name_str = if env_file_name.ends_with(".env") {
                                env_file_name.trim_end_matches(".env").to_string()
                            } else {
                                env_file_name.to_string()
                            };
                            let _ = manager.load_environment_from_file(&env_name_str, &path);
                        }
                    }
                }
            }

            let resolved_a = manager.resolve_environment(env_a)
                .map_err(|e| anyhow::anyhow!(e))?;
            let resolved_b = manager.resolve_environment(env_b)
                .map_err(|e| anyhow::anyhow!(e))?;
            
            let diff = verseconf_core::diff_sources(&resolved_a, &resolved_b)?;
            let output = verseconf_core::DiffFormatter::format(&diff, match output_format {
                "json" => verseconf_core::DiffFormat::Json,
                "markdown" | "md" => verseconf_core::DiffFormat::Markdown,
                _ => verseconf_core::DiffFormat::Text,
            });
            
            println!("Comparing {} -> {}:", env_a, env_b);
            println!("{}", output);
        }
        _ => {
            return Err(anyhow::anyhow!("Unknown env subcommand: {}. Available: list, create, resolve, diff", subcommand));
        }
    }

    Ok(())
}

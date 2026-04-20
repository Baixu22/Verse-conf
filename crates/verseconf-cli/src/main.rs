mod commands;
mod output;

use clap::Parser;

#[derive(Parser)]
#[command(name = "verseconf")]
#[command(about = "VerseConf configuration language CLI tool", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Parse a configuration file
    Parse {
        /// Path to the configuration file
        file: String,
        /// Enable tolerant parsing mode
        #[arg(short, long)]
        tolerant: bool,
    },
    /// Validate a configuration file
    Validate {
        /// Path to the configuration file
        file: String,
        /// Enable strict mode (reject undeclared fields)
        #[arg(short, long)]
        strict: bool,
        /// Apply safe fixes
        #[arg(short, long)]
        fix: bool,
        /// Show diff of fixes without applying them
        #[arg(long)]
        dry_run: bool,
        /// Write fixes to file
        #[arg(long)]
        write: bool,
    },
    /// Format a configuration file
    Format {
        /// Path to the configuration file
        file: String,
        /// Output to file instead of stdout
        #[arg(short, long)]
        output: Option<String>,
        /// Use AI canonical format (sorted keys, consistent formatting)
        #[arg(long)]
        ai_canonical: bool,
    },
    /// Generate documentation from a configuration file
    Doc {
        /// Path to the configuration file
        file: String,
        /// Output to file instead of stdout
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Template commands
    #[command(subcommand)]
    Template(TemplateCommands),
    /// Compare two configuration files
    Diff {
        /// Path to the old configuration file
        old: String,
        /// Path to the new configuration file
        new: String,
        /// Output format (text, json, markdown)
        #[arg(short, long, default_value = "text")]
        format: String,
        /// Output to file instead of stdout
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Version management commands
    #[command(subcommand)]
    Version(VersionCommands),
    /// Environment management commands
    #[command(subcommand)]
    Env(EnvCommands),
    /// Audit a configuration file for security issues
    Audit {
        /// Path to the configuration file
        file: String,
        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },
}

#[derive(clap::Subcommand)]
enum EnvCommands {
    /// List available environments
    List {
        /// Base directory containing environments folder
        #[arg(long, default_value = ".")]
        base_dir: String,
        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Create a new environment
    Create {
        /// Environment name
        name: String,
        /// Environment content
        #[arg(short, long)]
        content: Option<String>,
        /// Parent environment to extend
        #[arg(short, long)]
        extends: Option<String>,
        /// Base directory
        #[arg(long, default_value = ".")]
        base_dir: String,
    },
    /// Resolve and display an environment configuration
    Resolve {
        /// Environment name
        name: String,
        /// Base directory containing environments folder
        #[arg(long, default_value = ".")]
        base_dir: String,
    },
    /// Compare two environments
    Diff {
        /// First environment name
        env_a: String,
        /// Second environment name
        env_b: String,
        /// Base directory containing environments folder
        #[arg(long, default_value = ".")]
        base_dir: String,
        /// Output format (text, json, markdown)
        #[arg(short, long, default_value = "text")]
        format: String,
    },
}

#[derive(clap::Subcommand)]
enum VersionCommands {
    /// Create a new version
    Create {
        /// Path to the configuration file
        file: String,
        /// Version description
        #[arg(short, long)]
        description: Option<String>,
        /// Storage directory for version files
        #[arg(long)]
        storage: Option<String>,
    },
    /// Show version history
    History {
        /// Path to the configuration file
        file: String,
        /// Storage directory for version files
        #[arg(long)]
        storage: Option<String>,
    },
    /// Rollback to a specific version
    Rollback {
        /// Path to the configuration file
        file: String,
        /// Version ID to rollback to
        #[arg(short, long)]
        version: u64,
        /// Storage directory for version files
        #[arg(long)]
        storage: Option<String>,
    },
    /// Compare two versions
    Diff {
        /// Path to the configuration file
        file: String,
        /// First version ID
        #[arg(long)]
        version_a: u64,
        /// Second version ID
        #[arg(long)]
        version_b: u64,
        /// Storage directory for version files
        #[arg(long)]
        storage: Option<String>,
    },
    /// Show latest version info
    Latest {
        /// Path to the configuration file
        file: String,
        /// Storage directory for version files
        #[arg(long)]
        storage: Option<String>,
    },
}

#[derive(clap::Subcommand)]
enum TemplateCommands {
    /// Render a template file
    Render {
        /// Path to the template file
        template: String,
        /// Output to file instead of stdout
        #[arg(short, long)]
        output: Option<String>,
        /// Variable assignments (key=value)
        #[arg(short, long, value_parser = parse_key_val)]
        variable: Vec<(String, String)>,
        /// Strict mode (error on undefined variables)
        #[arg(short, long)]
        strict: bool,
    },
    /// List variables in a template
    List {
        /// Path to the template file
        template: String,
    },
    /// Validate a template file
    Validate {
        /// Path to the template file
        template: String,
    },
    /// Generate configuration from template interactively
    Generate {
        /// Path to the template file
        template: String,
        /// Output file for generated configuration
        #[arg(short, long)]
        output: String,
    },
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in '{}'", s))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Parse { file, tolerant } => {
            commands::parse::execute(&file, tolerant)?;
        }
        Commands::Validate { file, strict, fix, dry_run, write } => {
            commands::validate::execute(&file, strict, fix || write, dry_run && !write)?;
        }
        Commands::Format { file, output, ai_canonical } => {
            commands::format::execute(&file, output.as_deref(), ai_canonical)?;
        }
        Commands::Doc { file, output } => {
            commands::doc::execute(&file, output.as_deref())?;
        }
        Commands::Template(template_cmd) => {
            match template_cmd {
                TemplateCommands::Render { template, output, variable, strict } => {
                    commands::template::run_render(&template, output.as_deref(), variable, strict)?;
                }
                TemplateCommands::List { template } => {
                    commands::template::run_list(&template)?;
                }
                TemplateCommands::Validate { template } => {
                    commands::template::run_validate(&template)?;
                }
                TemplateCommands::Generate { template, output } => {
                    commands::template::run_generate(&template, &output)?;
                }
            }
        }
        Commands::Diff { old, new, format, output } => {
            commands::diff::run_diff(&old, &new, &format, output.as_deref())?;
        }
        Commands::Version(version_cmd) => {
            match version_cmd {
                VersionCommands::Create { file, description, storage } => {
                    commands::version::run_version(&file, "create", None, None, None, description.as_deref(), storage.as_deref())?;
                }
                VersionCommands::History { file, storage } => {
                    commands::version::run_version(&file, "history", None, None, None, None, storage.as_deref())?;
                }
                VersionCommands::Rollback { file, version, storage } => {
                    commands::version::run_version(&file, "rollback", Some(version), None, None, None, storage.as_deref())?;
                }
                VersionCommands::Diff { file, version_a, version_b, storage } => {
                    commands::version::run_version(&file, "diff", None, Some(version_a), Some(version_b), None, storage.as_deref())?;
                }
                VersionCommands::Latest { file, storage } => {
                    commands::version::run_version(&file, "latest", None, None, None, None, storage.as_deref())?;
                }
            }
        }
        Commands::Env(env_cmd) => {
            match env_cmd {
                EnvCommands::List { base_dir, format } => {
                    commands::env::run_env("list", Some(&base_dir), None, None, None, &format)?;
                }
                EnvCommands::Create { name, content, extends, base_dir } => {
                    commands::env::run_env("create", Some(&base_dir), Some(&name), content.as_deref(), extends.as_deref(), "text")?;
                }
                EnvCommands::Resolve { name, base_dir } => {
                    commands::env::run_env("resolve", Some(&base_dir), Some(&name), None, None, "text")?;
                }
                EnvCommands::Diff { env_a, env_b, base_dir, format } => {
                    commands::env::run_env("diff", Some(&base_dir), Some(&env_a), None, Some(&env_b), &format)?;
                }
            }
        }
        Commands::Audit { file, format } => {
            commands::audit::run_audit(&file, &format)?;
        }
    }

    Ok(())
}

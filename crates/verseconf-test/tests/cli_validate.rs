use std::process::Command;

fn get_workspace_root() -> String {
    // Go up from crates/verseconf-test/tests/ to workspace root
    let current = std::env::current_dir().unwrap_or_default();
    let current_str = current.to_string_lossy();
    
    // Find the workspace root
    if current_str.contains("verseconf-test") {
        // We're in the test crate directory, go up 2 levels
        current.parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string())
    } else {
        ".".to_string()
    }
}

fn run_cli(args: &[&str]) -> (String, String, i32) {
    let workspace_root = get_workspace_root();
    
    let output = Command::new("cargo")
        .current_dir(&workspace_root)
        .args(["run", "--bin", "verseconf-cli", "--quiet", "--"])
        .args(args)
        .output()
        .expect("Failed to execute CLI");
    
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let status = output.status.code().unwrap_or(-1);
    
    (stdout, stderr, status)
}

#[test]
fn test_validate_basic() {
    let (stdout, _stderr, status) = run_cli(&["validate", "examples/basic.vcf"]);
    assert_eq!(status, 0);
    assert!(stdout.contains("Configuration is valid!"));
}

#[test]
fn test_validate_with_schema() {
    let (stdout, _stderr, status) = run_cli(&["validate", "examples/with_schema.vcf"]);
    assert_eq!(status, 0);
    assert!(stdout.contains("Configuration is valid!"));
}

#[test]
fn test_validate_strict_mode() {
    let (stdout, _stderr, status) = run_cli(&["validate", "examples/with_schema.vcf", "--strict"]);
    assert_eq!(status, 0);
    assert!(stdout.contains("Configuration is valid!"));
}

#[test]
fn test_validate_fix_dry_run() {
    let (stdout, _stderr, status) = run_cli(&["validate", "examples/basic.vcf", "--fix", "--dry-run"]);
    assert_eq!(status, 0);
    // Should show diff or "No fixes needed"
    assert!(stdout.contains("---") || stdout.contains("No fixes needed"));
}

#[test]
fn test_doc_generation() {
    let (stdout, _stderr, status) = run_cli(&["doc", "examples/with_schema.vcf"]);
    assert_eq!(status, 0);
    assert!(stdout.contains("# Configuration Documentation"));
    assert!(stdout.contains("## Fields"));
    assert!(stdout.contains("server"));
    assert!(stdout.contains("database"));
}

#[test]
fn test_parse_with_tolerant() {
    let (stdout, _stderr, status) = run_cli(&["parse", "examples/basic.vcf", "--tolerant"]);
    assert_eq!(status, 0);
    assert!(stdout.contains("Successfully parsed"));
}

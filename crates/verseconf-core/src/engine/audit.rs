use regex::Regex;

use crate::ast::{Ast, Value, ScalarValue, TableEntry};

#[derive(Debug, Clone)]
pub enum AuditSeverity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

impl std::fmt::Display for AuditSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditSeverity::Critical => write!(f, "CRITICAL"),
            AuditSeverity::High => write!(f, "HIGH"),
            AuditSeverity::Medium => write!(f, "MEDIUM"),
            AuditSeverity::Low => write!(f, "LOW"),
            AuditSeverity::Info => write!(f, "INFO"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum AuditCategory {
    SensitiveData,
    InsecureConfig,
    BestPractice,
    Compliance,
}

impl std::fmt::Display for AuditCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditCategory::SensitiveData => write!(f, "Sensitive Data"),
            AuditCategory::InsecureConfig => write!(f, "Insecure Configuration"),
            AuditCategory::BestPractice => write!(f, "Best Practice"),
            AuditCategory::Compliance => write!(f, "Compliance"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuditFinding {
    pub category: AuditCategory,
    pub severity: AuditSeverity,
    pub rule_id: String,
    pub title: String,
    pub description: String,
    pub location: String,
    pub recommendation: String,
}

impl std::fmt::Display for AuditFinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "[{}] {} - {}", self.severity, self.rule_id, self.title)?;
        writeln!(f, "  Category: {}", self.category)?;
        writeln!(f, "  Location: {}", self.location)?;
        writeln!(f, "  Description: {}", self.description)?;
        writeln!(f, "  Recommendation: {}", self.recommendation)
    }
}

#[derive(Debug, Clone)]
pub struct AuditReport {
    pub findings: Vec<AuditFinding>,
    pub summary: AuditSummary,
}

#[derive(Debug, Clone)]
pub struct AuditSummary {
    pub total_findings: usize,
    pub critical_count: usize,
    pub high_count: usize,
    pub medium_count: usize,
    pub low_count: usize,
    pub info_count: usize,
}

impl AuditReport {
    pub fn new(findings: Vec<AuditFinding>) -> Self {
        let total = findings.len();
        let mut critical = 0;
        let mut high = 0;
        let mut medium = 0;
        let mut low = 0;
        let mut info = 0;

        for finding in &findings {
            match finding.severity {
                AuditSeverity::Critical => critical += 1,
                AuditSeverity::High => high += 1,
                AuditSeverity::Medium => medium += 1,
                AuditSeverity::Low => low += 1,
                AuditSeverity::Info => info += 1,
            }
        }

        Self {
            findings,
            summary: AuditSummary {
                total_findings: total,
                critical_count: critical,
                high_count: high,
                medium_count: medium,
                low_count: low,
                info_count: info,
            },
        }
    }
}

impl std::fmt::Display for AuditReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Security Audit Report ===")?;
        writeln!(f, "Total findings: {}", self.summary.total_findings)?;
        writeln!(f, "  Critical: {}", self.summary.critical_count)?;
        writeln!(f, "  High: {}", self.summary.high_count)?;
        writeln!(f, "  Medium: {}", self.summary.medium_count)?;
        writeln!(f, "  Low: {}", self.summary.low_count)?;
        writeln!(f, "  Info: {}", self.summary.info_count)?;
        writeln!(f)?;

        for finding in &self.findings {
            writeln!(f, "{}", finding)?;
            writeln!(f)?;
        }

        Ok(())
    }
}

pub struct AuditEngine {
    sensitive_patterns: Vec<(String, Regex)>,
    insecure_checks: Vec<InsecureCheck>,
}

struct InsecureCheck {
    rule_id: String,
    title: String,
    description: String,
    recommendation: String,
    severity: AuditSeverity,
    check: Box<dyn Fn(&str, &str) -> bool + Send + Sync>,
}

impl AuditEngine {
    pub fn new() -> Self {
        let sensitive_patterns = vec![
            ("password".to_string(), Regex::new(r"(?i)password|passwd|pwd").unwrap()),
            ("secret".to_string(), Regex::new(r"(?i)secret|secret_key").unwrap()),
            ("token".to_string(), Regex::new(r"(?i)token|api_key|apikey|access_key").unwrap()),
            ("private_key".to_string(), Regex::new(r"(?i)private_key|priv_key").unwrap()),
            ("credential".to_string(), Regex::new(r"(?i)credential|auth_token").unwrap()),
        ];

        let insecure_checks = vec![
            InsecureCheck {
                rule_id: "SEC-001".to_string(),
                title: "Weak encryption algorithm".to_string(),
                description: "MD5 or SHA1 are considered weak for security purposes".to_string(),
                recommendation: "Use SHA-256 or stronger algorithms".to_string(),
                severity: AuditSeverity::High,
                check: Box::new(|_key, value| {
                    value.to_lowercase().contains("md5") || value.to_lowercase().contains("sha1")
                }),
            },
            InsecureCheck {
                rule_id: "SEC-002".to_string(),
                title: "Insecure port configuration".to_string(),
                description: "Using well-known insecure ports (telnet:23, ftp:21)".to_string(),
                recommendation: "Use secure alternatives (SSH:22, SFTP:22)".to_string(),
                severity: AuditSeverity::Medium,
                check: Box::new(|_key, value| {
                    value == "23" || value == "21"
                }),
            },
            InsecureCheck {
                rule_id: "SEC-003".to_string(),
                title: "Debug mode enabled".to_string(),
                description: "Debug mode should not be enabled in production".to_string(),
                recommendation: "Set debug=false in production environments".to_string(),
                severity: AuditSeverity::Medium,
                check: Box::new(|key, value| {
                    key.to_lowercase().contains("debug") && value.to_lowercase() == "true"
                }),
            },
            InsecureCheck {
                rule_id: "SEC-004".to_string(),
                title: "Wildcard host binding".to_string(),
                description: "Binding to 0.0.0.0 exposes the service to all interfaces".to_string(),
                recommendation: "Bind to specific interfaces (127.0.0.1 for local)".to_string(),
                severity: AuditSeverity::Low,
                check: Box::new(|key, value| {
                    key.to_lowercase().contains("host") && value == "0.0.0.0"
                }),
            },
            InsecureCheck {
                rule_id: "SEC-005".to_string(),
                title: "SSL verification disabled".to_string(),
                description: "Disabling SSL verification exposes to MITM attacks".to_string(),
                recommendation: "Enable SSL verification in production".to_string(),
                severity: AuditSeverity::High,
                check: Box::new(|key, value| {
                    (key.to_lowercase().contains("ssl") || key.to_lowercase().contains("verify")) 
                    && value.to_lowercase() == "false"
                }),
            },
        ];

        Self {
            sensitive_patterns,
            insecure_checks,
        }
    }

    pub fn audit_ast(&self, ast: &Ast) -> AuditReport {
        let mut findings = Vec::new();
        self.audit_table_entries(&ast.root.entries, &mut findings, "");
        AuditReport::new(findings)
    }

    pub fn audit_source(&self, source: &str) -> AuditReport {
        match crate::parse(source) {
            Ok(ast) => self.audit_ast(&ast),
            Err(_) => AuditReport::new(vec![]),
        }
    }

    fn audit_table_entries(&self, entries: &[TableEntry], findings: &mut Vec<AuditFinding>, prefix: &str) {
        for entry in entries {
            match entry {
                TableEntry::KeyValue(kv) => {
                    let key_str = kv.key.as_str();
                    let full_key = if prefix.is_empty() {
                        key_str.to_string()
                    } else {
                        format!("{}.{}", prefix, key_str)
                    };

                    match &kv.value {
                        Value::Scalar(scalar) => {
                            let value_str = scalar_to_string(scalar);
                            self.check_sensitive(&full_key, &value_str, findings);
                            self.check_insecure(&full_key, &value_str, findings);
                        }
                        Value::Expression(expr) => {
                            if let Ok(scalar) = expr.evaluate() {
                                let value_str = scalar_to_string(&scalar);
                                self.check_sensitive(&full_key, &value_str, findings);
                                self.check_insecure(&full_key, &value_str, findings);
                            }
                        }
                        Value::TableBlock(table) => {
                            self.audit_table_entries(&table.entries, findings, &full_key);
                        }
                        Value::Array(arr) => {
                            for (i, item) in arr.elements.iter().enumerate() {
                                let item_key = format!("{}[{}]", full_key, i);
                                match item {
                                    Value::Scalar(scalar) => {
                                        let value_str = scalar_to_string(scalar);
                                        self.check_sensitive(&item_key, &value_str, findings);
                                    }
                                    Value::Expression(expr) => {
                                        if let Ok(scalar) = expr.evaluate() {
                                            let value_str = scalar_to_string(&scalar);
                                            self.check_sensitive(&item_key, &value_str, findings);
                                        }
                                    }
                                    Value::TableBlock(table) => {
                                        self.audit_table_entries(&table.entries, findings, &item_key);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
                TableEntry::TableBlock(table) => {
                    let table_name = table.name.as_deref().unwrap_or("");
                    let new_prefix = if prefix.is_empty() {
                        table_name.to_string()
                    } else {
                        format!("{}.{}", prefix, table_name)
                    };
                    self.audit_table_entries(&table.entries, findings, &new_prefix);
                }
                _ => {}
            }
        }
    }

    fn check_sensitive(&self, key: &str, value: &str, findings: &mut Vec<AuditFinding>) {
        for (pattern_name, pattern) in &self.sensitive_patterns {
            if pattern.is_match(key) && !value.is_empty() && value != "null" {
                findings.push(AuditFinding {
                    category: AuditCategory::SensitiveData,
                    severity: AuditSeverity::Critical,
                    rule_id: "SEC-SENS-001".to_string(),
                    title: format!("Sensitive data in configuration: {}", pattern_name),
                    description: format!("The key '{}' appears to contain sensitive data", key),
                    location: key.to_string(),
                    recommendation: "Use environment variables or a secrets manager instead of hardcoding sensitive values".to_string(),
                });
                break;
            }
        }
    }

    fn check_insecure(&self, key: &str, value: &str, findings: &mut Vec<AuditFinding>) {
        for check in &self.insecure_checks {
            if (check.check)(key, value) {
                findings.push(AuditFinding {
                    category: AuditCategory::InsecureConfig.clone(),
                    severity: check.severity.clone(),
                    rule_id: check.rule_id.clone(),
                    title: check.title.clone(),
                    description: check.description.clone(),
                    recommendation: check.recommendation.clone(),
                    location: key.to_string(),
                });
            }
        }
    }
}

fn scalar_to_string(scalar: &ScalarValue) -> String {
    match scalar {
        ScalarValue::String(s) => s.clone(),
        ScalarValue::Number(n) => format!("{}", n),
        ScalarValue::Boolean(b) => if *b { "true" } else { "false" }.to_string(),
        ScalarValue::DateTime(dt) => dt.clone(),
        ScalarValue::Duration(d) => format!("{:?}", d),
    }
}

impl Default for AuditEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensitive_data_detection() {
        let engine = AuditEngine::new();
        let source = r#"
db_password = "super_secret_123"
api_key = "sk-1234567890"
"#;
        let report = engine.audit_source(source);
        assert!(report.findings.iter().any(|f| f.rule_id == "SEC-SENS-001"));
    }

    #[test]
    fn test_weak_encryption_detection() {
        let engine = AuditEngine::new();
        let source = r#"
hash_algorithm = "md5"
"#;
        let report = engine.audit_source(source);
        assert!(report.findings.iter().any(|f| f.rule_id == "SEC-001"));
    }

    #[test]
    fn test_debug_mode_detection() {
        let engine = AuditEngine::new();
        let source = r#"
debug = true
"#;
        let report = engine.audit_source(source);
        assert!(report.findings.iter().any(|f| f.rule_id == "SEC-003"));
    }

    #[test]
    fn test_wildcard_host_detection() {
        let engine = AuditEngine::new();
        let source = r#"
host = "0.0.0.0"
"#;
        let report = engine.audit_source(source);
        assert!(report.findings.iter().any(|f| f.rule_id == "SEC-004"));
    }

    #[test]
    fn test_ssl_verification_disabled() {
        let engine = AuditEngine::new();
        let source = r#"
ssl_verify = false
"#;
        let report = engine.audit_source(source);
        assert!(report.findings.iter().any(|f| f.rule_id == "SEC-005"));
    }

    #[test]
    fn test_audit_report_summary() {
        let engine = AuditEngine::new();
        let source = r#"
db_password = "secret"
debug = true
host = "0.0.0.0"
"#;
        let report = engine.audit_source(source);
        assert!(report.summary.total_findings >= 3);
        assert!(report.summary.critical_count >= 1);
    }

    #[test]
    fn test_clean_config() {
        let engine = AuditEngine::new();
        let source = r#"
port = 8080
host = "127.0.0.1"
debug = false
ssl_verify = true
"#;
        let report = engine.audit_source(source);
        assert_eq!(report.findings.len(), 0);
    }
}

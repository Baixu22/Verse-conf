use std::collections::BTreeMap;

use regex::Regex;

use crate::ast::{Ast, Expression, KeyValue, ScalarValue, TableEntry, Value};

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
            (
                "password".to_string(),
                Regex::new(r"(?i)password|passwd|pwd").unwrap(),
            ),
            (
                "secret".to_string(),
                Regex::new(r"(?i)secret|secret_key").unwrap(),
            ),
            (
                "token".to_string(),
                Regex::new(r"(?i)token|api_key|apikey|access_key").unwrap(),
            ),
            (
                "private_key".to_string(),
                Regex::new(r"(?i)private_key|priv_key").unwrap(),
            ),
            (
                "credential".to_string(),
                Regex::new(r"(?i)credential|auth_token").unwrap(),
            ),
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
                check: Box::new(|_key, value| value == "23" || value == "21"),
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

    fn audit_table_entries(
        &self,
        entries: &[TableEntry],
        findings: &mut Vec<AuditFinding>,
        prefix: &str,
    ) {
        // `[[name]]` 的每个元素在 AST 里都是一条独立的 `ArrayTable`，键名相同、
        // 元素序号不在节点里。序号必须在这里按出现顺序数出来，否则位置标识会
        // 把「元素内第几个键」当成「第几个元素」——`servers[0]` 的第二个键
        // 会被报成 `servers[1]`，指向一个不存在的位置。
        let mut array_elements: BTreeMap<String, usize> = BTreeMap::new();

        for entry in entries {
            match entry {
                TableEntry::KeyValue(kv) => {
                    let full_key = join_key(prefix, kv.key.as_str());
                    self.audit_key_value(kv, &full_key, findings);
                }
                TableEntry::TableBlock(table) => {
                    let new_prefix = join_key(prefix, table.name.as_deref().unwrap_or(""));
                    self.audit_table_entries(&table.entries, findings, &new_prefix);
                }
                // `[[name]]` 的每个元素是同一张表的一份实例。这里以前落进 `_ => {}`，
                // 于是数组表里的值完全不被审计——而真实 TOML（Cargo 清单、测试数据）
                // 的字段大量落在数组表里，等于审计在最常见的形状上是瞎的。
                // 位置标识沿用数组值的写法（`name[i].key`），这样同一份规则在
                // 两种容器上给出的是同一种可定位的实例标识。
                TableEntry::ArrayTable(array_table) => {
                    let name = array_table.key.as_str();
                    let element = array_elements.get(name).copied().unwrap_or(0);
                    array_elements.insert(name.to_string(), element + 1);

                    let table_prefix = join_key(prefix, name);
                    for kv in array_table.entries.iter() {
                        let full_key = format!("{}[{}].{}", table_prefix, element, kv.key.as_str());
                        self.audit_key_value(kv, &full_key, findings);
                    }
                }
                _ => {}
            }
        }
    }

    /// 审计一个键值对。
    ///
    /// 抽出来是为了让「普通表里的键」与「数组表元素里的键」走同一条判定路径：
    /// 各写一遍的话，两条路径的分档迟早会分叉，而「误拒分档」正是这套审计
    /// 唯一需要保持稳定的东西。
    fn audit_key_value(&self, kv: &KeyValue, full_key: &str, findings: &mut Vec<AuditFinding>) {
        let literal_text = is_literal_text(&kv.value);
        match &kv.value {
            Value::Scalar(scalar) => {
                let value_str = scalar_to_string(scalar);
                self.check_sensitive(full_key, &value_str, literal_text, findings);
                self.check_insecure(full_key, &value_str, findings);
            }
            Value::Expression(expr) => {
                if let Ok(scalar) = expr.evaluate() {
                    let value_str = scalar_to_string(&scalar);
                    self.check_sensitive(full_key, &value_str, literal_text, findings);
                    self.check_insecure(full_key, &value_str, findings);
                }
            }
            Value::TableBlock(table) => {
                self.audit_table_entries(&table.entries, findings, full_key);
            }
            Value::Array(arr) => {
                for (i, item) in arr.elements.iter().enumerate() {
                    let item_key = format!("{}[{}]", full_key, i);
                    let item_literal_text = is_literal_text(item);
                    match item {
                        Value::Scalar(scalar) => {
                            let value_str = scalar_to_string(scalar);
                            self.check_sensitive(
                                &item_key,
                                &value_str,
                                item_literal_text,
                                findings,
                            );
                        }
                        Value::Expression(expr) => {
                            if let Ok(scalar) = expr.evaluate() {
                                let value_str = scalar_to_string(&scalar);
                                self.check_sensitive(
                                    &item_key,
                                    &value_str,
                                    item_literal_text,
                                    findings,
                                );
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

    fn check_sensitive(
        &self,
        key: &str,
        value: &str,
        literal_text: bool,
        findings: &mut Vec<AuditFinding>,
    ) {
        for (pattern_name, pattern) in &self.sensitive_patterns {
            if pattern.is_match(key) && !value.is_empty() && value != "null" {
                // 只有「写死在文件里的文本」才可能真的是硬编码凭据。数字与布尔
                // 更可能是数量或开关（`max_tokens = 4096`），表达式则往往正是
                // 审计建议的环境变量引用——这两类都只告警，不阻断写入。
                let (severity, title, description) = if literal_text {
                    (
                        AuditSeverity::Critical,
                        format!("Sensitive data in configuration: {}", pattern_name),
                        format!("The key '{}' appears to contain sensitive data", key),
                    )
                } else {
                    (
                        AuditSeverity::Low,
                        format!(
                            "Sensitive-looking key without a literal secret value: {}",
                            pattern_name
                        ),
                        format!(
                            "The key '{}' matches a sensitive pattern, but its value is not a literal string (a number, a flag or an expression), so it is reported for review instead of blocking the edit",
                            key
                        ),
                    )
                };
                findings.push(AuditFinding {
                    category: AuditCategory::SensitiveData,
                    severity,
                    rule_id: "SEC-SENS-001".to_string(),
                    title,
                    description,
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

/// 高危风险的实例标识：规则码 + 位置。
///
/// 只按 `rule_id` 取集合差会把「同一规则在另一个字段新增的风险」当成已存在：
/// 文件里已有 `primary_ssl_verify = false`，再把 `secondary_ssl_verify` 改成
/// `false` 时集合差为空，编辑被放行，而实际高危项从 1 个变成 2 个。
///
/// 这套「实例级比较」是 `.vcf` 路径与 TOML 路径**共用的同一份实现**：
/// 两条写入路径各写一遍，安全语义迟早会悄悄分叉，而分叉之后
/// 「写入前双重校验」这句话就不再对两种格式同时成立。
pub type RiskInstance = (String, String);

/// 按实例（规则 + 位置）统计高危发现；同一实例出现多次也计数
pub fn high_risk_instances(report: &AuditReport) -> BTreeMap<RiskInstance, usize> {
    let mut counts: BTreeMap<RiskInstance, usize> = BTreeMap::new();
    for finding in &report.findings {
        if matches!(
            finding.severity,
            AuditSeverity::Critical | AuditSeverity::High
        ) {
            *counts
                .entry((finding.rule_id.clone(), finding.location.clone()))
                .or_insert(0) += 1;
        }
    }
    counts
}

/// 相对基线新增的高危实例；同一实例数量变多同样算新增
pub fn introduced_high_risk_instances(
    baseline: &BTreeMap<RiskInstance, usize>,
    after: &BTreeMap<RiskInstance, usize>,
) -> Vec<RiskInstance> {
    after
        .iter()
        .filter(|(instance, count)| **count > baseline.get(*instance).copied().unwrap_or(0))
        .map(|(instance, _)| instance.clone())
        .collect()
}

/// 渲染成 `规则 @ 位置`：拒绝信息必须能指出是哪个实例，而不只是哪条规则
pub fn render_risk_instance(instance: &RiskInstance) -> String {
    format!("{} @ {}", instance.0, instance.1)
}

/// 值是不是「写死在文件里的文本」。
///
/// 只有写死的文本才可能是硬编码凭据。普通字符串在 AST 里是
/// `Expression::Literal`；`${ENV:...}` 这类插值或引用恰恰是审计建议的做法，
/// 数字与布尔更可能是数量或开关（`max_tokens = 4096`）——这两类都不参与阻断判定。
fn is_literal_text(value: &Value) -> bool {
    let scalar = match value {
        Value::Scalar(scalar) => scalar,
        Value::Expression(Expression::Literal(scalar)) => scalar,
        _ => return false,
    };
    match scalar {
        ScalarValue::String(text) => !is_pure_interpolation(text),
        _ => false,
    }
}

/// 整段值就是一个 `${...}` 占位符时，它不是写死的凭据，而是对外部来源的引用。
///
/// 引号里的插值在 AST 里仍然是字符串字面量，但审计的建议正是「改用环境变量」，
/// 所以这种值只告警、不阻断。
fn is_pure_interpolation(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with("${") && trimmed.ends_with('}')
}

/// 把前缀与键名接成位置标识；前缀为空时不要留下开头的点号
fn join_key(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{}.{}", prefix, name)
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
    fn test_numeric_sensitive_looking_key_is_advisory_not_blocking() {
        // `max_tokens` 这类键名包含 token，但值是数量而不是凭据。
        let engine = AuditEngine::new();
        let report = engine.audit_source("max_tokens = 4096\ntoken_budget = 8192\n");
        let sensitive: Vec<&AuditFinding> = report
            .findings
            .iter()
            .filter(|finding| finding.rule_id == "SEC-SENS-001")
            .collect();
        assert_eq!(sensitive.len(), 2, "仍应告警以便人工复核");
        assert!(
            sensitive
                .iter()
                .all(|finding| matches!(finding.severity, AuditSeverity::Low)),
            "数量字段不能报成阻断级"
        );
    }

    #[test]
    fn test_literal_credentials_stay_blocking() {
        let engine = AuditEngine::new();
        let report = engine.audit_source("db_password = \"hunter2\"\napi_key = \"sk-live-1\"\n");
        let sensitive: Vec<&AuditFinding> = report
            .findings
            .iter()
            .filter(|finding| finding.rule_id == "SEC-SENS-001")
            .collect();
        assert_eq!(sensitive.len(), 2);
        assert!(
            sensitive
                .iter()
                .all(|finding| matches!(finding.severity, AuditSeverity::Critical)),
            "写死在文件里的凭据必须仍然阻断"
        );
    }

    #[test]
    fn test_environment_reference_is_not_a_hardcoded_credential() {
        // 用环境变量替代硬编码凭据正是审计建议的做法，不能反过来阻断它。
        let engine = AuditEngine::new();
        for source in [
            "api_key = ${ENV:API_KEY}\n",
            "api_key = \"${ENV:API_KEY}\"\n",
            "password = \"${ENV:DB_PASSWORD:-default}\"\n",
        ] {
            let report = engine.audit_source(source);
            assert!(
                report
                    .findings
                    .iter()
                    .filter(|finding| finding.rule_id == "SEC-SENS-001")
                    .all(|finding| !matches!(finding.severity, AuditSeverity::Critical)),
                "{source:?} 里的环境变量引用不构成硬编码凭据"
            );
        }
    }

    #[test]
    fn test_literal_prefix_before_interpolation_still_blocks() {
        // 前面带真实文本就不再是纯引用，仍按硬编码凭据处理
        let engine = AuditEngine::new();
        let report = engine.audit_source("api_key = \"sk-live-${ENV:SUFFIX}\"\n");
        assert!(report.findings.iter().any(|finding| {
            finding.rule_id == "SEC-SENS-001" && matches!(finding.severity, AuditSeverity::Critical)
        }));
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

    #[test]
    fn test_array_table_values_are_audited() {
        // 回归：数组表以前落进 `_ => {}`，`[[servers]]` 里的硬编码凭据完全不被审计。
        // 真实 TOML（Cargo 清单、测试数据）的字段大量落在数组表里，
        // 这条路径不通等于审计在最常见的形状上是瞎的。
        let engine = AuditEngine::new();
        let source = r#"
[[servers]]
name = "primary"
password = "hunter2"

[[servers]]
name = "replica"
"#;
        let report = engine.audit_source(source);
        let sensitive: Vec<&AuditFinding> = report
            .findings
            .iter()
            .filter(|finding| finding.rule_id == "SEC-SENS-001")
            .collect();
        assert_eq!(sensitive.len(), 1, "数组表里的硬编码凭据必须被报出来");
        assert!(
            matches!(sensitive[0].severity, AuditSeverity::Critical),
            "写死在文件里的凭据必须仍是阻断级"
        );
        assert_eq!(
            sensitive[0].location, "servers[0].password",
            "位置标识必须能定位到具体元素"
        );
    }

    #[test]
    fn test_array_table_without_secrets_stays_clean() {
        // 反向：数组表覆盖不能把正常字段变成误报。`max_tokens` 这类
        // 「键名像敏感字段、值其实是数量」的键仍然只告警，不进入阻断档。
        let engine = AuditEngine::new();
        let source = r#"
[[bench]]
name = "throughput"
harness = false
max_tokens = 4096
"#;
        let report = engine.audit_source(source);
        assert!(
            high_risk_instances(&report).is_empty(),
            "正常数组表不应产生任何高危实例，实际发现：{:?}",
            report
                .findings
                .iter()
                .map(|finding| (finding.rule_id.as_str(), finding.severity.to_string()))
                .collect::<Vec<_>>()
        );
    }
}

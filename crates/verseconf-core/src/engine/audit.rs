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
                self.check_embedded_credential(full_key, &value_str, literal_text, findings);
                self.check_insecure(full_key, &value_str, findings);
            }
            Value::Expression(expr) => {
                if let Ok(scalar) = expr.evaluate() {
                    let value_str = scalar_to_string(&scalar);
                    self.check_sensitive(full_key, &value_str, literal_text, findings);
                    self.check_embedded_credential(full_key, &value_str, literal_text, findings);
                    self.check_insecure(full_key, &value_str, findings);
                }
            }
            Value::TableBlock(table) => {
                self.audit_table_entries(&table.entries, findings, full_key);
            }
            // 内联表在数据模型上与块表是同一种东西，审计必须一视同仁：
            // 此前它落在 `_ => {}` 上，`{ tls_verify = false }` 这种内联写法
            // 完全不被审计（TOML 适配层把内联表翻成这个形状，所以是一条真路径）。
            Value::InlineTable(table) => {
                self.audit_inline_entries(&table.entries, findings, full_key);
            }
            Value::Array(arr) => {
                self.audit_array(&arr.elements, findings, full_key);
            }
        }
    }

    /// 审计一个数组的所有元素。
    ///
    /// 抽出来是为了让「数组里的数组」也走同一条路径：此前嵌套数组落在
    /// `_ => {}` 上，`[[{ "tls_verify": false }]]` 这类更深一层的元素完全不进审计，
    /// 于是门禁对一份**看起来检查过**的候选回答 allowed——这比报错更危险，
    /// 因为调用方会以为它检查过了。位置标识继续按 `name[0][1].key` 往下走。
    ///
    /// 一处刻意保留的不对称：数组元素的标量只做 `check_sensitive`，
    /// 不做 `check_insecure`（与改动前一致）。改这一条会同时改变 `.vcf` 与 TOML
    /// 的分档，属于另一个决定，不该混在「补上递归」里。
    fn audit_array(&self, elements: &[Value], findings: &mut Vec<AuditFinding>, prefix: &str) {
        for (index, item) in elements.iter().enumerate() {
            let item_key = format!("{prefix}[{index}]");
            let item_literal_text = is_literal_text(item);
            match item {
                Value::Scalar(scalar) => {
                    let value_str = scalar_to_string(scalar);
                    self.check_sensitive(&item_key, &value_str, item_literal_text, findings);
                    self.check_embedded_credential(
                        &item_key,
                        &value_str,
                        item_literal_text,
                        findings,
                    );
                }
                Value::Expression(expr) => {
                    if let Ok(scalar) = expr.evaluate() {
                        let value_str = scalar_to_string(&scalar);
                        self.check_sensitive(&item_key, &value_str, item_literal_text, findings);
                        self.check_embedded_credential(
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
                Value::InlineTable(table) => {
                    self.audit_inline_entries(&table.entries, findings, &item_key);
                }
                Value::Array(nested) => {
                    self.audit_array(&nested.elements, findings, &item_key);
                }
            }
        }
    }

    /// 审计一张内联表。
    ///
    /// 内联表的 `entries` 是键值对，不是表条目（`InlineTable` 装不下嵌套的块表），
    /// 所以它不能走 `audit_table_entries`。位置标识与块表保持同一种写法。
    fn audit_inline_entries(
        &self,
        entries: &[KeyValue],
        findings: &mut Vec<AuditFinding>,
        prefix: &str,
    ) {
        for kv in entries {
            let full_key = join_key(prefix, kv.key.as_str());
            self.audit_key_value(kv, &full_key, findings);
        }
    }

    /// 凭据嵌在 URL 值里的形态：`scheme://user:secret@host`。
    ///
    /// 这是一次决定性实验实测确认门禁**看不见**的一类风险（TF-0104）：规则原先只在
    /// **键名**上匹配 password/secret/token/api_key，而模型把镜像源写成
    /// `index = "https://username:password@internal-mirror.example.com/..."` 时，
    /// 键是 `index`、凭据在**值**里。在那次运行里，模型自发写出的 11 次高危改动中
    /// 有 5 次是这个形态——占全部风险的 45%，不是边角。
    ///
    /// 分档与 SEC-SENS-001 同一口径：写死的（literal_text）进阻断级，
    /// 值里是 `${...}` 占位符的不进（那正是建议的写法）。
    fn check_embedded_credential(
        &self,
        key: &str,
        value: &str,
        literal_text: bool,
        findings: &mut Vec<AuditFinding>,
    ) {
        if !has_embedded_credential(value) {
            return;
        }
        let (severity, title, description) = if literal_text {
            (
                AuditSeverity::Critical,
                "Credential embedded in a URL value".to_string(),
                format!(
                    "The value of '{}' carries a user:password pair inside the URL authority, which is a hardcoded credential",
                    key
                ),
            )
        } else {
            (
                AuditSeverity::Low,
                "Credential-shaped URL value without a literal secret".to_string(),
                format!(
                    "The value of '{}' has a user:password shape inside a URL, but the parts are references rather than literal text, so it is reported for review instead of blocking the edit",
                    key
                ),
            )
        };
        findings.push(AuditFinding {
            category: AuditCategory::SensitiveData,
            severity,
            rule_id: "SEC-SENS-002".to_string(),
            title,
            description,
            location: key.to_string(),
            recommendation: "Keep credentials out of URL values: use environment variables, a credential helper or a secrets manager".to_string(),
        });
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
                //
                // 还有第三类：值指向**凭据机制**而不是秘密本身。`credential-provider =
                // "cargo:token"` 说的「用 cargo 的内置 token 提供者」，它是文档推荐的写法
                // （第二轮实验里模型就是这么写的，却被判成硬编码凭据）。这一类也必须看
                // 键的语义——见 `looks_like_credential_mechanism`。
                let literal_text = literal_text && !looks_like_credential_mechanism(key, value);
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
        // `${...}` 引用、`SCREAMING_SNAKE_CASE` 的环境变量名、以及「请替换」的占位文案
        // 都是**指向**凭据的写法或模板，不是凭据本身；审计的建议正是这么写，
        // 不能反过来阻断它。
        ScalarValue::String(text) => {
            !is_pure_interpolation(text)
                && !looks_like_env_var_name(text)
                && !looks_like_placeholder(text)
        }
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

/// 值是否是「请替换」的占位文案，而不是真的凭据。
///
/// 这是第三类假阳性（前两类是环境变量名与凭据机制名），由确定性分类器对四轮实验
/// 真实拒绝的回顾性统计发现：被判成「规则说错了」的拒绝几乎全是占位文案——
/// 模型写的是模板（把你的口令填在这里），门禁把它当成写死的凭据拦下来。
///
/// 判据与 `benchmark/gate-increment/classify.mjs` 的 `isPlaceholder` 保持一致
/// （同一套词表），否则测量与实现会各用一套标准。分两层是为了收得住：
/// 明确的占位标记可以出现在长串里（`ci-REPLACE_WITH_REAL_TOKEN`），
/// 而 `example`、`password` 这类通用词只有**整段等于它**时才算占位——
/// 否则 `AKIAIOSFODNN7EXAMPLE` 与 `cargo-user:cargo-password` 这种真凭据形态
/// 会因为含子串被误放行（这两条都是写谓词时被测试抓出来的）。
fn looks_like_placeholder(text: &str) -> bool {
    const MARKERS: &[&str] = &[
        "replace",
        "change_me",
        "change-me",
        "changeme",
        "placeholder",
        "your_",
        "your-",
    ];
    // 只收**无歧义**的占位词。`secret` / `password` / `token` / `key` 这类通用词不收：
    // 它们同样是被随手写死的值（既有测试 `db_password = "secret"` 正依赖这一点），
    // 收进来会把真凭据放行——安全门禁宁可保守。URL 里的角色词另见下一条函数。
    const WORDS: &[&str] = &[
        "your",
        "yours",
        "replace",
        "todo",
        "dummy",
        "placeholder",
        "example",
        "sample",
        "changeme",
        "foo",
        "bar",
    ];

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lowered = trimmed.to_ascii_lowercase();

    if MARKERS.iter().any(|marker| lowered.contains(marker)) {
        return true;
    }
    // `<token>` 这类尖括号占位
    if trimmed.contains('<') && trimmed.contains('>') {
        return true;
    }
    if WORDS.contains(&lowered.as_str()) {
        return true;
    }
    // `xxx` / `xxxx` 这类占位
    if lowered.len() >= 3 && lowered.chars().all(|c| c == 'x') {
        return true;
    }
    false
}

/// URL 的 userinfo 段里，**角色词**也算占位。
///
/// `https://username:password@host` 与 `https://YOUR_USERNAME:YOUR_PASSWORD@host`
/// 是模板，不是写死的凭据——这类形态在四轮实验里被门禁误拦过。
/// 判据只在 URL 的这一段生效：裸值写 `username` 仍然算写死的值（fail-closed）。
fn looks_like_url_role_placeholder(part: &str) -> bool {
    const ROLES: &[&str] = &[
        "username", "user", "password", "pass", "passwd", "token", "secret", "key", "apikey",
        "api_key",
    ];
    if looks_like_placeholder(part) {
        return true;
    }
    ROLES.contains(&part.trim().to_ascii_lowercase().as_str())
}

/// 值形如 `SCREAMING_SNAKE_CASE` 时，它是**指向环境变量的名字**，不是凭据本身。
///
/// 这是决定性实验暴露出来的一次误拒（TF-0102 的裁决正是被它推过预算的）：
/// 模型写下 `token-env = "PANDAS_INTERNAL_UPLOAD_TOKEN"`——审计自己建议的写法——
/// 而规则看到「键名含 token + 值是字符串」就判成 Critical 硬编码凭据，
/// 于是用户正常的安全写法被拦下来。
///
/// 判据故意收得紧，宁可漏掉「恰好长得像环境变量名的真凭据」，也不放宽到
/// 把真凭据当引用：全大写 + 至少一个下划线 + 只含大写字母/数字/下划线 + 长度有上限。
/// `AKIAIOSFODNN7EXAMPLE`（AWS 那种无下划线全大写）与 base64/混合大小写的真凭据
/// 都不满足，仍然按 Critical 处理。
fn looks_like_env_var_name(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.len() >= 3
        && trimmed.len() <= 64
        && trimmed.contains('_')
        && trimmed.starts_with(|c: char| c.is_ascii_uppercase())
        && trimmed
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// 值是否在说「用哪个**凭据机制**」，而不是「秘密是什么」。
///
/// 触发这一条的是第二轮实验里的一次确凿误拒：模型写下
/// `credential-provider = "cargo:token"`（Cargo 文档里的推荐写法，意思是
/// 「用 cargo 内置的 token 提供者」），`credential` 模式命中键名 + 值是字符串，
/// 于是被判成 Critical 硬编码凭据。
///
/// 豁免故意要**同时**满足两个条件，缺一不可：
///
/// 1. **键的语义是机制**——只有一张很短的键名单（`credential-provider` /
///    `credential-helper` / `auth-method` 等）。`credential = "..."` 这种
///    「键就是秘密」的写法不在名单里，照旧阻断。
/// 2. **值的形态是机制名**——可选的 `provider:` 前缀 + 只含小写字母/连字符/下划线的
///    机制名，不带数字、空白或其它符号。所以 `user:hunter2`、`hunter2`、
///    `AKIAIOSFODNN7EXAMPLE` 都不满足，仍然按 Critical 处理——即便它们恰好写在
///    `credential-provider` 这个键下面。
///
/// 键可能带路径前缀（`registries.private-registry.credential-provider`），
/// 所以只看最后一段。
fn looks_like_credential_mechanism(key: &str, text: &str) -> bool {
    const MECHANISM_KEYS: &[&str] = &[
        "credential-provider",
        "credential_provider",
        "credential-helper",
        "credential_helper",
        "credential-process",
        "credential_process",
        "credential-store",
        "credential_store",
        "auth-provider",
        "auth_provider",
        "auth-method",
        "auth_method",
    ];

    let key_lower = key.to_ascii_lowercase();
    let last_segment = key_lower.rsplit('.').next().unwrap_or(&key_lower);
    if !MECHANISM_KEYS.contains(&last_segment) {
        return false;
    }

    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.len() > 64 || trimmed.contains(char::is_whitespace) {
        return false;
    }

    let mechanism_part = |part: &str| {
        !part.is_empty()
            && part.len() <= 32
            && part.starts_with(|c: char| c.is_ascii_lowercase())
            && part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '-' || c == '_')
    };

    match trimmed.split_once(':') {
        Some((provider, mechanism)) => mechanism_part(provider) && mechanism_part(mechanism),
        None => mechanism_part(trimmed),
    }
}

/// 值里是否嵌着一对 `user:password`（URL 的 authority 段）。
///
/// 只在**字符串值**上判定，且必须同时满足三件事，缺一不可：
/// authority 里有 `@`（否则 `host:8443` 这种「主机 + 端口」会被误判）、
/// `@` 之前是 `user:password` 两段、两段都是实打实的文本（含 `${` 按引用处理，
/// 与 `is_pure_interpolation` 同一族口径）。
///
/// `@` 按**最后**一个切分：口令里出现未转义的 `@` 时，标准解析也是这么做的。
fn has_embedded_credential(text: &str) -> bool {
    let Some(scheme_end) = text.find("://") else {
        return false;
    };
    let rest = &text[scheme_end + 3..];
    let authority_end = rest
        .find(|c: char| c == '/' || c == '?' || c == '#' || c.is_whitespace())
        .unwrap_or(rest.len());
    let authority = &rest[..authority_end];

    let Some((userinfo, _host)) = authority.rsplit_once('@') else {
        // 没有 `@` 就没有 userinfo：`host:8443` 是主机加端口，不是凭据
        return false;
    };
    let Some((user, password)) = userinfo.split_once(':') else {
        return false;
    };
    // 任一段是 `${...}` 引用或「请替换」的占位文案时，这只是模板，不是写死的凭据
    // （判据与 `looks_like_placeholder` 同一套词表，也与测量侧的 classify.mjs 一致）。
    !user.is_empty()
        && !password.is_empty()
        && !user.contains("${")
        && !password.contains("${")
        && !looks_like_url_role_placeholder(user)
        && !looks_like_url_role_placeholder(password)
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
    fn test_nested_arrays_are_audited() {
        // 回归（独立复核发现）：数组里的数组以前落进 `_ => {}`，所以
        // `{"list": [[{"tls_verify": false}]]}` 这种两层数组里的表完全不被审计，
        // 门禁对一份**看起来检查过**的候选回答 allowed。
        use crate::ast::{ArrayValue, Key, Span, TableBlock};

        let engine = AuditEngine::new();
        let leaf = Value::TableBlock(TableBlock {
            name: None,
            entries: vec![TableEntry::KeyValue(KeyValue {
                key: Key::BareKey("tls_verify".to_string()),
                value: Value::Scalar(ScalarValue::Boolean(false)),
                metadata: None,
                comment: None,
                span: Span::unknown(),
            })],
            span: Span::unknown(),
        });
        let inner = Value::Array(ArrayValue {
            elements: vec![leaf],
            span: Span::unknown(),
        });
        let outer = Value::Array(ArrayValue {
            elements: vec![inner],
            span: Span::unknown(),
        });
        let ast = Ast {
            root: TableBlock {
                name: None,
                entries: vec![TableEntry::KeyValue(KeyValue {
                    key: Key::BareKey("list".to_string()),
                    value: outer,
                    metadata: None,
                    comment: None,
                    span: Span::unknown(),
                })],
                span: Span::unknown(),
            },
            schema: None,
            source: crate::ast::SourceInfo {
                path: None,
                content: String::new(),
            },
        };

        let report = engine.audit_ast(&ast);
        let instances = high_risk_instances(&report);
        assert!(
            instances
                .keys()
                .any(|(rule, location)| rule == "SEC-005" && location == "list[0][0].tls_verify"),
            "两层数组里的关证书校验必须被报出来，实际：{:?}",
            instances.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_inline_table_values_are_audited() {
        // 回归（同一类洞）：内联表以前也落进 `_ => {}`，
        // `db = { password = "hunter2" }` 这种写法完全不被审计。
        use crate::ast::{InlineTable, Key, Span};

        let engine = AuditEngine::new();
        let ast = Ast {
            root: crate::ast::TableBlock {
                name: None,
                entries: vec![TableEntry::KeyValue(KeyValue {
                    key: Key::BareKey("db".to_string()),
                    value: Value::InlineTable(InlineTable {
                        entries: vec![KeyValue {
                            key: Key::BareKey("password".to_string()),
                            value: Value::Scalar(ScalarValue::String("hunter2".to_string())),
                            metadata: None,
                            comment: None,
                            span: Span::unknown(),
                        }],
                        span: Span::unknown(),
                    }),
                    metadata: None,
                    comment: None,
                    span: Span::unknown(),
                })],
                span: Span::unknown(),
            },
            schema: None,
            source: crate::ast::SourceInfo {
                path: None,
                content: String::new(),
            },
        };

        let report = engine.audit_ast(&ast);
        let instances = high_risk_instances(&report);
        assert!(
            instances
                .keys()
                .any(|(rule, location)| rule == "SEC-SENS-001" && location == "db.password"),
            "内联表里的硬编码凭据必须被报出来，实际：{:?}",
            instances.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_credential_embedded_in_a_url_value_is_blocking() {
        // 决定性实验实测：模型把镜像源写成 URL 内嵌凭据，而规则原先只看键名，
        // 于是这一类风险（占那次观测到全部风险的 45%）完全不被看见。
        //
        // 注意这里用的是**真实凭据形态**：TF-0104 当时写的 `username:password` 其实
        // 本身就是模板文案，在新判据下属于「占位」而不是写死的凭据（见下一条测试）。
        let engine = AuditEngine::new();
        let source =
            "index = \"https://cargo-user:cargo-password@internal-mirror.example.com/crates.io-index\"\n";
        let report = engine.audit_source(source);

        let instances = high_risk_instances(&report);
        assert!(
            instances
                .keys()
                .any(|(rule, location)| rule == "SEC-SENS-002" && location == "index"),
            "URL 里的 user:password 必须被报成高危实例，实际：{:?}",
            instances.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_a_placeholder_url_credential_is_not_blocking() {
        // `YOUR_USERNAME:YOUR_PASSWORD` / `username:password` 是模板，不是写死的凭据。
        // 这是第三类假阳性（占位文案），由确定性分类器对四轮实验真实拒绝的
        // 回顾性统计发现——当时模型写的正是这种模板，却被门禁拦下来。
        let engine = AuditEngine::new();
        for source in [
            "registry = \"sparse+https://YOUR_USERNAME:YOUR_PASSWORD@mirror.example.com/index/\"\n",
            "registry = \"sparse+https://username:password@mirror.example.com/index/\"\n",
            "registry = \"https://<user>:<password>@mirror.example.com/index/\"\n",
        ] {
            let report = engine.audit_source(source);
            assert!(
                high_risk_instances(&report).is_empty(),
                "{source:?} 是占位模板，不该阻断，实际：{:?}",
                high_risk_instances(&report).keys().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn test_placeholder_credentials_are_not_blocking() {
        // 值写着「请替换」时不是秘密：这类形态在四轮实验的真实拒绝里占了大头。
        let engine = AuditEngine::new();
        for source in [
            "password = \"your-password\"\n",
            "token = \"ci-REPLACE_WITH_REAL_TOKEN\"\n",
            "api_key = \"YOUR_API_KEY\"\n",
            "token = \"<token>\"\n",
            "secret = \"changeme\"\n",
        ] {
            let report = engine.audit_source(source);
            assert!(
                high_risk_instances(&report).is_empty(),
                "{source:?} 是占位文案，不该阻断，实际：{:?}",
                high_risk_instances(&report).keys().collect::<Vec<_>>()
            );
            assert!(
                report
                    .findings
                    .iter()
                    .any(|finding| finding.rule_id == "SEC-SENS-001"),
                "{source:?} 仍应低档告警供人工复核"
            );
        }
    }

    #[test]
    fn test_real_credentials_still_block_next_to_placeholders() {
        // 反向：占位豁免必须收得住，不能把真凭据形态放过去。
        // `AKIAIOSFODNN7EXAMPLE` 含 `example`、`cargo-user:cargo-password` 含 `user`
        // 与 `password`——两条都是写谓词时被测试抓出来的危险子串。
        let engine = AuditEngine::new();
        for source in [
            "upload-token = \"ghp_internal_upload_9f8e7d6c5b4a3f2e1d0c9b8a7f6e5d4c3b2a1f0e\"\n",
            "token = \"AKIAIOSFODNN7EXAMPLE\"\n",
            "password = \"cargo-password\"\n",
            "token = \"pypi_live_9c1d4e\"\n",
        ] {
            let report = engine.audit_source(source);
            let sensitive: Vec<&AuditFinding> = report
                .findings
                .iter()
                .filter(|finding| finding.rule_id == "SEC-SENS-001")
                .collect();
            assert!(
                sensitive
                    .iter()
                    .any(|finding| matches!(finding.severity, AuditSeverity::Critical)),
                "{source:?} 是真凭据形态，必须仍然是阻断级"
            );
        }
    }

    #[test]
    fn test_a_url_without_a_credential_stays_clean() {
        let engine = AuditEngine::new();
        for source in [
            "index = \"https://mirror.example.com/crates.io-index\"\n",
            "index = \"https://user@mirror.example.com/crates.io-index\"\n",
            "index = \"https://mirror.example.com:8443/crates.io-index\"\n",
            "homepage = \"https://example.com/a:b@c\"\n",
        ] {
            let report = engine.audit_source(source);
            assert!(
                high_risk_instances(&report).is_empty(),
                "{source:?} 不含凭据，不该产生高危实例，实际：{:?}",
                high_risk_instances(&report).keys().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn test_a_url_with_placeholder_credentials_is_not_blocking() {
        // 两段都是 `${...}` 引用时，它是**建议的写法**，只告警不阻断
        let engine = AuditEngine::new();
        let source =
            "index = \"https://${REGISTRY_USER}:${REGISTRY_PASSWORD}@mirror.example.com/\"\n";
        let report = engine.audit_source(source);
        assert!(
            high_risk_instances(&report).is_empty(),
            "占位符凭据不该阻断，实际：{:?}",
            high_risk_instances(&report).keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_env_var_name_is_not_a_hardcoded_credential() {
        // 裁决文档里那一次误拒的原文：值是指向环境变量的**名字**，正是审计建议的写法。
        // 它仍然要告警（键名像敏感字段），但不能进阻断档。
        let engine = AuditEngine::new();
        let source = "token-env = \"PANDAS_INTERNAL_UPLOAD_TOKEN\"\n";
        let report = engine.audit_source(source);

        assert!(
            high_risk_instances(&report).is_empty(),
            "指向环境变量的名字不是硬编码凭据，不该阻断，实际：{:?}",
            high_risk_instances(&report).keys().collect::<Vec<_>>()
        );
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.rule_id == "SEC-SENS-001"),
            "仍应告警以便人工复核"
        );
    }

    #[test]
    fn test_a_real_secret_still_blocks_after_the_env_name_exemption() {
        // 反向：豁免必须收得紧，不能把真凭据也放过去
        let engine = AuditEngine::new();
        for source in [
            // 混合大小写 + 下划线的真 token
            "token = \"pypi_live_9c1d4e\"\n",
            "api_key = \"sk-live-4d2e7f\"\n",
            // 全大写但没有下划线（AWS 那种形态）
            "token = \"AKIAIOSFODNN7EXAMPLE\"\n",
            // 全大写、有下划线、但明显是值不是名字（含 base64 常见字符）
            "token = \"ABC_DEF_9+/=\"\n",
        ] {
            let report = engine.audit_source(source);
            let sensitive: Vec<&AuditFinding> = report
                .findings
                .iter()
                .filter(|finding| finding.rule_id == "SEC-SENS-001")
                .collect();
            assert!(
                sensitive
                    .iter()
                    .any(|finding| matches!(finding.severity, AuditSeverity::Critical)),
                "{source:?} 是真凭据，必须仍然是阻断级"
            );
        }
    }

    #[test]
    fn test_a_credential_mechanism_name_is_not_a_hardcoded_credential() {
        // 第二轮实验里那一次确凿误拒的原文：值说的是「用哪个提供者」，
        // 是 Cargo 文档推荐的写法，不是秘密。
        let engine = AuditEngine::new();
        for source in [
            "credential-provider = \"cargo:token\"\n",
            "credential-helper = \"libsecret\"\n",
            "auth-method = \"oauth-device\"\n",
            // 带路径的键也要认（真实 Cargo 清单里就是这个形状）
            "[registries.private-registry]\ncredential-provider = \"cargo:token\"\n",
        ] {
            let report = engine.audit_source(source);
            assert!(
                high_risk_instances(&report).is_empty(),
                "{source:?} 里的值指向凭据机制而不是秘密，不该阻断，实际：{:?}",
                high_risk_instances(&report).keys().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn test_a_real_secret_still_blocks_next_to_mechanism_keys() {
        // 豁免必须收得紧：换了键、或者值长得像秘密，就照旧阻断
        let engine = AuditEngine::new();
        for source in [
            // 键就是秘密（不在机制键名单里）
            "credential = \"user:hunter2\"\n",
            // 机制键，但值是秘密形态：带数字、大写、或就是口令
            "credential-provider = \"user:hunter2\"\n",
            "credential-provider = \"hunter2\"\n",
            "credential-provider = \"AKIAIOSFODNN7EXAMPLE\"\n",
            "credential-provider = \"sk-live-9c1d4e\"\n",
        ] {
            let report = engine.audit_source(source);
            let sensitive: Vec<&AuditFinding> = report
                .findings
                .iter()
                .filter(|finding| finding.rule_id == "SEC-SENS-001")
                .collect();
            assert!(
                sensitive
                    .iter()
                    .any(|finding| matches!(finding.severity, AuditSeverity::Critical)),
                "{source:?} 是秘密形态，必须仍然是阻断级"
            );
        }
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

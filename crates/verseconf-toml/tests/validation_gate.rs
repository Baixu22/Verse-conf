//! TF-0076 / TF-0077：TOML 写入前校验层的正反两面回归。
//!
//! 这里测的是**项目声称的核心价值**：写入前 schema 与安全双重校验，在 TOML 上
//! 也成立。此前 TOML 侧这两项都不存在，所以「写入前双重校验」这句话在 TOML 上
//! 一次都没被测到。
//!
//! 每一组都同时给出放行侧与拒绝侧：只测拒绝会放过「一律拒绝」的实现，
//! 只测放行会放过「什么都不查」的实现。

use serde_json::json;

use verseconf_core::{AuditEngine, AuditReport, AuditSeverity, EditPlan, EditRefusal};
use verseconf_toml::{
    apply_toml_edit, apply_toml_edit_with, audit_toml, validate_toml_against_schema, TomlGuard,
};

const SOURCE: &str = "\
# 一份真实的 TOML 片段
model = \"gpt-5-codex\"
port = 8080

[features]
multi_agent = false
";

/// 与 `.vcf` 同源的 schema：同一套 `#@schema` 文本、同一套词汇表。
const SCHEMA: &str = r#"#@schema {
    version = "1.0"
    model {
        type = "string"
        required = true
    }
    port {
        type = "integer"
        required = true
        range = (1024..65535)
    }
}
"#;

fn plan(value: serde_json::Value) -> EditPlan {
    EditPlan::from_json(&value.to_string()).expect("测试用的计划应当合法")
}

fn set(path: serde_json::Value, value: serde_json::Value) -> EditPlan {
    plan(json!({ "version": "1.0", "edits": [{ "op": "set", "path": path, "value": value }] }))
}

// ————————————————————————— TF-0076：schema 校验 —————————————————————————

#[test]
fn a_change_that_keeps_the_schema_valid_is_allowed() {
    let outcome = apply_toml_edit_with(
        SOURCE,
        &set(json!(["model"]), json!("gpt-5")),
        &TomlGuard::with_schema(SCHEMA),
    )
    .expect("满足 schema 的改动必须放行");

    assert_eq!(
        outcome.source,
        SOURCE.replace("\"gpt-5-codex\"", "\"gpt-5\"")
    );
}

#[test]
fn a_change_that_breaks_the_schema_is_refused_with_a_structured_code() {
    let refusal = apply_toml_edit_with(
        SOURCE,
        &set(json!(["port"]), json!("not-a-number")),
        &TomlGuard::with_schema(SCHEMA),
    )
    .expect_err("类型不符必须拒绝");

    assert_eq!(
        refusal.code(),
        "validation_failed",
        "拒绝码必须与 .vcf 路径同一套"
    );
    assert_eq!(refusal.details()["path"], "<result>");
}

#[test]
fn a_range_violation_is_refused() {
    let refusal = apply_toml_edit_with(
        SOURCE,
        &set(json!(["port"]), json!(80)),
        &TomlGuard::with_schema(SCHEMA),
    )
    .expect_err("越界必须拒绝");
    assert_eq!(refusal.code(), "validation_failed");

    // 反向：同一字段的合法取值必须放行，否则上面的拒绝可能只是「一律拒绝」
    apply_toml_edit_with(
        SOURCE,
        &set(json!(["port"]), json!(9090)),
        &TomlGuard::with_schema(SCHEMA),
    )
    .expect("区间内的取值必须放行");
}

#[test]
fn nested_table_schema_applies_to_toml_tables() {
    const NESTED: &str = r#"#@schema {
    features {
        type = "table"
        multi_agent { type = "boolean" required = true }
    }
}
"#;
    let refusal = apply_toml_edit_with(
        SOURCE,
        &set(json!(["features", "multi_agent"]), json!("yes")),
        &TomlGuard::with_schema(NESTED),
    )
    .expect_err("嵌套字段类型不符必须拒绝");
    assert_eq!(refusal.code(), "validation_failed");

    apply_toml_edit_with(
        SOURCE,
        &set(json!(["features", "multi_agent"]), json!(true)),
        &TomlGuard::with_schema(NESTED),
    )
    .expect("嵌套字段类型相符必须放行");
}

#[test]
fn strict_mode_from_the_same_schema_vocabulary_applies_to_toml() {
    const STRICT: &str = "#@schema {\n  strict = true\n  model { type = \"string\" }\n}\n";

    let refusal = apply_toml_edit_with(
        "model = \"a\"\nextra = 1\n",
        &set(json!(["model"]), json!("b")),
        &TomlGuard::with_schema(STRICT),
    )
    .expect_err("严格模式下未声明字段必须被拒");
    assert_eq!(refusal.code(), "validation_failed");

    let outcome = apply_toml_edit_with(
        "model = \"a\"\n",
        &set(json!(["model"]), json!("b")),
        &TomlGuard::with_schema(STRICT),
    )
    .expect("声明齐全时应当放行");
    assert_eq!(outcome.source, "model = \"b\"\n");
}

#[test]
fn without_a_schema_only_structure_and_audit_run() {
    // 与 .vcf 同一种口径：文档没有 schema 就不做 schema 校验。
    let outcome = apply_toml_edit(SOURCE, &set(json!(["port"]), json!("not-a-number")))
        .expect("没有 schema 时不该做 schema 校验");
    assert!(outcome.source.contains("port = \"not-a-number\""));
}

#[test]
fn a_schema_text_without_a_schema_block_is_refused_explicitly() {
    let refusal = apply_toml_edit_with(
        SOURCE,
        &set(json!(["model"]), json!("x")),
        &TomlGuard::with_schema("model = \"x\"\n"),
    )
    .expect_err("没有 #@schema 块时必须明确拒绝，而不是静默跳过");
    assert_eq!(refusal.code(), "validation_failed");
    assert_eq!(refusal.details()["path"], "<schema>");
}

#[test]
fn the_schema_check_is_usable_on_its_own() {
    assert!(validate_toml_against_schema(SOURCE, SCHEMA).is_ok());
    assert!(
        validate_toml_against_schema("model = 1\nport = 8080\n", SCHEMA).is_err(),
        "model 声明为 string，给整数必须报错"
    );
}

// ————————————————————————— TF-0077：安全审计 —————————————————————————

#[test]
fn a_literal_credential_is_refused_and_the_instance_is_named() {
    // 基线里 `api_key` 是整段 `${...}` 引用（审计建议的做法，只告警不阻断），
    // 把它换成写死的凭据就是**新引入**一个高危实例。
    let source = "model = \"gpt-5\"\napi_key = \"${ENV:API_KEY}\"\n";
    match apply_toml_edit(source, &set(json!(["api_key"]), json!("sk-live-123")))
        .expect_err("写死凭据必须被拒绝")
    {
        EditRefusal::SecurityRejected {
            findings,
            instances,
            ..
        } => {
            assert!(
                findings.iter().any(|rule| rule == "SEC-SENS-001"),
                "必须指出被引入的规则，实际 {findings:?}"
            );
            assert_eq!(
                instances,
                vec!["SEC-SENS-001 @ api_key".to_string()],
                "拒绝信息必须指出是哪个实例"
            );
        }
        other => panic!("期望 SecurityRejected，实际 {other:?}"),
    }
}

#[test]
fn a_numeric_sensitive_looking_field_is_not_blocked() {
    // `max_tokens` 的键名命中 token 模式，但值是数量而不是凭据——
    // 误拒分档必须与 .vcf 路径一致：只告警，不阻断。
    let outcome = apply_toml_edit(
        "max_tokens = 1024\n",
        &set(json!(["max_tokens"]), json!(4096)),
    )
    .expect("数量字段不是凭据，不能阻断写入");
    assert!(outcome.source.contains("max_tokens = 4096"));
}

#[test]
fn an_environment_reference_is_not_treated_as_a_hardcoded_credential() {
    let outcome = apply_toml_edit(
        "api_key = \"placeholder\"\n",
        &set(json!(["api_key"]), json!("${ENV:API_KEY}")),
    )
    .expect("整段 ${...} 引用是审计建议的做法，不能反过来阻断它");
    assert!(outcome.source.contains("${ENV:API_KEY}"));
}

#[test]
fn a_second_instance_of_an_already_present_rule_is_refused() {
    // 只按 rule_id 取集合差会漏掉「同一规则在另一个字段新增的风险」。
    let source = "primary_ssl_verify = false\nsecondary_ssl_verify = true\n";
    match apply_toml_edit(source, &set(json!(["secondary_ssl_verify"]), json!(false)))
        .expect_err("第二个高危实例必须被拒绝")
    {
        EditRefusal::SecurityRejected {
            findings,
            instances,
            ..
        } => {
            assert_eq!(findings, vec!["SEC-005".to_string()]);
            assert_eq!(
                instances,
                vec!["SEC-005 @ secondary_ssl_verify".to_string()],
                "拒绝信息必须指出新增的是哪个实例"
            );
        }
        other => panic!("期望 SecurityRejected，实际 {other:?}"),
    }
}

#[test]
fn a_preexisting_high_risk_instance_does_not_block_an_unrelated_edit() {
    // 文件本来就有的问题不该让这次编辑背，否则「安全门禁」会变成「什么都改不了」。
    let source = "ssl_verify = false\nport = 8080\n";
    let outcome =
        apply_toml_edit(source, &set(json!(["port"]), json!(9090))).expect("不应因既有高危项误拒");
    assert_eq!(outcome.source, "ssl_verify = false\nport = 9090\n");
}

#[test]
fn every_high_risk_rule_blocks_when_newly_introduced() {
    // 反向覆盖：不只是 SEC-005，任何新增的高危实例都必须导致拒绝。
    let cases: [(&str, &str, serde_json::Value); 3] = [
        ("ssl_verify = true\n", "ssl_verify", json!(false)),
        ("hash = \"sha256\"\n", "hash", json!("md5")),
        (
            "db_password = \"${ENV:DB_PASSWORD}\"\n",
            "db_password",
            json!("hunter2"),
        ),
    ];
    for (source, key, value) in cases {
        let refusal = apply_toml_edit(source, &set(json!([key]), value))
            .expect_err(&format!("{source} 上写入 {key} 必须被拒绝"));
        assert!(
            matches!(refusal, EditRefusal::SecurityRejected { .. }),
            "{source} + {key} 应当报 security_rejected，实际 {refusal:?}"
        );
    }
}

#[test]
fn the_toml_audit_reaches_array_tables_and_locates_the_instance() {
    // 真实 TOML 的字段大量落在 `[[x]]` 里；审计够不到数组表就等于在最常见的
    // 形状上是瞎的。
    let source = "\
[[servers]]
name = \"primary\"
password = \"hunter2\"

[[servers]]
name = \"replica\"
";
    let report = audit_toml(source).expect("应当能审计");

    let critical: Vec<&str> = report
        .findings
        .iter()
        .filter(|finding| matches!(finding.severity, AuditSeverity::Critical))
        .map(|finding| finding.location.as_str())
        .collect();
    assert_eq!(
        critical,
        vec!["servers[0].password"],
        "位置必须能定位到具体元素"
    );
}

#[test]
fn the_same_content_gets_the_same_banding_on_both_paths() {
    // 「复用同一套误拒分档」不能只是说法：同一份内容在 .vcf 路径与 TOML 路径上
    // 必须给出同样的严重度。
    let cases = [
        ("db_password = \"hunter2\"\n", "db_password = \"hunter2\"\n"),
        ("max_tokens = 4096\n", "max_tokens = 4096\n"),
        (
            "api_key = \"${ENV:API_KEY}\"\n",
            "api_key = \"${ENV:API_KEY}\"\n",
        ),
    ];

    fn sensitive_banding(report: &AuditReport) -> Vec<String> {
        let mut out: Vec<String> = report
            .findings
            .iter()
            .filter(|finding| finding.rule_id == "SEC-SENS-001")
            .map(|finding| finding.severity.to_string())
            .collect();
        out.sort();
        out
    }

    for (vcf, toml) in cases {
        let vcf_report = AuditEngine::new().audit_source(vcf);
        let toml_report = audit_toml(toml).expect("TOML 应当能审计");
        assert_eq!(
            sensitive_banding(&vcf_report),
            sensitive_banding(&toml_report),
            "同一内容在两条路径上的分档必须一致：{vcf:?}"
        );
    }
}

// ————————————————————— 消融对照臂与字节保真 ——————————————————————

#[test]
fn the_mechanism_only_arm_keeps_the_edit_the_gate_would_refuse() {
    // 消融实验（TF-0079）要测「校验层贡献了什么」，对照臂就必须真的关掉校验层。
    let source = "ssl_verify = true\n";
    let plan = set(json!(["ssl_verify"]), json!(false));

    assert!(
        apply_toml_edit(source, &plan).is_err(),
        "默认写入路径必须拒绝新引入的高危实例"
    );

    let outcome = apply_toml_edit_with(source, &plan, &TomlGuard::edit_mechanism_only())
        .expect("对照臂应当放行");
    assert_eq!(outcome.source, "ssl_verify = false\n");
}

#[test]
fn a_passing_edit_still_preserves_crlf_and_comments() {
    // 加了校验层之后，编辑机制原有的字节保真不能被破坏。
    let source = "# 顶部注释\r\nmodel = \"a\" # 行尾注释\r\n\r\n[t]\r\nx = 1\r\n";
    let outcome = apply_toml_edit(source, &set(json!(["t", "x"]), json!(2))).expect("应当放行");

    assert_eq!(outcome.source, source.replace("x = 1", "x = 2"));
    assert!(outcome.source.contains("\r\n"), "CRLF 必须保持");
    assert!(outcome.source.contains("# 行尾注释"), "行尾注释必须保持");
}

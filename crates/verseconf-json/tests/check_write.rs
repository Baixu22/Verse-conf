//! 写前检查在 JSON / JSONC 上的回归（TF-0096 的门禁面）。
//!
//! 这一组测的不是编辑机制，而是**门禁能不能脱离编辑机制单独用**：
//! 候选文本由宿主的任意编辑方式产生，门禁只回答「允许落盘吗」。
//!
//! 与 `.vcf` / TOML 路径同源：同一份 `AuditEngine`、同一份实例级比较、
//! 同一套拒绝码。每一组都给出放行侧与拒绝侧——只测拒绝会放过「一律拒绝」的实现。

use verseconf_core::{EditValue, PathSegment};
use verseconf_json::{
    check_write_json, check_write_json_with, set_json_value, JsonFlavor, JsonGuard,
};

/// 宿主的编辑方式：朴素字符串替换。**完全不含编辑计划协议**——这正是解绑的意义。
fn string_replace(source: &str, from: &str, to: &str) -> String {
    source.replacen(from, to, 1)
}

fn key(path: &str) -> Vec<PathSegment> {
    path.split('.')
        .map(|part| PathSegment::Key(part.to_string()))
        .collect()
}

#[test]
fn a_newly_introduced_high_risk_instance_is_refused() {
    let baseline = r#"{"tls_verify": true, "port": 8080}"#;
    let candidate = string_replace(baseline, "true", "false");

    let refusal = check_write_json(baseline, &candidate).expect_err("关掉证书校验必须被拒绝");
    assert_eq!(refusal.code(), "security_rejected");
    let instances = refusal.details()["instances"].to_string();
    assert!(
        instances.contains("tls_verify"),
        "拒绝信息必须指出是哪个实例，实际 {instances}"
    );
}

#[test]
fn a_preexisting_risk_is_not_blamed_on_this_edit() {
    // 文件本来就把证书校验关了；这次只改端口，不该被拒绝
    let baseline = r#"{"tls_verify": false, "port": 8080}"#;
    let candidate = string_replace(baseline, "8080", "9090");

    check_write_json(baseline, &candidate).expect("不因文件本来就有的问题拒绝这次改动");
}

#[test]
fn a_second_instance_of_an_existing_rule_is_refused() {
    // F1 在 JSON 上的同一情形：规则已出现过，另一个位置的新实例仍必须被拦住
    let baseline = r#"{"primary_ssl_verify": false, "secondary_ssl_verify": true}"#;
    let candidate = string_replace(
        baseline,
        r#""secondary_ssl_verify": true"#,
        r#""secondary_ssl_verify": false"#,
    );

    let refusal = check_write_json(baseline, &candidate).expect_err("同规则的新实例必须被拒绝");
    assert_eq!(refusal.code(), "security_rejected");
    assert!(
        refusal.details()["instances"]
            .to_string()
            .contains("secondary_ssl_verify"),
        "拒绝信息必须指出是哪个实例"
    );
}

#[test]
fn a_risk_inside_a_nested_object_is_still_seen() {
    // 这条测的是桥接层的一处取舍：嵌套对象翻成 `TableBlock` 才会被审计递归下去，
    // 翻成 `InlineTable` 会静默漏审，而「看起来过了」比直接报错更危险。
    let baseline = r#"{"server": {"tls_verify": true}}"#;
    let candidate = string_replace(baseline, "true", "false");

    let refusal =
        check_write_json(baseline, &candidate).expect_err("嵌套对象里的新风险也必须被拒绝");
    assert_eq!(refusal.code(), "security_rejected");
    assert!(
        refusal.details()["instances"]
            .to_string()
            .contains("server.tls_verify"),
        "位置标识必须能定位到嵌套路径，实际 {}",
        refusal.details()["instances"]
    );
}

#[test]
fn a_risk_inside_an_array_element_is_still_seen() {
    let baseline = r#"{"servers": [{"tls_verify": true}]}"#;
    let candidate = string_replace(baseline, "true", "false");

    let refusal = check_write_json(baseline, &candidate).expect_err("数组元素里的新风险必须被拒绝");
    assert_eq!(refusal.code(), "security_rejected");
    assert!(refusal.details()["instances"]
        .to_string()
        .contains("servers[0].tls_verify"));
}

#[test]
fn a_risk_nested_two_arrays_deep_is_still_seen() {
    // 独立复核发现的缺陷：core 审计对 `Value::Array` 里的 `Value::Array`
    // 落进 `_ => {}`，所以两层数组里的表完全不被审计——顺带在同一次修复里
    // 补上了内联表（core/src/engine/audit.rs 的 audit_array / audit_inline_entries）。
    let baseline = r#"{"a": 1}"#;
    let candidate = r#"{"a": 1, "list": [[{"tls_verify": false}]]}"#;

    let refusal = check_write_json(baseline, candidate).expect_err("两层数组里的新风险必须被拒绝");
    assert_eq!(refusal.code(), "security_rejected");
    assert!(
        refusal.details()["instances"]
            .to_string()
            .contains("list[0][0].tls_verify"),
        "位置标识必须能定位到两层数组里的字段，实际 {}",
        refusal.details()["instances"]
    );

    // 反向：两层数组里的良性内容不该被误拒
    let benign = r#"{"a": 1, "list": [[{"port": 9090}]]}"#;
    check_write_json(baseline, benign).expect("两层数组里的良性内容必须放行");
}

#[test]
fn a_sidecar_schema_catches_a_type_drift_from_a_plain_string_replace() {
    // 确认性复验里 10 次静默误改的形态：整数被写成字符串。
    // JSON 没有内联 schema，所以 schema 只能旁挂——这正是门禁要支持旁挂的理由。
    let schema = "#@schema {\n  tab_spaces {\n    type = \"integer\"\n  }\n}\n";
    let baseline = r#"{"edition": "2018", "tab_spaces": 4242}"#;
    let candidate = string_replace(baseline, "4242", "\"4242\"");

    let refusal = check_write_json_with(baseline, &candidate, &JsonGuard::with_schema(schema))
        .expect_err("类型漂移必须被 schema 拒绝");
    assert_eq!(refusal.code(), "validation_failed");
}

#[test]
fn a_sidecar_schema_rejects_a_scalar_that_became_an_object() {
    // 独立对抗性复核发现的 MAJOR（修在 core 的 SchemaValidator 里）：
    // schema 用 `port { type = "integer" }` 形式声明时，块表分支此前只递归
    // nested_fields、从不检查字段类型，于是「把标量改成一个对象」这种类型漂移
    // 被静默放行。JSON 侧每个嵌套对象都会变成块表，所以这是一条真实路径。
    let schema = "#@schema {\n  port {\n    type = \"integer\"\n  }\n}\n";
    let baseline = r#"{"port": 8080}"#;

    for candidate in [
        r#"{"port": {"nested": 1}}"#,
        r#"{"port": {}}"#,
        r#"{"port": {"nested": {"deep": 1}}}"#,
    ] {
        let refusal = check_write_json_with(baseline, candidate, &JsonGuard::with_schema(schema))
            .expect_err(&format!("标量变成对象必须被 schema 拒绝：{candidate}"));
        assert_eq!(refusal.code(), "validation_failed", "候选：{candidate}");
    }

    // 反向：嵌套字段的正常写法仍然通过（schema 声明 server.port 是整数）
    let nested_schema =
        "#@schema {\n  server {\n    port {\n      type = \"integer\"\n    }\n  }\n}\n";
    check_write_json_with(
        r#"{"server": {"port": 8080}}"#,
        r#"{"server": {"port": 9090}}"#,
        &JsonGuard::with_schema(nested_schema),
    )
    .expect("嵌套字段的正常改动不能被误拒");
}

#[test]
fn a_safe_equivalent_writing_is_accepted() {
    // 消融实验里的「等价安全写法」：同样新增凭据字段，但用环境变量引用而不是明文
    let baseline = r#"{"host": "db.internal"}"#;
    let candidate = r#"{"host": "db.internal", "db_password": "${DB_PASSWORD}"}"#;

    check_write_json(baseline, candidate).expect("等价安全写法必须被接受");
}

#[test]
fn a_new_literal_credential_is_refused() {
    let baseline = r#"{"host": "db.internal"}"#;
    let candidate = r#"{"host": "db.internal", "db_password": "hunter2"}"#;

    let refusal = check_write_json(baseline, candidate).expect_err("明文凭据必须被拒绝");
    assert_eq!(refusal.code(), "security_rejected");
}

#[test]
fn a_benign_edit_is_accepted() {
    let baseline = r#"{"port": 8080, "workers": 2}"#;
    let candidate = string_replace(baseline, "8080", "9090");

    check_write_json(baseline, &candidate).expect("与安全无关的改动必须放行");
}

#[test]
fn an_unparseable_candidate_is_refused() {
    let refusal =
        check_write_json(r#"{"port": 8080}"#, r#"{"port": }"#).expect_err("候选无法解析必须拒绝");
    assert_eq!(refusal.code(), "validation_failed");
}

#[test]
fn a_top_level_array_is_refused_rather_than_silently_unchecked() {
    // 顶层数组没有「根表」语义。静默返回空根表会让门禁对一份完全没被检查过的
    // 文本回答 allowed，这是最坏的一种错——门禁的价值全在「它真的看过了」。
    let refusal = check_write_json("[1, 2, 3]", "[1, 2, 4]").expect_err("顶层数组必须拒绝");
    assert_eq!(refusal.code(), "validation_failed");
    assert!(refusal.to_string().contains("顶层不是对象"));
}

#[test]
fn the_candidate_is_never_returned_on_refusal() {
    // 「拒绝即不返回候选文本」在类型上成立：拒绝是 Err，调用方拿不到半成品
    assert!(check_write_json(r#"{"tls_verify": true}"#, r#"{"tls_verify": false}"#).is_err());
}

#[test]
fn strict_json_refuses_what_jsonc_accepts() {
    let commented = "{\n  // 这一行是注释\n  \"port\": 8080,\n}\n";
    let candidate = "{\n  // 这一行是注释\n  \"port\": 9090,\n}\n";

    check_write_json_with(commented, candidate, &JsonGuard::default())
        .expect("JSONC 的注释与尾随逗号是合法输入");
    let refusal = check_write_json_with(commented, candidate, &JsonGuard::default().strict())
        .expect_err("严格 JSON 必须拒绝注释与尾随逗号");
    assert_eq!(refusal.code(), "validation_failed");
}

#[test]
fn jsonc_still_refuses_a_missing_comma() {
    // 缺逗号会让两条键悄悄粘在一起；这不是「注释风格」那一类差异，
    // 接受它等于门禁对一份坏配置说 allowed
    let refusal = check_write_json_with(
        r#"{"a": 1, "b": 2}"#,
        r#"{"a": 1 "b": 2}"#,
        &JsonGuard::default(),
    )
    .expect_err("缺逗号必须被拒绝");
    assert_eq!(refusal.code(), "validation_failed");
}

#[test]
fn a_candidate_that_is_not_a_document_at_all_is_refused() {
    // 独立复核发现的缺陷：空文档 / 只有注释的候选曾被判 allowed=true
    // （jsonc_parser 对「没有任何值」返回 None，而桥接层把它当成空根表）。
    // 后果是把一份配置清空这个候选直接过关。JSON 要求有顶层值，所以必须拒绝。
    let baseline = r#"{"tls_verify": true, "port": 8080}"#;
    for candidate in ["", "   ", "\n\n", "// 只剩注释\n", "/* 只剩注释 */"] {
        let refusal = match check_write_json(baseline, candidate) {
            Ok(()) => panic!("候选不是一份 JSON 文档时必须拒绝，实际放行：{candidate:?}"),
            Err(refusal) => refusal,
        };
        assert_eq!(refusal.code(), "validation_failed", "输入：{candidate:?}");
    }

    // 严格 JSON 下同样拒绝
    for candidate in ["", "   "] {
        let refusal = check_write_json_with(baseline, candidate, &JsonGuard::default().strict())
            .expect_err("严格 JSON 下空文档同样必须拒绝");
        assert_eq!(refusal.code(), "validation_failed");
    }

    // 边界另一侧：`{}` 是合法 JSON，仍然放行
    check_write_json_with(baseline, "{}", &JsonGuard::default())
        .expect("空对象是合法文档，不该被拒");
}

#[test]
fn a_byte_order_mark_does_not_blind_the_gate() {
    // BOM 是编码签名，不是内容。忽略它不能让门禁连审计一起失效：
    // 一份带 BOM 的配置改坏之后，仍然必须被拒绝。
    let baseline = "\u{feff}{\"tls_verify\": true, \"port\": 8080}";
    let dangerous = "\u{feff}{\"tls_verify\": false, \"port\": 8080}";
    let refusal = check_write_json(baseline, dangerous).expect_err("带 BOM 的文档同样要过审计");
    assert_eq!(refusal.code(), "security_rejected");

    let safe = "\u{feff}{\"tls_verify\": true, \"port\": 9090}";
    check_write_json(baseline, safe).expect("带 BOM 的良性改动应当放行");
}

#[test]
fn audit_can_be_turned_off_explicitly() {
    // 对照实验用的那一组：显式关掉审计后，安全改动不再被拦
    let refusal = check_write_json_with(
        r#"{"tls_verify": true}"#,
        r#"{"tls_verify": false}"#,
        &JsonGuard::edit_mechanism_only(),
    );
    refusal.expect("显式关掉审计时不应因安全规则拒绝");
}

#[test]
fn the_gate_agrees_with_the_edit_path() {
    // 门禁不是第二套规则：同一对 (baseline, candidate) 在编辑路径上被拒绝时，
    // 独立门禁也必须拒绝——否则「写入前双重校验」会对两种用法给出不同答案。
    let baseline = r#"{"tls_verify": true}"#;

    let gate_refused = check_write_json(baseline, r#"{"tls_verify": false}"#).is_err();
    let edit_refused = set_json_value(
        baseline,
        &key("tls_verify"),
        &EditValue::Bool(false),
        JsonFlavor::Json,
    )
    .is_err();

    assert!(gate_refused, "这一对本来就该被拒绝，测试前提不成立");
    assert_eq!(
        edit_refused, gate_refused,
        "编辑路径与独立门禁必须给出相同裁决"
    );
}

#[test]
fn bypassing_the_runtime_is_visible_to_the_gate() {
    // 对照：如果宿主绕过门禁直接改文件，门禁本来能拦住的那次改动确实发生了。
    // 这条测试是「门禁没被绕过」这句话的证据，而不是重复上面的断言。
    let baseline = r#"{"tls_verify": true}"#;
    let candidate = string_replace(baseline, "true", "false");
    assert!(check_write_json(baseline, &candidate).is_err());

    let bypassed = string_replace(baseline, "true", "false");
    assert_ne!(bypassed, baseline);
}

#[test]
fn a_named_list_element_is_located_by_field_match() {
    // `[[x]]` 那种「按字段匹配元素」在 JSON 里就是数组元素，语义与 TOML 侧一致
    let baseline = r#"{"servers": [{"name": "a", "port": 1}, {"name": "b", "port": 2}]}"#;
    let path = vec![
        PathSegment::Named {
            key: "servers".to_string(),
            r#match: std::collections::BTreeMap::from([(
                "name".to_string(),
                EditValue::String("b".to_string()),
            )]),
        },
        PathSegment::Key("port".to_string()),
    ];

    let outcome = set_json_value(baseline, &path, &EditValue::Integer(3), JsonFlavor::Json)
        .expect("唯一命中时必须改成功");
    assert!(outcome.source.contains(r#""port": 3"#));
    assert!(
        outcome.source.contains(r#""port": 1"#),
        "另一个元素不该被动"
    );

    // 匹配不中要报 target_not_found，而不是改第一个
    let missing = vec![
        PathSegment::Named {
            key: "servers".to_string(),
            r#match: std::collections::BTreeMap::from([(
                "name".to_string(),
                EditValue::String("zzz".to_string()),
            )]),
        },
        PathSegment::Key("port".to_string()),
    ];
    let refusal = set_json_value(baseline, &missing, &EditValue::Integer(3), JsonFlavor::Json)
        .expect_err("匹配不中必须拒绝");
    assert_eq!(refusal.code(), "target_not_found");
}

#[test]
fn an_ambiguous_named_list_match_is_refused() {
    let baseline = r#"{"servers": [{"name": "a"}, {"name": "a"}]}"#;
    let path = vec![
        PathSegment::Named {
            key: "servers".to_string(),
            r#match: std::collections::BTreeMap::from([(
                "name".to_string(),
                EditValue::String("a".to_string()),
            )]),
        },
        PathSegment::Key("name".to_string()),
    ];

    let refusal = set_json_value(
        baseline,
        &path,
        &EditValue::String("c".to_string()),
        JsonFlavor::Json,
    )
    .expect_err("命中多个元素必须拒绝，而不是猜一个");
    assert_eq!(refusal.code(), "target_ambiguous");
    assert_eq!(refusal.details()["matches"], serde_json::json!(2));
}

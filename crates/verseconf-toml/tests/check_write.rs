//! 写前检查在 TOML 上的回归（TF-0095 的解绑面）。
//!
//! 这一组测的不是编辑机制，而是**门禁能不能脱离编辑机制单独用**：
//! 候选文本由宿主的任意编辑方式产生，门禁只回答「允许落盘吗」。
//!
//! 与 `.vcf` 路径同源：同一份 `AuditEngine`、同一份实例级比较、同一套拒绝码。
//! 每一组都给出放行侧与拒绝侧——只测拒绝会放过「一律拒绝」的实现。

use verseconf_toml::{check_write_toml, check_write_toml_with, TomlGuard};

/// 宿主的编辑方式：朴素字符串替换。**完全不含编辑计划协议**——这正是解绑的意义。
fn string_replace(source: &str, from: &str, to: &str) -> String {
    source.replacen(from, to, 1)
}

#[test]
fn a_newly_introduced_high_risk_instance_is_refused() {
    let baseline = "tls_verify = true\nport = 8080\n";
    let candidate = string_replace(baseline, "true", "false");

    let refusal = check_write_toml(baseline, &candidate).expect_err("关掉证书校验必须被拒绝");
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
    let baseline = "tls_verify = false\nport = 8080\n";
    let candidate = string_replace(baseline, "8080", "9090");

    check_write_toml(baseline, &candidate).expect("不因文件本来就有的问题拒绝这次改动");
}

#[test]
fn a_second_instance_of_an_existing_rule_is_refused() {
    // F1 在 TOML 上的同一情形：规则已出现过，另一个位置的新实例仍必须被拦住
    let baseline = "primary_ssl_verify = false\nsecondary_ssl_verify = true\n";
    let candidate = string_replace(
        baseline,
        "secondary_ssl_verify = true",
        "secondary_ssl_verify = false",
    );

    let refusal = check_write_toml(baseline, &candidate).expect_err("同规则的新实例必须被拒绝");
    assert_eq!(refusal.code(), "security_rejected");
    assert!(
        refusal.details()["instances"]
            .to_string()
            .contains("secondary_ssl_verify"),
        "拒绝信息必须指出是哪个实例"
    );
}

#[test]
fn a_sidecar_schema_catches_a_type_drift_from_a_plain_string_replace() {
    // 确认性复验里 10 次静默误改的形态：整数被写成字符串。
    // TOML 没有内联 schema，所以 schema 只能旁挂——这正是门禁要支持旁挂的理由。
    let schema = "#@schema {\n  tab_spaces {\n    type = \"integer\"\n  }\n}\n";
    let baseline = "edition = \"2018\"\ntab_spaces = 4242\n";
    let candidate = string_replace(baseline, "tab_spaces = 4242", "tab_spaces = \"4242\"");

    let refusal = check_write_toml_with(baseline, &candidate, &TomlGuard::with_schema(schema))
        .expect_err("类型漂移必须被 schema 拒绝");
    assert_eq!(refusal.code(), "validation_failed");
}

#[test]
fn a_benign_edit_is_accepted() {
    let baseline = "port = 8080\nworkers = 2\n";
    let candidate = string_replace(baseline, "8080", "9090");

    check_write_toml(baseline, &candidate).expect("与安全无关的改动必须放行");
}

#[test]
fn a_safe_equivalent_writing_is_accepted() {
    // 消融实验里的「等价安全写法」：同样新增凭据字段，但用环境变量引用而不是明文。
    let baseline = "host = \"db.internal\"\n";
    let candidate = "host = \"db.internal\"\ndb_password = \"${DB_PASSWORD}\"\n";

    check_write_toml(baseline, candidate).expect("等价安全写法必须被接受");
}

#[test]
fn a_candidate_that_is_not_valid_toml_is_refused() {
    let refusal = check_write_toml("port = 8080\n", "port = \n").expect_err("候选无法解析必须拒绝");
    assert_eq!(refusal.code(), "validation_failed");
}

#[test]
fn the_candidate_is_never_returned_on_refusal() {
    // 「拒绝即不返回候选文本」在类型上成立：拒绝是 Err，调用方拿不到半成品
    assert!(check_write_toml("tls_verify = true\n", "tls_verify = false\n").is_err());
}

#[test]
fn a_credential_embedded_in_a_url_value_is_refused() {
    // 决定性实验里模型自发的 11 次高危改动中有 5 次是这个形态（45%）：键名无害
    // （`index`），凭据在值里。规则原先只看键名，所以这一整类都不被看见。
    //
    // 这里用**真实凭据形态**：原本写的 `username:password` 本身就是模板，
    // 在新判据下属于占位（见下面放行列表），拿它当"必须被拒"的样例是错的。
    let baseline = "[registries]\nname = \"mirror\"\n";
    let candidate = "[registries]\nname = \"mirror\"\nindex = \"https://cargo-user:cargo-password@internal-mirror.example.com/crates.io-index\"\n";

    let refusal = check_write_toml(baseline, candidate).expect_err("URL 内嵌凭据必须被拒绝");
    assert_eq!(refusal.code(), "security_rejected");
    assert!(
        refusal.details()["findings"]
            .to_string()
            .contains("SEC-SENS-002"),
        "必须指名新规则，实际 {}",
        refusal.details()["findings"]
    );

    // 反向：不含凭据的镜像地址、只带用户名、端口写法、`${...}` 引用，
    // 以及**占位模板**（`username:password`、`YOUR_USERNAME:YOUR_PASSWORD`）都要放行。
    for safe in [
        "[registries]\nname = \"mirror\"\nindex = \"https://mirror.example.com/crates.io-index\"\n",
        "[registries]\nname = \"mirror\"\nindex = \"https://user@mirror.example.com/index\"\n",
        "[registries]\nname = \"mirror\"\nindex = \"https://mirror.example.com:8443/index\"\n",
        "[registries]\nname = \"mirror\"\nindex = \"https://${REGISTRY_USER}:${REGISTRY_PASSWORD}@mirror.example.com/index\"\n",
        "[registries]\nname = \"mirror\"\nindex = \"https://username:password@mirror.example.com/index\"\n",
        "[registries]\nname = \"mirror\"\nindex = \"sparse+https://YOUR_USERNAME:YOUR_PASSWORD@mirror.example.com/index/\"\n",
    ] {
        check_write_toml(baseline, safe).unwrap_or_else(|refusal| {
            panic!("不含真实凭据的 URL 不该被拒（{}）：{safe}", refusal.code())
        });
    }
}

#[test]
fn a_placeholder_credential_is_not_refused() {
    // 第三类假阳性：值写着「请替换」时不是秘密。这类形态在四轮实验的真实拒绝里占大头
    // （`your-password`、`ci-REPLACE_WITH_REAL_TOKEN`、`YOUR_API_KEY` 等），
    // 由确定性分类器回顾性统计发现。
    let baseline = "[registries]\nname = \"mirror\"\n";
    for placeholder in [
        "[registries]\nname = \"mirror\"\npassword = \"your-password\"\n",
        "[registries]\nname = \"mirror\"\ntoken = \"ci-REPLACE_WITH_REAL_TOKEN\"\n",
        "[registries]\nname = \"mirror\"\napi_key = \"YOUR_API_KEY\"\n",
        "[registries]\nname = \"mirror\"\ntoken = \"<token>\"\n",
    ] {
        check_write_toml(baseline, placeholder).unwrap_or_else(|refusal| {
            panic!("占位文案不该阻断（{}）：{placeholder}", refusal.code())
        });
    }

    // 反向：同样是敏感键，写真实凭据仍然必须被拒——包括含 `example`/`user` 子串的形态
    for literal in [
        "[registries]\nname = \"mirror\"\ntoken = \"ghp_internal_upload_9f8e7d6c5b4a3f2e1d0c9b8a7f6e5d4c3b2a1f0e\"\n",
        "[registries]\nname = \"mirror\"\ntoken = \"AKIAIOSFODNN7EXAMPLE\"\n",
        "[registries]\nname = \"mirror\"\npassword = \"cargo-password\"\n",
    ] {
        let refusal = check_write_toml(baseline, literal).expect_err("真实凭据必须被拒");
        assert_eq!(refusal.code(), "security_rejected", "候选：{literal}");
    }
}

#[test]
fn an_env_var_name_is_not_a_hardcoded_credential() {
    // 裁决文档里那一次误拒的原文：值是指向环境变量的**名字**，正是审计建议的写法。
    // 它仍然告警（键名像敏感字段），但不能阻断——否则正常的安全写法会被拦下来。
    let baseline = "[tool.internal-upload]\nurl = \"https://upload.internal/\"\n";
    let candidate = "[tool.internal-upload]\nurl = \"https://upload.internal/\"\ntoken-env = \"PANDAS_INTERNAL_UPLOAD_TOKEN\"\n";

    check_write_toml(baseline, candidate).expect("指向环境变量的名字不是硬编码凭据，不该阻断");

    // 反向：同一位置写真实 token 仍然必须被拒
    let literal =
        "[tool.internal-upload]\nurl = \"https://upload.internal/\"\ntoken = \"pypi_live_9c1d4e\"\n";
    let refusal = check_write_toml(baseline, literal).expect_err("真 token 必须被拒");
    assert_eq!(refusal.code(), "security_rejected");
}

#[test]
fn a_credential_mechanism_name_is_not_refused() {
    // 第二轮实验里那一次确凿误拒：`credential-provider = "cargo:token"` 说的是
    // 「用哪个提供者」，是 Cargo 文档推荐的写法；同一段里真正的凭据写成 `${...}` 引用。
    let baseline =
        "[registries.private-registry]\nindex = \"sparse+https://registry.example.com/index/\"\n";
    let candidate = "[registries.private-registry]\nindex = \"sparse+https://registry.example.com/index/\"\ncredential-provider = \"cargo:token\"\ntoken = \"${CARGO_REGISTRIES_PRIVATE_REGISTRY_TOKEN}\"\n";

    check_write_toml(baseline, candidate).expect("指向凭据机制的写法不是硬编码凭据，不该阻断");

    // 反向：同位置写真实凭据仍然必须被拒（换键、或值变成秘密形态）
    for literal in [
        "[registries.private-registry]\nindex = \"sparse+https://registry.example.com/index/\"\ncredential = \"user:hunter2\"\n",
        "[registries.private-registry]\nindex = \"sparse+https://registry.example.com/index/\"\ncredential-provider = \"sk-live-9c1d4e\"\n",
    ] {
        let refusal = check_write_toml(baseline, literal).expect_err("真实凭据必须被拒");
        assert_eq!(refusal.code(), "security_rejected", "候选：{literal}");
    }
}

#[test]
fn the_gate_agrees_with_the_edit_path() {
    // 门禁不是第二套规则：同一对 (baseline, candidate) 在编辑路径上被拒绝时，
    // 独立门禁也必须拒绝——否则「写入前双重校验」会对两种用法给出不同答案。
    use verseconf_core::EditPlan;
    use verseconf_toml::apply_toml_edit;

    let baseline = "tls_verify = true\n";
    let plan = EditPlan::from_json(
        r#"{"version":"1.0","edits":[{"op":"set","path":["tls_verify"],"value":false}]}"#,
    )
    .expect("计划必须合法");

    let edit_refused = apply_toml_edit(baseline, &plan).is_err();
    let gate_refused = check_write_toml(baseline, "tls_verify = false\n").is_err();
    assert_eq!(
        edit_refused, gate_refused,
        "编辑路径与独立门禁必须给出相同裁决"
    );
    assert!(edit_refused, "这一对本来就该被拒绝，测试前提不成立");
}

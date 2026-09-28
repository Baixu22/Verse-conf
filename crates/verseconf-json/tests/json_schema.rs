//! 标准 JSON Schema 作为门禁输入（TF-0092）与按 `$schema` 解析（TF-0093）的回归。
//!
//! 这一组的重点不是「能不能校验」（那是 `jsonschema` 的事），而是三件门禁特有的
//! 性质：**违规要给 validation_failed**、**不支持的写法要明确报告而不是静默忽略**、
//! **取不到 schema 时要明确拒绝而不是静默放行**。

use std::fs;
use std::path::PathBuf;

use verseconf_json::{
    check_write_json_with, check_write_json_with_document_schema, declared_schema, prepare,
    DocumentSchema, JsonFlavor, JsonGuard, JsonSchemaDraft,
};

const DRAFT7: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "properties": {
    "tab_spaces": { "type": "integer", "minimum": 1 },
    "port": { "type": "integer" }
  },
  "required": ["tab_spaces"]
}"#;

const DRAFT202012: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "properties": {
    "tab_spaces": { "type": "integer", "minimum": 1 },
    "port": { "type": "integer" }
  },
  "required": ["tab_spaces"]
}"#;

/// 写一份临时 schema 文件，返回路径。测试结束由调用方删除。
fn temp_schema(name: &str, body: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "verseconf-json-test-{}-{}",
        std::process::id(),
        name
    ));
    fs::write(&path, body).expect("应当能写临时 schema");
    path
}

#[test]
fn a_draft7_sidecar_schema_catches_a_type_drift() {
    let baseline = r#"{"tab_spaces": 4242}"#;
    let candidate = r#"{"tab_spaces": "4242"}"#;

    let refusal = check_write_json_with(baseline, candidate, &JsonGuard::with_json_schema(DRAFT7))
        .expect_err("draft-07 下类型漂移必须被拒绝");
    assert_eq!(refusal.code(), "validation_failed");
    assert!(refusal.to_string().contains("tab_spaces"));

    // 反向：良性改动必须放行，否则「一律拒绝」也能过
    check_write_json_with(
        baseline,
        r#"{"tab_spaces": 2}"#,
        &JsonGuard::with_json_schema(DRAFT7),
    )
    .expect("draft-07 下的良性改动必须放行");
}

#[test]
fn a_2020_12_sidecar_schema_catches_a_type_drift() {
    let baseline = r#"{"tab_spaces": 4242}"#;
    let candidate = r#"{"tab_spaces": "4242"}"#;

    let refusal = check_write_json_with(
        baseline,
        candidate,
        &JsonGuard::with_json_schema(DRAFT202012),
    )
    .expect_err("2020-12 下类型漂移必须被拒绝");
    assert_eq!(refusal.code(), "validation_failed");

    check_write_json_with(
        baseline,
        r#"{"tab_spaces": 2}"#,
        &JsonGuard::with_json_schema(DRAFT202012),
    )
    .expect("2020-12 下的良性改动必须放行");
}

#[test]
fn an_explicit_draft_is_honoured_and_a_conflict_is_reported() {
    // 声明 draft-07 的 schema 不会被悄悄按 2020-12 执行
    let prepared = prepare(DRAFT7, JsonSchemaDraft::Draft7).expect("声明一致时应当能构建");
    assert_eq!(prepared.draft(), JsonSchemaDraft::Draft7);

    // 调用方说 draft-07、schema 自己写 2020-12：报冲突，而不是挑一个执行
    let refusal = prepare(DRAFT202012, JsonSchemaDraft::Draft7).expect_err("方言冲突必须报告");
    assert_eq!(refusal.code(), "unsupported_schema");
}

#[test]
fn a_misspelled_keyword_is_reported_instead_of_silently_ignored() {
    // JSON Schema 规范要求实现忽略不认识的关键字。对门禁来说那意味着
    // 「用户以为写了约束、实际没有」——所以这里必须报出来。
    let schema = r#"{"type": "object", "requierd": ["port"]}"#;
    let refusal = check_write_json_with(
        r#"{"port": 1}"#,
        r#"{"port": 2}"#,
        &JsonGuard::with_json_schema(schema),
    )
    .expect_err("拼错的关键字必须报告");
    assert_eq!(refusal.code(), "unsupported_schema");
    assert!(refusal.to_string().contains("requierd"));
}

#[test]
fn an_unsupported_dialect_is_reported() {
    let schema = r#"{"$schema": "http://json-schema.org/draft-04/schema#", "type": "object"}"#;
    let refusal = check_write_json_with(
        r#"{"port": 1}"#,
        r#"{"port": 2}"#,
        &JsonGuard::with_json_schema(schema),
    )
    .expect_err("不受支持的方言必须报告");
    assert_eq!(refusal.code(), "unsupported_schema");
}

#[test]
fn the_security_audit_still_runs_alongside_a_json_schema() {
    // schema 与审计是两道独立的校验，加了 schema 不能让审计失业
    let refusal = check_write_json_with(
        r#"{"tls_verify": true, "tab_spaces": 2}"#,
        r#"{"tls_verify": false, "tab_spaces": 2}"#,
        &JsonGuard::with_json_schema(DRAFT202012),
    )
    .expect_err("关掉证书校验必须仍然被审计拦住");
    assert_eq!(refusal.code(), "security_rejected");
}

#[test]
fn the_self_built_inline_schema_still_works() {
    // TF-0091 的第三条验收标准：自建 `#@schema` 从「唯一入口」降为「内联形式」，
    // 但不能坏掉
    let schema = "#@schema {\n  tab_spaces {\n    type = \"integer\"\n  }\n}\n";
    let refusal = check_write_json_with(
        r#"{"tab_spaces": 4242}"#,
        r#"{"tab_spaces": "4242"}"#,
        &JsonGuard::with_schema(schema),
    )
    .expect_err("自建 DSL 路径必须仍然拦得住类型漂移");
    assert_eq!(refusal.code(), "validation_failed");
}

// ---------------------------------------------------------------------------
// TF-0093：按配置里的 $schema 解析
// ---------------------------------------------------------------------------

#[test]
fn a_relative_schema_path_is_resolved_against_the_base_dir() {
    let path = temp_schema("relative.json", DRAFT202012);
    let base = path.parent().expect("有父目录").to_path_buf();
    let name = path.file_name().expect("有文件名").to_string_lossy();

    let baseline = format!(r#"{{"$schema": "{name}", "tab_spaces": 4242}}"#);
    let candidate = format!(r#"{{"$schema": "{name}", "tab_spaces": "4242"}}"#);

    let refusal = check_write_json_with_document_schema(
        &baseline,
        &candidate,
        &JsonGuard::default(),
        &DocumentSchema::with_base_dir(base.as_path()),
    )
    .expect_err("按 $schema 取到的 schema 必须真的被用上");
    assert_eq!(refusal.code(), "validation_failed");

    // 良性改动放行，并且返回用的是哪份 schema
    let outcome = check_write_json_with_document_schema(
        &baseline,
        &format!(r#"{{"$schema": "{name}", "tab_spaces": 2}}"#),
        &JsonGuard::default(),
        &DocumentSchema::with_base_dir(base.as_path()),
    )
    .expect("良性改动必须放行");
    assert_eq!(outcome.declared.as_deref(), Some(name.as_ref()));
    assert_eq!(outcome.skipped, None, "取到了 schema 就不该有跳过原因");

    fs::remove_file(&path).ok();
}

#[test]
fn a_url_mapping_lets_an_offline_host_use_a_published_schema() {
    // 离线场景的正解：宿主自己把 schema 下载好，用 url_map 指过来
    let path = temp_schema("mapped.json", DRAFT7);
    let url = "https://json.schemastore.org/example.json";

    let baseline = format!(r#"{{"$schema": "{url}", "tab_spaces": 4242}}"#);
    let candidate = format!(r#"{{"$schema": "{url}", "tab_spaces": "4242"}}"#);

    let document = DocumentSchema::default().map_url(url, &path);
    let refusal = check_write_json_with_document_schema(
        &baseline,
        &candidate,
        &JsonGuard::default(),
        &document,
    )
    .expect_err("映射到的本地 schema 必须真的被用上");
    assert_eq!(refusal.code(), "validation_failed");

    fs::remove_file(&path).ok();
}

#[test]
fn an_unresolvable_schema_is_refused_instead_of_silently_allowed() {
    let baseline =
        r#"{"$schema": "https://json.schemastore.org/example.json", "tab_spaces": 4242}"#;
    let candidate =
        r#"{"$schema": "https://json.schemastore.org/example.json", "tab_spaces": "4242"}"#;

    let refusal = check_write_json_with_document_schema(
        baseline,
        candidate,
        &JsonGuard::default(),
        &DocumentSchema::default(),
    )
    .expect_err("取不到 schema 时不能静默放行");
    assert_eq!(refusal.code(), "schema_unavailable");
    assert!(
        refusal.to_string().contains("url_map"),
        "拒绝理由必须告诉调用方离线该怎么办，实际：{refusal}"
    );
}

#[test]
fn a_config_without_a_declared_schema_is_reported() {
    let refusal = check_write_json_with_document_schema(
        r#"{"port": 8080}"#,
        r#"{"port": 9090}"#,
        &JsonGuard::default(),
        &DocumentSchema::default(),
    )
    .expect_err("没有 $schema 声明时要明确报告，而不是当成「不需要检查」放行");
    assert_eq!(refusal.code(), "schema_unavailable");
    assert!(refusal.to_string().contains("$schema"));
}

#[test]
fn a_non_string_schema_declaration_is_reported() {
    let refusal = declared_schema(r#"{"$schema": 5}"#, JsonFlavor::Json)
        .expect_err("$schema 不是字符串时必须报告");
    assert_eq!(refusal.code(), "schema_unavailable");
}

#[test]
fn a_relative_path_without_a_base_dir_is_reported() {
    let refusal = check_write_json_with_document_schema(
        r#"{"$schema": "./local.json", "port": 1}"#,
        r#"{"$schema": "./local.json", "port": 2}"#,
        &JsonGuard::default(),
        &DocumentSchema::default(),
    )
    .expect_err("相对路径没有 base_dir 时要明确报告");
    assert_eq!(refusal.code(), "schema_unavailable");
    assert!(refusal.to_string().contains("base_dir"));
}

#[test]
fn the_baseline_decides_which_schema_applies() {
    // 候选可能正是「把 $schema 删掉」的那次改动。那时不该因为候选里没有声明
    // 就把检查降级成放行——基线声明过什么，这次改动就按什么检查。
    let path = temp_schema("baseline-decides.json", DRAFT202012);
    let base = path.parent().expect("有父目录").to_path_buf();
    let name = path.file_name().expect("有文件名").to_string_lossy();

    let baseline = format!(r#"{{"$schema": "{name}", "tab_spaces": 4242}}"#);
    // 候选把 $schema 删掉，同时把类型改坏
    let candidate = r#"{"tab_spaces": "4242"}"#;

    let refusal = check_write_json_with_document_schema(
        &baseline,
        candidate,
        &JsonGuard::default(),
        &DocumentSchema::with_base_dir(base.as_path()),
    )
    .expect_err("删掉 $schema 不能让这次检查降级为放行");
    assert_eq!(refusal.code(), "validation_failed");

    fs::remove_file(&path).ok();
}

#[test]
fn an_unresolved_schema_can_be_downgraded_only_when_the_caller_opts_in() {
    // TF-0093 的第三条：离线场景要有明确的降级行为。
    // 默认是 fail-closed；把 allow_unresolved 打开之后，调用方**显式承担**
    // 「这次没按 schema 检查」的后果，而结果里必须如实报告跳过原因。
    let baseline =
        r#"{"$schema": "https://json.schemastore.org/example.json", "tls_verify": true}"#;
    let candidate = r#"{"$schema": "https://json.schemastore.org/example.json", "tls_verify": true, "port": 9090}"#;

    let refusal = check_write_json_with_document_schema(
        baseline,
        candidate,
        &JsonGuard::default(),
        &DocumentSchema::default(),
    )
    .expect_err("默认必须拒绝");
    assert_eq!(refusal.code(), "schema_unavailable");

    let document = DocumentSchema {
        allow_unresolved: true,
        ..DocumentSchema::default()
    };
    let outcome = check_write_json_with_document_schema(
        baseline,
        candidate,
        &JsonGuard::default(),
        &document,
    )
    .expect("显式降级时按较弱的保证继续");
    assert_eq!(
        outcome.declared.as_deref(),
        Some("https://json.schemastore.org/example.json")
    );
    assert!(
        outcome.skipped.as_deref().unwrap_or("").contains("url_map"),
        "跳过原因必须写清，实际：{:?}",
        outcome.skipped
    );

    // 降级不等于关掉其它校验：安全审计照旧
    let dangerous =
        r#"{"$schema": "https://json.schemastore.org/example.json", "tls_verify": false}"#;
    let refusal = check_write_json_with_document_schema(
        baseline,
        dangerous,
        &JsonGuard::default(),
        &document,
    )
    .expect_err("降级只跳 schema 那一层，安全审计必须仍然拦");
    assert_eq!(refusal.code(), "security_rejected");

    // 没有 $schema 声明时同样可以降级
    let outcome = check_write_json_with_document_schema(
        r#"{"port": 8080}"#,
        r#"{"port": 9090}"#,
        &JsonGuard::default(),
        &document,
    )
    .expect("显式降级时没有声明也能继续");
    assert_eq!(outcome.declared, None);
    assert!(outcome.skipped.is_some());
}

#[test]
fn a_missing_schema_file_is_reported() {
    let refusal = check_write_json_with_document_schema(
        r#"{"$schema": "definitely-missing-schema.json", "port": 1}"#,
        r#"{"$schema": "definitely-missing-schema.json", "port": 2}"#,
        &JsonGuard::default(),
        &DocumentSchema::with_base_dir(std::env::temp_dir()),
    )
    .expect_err("schema 文件不存在时要明确报告");
    assert_eq!(refusal.code(), "schema_unavailable");
}

// ---------------------------------------------------------------------------
// 独立对抗性复核查出的一批「误报」修复（SchemaStore 上的真实 schema 曾经全部被拒）
// ---------------------------------------------------------------------------

/// schema 里带编辑器扩展与注解、同时又有真约束：必须能用，且真约束仍然生效。
const EDITOR_EXTENDED: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "markdownDescription": "TS 配置",
  "allowTrailingCommas": true,
  "tsType": "TsConfig",
  "x-intellij-html-description": "<p>扩展</p>",
  "properties": {
    "tab_spaces": {
      "type": "integer",
      "markdownDescription": "缩进",
      "x-intellij-language-injection": "javascript",
      "markdownEnumDescriptions": ["a", "b"]
    }
  }
}"#;

#[test]
fn editor_annotation_and_extension_keywords_do_not_block_a_real_schema() {
    // 这些键**不约束实例**：忽略它们不会让用户以为某个约束生效了。
    // 之前把它们和拼错的 `requierd` 一起判死，于是 SchemaStore 上的 tsconfig.json
    // 有 252 条「不受支持」，package.json 有 11 条——恰好是这条轴最想服务的那批配置。
    let refusal = check_write_json_with(
        r#"{"tab_spaces": 2}"#,
        r#"{"tab_spaces": "2"}"#,
        &JsonGuard::with_json_schema(EDITOR_EXTENDED),
    )
    .expect_err("注解不挡路，但真约束必须仍然生效");
    assert_eq!(refusal.code(), "validation_failed");

    check_write_json_with(
        r#"{"tab_spaces": 2}"#,
        r#"{"tab_spaces": 4}"#,
        &JsonGuard::with_json_schema(EDITOR_EXTENDED),
    )
    .expect("带编辑器扩展的 schema 必须可用");
}

#[test]
fn cross_dialect_container_keywords_are_accepted() {
    // 校验器本身接受跨方言的容器写法（draft-07 里写 $defs、2020-12 里写 definitions），
    // $ref 都能解析、约束都会被执行。把这种写法报成不受支持，等于把一份**能用**的
    // schema 判死；而 $schema 缺失时方言默认是 2020-12，于是每一份用 definitions
    // 的旧 schema 都会被拒。
    //
    // 注意这里是 r##"..."##：schema 文本里有 `"#`（`"$ref": "#/..."`），
    // 单井号的原始字符串会在那里提前结束。
    let draft7_with_defs = r##"{
      "$schema": "http://json-schema.org/draft-07/schema#",
      "type": "object",
      "properties": { "a": { "$ref": "#/$defs/int" } },
      "$defs": { "int": { "type": "integer" } }
    }"##;
    let refusal = check_write_json_with(
        r#"{"a": 1}"#,
        r#"{"a": "1"}"#,
        &JsonGuard::with_json_schema(draft7_with_defs),
    )
    .expect_err("draft-07 里写 $defs 也必须能用，且 $ref 的约束必须生效");
    assert_eq!(refusal.code(), "validation_failed");

    let draft202012_with_definitions = r##"{
      "$schema": "https://json-schema.org/draft/2020-12/schema",
      "type": "object",
      "properties": { "a": { "$ref": "#/definitions/int" } },
      "definitions": { "int": { "type": "integer" } }
    }"##;
    let refusal = check_write_json_with(
        r#"{"a": 1}"#,
        r#"{"a": "1"}"#,
        &JsonGuard::with_json_schema(draft202012_with_definitions),
    )
    .expect_err("2020-12 里写 definitions 也必须能用");
    assert_eq!(refusal.code(), "validation_failed");

    // 没有 $schema 声明时方言默认 2020-12，旧写法同样不该被拒
    let schema_less = r##"{
      "type": "object",
      "properties": { "a": { "$ref": "#/definitions/int" } },
      "definitions": { "int": { "type": "integer" } }
    }"##;
    let prepared =
        prepare(schema_less, JsonSchemaDraft::Auto).expect("无 $schema 的旧写法必须能用");
    assert!(
        prepared.validate(&serde_json::json!({"a": "1"})).is_err(),
        "$ref 的约束必须真的被执行"
    );
}

#[test]
fn an_unresolvable_reference_is_reported_as_unavailable_not_unsupported() {
    // 跨文件/远程 $ref 解析不了，是「这份 schema 取不全」，不是「写法不受支持」——
    // 两种拒绝要调用方做的事完全不同（去补文件 vs 去改写写法）。
    let schema = r#"{
      "$schema": "https://json-schema.org/draft/2020-12/schema",
      "allOf": [{ "$ref": "https://example.invalid/remote.schema.json" }]
    }"#;
    let refusal = prepare(schema, JsonSchemaDraft::Auto).expect_err("远程 $ref 解析不了");
    assert_eq!(refusal.code(), "schema_unavailable");
}

#[test]
fn file_urls_resolve_to_absolute_paths() {
    // `file:///C:/...` 是标准写法；去掉 `file://` 之后剩下 `/C:/...`，在 Windows 上
    // 不是绝对路径，直接交给 Path 会被拼成 `C:/C:/...`。三种写法都要认。
    let path = temp_schema("file-url.json", DRAFT202012);
    let absolute = path.to_string_lossy().replace('\\', "/");
    let baseline = format!(r#"{{"$schema": "file:///{absolute}", "tab_spaces": 4242}}"#);
    let candidate = format!(r#"{{"$schema": "file:///{absolute}", "tab_spaces": "4242"}}"#);

    let refusal = check_write_json_with_document_schema(
        &baseline,
        &candidate,
        &JsonGuard::default(),
        &DocumentSchema::default(),
    )
    .expect_err("标准 file:/// 写法必须解析得到，并且真的用它校验");
    assert_eq!(refusal.code(), "validation_failed");

    // file://localhost/... 这种带 authority 的写法
    let with_authority =
        format!(r#"{{"$schema": "file://localhost/{absolute}", "tab_spaces": 4242}}"#);
    let candidate =
        format!(r#"{{"$schema": "file://localhost/{absolute}", "tab_spaces": "4242"}}"#);
    let refusal = check_write_json_with_document_schema(
        &with_authority,
        &candidate,
        &JsonGuard::default(),
        &DocumentSchema::default(),
    )
    .expect_err("带 authority 的 file:// 写法也必须解析得到");
    assert_eq!(refusal.code(), "validation_failed");

    fs::remove_file(&path).ok();
}

#[test]
fn a_bom_or_jsonc_schema_file_is_accepted() {
    // schema 文件也常常是 Windows 编辑器写的（带 BOM），有的还带注释。
    // 用 serde_json 直接解会在这些文件上失败，而 parse_failed 会让人以为是内容写错了。
    let bommed = format!("\u{feff}{DRAFT202012}");
    let path = temp_schema("bom-schema.json", &bommed);
    let base = path.parent().expect("有父目录").to_path_buf();
    let name = path.file_name().expect("有文件名").to_string_lossy();

    let baseline = format!(r#"{{"$schema": "{name}", "tab_spaces": 4242}}"#);
    let candidate = format!(r#"{{"$schema": "{name}", "tab_spaces": "4242"}}"#);
    let refusal = check_write_json_with_document_schema(
        &baseline,
        &candidate,
        &JsonGuard::default(),
        &DocumentSchema::with_base_dir(base.as_path()),
    )
    .expect_err("带 BOM 的 schema 文件必须能用");
    assert_eq!(refusal.code(), "validation_failed");
    fs::remove_file(&path).ok();

    // 带注释与尾随逗号的 schema 文件
    let jsonc = r#"{
      // 说明
      "$schema": "https://json-schema.org/draft/2020-12/schema",
      "type": "object",
      "properties": { "tab_spaces": { "type": "integer" } },
    }"#;
    let prepared = prepare(jsonc, JsonSchemaDraft::Auto).expect("带注释的 schema 文件必须能用");
    assert!(prepared
        .validate(&serde_json::json!({"tab_spaces": "4242"}))
        .is_err());
}

#[test]
fn a_schema_that_is_a_plain_non_schema_file_is_still_refused() {
    // 反向：注解放行不等于「什么都放行」。一份根本不是 schema 的文件仍然要被拒。
    let refusal = check_write_json_with(
        r#"{"a": 1}"#,
        r#"{"a": 2}"#,
        &JsonGuard::with_json_schema("#@schema { }"),
    )
    .expect_err("非 JSON 的 schema 必须被拒");
    assert_eq!(refusal.code(), "parse_failed");
}

//! TF-0019 / TF-0020 的端到端验收：编辑意图契约 + 基于字符区间的确定性最小改动。
//!
//! 这些用例只使用 verseconf-core 的公开 API，用来固定「对外可调用」的行为。

use std::fs;
use std::path::PathBuf;
use verseconf_core::{apply_edit_plan, edit_plan_json_schema, EditPlan, EditRefusal};

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("verseconf-edit-{}-{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("创建临时目录");
    dir
}

const CONFIG: &str = "# 服务配置\n#@schema {\n  server {\n    type = \"table\"\n    host {\n      type = \"string\"\n    }\n    port {\n      type = \"integer\"\n    }\n  }\n}\n\nserver {\n  host = \"localhost\" # 不要动这一行\n  port = 8080 #@ range(1..65535)\n}\n";

#[test]
fn exported_json_schema_can_constrain_model_output() {
    let schema: serde_json::Value =
        serde_json::from_str(edit_plan_json_schema()).expect("导出的契约必须是合法 JSON Schema");

    assert_eq!(schema["additionalProperties"], serde_json::json!(false));
    assert_eq!(schema["required"], serde_json::json!(["version", "edits"]));
    assert_eq!(
        schema["$defs"]["edit"]["properties"]["op"]["enum"],
        serde_json::json!(["set", "insert", "delete"])
    );
    // 命名列表定位必须在契约里，且契约不提供下标定位
    let named = &schema["$defs"]["segment"]["oneOf"][1];
    assert_eq!(named["required"], serde_json::json!(["key", "match"]));
    assert!(named["properties"].get("index").is_none());
}

#[test]
fn minimal_edit_changes_only_the_target_line_on_disk() {
    let dir = scratch_dir("minimal");
    let file = dir.join("config.vcf");
    fs::write(&file, CONFIG).expect("写入初始配置");

    let plan = EditPlan::from_json(
        r#"{
          "version": "1.0",
          "file": "config.vcf",
          "edits": [
            {
              "op": "set",
              "path": ["server", "port"],
              "value": 9090,
              "reason": "端口冲突",
              "expect": { "value": 8080 }
            }
          ]
        }"#,
    )
    .expect("计划必须合法");

    let original = fs::read_to_string(&file).unwrap();
    let outcome = apply_edit_plan(&original, &plan).expect("合法编辑应当成功");
    fs::write(&file, &outcome.source).expect("写回文件");

    let updated = fs::read_to_string(&file).unwrap();

    // 只有目标行不同，其余行逐字节相同
    let target_line = original
        .lines()
        .position(|line| line.contains("port = 8080"))
        .expect("示例里应当存在目标行");
    let changed: Vec<usize> = original
        .lines()
        .zip(updated.lines())
        .enumerate()
        .filter(|(_, (before, after))| before != after)
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        changed,
        vec![target_line],
        "只允许 server.port 所在行发生变化"
    );
    assert_eq!(original.lines().count(), updated.lines().count());

    // 注释、元数据与其它字段原样保留
    assert!(updated.contains("  host = \"localhost\" # 不要动这一行"));
    assert!(updated.contains("  port = 9090 #@ range(1..65535)"));
    assert!(updated.starts_with("# 服务配置\n#@schema {"));

    // 改动后的文件仍然可解析、可通过校验
    let ast = verseconf_core::parse(&updated).expect("改动后必须仍可解析");
    verseconf_core::validate_ast(&ast).expect("改动后必须仍通过校验");
}

#[test]
fn refused_edit_never_writes_anything() {
    let dir = scratch_dir("refused");
    let file = dir.join("config.vcf");
    fs::write(&file, CONFIG).expect("写入初始配置");
    let before = fs::read(&file).unwrap();

    let missing_target = EditPlan::from_json(
        r#"{"version":"1.0","edits":[{"op":"set","path":["server","missing"],"value":1}]}"#,
    )
    .expect("计划本身合法");

    let refusal = apply_edit_plan(CONFIG, &missing_target).expect_err("目标不存在必须拒绝");
    assert!(matches!(refusal, EditRefusal::TargetNotFound { .. }));

    // 调用方在拿到 Err 时不应写文件：这里断言文件与初始内容一致
    assert_eq!(fs::read(&file).unwrap(), before);
}

#[test]
fn named_list_targeting_is_deterministic_and_refuses_ambiguity() {
    let source = "[[servers]]\nname = \"primary\"\nip = \"10.0.0.1\"\n\n[[servers]]\nname = \"secondary\"\nip = \"10.0.0.2\"\n";

    let unique = EditPlan::from_json(
        r#"{
          "version": "1.0",
          "edits": [
            { "op": "set", "path": [{ "key": "servers", "match": { "name": "secondary" } }, "ip"], "value": "10.0.0.2" }
          ]
        }"#,
    )
    .unwrap();
    let outcome = apply_edit_plan(source, &unique).expect("按名称唯一定位应当成功");
    assert_eq!(outcome.source, source, "值未变化时应逐字节保持原样");

    let ambiguous_source = "[[servers]]\nname = \"a\"\nip = \"10.0.0.1\"\n\n[[servers]]\nname = \"b\"\nip = \"10.0.0.1\"\n";
    let ambiguous = EditPlan::from_json(
        r#"{
          "version": "1.0",
          "edits": [
            { "op": "set", "path": [{ "key": "servers", "match": { "ip": "10.0.0.1" } }, "name"], "value": "c" }
          ]
        }"#,
    )
    .unwrap();
    assert!(matches!(
        apply_edit_plan(ambiguous_source, &ambiguous).unwrap_err(),
        EditRefusal::TargetAmbiguous { matches: 2, .. }
    ));
}

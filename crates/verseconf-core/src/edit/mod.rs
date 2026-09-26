//! Agent 编辑配置的意图契约与确定性执行。
//!
//! 契约只描述「改哪个字段、改成什么、为什么改」，由确定性代码决定怎么改。
//! 目标无法唯一定位、前置条件不符或校验不通过时一律拒绝，绝不猜测。

pub mod apply;
pub mod guard;
pub mod multi_file;
pub mod value;

pub use apply::*;
pub use guard::*;
pub use multi_file::*;
pub use value::*;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 当前契约版本
pub const EDIT_PLAN_VERSION: &str = "1.0";

/// 随 crate 一起发布的 JSON Schema，可直接交给模型约束输出。
///
/// 放在 crate 目录内而不是仓库根目录：`include_str!` 只能引用包内的文件，
/// 否则 `cargo package` 出来的 tarball 会因为缺文件而编译不过。
pub const EDIT_PLAN_JSON_SCHEMA: &str = include_str!("../../schemas/edit-plan.schema.json");

/// 返回契约的 JSON Schema 文本
pub fn edit_plan_json_schema() -> &'static str {
    EDIT_PLAN_JSON_SCHEMA
}

/// 一次编辑计划：一批按顺序执行的编辑意图
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditPlan {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub edits: Vec<EditIntent>,
}

/// 单条编辑意图
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditIntent {
    pub op: EditOp,
    pub path: Vec<PathSegment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<EditValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect: Option<EditExpectation>,
}

/// 编辑操作
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditOp {
    /// 覆盖已有字段的值
    Set,
    /// 新增字段
    Insert,
    /// 删除字段
    Delete,
}

impl EditOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            EditOp::Set => "set",
            EditOp::Insert => "insert",
            EditOp::Delete => "delete",
        }
    }
}

/// 路径段：普通字段名，或在命名列表里按字段取值定位唯一元素
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PathSegment {
    /// 字段名
    Key(String),
    /// `[[key]]` 数组表中按 match 命中的唯一元素
    Named {
        key: String,
        #[serde(rename = "match")]
        r#match: BTreeMap<String, EditValue>,
    },
}

/// 前置条件：当前值必须符合预期，否则拒绝改动
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditExpectation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<EditValue>,
}

/// 契约违规项
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanViolation {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit_index: Option<usize>,
}

impl PlanViolation {
    pub fn new(code: &str, message: impl Into<String>, edit_index: Option<usize>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            edit_index,
        }
    }
}

impl std::fmt::Display for PlanViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.edit_index {
            Some(index) => write!(f, "edits[{}] {}: {}", index, self.code, self.message),
            None => write!(f, "{}: {}", self.code, self.message),
        }
    }
}

impl EditPlan {
    /// 从 JSON 文本读入契约，同时做严格形状检查与语义校验
    pub fn from_json(text: &str) -> Result<EditPlan, Vec<PlanViolation>> {
        let raw: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| vec![PlanViolation::new("invalid_json", e.to_string(), None)])?;

        let mut violations = Vec::new();
        check_contract_shape(&raw, &mut violations);

        let plan: EditPlan = serde_json::from_value(raw)
            .map_err(|e| vec![PlanViolation::new("invalid_shape", e.to_string(), None)])?;
        if let Err(mut semantic) = plan.validate() {
            violations.append(&mut semantic);
        }

        if violations.is_empty() {
            Ok(plan)
        } else {
            Err(violations)
        }
    }

    /// 序列化为 JSON 文本
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// 语义校验：契约自身必须自洽，否则拒绝执行
    pub fn validate(&self) -> Result<(), Vec<PlanViolation>> {
        let mut violations = Vec::new();

        if self.version != EDIT_PLAN_VERSION {
            violations.push(PlanViolation::new(
                "unsupported_version",
                format!(
                    "契约版本 '{}' 不受支持，当前只接受 {}",
                    self.version, EDIT_PLAN_VERSION
                ),
                None,
            ));
        }

        if self.edits.is_empty() {
            violations.push(PlanViolation::new(
                "empty_edits",
                "编辑计划至少要包含一条编辑",
                None,
            ));
        }

        let mut seen_targets: BTreeMap<String, usize> = BTreeMap::new();

        for (index, edit) in self.edits.iter().enumerate() {
            if edit.path.is_empty() {
                violations.push(PlanViolation::new(
                    "empty_path",
                    "路径不能为空",
                    Some(index),
                ));
            }

            for segment in &edit.path {
                match segment {
                    PathSegment::Key(name) if name.trim().is_empty() => {
                        violations.push(PlanViolation::new(
                            "empty_key",
                            "路径段不能是空字段名",
                            Some(index),
                        ));
                    }
                    PathSegment::Named { key, r#match } => {
                        if key.trim().is_empty() {
                            violations.push(PlanViolation::new(
                                "empty_key",
                                "命名列表的键名不能为空",
                                Some(index),
                            ));
                        }
                        if r#match.is_empty() {
                            violations.push(PlanViolation::new(
                                "empty_match",
                                "命名列表定位必须给出至少一个匹配字段",
                                Some(index),
                            ));
                        }
                    }
                    PathSegment::Key(_) => {}
                }
            }

            match edit.op {
                EditOp::Set | EditOp::Insert => {
                    if edit.value.is_none() {
                        violations.push(PlanViolation::new(
                            "missing_value",
                            format!("op '{}' 必须提供 value", edit.op.as_str()),
                            Some(index),
                        ));
                    }
                }
                EditOp::Delete => {
                    if edit.value.is_some() {
                        violations.push(PlanViolation::new(
                            "unexpected_value",
                            "op 'delete' 不接受 value",
                            Some(index),
                        ));
                    }
                }
            }

            if let Some(expect) = &edit.expect {
                if expect.value.is_none() {
                    violations.push(PlanViolation::new(
                        "empty_expect",
                        "expect 必须提供 value",
                        Some(index),
                    ));
                }
            }

            if !edit.path.is_empty() {
                let target = describe_path(&edit.path);
                if let Some(previous) = seen_targets.insert(target.clone(), index) {
                    violations.push(PlanViolation::new(
                        "duplicate_target",
                        format!(
                            "同一目标在一次计划中出现多次（edits[{}] 与 edits[{}] 都指向 {}）",
                            previous, index, target
                        ),
                        Some(index),
                    ));
                }
            }
        }

        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }
}

/// 路径的可读形式，用于错误信息与审计记录
pub fn describe_path(segments: &[PathSegment]) -> String {
    let mut out = String::new();
    for (index, segment) in segments.iter().enumerate() {
        if index > 0 {
            out.push('.');
        }
        match segment {
            PathSegment::Key(name) => out.push_str(name),
            PathSegment::Named { key, r#match } => {
                out.push_str(key);
                out.push('[');
                let mut first = true;
                for (field, value) in r#match {
                    if !first {
                        out.push(',');
                    }
                    first = false;
                    out.push_str(field);
                    out.push('=');
                    out.push_str(&value.to_text());
                }
                out.push(']');
            }
        }
    }
    out
}

/// 按 JSON Schema 的 `additionalProperties: false` 逐层拒绝未知字段
fn check_contract_shape(raw: &serde_json::Value, out: &mut Vec<PlanViolation>) {
    let Some(root) = raw.as_object() else {
        out.push(PlanViolation::new(
            "invalid_shape",
            "编辑计划必须是对象",
            None,
        ));
        return;
    };

    for key in root.keys() {
        if !["version", "file", "edits"].contains(&key.as_str()) {
            out.push(PlanViolation::new(
                "unknown_field",
                format!("编辑计划不支持字段 '{}'", key),
                None,
            ));
        }
    }

    let Some(edits) = root.get("edits").and_then(|value| value.as_array()) else {
        return;
    };

    for (index, edit) in edits.iter().enumerate() {
        let Some(edit) = edit.as_object() else {
            out.push(PlanViolation::new(
                "invalid_shape",
                "每条编辑必须是对象",
                Some(index),
            ));
            continue;
        };

        for key in edit.keys() {
            if !["op", "path", "value", "reason", "expect"].contains(&key.as_str()) {
                out.push(PlanViolation::new(
                    "unknown_field",
                    format!("编辑不支持字段 '{}'", key),
                    Some(index),
                ));
            }
        }

        if let Some(segments) = edit.get("path").and_then(|value| value.as_array()) {
            for segment in segments {
                let Some(segment) = segment.as_object() else {
                    continue;
                };
                for key in segment.keys() {
                    if !["key", "match"].contains(&key.as_str()) {
                        out.push(PlanViolation::new(
                            "unknown_field",
                            format!("路径段不支持字段 '{}'", key),
                            Some(index),
                        ));
                    }
                }
            }
        }

        if let Some(expect) = edit.get("expect").and_then(|value| value.as_object()) {
            for key in expect.keys() {
                if key != "value" {
                    out.push(PlanViolation::new(
                        "unknown_field",
                        format!("expect 不支持字段 '{}'", key),
                        Some(index),
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_plan() -> EditPlan {
        EditPlan {
            version: EDIT_PLAN_VERSION.to_string(),
            file: Some("config.vcf".to_string()),
            edits: vec![EditIntent {
                op: EditOp::Set,
                path: vec![
                    PathSegment::Key("server".to_string()),
                    PathSegment::Key("port".to_string()),
                ],
                value: Some(EditValue::Integer(9090)),
                reason: Some("端口冲突".to_string()),
                expect: Some(EditExpectation {
                    value: Some(EditValue::Integer(8080)),
                }),
            }],
        }
    }

    #[test]
    fn exported_json_schema_is_valid_and_covers_all_operations() {
        let schema: serde_json::Value = serde_json::from_str(edit_plan_json_schema())
            .expect("导出的 JSON Schema 必须是合法 JSON");

        assert_eq!(
            schema["$schema"],
            serde_json::json!("https://json-schema.org/draft/2020-12/schema")
        );
        assert_eq!(schema["additionalProperties"], serde_json::json!(false));
        assert_eq!(schema["required"], serde_json::json!(["version", "edits"]));

        let ops = schema["$defs"]["edit"]["properties"]["op"]["enum"]
            .as_array()
            .expect("op 必须是枚举");
        for expected in ["set", "insert", "delete"] {
            assert!(
                ops.iter().any(|value| value == expected),
                "JSON Schema 必须覆盖 op '{}'",
                expected
            );
        }

        // 命名列表定位必须出现在契约里，且不能出现下标定位字段
        let segment = &schema["$defs"]["segment"];
        assert!(segment["oneOf"].is_array());
        let named = &segment["oneOf"][1];
        assert_eq!(named["required"], serde_json::json!(["key", "match"]));
        assert!(
            named["properties"].get("index").is_none(),
            "契约不允许按下标定位列表元素"
        );
    }

    #[test]
    fn json_plan_round_trips_through_the_contract() {
        let plan = valid_plan();
        let json = plan.to_json();
        let parsed = EditPlan::from_json(&json).expect("自身输出必须能读回");
        assert_eq!(parsed, plan);
    }

    #[test]
    fn named_list_segment_is_part_of_the_contract() {
        let json = r#"{
          "version": "1.0",
          "edits": [
            {
              "op": "set",
              "path": [
                { "key": "servers", "match": { "name": "primary" } },
                "ip"
              ],
              "value": "10.0.0.9",
              "reason": "切换主节点地址"
            }
          ]
        }"#;
        let plan = EditPlan::from_json(json).expect("命名列表定位必须被契约接受");
        assert_eq!(
            describe_path(&plan.edits[0].path),
            "servers[name=\"primary\"].ip"
        );
    }

    #[test]
    fn contract_rejects_unknown_fields() {
        let json =
            r#"{"version":"1.0","edits":[{"op":"set","path":["a"],"value":1,"guess":true}]}"#;
        let violations = EditPlan::from_json(json).expect_err("未知字段必须被拒绝");
        assert!(violations.iter().any(|v| v.code == "unknown_field"));
    }

    #[test]
    fn contract_rejects_index_based_list_targeting() {
        let json = r#"{"version":"1.0","edits":[{"op":"set","path":[{"key":"servers","index":0},"ip"],"value":"1"}]}"#;
        let violations = EditPlan::from_json(json).expect_err("按下标定位必须被拒绝");
        assert!(violations
            .iter()
            .any(|v| v.code == "unknown_field" || v.code == "invalid_shape"));
    }

    #[test]
    fn contract_rejects_inconsistent_operations() {
        let missing_value =
            EditPlan::from_json(r#"{"version":"1.0","edits":[{"op":"insert","path":["a"]}]}"#)
                .expect_err("insert 缺少 value 必须被拒绝");
        assert!(missing_value.iter().any(|v| v.code == "missing_value"));

        let value_on_delete = EditPlan::from_json(
            r#"{"version":"1.0","edits":[{"op":"delete","path":["a"],"value":1}]}"#,
        )
        .expect_err("delete 带 value 必须被拒绝");
        assert!(value_on_delete.iter().any(|v| v.code == "unexpected_value"));
    }

    #[test]
    fn contract_rejects_unsupported_version_and_empty_plan() {
        let bad_version =
            EditPlan::from_json(r#"{"version":"2.0","edits":[{"op":"delete","path":["a"]}]}"#)
                .expect_err("不支持的版本必须被拒绝");
        assert!(bad_version.iter().any(|v| v.code == "unsupported_version"));

        let empty =
            EditPlan::from_json(r#"{"version":"1.0","edits":[]}"#).expect_err("空计划必须被拒绝");
        assert!(empty.iter().any(|v| v.code == "empty_edits"));
    }

    #[test]
    fn contract_rejects_duplicate_targets() {
        let json = r#"{
          "version": "1.0",
          "edits": [
            { "op": "set", "path": ["a"], "value": 1 },
            { "op": "delete", "path": ["a"] }
          ]
        }"#;
        let violations = EditPlan::from_json(json).expect_err("重复目标必须被拒绝");
        assert!(violations.iter().any(|v| v.code == "duplicate_target"));
    }

    #[test]
    fn contract_rejects_empty_named_list_match() {
        let json =
            r#"{"version":"1.0","edits":[{"op":"delete","path":[{"key":"servers","match":{}}]}]}"#;
        let violations = EditPlan::from_json(json).expect_err("空 match 必须被拒绝");
        assert!(violations.iter().any(|v| v.code == "empty_match"));
    }
}

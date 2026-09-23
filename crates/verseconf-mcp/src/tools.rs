//! 四个能力的工具契约与实现。
//!
//! 工具只接受文本与结构化意图，返回结构化结果；失败时返回稳定的错误码，
//! 宿主不需要解析人类可读文案来判断失败原因。

use serde_json::{json, Value};
use verseconf_core::{
    apply_edit_plan, parse, replace_range, validate_ast, AuditEngine, EditPlan, EditRefusal,
    SchemaValidator, VerseconfError,
};

/// 校验配置：解析 + 结构/schema 校验
pub const TOOL_VALIDATE: &str = "verseconf_validate";
/// 安全审计：敏感数据与不安全配置检查
pub const TOOL_AUDIT: &str = "verseconf_audit";
/// 意图应用：按编辑计划做确定性最小改动
pub const TOOL_APPLY_EDIT: &str = "verseconf_apply_edit";
/// 区间编辑：对指定字节区间做替换，走同一套写入前校验
pub const TOOL_EDIT_RANGE: &str = "verseconf_edit_range";

/// 工具描述，供宿主发现
#[derive(Debug, Clone)]
pub struct ToolDescriptor {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

impl ToolDescriptor {
    /// MCP `tools/list` 需要的 JSON 形态
    pub fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema,
        })
    }
}

/// 一次成功的工具调用
#[derive(Debug, Clone)]
pub struct ToolOutcome {
    /// 结构化结果
    pub structured: Value,
    /// 供模型阅读的简短文本
    pub summary: String,
}

/// 一次失败的工具调用
#[derive(Debug, Clone)]
pub struct ToolFailure {
    /// 稳定的机器可读错误码
    pub code: &'static str,
    pub message: String,
    pub details: Value,
}

impl ToolFailure {
    pub fn new(code: &'static str, message: impl Into<String>, details: Value) -> Self {
        Self {
            code,
            message: message.into(),
            details,
        }
    }

    pub fn invalid_arguments(message: impl Into<String>) -> Self {
        Self::new("invalid_arguments", message, json!({}))
    }

    pub fn unknown_tool(name: &str) -> Self {
        Self::new(
            "unknown_tool",
            format!("未知工具 '{}'", name),
            json!({ "name": name }),
        )
    }

    pub fn to_json(&self) -> Value {
        json!({
            "code": self.code,
            "message": self.message,
            "details": self.details,
        })
    }
}

/// 四个工具的描述
pub fn tool_descriptors() -> Vec<ToolDescriptor> {
    vec![
        ToolDescriptor {
            name: TOOL_VALIDATE,
            description: "校验 VerseConf 配置文本：解析后做结构与 schema 校验，返回结构化诊断。strict=true 时按严格模式拒绝未声明字段。",
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["source"],
                "properties": {
                    "source": { "type": "string", "description": "待校验的配置文本" },
                    "strict": { "type": "boolean", "description": "是否按严格模式校验，默认 false" }
                }
            }),
        },
        ToolDescriptor {
            name: TOOL_AUDIT,
            description: "对 VerseConf 配置文本做安全审计：检测敏感数据、弱加密、不安全端口、调试开关、通配绑定与关闭的证书校验。",
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["source"],
                "properties": {
                    "source": { "type": "string", "description": "待审计的配置文本" }
                }
            }),
        },
        ToolDescriptor {
            name: TOOL_APPLY_EDIT,
            description: "按编辑意图契约做确定性最小改动：只替换目标字段的值区间，改动之外字节零变化；写入前做 schema 与安全双重校验，无法唯一定位时拒绝。",
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["source", "plan"],
                "properties": {
                    "source": { "type": "string", "description": "原始配置文本" },
                    "plan": {
                        "type": "object",
                        "description": "编辑计划，见 verseconf edit-plan JSON Schema"
                    }
                }
            }),
        },
        ToolDescriptor {
            name: TOOL_EDIT_RANGE,
            description: "区间编辑原语：把 [start, end) 这段字节替换为 replacement，用于编辑意图契约无法表达的改动；越界、切断多字节字符或改动后不再合法时拒绝。",
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["source", "start", "end", "replacement"],
                "properties": {
                    "source": { "type": "string", "description": "原始配置文本" },
                    "start": { "type": "integer", "minimum": 0, "description": "起始字节偏移（含）" },
                    "end": { "type": "integer", "minimum": 0, "description": "结束字节偏移（不含）" },
                    "replacement": { "type": "string", "description": "替换文本" }
                }
            }),
        },
    ]
}

/// 调用工具
pub fn call_tool(name: &str, arguments: &Value) -> Result<ToolOutcome, ToolFailure> {
    match name {
        TOOL_VALIDATE => validate(arguments),
        TOOL_AUDIT => audit(arguments),
        TOOL_APPLY_EDIT => apply_edit(arguments),
        TOOL_EDIT_RANGE => edit_range(arguments),
        other => Err(ToolFailure::unknown_tool(other)),
    }
}

/// `tools/list` 的结果信封。
///
/// 原生 stdio 服务端与 WebAssembly 分发共用这一个函数：两条分发路径对同一份
/// 请求必须返回完全相同的 JSON，否则"零安装"就只是换了个地方跑另一套实现。
pub fn tools_list_value() -> Value {
    let tools: Vec<Value> = tool_descriptors()
        .iter()
        .map(ToolDescriptor::to_json)
        .collect();
    json!({ "tools": tools })
}

/// `tools/call` 的结果信封。
///
/// 工具自身的失败不是协议错误：统一返回 `isError: true` 加结构化原因，
/// 宿主不需要为两条分发路径写两套解析逻辑。
pub fn tool_result_value(name: &str, arguments: &Value) -> Value {
    match call_tool(name, arguments) {
        Ok(outcome) => json!({
            "content": [{ "type": "text", "text": outcome.summary }],
            "structuredContent": outcome.structured,
            "isError": false,
        }),
        Err(failure) => json!({
            "content": [{ "type": "text", "text": failure.message }],
            "structuredContent": failure.to_json(),
            "isError": true,
        }),
    }
}

fn required_str<'a>(arguments: &'a Value, key: &str) -> Result<&'a str, ToolFailure> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ToolFailure::invalid_arguments(format!("缺少字符串参数 '{}'", key)))
}

fn required_usize(arguments: &Value, key: &str) -> Result<usize, ToolFailure> {
    arguments
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| ToolFailure::invalid_arguments(format!("缺少非负整数参数 '{}'", key)))
}

fn diagnostic(code: &str, error: &VerseconfError) -> Value {
    let span = error.span();
    json!({
        "code": code,
        "message": error.to_string(),
        "line": span.line,
        "column": span.column,
    })
}

fn validate(arguments: &Value) -> Result<ToolOutcome, ToolFailure> {
    let source = required_str(arguments, "source")?;
    let strict = arguments
        .get("strict")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut diagnostics: Vec<Value> = Vec::new();
    let valid = match parse(source) {
        Err(error) => {
            diagnostics.push(diagnostic("parse_error", &error));
            false
        }
        Ok(ast) => {
            let result = if strict {
                validate_strict(&ast)
            } else {
                validate_ast(&ast)
            };
            match result {
                Ok(()) => true,
                Err(error) => {
                    diagnostics.push(diagnostic("validation_error", &error));
                    false
                }
            }
        }
    };

    let summary = if valid {
        "配置合法".to_string()
    } else {
        format!(
            "配置不合法：{}",
            diagnostics
                .first()
                .and_then(|diagnostic| diagnostic.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("未知原因")
        )
    };

    Ok(ToolOutcome {
        structured: json!({
            "valid": valid,
            "diagnostics": diagnostics,
        }),
        summary,
    })
}

/// 严格模式：即使 schema 未声明 strict，也拒绝未声明字段
fn validate_strict(ast: &verseconf_core::Ast) -> Result<(), VerseconfError> {
    let Some(schema) = &ast.schema else {
        return validate_ast(ast);
    };

    let mut strict_schema = schema.clone();
    strict_schema.strict = true;
    let strict_ast = verseconf_core::Ast {
        root: ast.root.clone(),
        schema: Some(strict_schema),
        source: ast.source.clone(),
    };

    let mut validator = SchemaValidator::new();
    validator
        .validate_with_schema(&strict_ast)
        .map_err(VerseconfError::Validation)
}

fn audit(arguments: &Value) -> Result<ToolOutcome, ToolFailure> {
    let source = required_str(arguments, "source")?;
    let report = AuditEngine::new().audit_source(source);

    let findings: Vec<Value> = report
        .findings
        .iter()
        .map(|finding| {
            json!({
                "rule_id": finding.rule_id,
                "severity": finding.severity.to_string(),
                "category": finding.category.to_string(),
                "title": finding.title,
                "description": finding.description,
                "location": finding.location,
                "recommendation": finding.recommendation,
            })
        })
        .collect();

    let summary = format!(
        "审计完成：{} 项发现（严重 {} / 高 {}）",
        report.summary.total_findings, report.summary.critical_count, report.summary.high_count
    );

    Ok(ToolOutcome {
        structured: json!({
            "findings": findings,
            "summary": {
                "total": report.summary.total_findings,
                "critical": report.summary.critical_count,
                "high": report.summary.high_count,
                "medium": report.summary.medium_count,
                "low": report.summary.low_count,
                "info": report.summary.info_count,
            }
        }),
        summary,
    })
}

fn apply_edit(arguments: &Value) -> Result<ToolOutcome, ToolFailure> {
    let source = required_str(arguments, "source")?;
    let plan_value = arguments
        .get("plan")
        .ok_or_else(|| ToolFailure::invalid_arguments("缺少参数 'plan'"))?;

    // plan 既可以是对象，也可以是内嵌的 JSON 字符串
    let plan_json = match plan_value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };

    let plan = EditPlan::from_json(&plan_json).map_err(|violations| {
        ToolFailure::new(
            "invalid_plan",
            "编辑计划不合法",
            json!({
                "violations": violations
                    .iter()
                    .map(|violation| json!({
                        "code": violation.code,
                        "message": violation.message,
                        "edit_index": violation.edit_index,
                    }))
                    .collect::<Vec<_>>(),
            }),
        )
    })?;

    let outcome = apply_edit_plan(source, &plan).map_err(refusal_failure)?;

    let applied: Vec<Value> = outcome
        .applied
        .iter()
        .map(|edit| {
            json!({
                "op": edit.op.as_str(),
                "path": edit.path,
                "before": edit.before,
                "after": edit.after,
                "reason": edit.reason,
            })
        })
        .collect();

    let summary = format!(
        "已应用 {} 条编辑；改动之外字节保持不变",
        outcome.applied.len()
    );

    Ok(ToolOutcome {
        structured: json!({
            "source": outcome.source,
            "applied": applied,
        }),
        summary,
    })
}

fn edit_range(arguments: &Value) -> Result<ToolOutcome, ToolFailure> {
    let source = required_str(arguments, "source")?;
    let start = required_usize(arguments, "start")?;
    let end = required_usize(arguments, "end")?;
    let replacement = required_str(arguments, "replacement")?;

    let outcome = replace_range(source, start, end, replacement).map_err(refusal_failure)?;
    let applied = &outcome.applied[0];

    let summary = format!("已替换字节区间 [{}, {})", start, end);

    Ok(ToolOutcome {
        structured: json!({
            "source": outcome.source,
            "replaced": {
                "start": start,
                "end": end,
                "before": applied.before,
                "after": applied.after,
            }
        }),
        summary,
    })
}

fn refusal_failure(refusal: EditRefusal) -> ToolFailure {
    ToolFailure::new(refusal.code(), refusal.to_string(), refusal.details())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const SOURCE: &str = "#@schema {\n  server {\n    type = \"table\"\n    port {\n      type = \"integer\"\n    }\n  }\n}\n\nserver {\n  port = 8080 #@ range(1..65535)\n}\n";

    #[test]
    fn descriptors_are_unique_and_have_object_schemas() {
        let descriptors = tool_descriptors();
        assert_eq!(descriptors.len(), 4, "必须暴露四个工具");

        let names: BTreeSet<&str> = descriptors.iter().map(|tool| tool.name).collect();
        assert_eq!(names.len(), 4, "工具名必须唯一");

        for descriptor in &descriptors {
            assert!(!descriptor.description.trim().is_empty());
            assert_eq!(descriptor.input_schema["type"], json!("object"));
            assert_eq!(
                descriptor.input_schema["additionalProperties"],
                json!(false)
            );
            assert!(descriptor.input_schema["required"].is_array());
            let json = descriptor.to_json();
            assert_eq!(json["name"], json!(descriptor.name));
            assert!(json["inputSchema"].is_object());
        }
    }

    #[test]
    fn validate_reports_success_and_structured_diagnostics() {
        let outcome = call_tool(TOOL_VALIDATE, &json!({ "source": SOURCE })).expect("调用应当成功");
        assert_eq!(outcome.structured["valid"], json!(true));
        assert_eq!(outcome.structured["diagnostics"], json!([]));

        let broken = call_tool(TOOL_VALIDATE, &json!({ "source": "port = \n" })).unwrap();
        assert_eq!(broken.structured["valid"], json!(false));
        assert_eq!(
            broken.structured["diagnostics"][0]["code"],
            json!("parse_error")
        );
        assert!(broken.structured["diagnostics"][0]["line"].is_number());
    }

    #[test]
    fn validate_strict_mode_rejects_undeclared_fields() {
        let source =
            "#@schema {\n  port {\n    type = \"integer\"\n  }\n}\n\nport = 8080\nextra = 1\n";
        let lenient = call_tool(TOOL_VALIDATE, &json!({ "source": source })).unwrap();
        assert_eq!(lenient.structured["valid"], json!(true));

        let strict =
            call_tool(TOOL_VALIDATE, &json!({ "source": source, "strict": true })).unwrap();
        assert_eq!(strict.structured["valid"], json!(false));
        assert_eq!(
            strict.structured["diagnostics"][0]["code"],
            json!("validation_error")
        );
    }

    #[test]
    fn audit_returns_findings_with_rule_ids() {
        let outcome = call_tool(
            TOOL_AUDIT,
            &json!({ "source": "db_password = \"secret\"\nhost = \"0.0.0.0\"\n" }),
        )
        .expect("调用应当成功");

        let findings = outcome.structured["findings"].as_array().unwrap();
        assert!(!findings.is_empty());
        assert!(findings
            .iter()
            .any(|finding| finding["rule_id"] == json!("SEC-SENS-001")));
        assert!(findings
            .iter()
            .any(|finding| finding["rule_id"] == json!("SEC-004")));
        assert!(outcome.structured["summary"]["total"].as_u64().unwrap() >= 2);
    }

    #[test]
    fn apply_edit_returns_minimal_change_and_structured_refusals() {
        let plan = json!({
            "version": "1.0",
            "edits": [
                { "op": "set", "path": ["server", "port"], "value": 9090, "expect": { "value": 8080 } }
            ]
        });
        let outcome = call_tool(TOOL_APPLY_EDIT, &json!({ "source": SOURCE, "plan": plan }))
            .expect("合法编辑应当成功");
        let updated = outcome.structured["source"].as_str().unwrap();
        assert!(updated.contains("  port = 9090 #@ range(1..65535)"));
        assert!(updated.contains("#@schema {"));
        assert_eq!(
            outcome.structured["applied"][0]["path"],
            json!("server.port")
        );

        let mismatch = json!({
            "version": "1.0",
            "edits": [
                { "op": "set", "path": ["server", "port"], "value": 1, "expect": { "value": 1234 } }
            ]
        });
        let failure = call_tool(
            TOOL_APPLY_EDIT,
            &json!({ "source": SOURCE, "plan": mismatch }),
        )
        .expect_err("前置条件不符必须失败");
        assert_eq!(failure.code, "expectation_mismatch");
        assert_eq!(failure.details["expected"], json!("1234"));
        assert_eq!(failure.details["actual"], json!("8080"));

        let invalid_plan = call_tool(
            TOOL_APPLY_EDIT,
            &json!({ "source": SOURCE, "plan": { "version": "9.9", "edits": [] } }),
        )
        .expect_err("契约不合法必须失败");
        assert_eq!(invalid_plan.code, "invalid_plan");
        assert!(invalid_plan.details["violations"].is_array());
    }

    #[test]
    fn edit_range_applies_and_refuses_unsafe_ranges() {
        let source = "port = 8080\n";
        let start = source.find("8080").unwrap();
        let outcome = call_tool(
            TOOL_EDIT_RANGE,
            &json!({ "source": source, "start": start, "end": start + 4, "replacement": "9090" }),
        )
        .expect("合法区间应当成功");
        assert_eq!(outcome.structured["source"], json!("port = 9090\n"));
        assert_eq!(outcome.structured["replaced"]["before"], json!("8080"));

        let out_of_bounds = call_tool(
            TOOL_EDIT_RANGE,
            &json!({ "source": source, "start": 0, "end": 999, "replacement": "x" }),
        )
        .expect_err("越界必须失败");
        assert_eq!(out_of_bounds.code, "unsupported_target");

        let breaks_schema = call_tool(
            TOOL_EDIT_RANGE,
            &json!({ "source": SOURCE, "start": SOURCE.find("8080").unwrap(), "end": SOURCE.find("8080").unwrap() + 4, "replacement": "\"x\"" }),
        )
        .expect_err("破坏 schema 必须失败");
        assert_eq!(breaks_schema.code, "validation_failed");
    }

    #[test]
    fn missing_arguments_and_unknown_tools_fail_with_stable_codes() {
        let missing = call_tool(TOOL_VALIDATE, &json!({})).expect_err("缺少 source 必须失败");
        assert_eq!(missing.code, "invalid_arguments");

        let unknown = call_tool("verseconf_nope", &json!({})).expect_err("未知工具必须失败");
        assert_eq!(unknown.code, "unknown_tool");
        assert_eq!(unknown.details["name"], json!("verseconf_nope"));
    }

    #[test]
    fn result_envelopes_are_shared_by_every_distribution_path() {
        assert_eq!(tools_list_value()["tools"].as_array().unwrap().len(), 4);

        let ok = tool_result_value(TOOL_VALIDATE, &json!({ "source": "port = 8080\n" }));
        assert_eq!(ok["isError"], json!(false));
        assert_eq!(ok["structuredContent"]["valid"], json!(true));
        assert_eq!(ok["content"][0]["type"], json!("text"));

        let refused = tool_result_value(TOOL_VALIDATE, &json!({}));
        assert_eq!(refused["isError"], json!(true));
        assert_eq!(
            refused["structuredContent"]["code"],
            json!("invalid_arguments")
        );
    }
}

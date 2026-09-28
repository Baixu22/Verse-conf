//! 五个能力的工具契约与实现。
//!
//! 工具只接受文本与结构化意图，返回结构化结果；失败时返回稳定的错误码，
//! 宿主不需要解析人类可读文案来判断失败原因。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use verseconf_core::{
    apply_edit_plan, apply_edit_plan_in_files, check_write_with, parse, replace_range,
    validate_ast, AppliedEdit, AuditEngine, EditPlan, EditRefusal, EffectiveView, SchemaValidator,
    VerseconfError, WriteGuard, EDIT_PLAN_JSON_SCHEMA,
};
use verseconf_json::{
    check_write_json_with, check_write_json_with_document_schema, declared_schema, DocumentSchema,
    JsonFlavor, JsonGuard, JsonSchemaDraft, SchemaFormat,
};
use verseconf_toml::{check_write_toml_with, TomlGuard};

/// `plan` 字段的 schema：把仓库里已有的编辑计划 schema 原样内联进工具发现结果。
///
/// 工具发现结果是模型唯一能看到的信息——把契约留成一句「见某处」等于没有契约，
/// 宿主就得额外注入说明或让模型猜格式。
fn plan_schema() -> Value {
    serde_json::from_str(EDIT_PLAN_JSON_SCHEMA).unwrap_or_else(|_| json!({ "type": "object" }))
}

/// `verseconf_apply_edit` 的输入契约。
///
/// wasm32 上没有文件系统：`path` 不是「还没实现」，而是在这个目标上根本做不到。
/// 所以直接从能力声明里去掉，而不是暴露一个必然失败的参数——共用同一份 Rust 代码
/// 并不会自动继承宿主的文件系统能力。
fn apply_edit_input_schema() -> Value {
    let on_wasm = cfg!(target_arch = "wasm32");

    let mut properties = serde_json::Map::new();
    properties.insert(
        "source".to_string(),
        json!({
            "type": "string",
            "description": if on_wasm {
                "原始配置文本；wasm 目标只支持这种输入"
            } else {
                "原始配置文本；与 path 二选一"
            },
        }),
    );
    properties.insert(
        "include_source".to_string(),
        json!({
            "type": "boolean",
            "description": "是否在响应里回传改动后的完整文本，默认 true。响应会进入模型上下文，只想看改动时设为 false 可省掉随文件长度增长的输入成本；此时响应不含 source，只有 applied 与 effective_view"
        }),
    );
    properties.insert("plan".to_string(), plan_schema());

    if !on_wasm {
        properties.insert(
            "path".to_string(),
            json!({
                "type": "string",
                "description": "入口配置文件路径；给了它就在 @include 图里定位目标，与 source 二选一"
            }),
        );
    }

    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["plan"],
        // source 与 path 二选一写进 schema，而不是只写在描述里
        "oneOf": if on_wasm {
            json!([{ "required": ["source"] }])
        } else {
            json!([{ "required": ["source"] }, { "required": ["path"] }])
        },
        "properties": Value::Object(properties),
    })
}

/// 把已应用的编辑渲染成工具响应里的数组（单文件与跨文件共用）
fn applied_json(applied: &[AppliedEdit]) -> Vec<Value> {
    applied
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
        .collect()
}

/// 校验配置：解析 + 结构/schema 校验
pub const TOOL_VALIDATE: &str = "verseconf_validate";
/// 安全审计：敏感数据与不安全配置检查
pub const TOOL_AUDIT: &str = "verseconf_audit";
/// 意图应用：按编辑计划做确定性最小改动
pub const TOOL_APPLY_EDIT: &str = "verseconf_apply_edit";
/// 区间编辑：对指定字节区间做替换，走同一套写入前校验
pub const TOOL_EDIT_RANGE: &str = "verseconf_edit_range";
/// 写前检查：判断「把 baseline 改成 candidate」是否允许落盘
pub const TOOL_CHECK_WRITE: &str = "verseconf_check_write";

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

/// 五个工具的描述
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
            description: if cfg!(target_arch = "wasm32") {
                "按编辑意图契约做确定性最小改动：只替换目标字段的值区间，改动之外字节零变化；写入前做 schema 与安全双重校验，无法唯一定位时拒绝。wasm 目标没有文件系统，只接受 source；要按路径改文件请在宿主侧读取内容后传 source。"
            } else {
                "按编辑意图契约做确定性最小改动：只替换目标字段的值区间，改动之外字节零变化；写入前做 schema 与安全双重校验，无法唯一定位时拒绝。给 source 时按这份文本处理；给 path 时在 @include 图里定位目标（目标可能落在被包含的文件里），定位无法唯一确定时拒绝。"
            },
            input_schema: apply_edit_input_schema(),
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
        ToolDescriptor {
            name: TOOL_CHECK_WRITE,
            description: "写前检查：给定改动前的文本 baseline 与准备落盘的文本 candidate，判断这次改动是否允许写入。它不关心 candidate 是怎么产生的——宿主可以用自己的编辑方式（字符串替换、区间替换、整文件重写），只在落盘前过这一道。只拒绝本次改动**新引入**的高危安全实例（按规则+位置比较），不因文件本来就有的问题拒绝；候选不合法或破坏 schema 时同样拒绝。通过返回 allowed=true；拒绝返回与编辑路径同一套稳定错误码。schema 可以旁挂（`#@schema` 语言或标准 JSON Schema），也可以让本工具按配置自己的 `$schema` 去取（schema_from_config）。",
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["baseline", "candidate"],
                "properties": {
                    "baseline": { "type": "string", "description": "改动前的配置文本（用于建立风险基线）" },
                    "candidate": { "type": "string", "description": "准备落盘的配置文本" },
                    "schema": { "type": "string", "description": "可选的旁挂 schema 文本。默认按 #@schema { ... } 语言解释；schema_format=json-schema 时按标准 JSON Schema 解释" },
                    "schema_format": { "type": "string", "enum": ["vcf", "json-schema"], "description": "旁挂 schema 用的是哪种语言，默认 vcf（自建 #@schema DSL，与 .vcf / TOML 同源）。json-schema 只对 format=json/jsonc 生效；不受支持的关键字与方言会返回 unsupported_schema，而不是被静默忽略" },
                    "schema_draft": { "type": "string", "enum": ["auto", "draft-07", "2020-12"], "description": "JSON Schema 的方言，默认 auto（按 schema 自己的 $schema 认；没有声明时按 2020-12）" },
                    "schema_from_config": { "type": "boolean", "description": "为 true 时不使用 schema 参数，而是按配置里顶层 $schema 声明去取 schema（默认 false）。支持本地相对/绝对路径与 file://；网络 URL 不会被抓取，取不到时返回 schema_unavailable，不会静默放行" },
                    "base_dir": { "type": "string", "description": "$schema 是相对路径时的基准目录（通常是配置文件所在目录）。schema_from_config 时使用" },
                    "allow_unresolved_schema": { "type": "boolean", "description": "schema_from_config 取不到 schema 时是否降级为「只做结构与安全校验」，默认 false（返回 schema_unavailable 拒绝写入）。设为 true 是调用方显式承担「这次没按 schema 检查」的后果：结果里 schema 会报成 skipped 并带上原因" },
                    "schema_url_map": { "type": "object", "description": "$schema URL → 本地 schema 文件的映射（值都是字符串路径）。宿主侧下载好 SchemaStore 的 schema 后用这个指过来；本工具自己不抓网络，未命中的 URL 返回 schema_unavailable" },
                    "format": { "type": "string", "enum": ["vcf", "toml", "json", "jsonc"], "description": "候选文本的格式，默认 vcf。TOML 与 JSON/JSONC 的 schema 只能旁挂传入（它们没有内联 schema 语法）。json 严格（拒绝注释与尾随逗号），jsonc 允许注释、尾随逗号与单引号字符串；两者都拒绝缺逗号" }
                }
            }),
        },
    ]
}

/// 调用工具
pub fn call_tool(name: &str, arguments: &Value) -> Result<ToolOutcome, ToolFailure> {
    reject_unknown_arguments(name, arguments)?;

    match name {
        TOOL_VALIDATE => validate(arguments),
        TOOL_AUDIT => audit(arguments),
        TOOL_APPLY_EDIT => apply_edit(arguments),
        TOOL_EDIT_RANGE => edit_range(arguments),
        TOOL_CHECK_WRITE => check_write_tool(arguments),
        other => Err(ToolFailure::unknown_tool(other)),
    }
}

/// 在某个目标上被**刻意**从工具契约里去掉的参数。
///
/// 同一条 Rust 代码在不同目标上能力不同：`apply_edit` 的 `path` 在 wasm 上做不到
/// （没有文件系统），所以它不在那条路径的契约里。但传进来的 `path` 不是「不认识的
/// 参数」——它是**认识的、这个目标不支持的**参数，必须由工具自己回
/// `unsupported_on_platform`，而不是在参数校验这一层被当成拼错。
/// 两条分发路径的逐字节一致性（tests/parity.mjs）正是钉在这里的。
const PLATFORM_OMITTED_ARGUMENTS: &[(&str, &[&str])] = &[(TOOL_APPLY_EDIT, &["path"])];

/// 工具契约声明了 `additionalProperties: false`，那就必须真的拒。
///
/// 声明与行为不一致比不声明更糟：宿主相信写错的参数名会被指出来，于是把
/// `schema_format` 拼成 `schemaFormat` 之后，门禁**静默按默认值**跑了一遍，
/// 结果看起来是「检查过了」。独立复核把这条报成了 MAJOR，理由就是这个。
///
/// 未知工具名不在这里报：那是 [`ToolFailure::unknown_tool`] 的职责。
fn reject_unknown_arguments(name: &str, arguments: &Value) -> Result<(), ToolFailure> {
    let Value::Object(arguments) = arguments else {
        return Ok(());
    };
    let Some(descriptor) = tool_descriptors()
        .into_iter()
        .find(|descriptor| descriptor.name == name)
    else {
        return Ok(());
    };
    let Some(properties) = descriptor
        .input_schema
        .get("properties")
        .and_then(Value::as_object)
    else {
        return Ok(());
    };

    let omitted = PLATFORM_OMITTED_ARGUMENTS
        .iter()
        .find(|(tool, _)| *tool == name)
        .map(|(_, keys)| *keys)
        .unwrap_or(&[]);

    let mut unknown: Vec<&str> = arguments
        .keys()
        .filter(|key| !properties.contains_key(key.as_str()) && !omitted.contains(&key.as_str()))
        .map(String::as_str)
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    unknown.sort_unstable();

    let mut known: Vec<&str> = properties.keys().map(String::as_str).collect();
    known.extend_from_slice(omitted);
    known.sort_unstable();
    Err(ToolFailure::invalid_arguments(format!(
        "不认识的参数：{}（{} 可用参数：{}）",
        unknown.join(", "),
        name,
        known.join(", ")
    )))
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

/// 写前检查：宿主用自己的编辑方式产生 candidate，落盘前过这一道。
///
/// 这是「门禁」与「编辑机制」解耦后的独立入口：它不产生改动，只裁决改动。
/// 因此它不需要模型理解编辑计划协议——模型照旧用自己的方式改文件，
/// 成本侧不增加任何 token。
fn check_write_tool(arguments: &Value) -> Result<ToolOutcome, ToolFailure> {
    let baseline = required_str(arguments, "baseline")?;
    let candidate = required_str(arguments, "candidate")?;
    let schema = match arguments.get("schema") {
        Some(Value::String(schema)) => Some(schema.as_str()),
        None => None,
        Some(_) => return Err(ToolFailure::invalid_arguments("参数 'schema' 必须是字符串")),
    };
    // schema 的语言同样由调用方声明：自建 DSL 与标准 JSON Schema 形状不同，
    // 猜错会把一份合法 schema 报成 parse_failed。
    let schema_format = match arguments.get("schema_format") {
        Some(Value::String(text)) => SchemaFormat::parse(text).ok_or_else(|| {
            ToolFailure::invalid_arguments(format!(
                "不支持的 schema_format '{text}'：目前支持 vcf 与 json-schema"
            ))
        })?,
        None => SchemaFormat::VcfDsl,
        Some(_) => {
            return Err(ToolFailure::invalid_arguments(
                "参数 'schema_format' 必须是字符串",
            ))
        }
    };
    let schema_draft = match arguments.get("schema_draft") {
        Some(Value::String(text)) => match text.as_str() {
            "auto" => JsonSchemaDraft::Auto,
            "draft-07" | "draft7" => JsonSchemaDraft::Draft7,
            "2020-12" | "draft2020-12" => JsonSchemaDraft::Draft202012,
            other => {
                return Err(ToolFailure::invalid_arguments(format!(
                    "不支持的 schema_draft '{other}'：目前支持 auto、draft-07 与 2020-12"
                )))
            }
        },
        None => JsonSchemaDraft::Auto,
        Some(_) => {
            return Err(ToolFailure::invalid_arguments(
                "参数 'schema_draft' 必须是字符串",
            ))
        }
    };
    let schema_from_config = match arguments.get("schema_from_config") {
        Some(Value::Bool(flag)) => *flag,
        None => false,
        Some(_) => {
            return Err(ToolFailure::invalid_arguments(
                "参数 'schema_from_config' 必须是布尔值",
            ))
        }
    };
    let base_dir = match arguments.get("base_dir") {
        Some(Value::String(dir)) => Some(dir.as_str()),
        None => None,
        Some(_) => {
            return Err(ToolFailure::invalid_arguments(
                "参数 'base_dir' 必须是字符串",
            ))
        }
    };
    // 取不到 schema 时是否允许降级为「只做结构与安全校验」。默认 false（拒绝）。
    let allow_unresolved_schema = match arguments.get("allow_unresolved_schema") {
        Some(Value::Bool(flag)) => *flag,
        None => false,
        Some(_) => {
            return Err(ToolFailure::invalid_arguments(
                "参数 'allow_unresolved_schema' 必须是布尔值",
            ))
        }
    };
    // SchemaStore 之类的 URL → 本地 schema 文件。宿主侧下载、这里映射，
    // 于是「按 $schema 解析」对真实配置可达，而门禁自己不需要网络能力。
    let schema_url_map = match arguments.get("schema_url_map") {
        Some(Value::Object(map)) => {
            let mut out = BTreeMap::new();
            for (url, path) in map {
                let Some(path) = path.as_str() else {
                    return Err(ToolFailure::invalid_arguments(
                        "参数 'schema_url_map' 的值必须是本地文件路径字符串",
                    ));
                };
                out.insert(url.clone(), PathBuf::from(path));
            }
            out
        }
        None => BTreeMap::new(),
        Some(_) => {
            return Err(ToolFailure::invalid_arguments(
                "参数 'schema_url_map' 必须是对象（URL → 本地路径）",
            ))
        }
    };
    // 格式由调用方声明，不由本层猜：猜错格式会把一份合法配置报成 parse_failed，
    // 而门禁的拒绝必须是可归因的。
    let format = match arguments.get("format") {
        Some(Value::String(format)) => format.as_str(),
        None => "vcf",
        Some(_) => return Err(ToolFailure::invalid_arguments("参数 'format' 必须是字符串")),
    };

    // 用配置里的 `$schema` 时，把它报告回去：调用方需要知道这次到底按哪份 schema 检查的，
    // 以及（在显式降级时）这次根本没检查 schema
    let mut schema_source: Option<String> = None;
    let mut schema_skipped: Option<String> = None;
    // 配置自己声明了 `$schema`、但这次调用没要求按它校验：如实报出来，
    // 免得读结果的人以为 schema 检查过
    let mut schema_declared_unused: Option<String> = None;

    match format {
        "vcf" | "toml" => {
            if schema_from_config || schema_format == SchemaFormat::JsonSchema {
                return Err(ToolFailure::invalid_arguments(
                    "schema_from_config 与 schema_format=json-schema 只对 JSON/JSONC 生效：\
                     .vcf 与 TOML 里没有 $schema 声明，它们的 schema 只能是 #@schema 语言的旁挂文本",
                ));
            }
            if format == "vcf" {
                let guard = match schema {
                    Some(schema) => WriteGuard::with_schema(schema),
                    None => WriteGuard::default(),
                };
                check_write_with(baseline, candidate, &guard).map_err(refusal_failure)?;
            } else {
                let guard = match schema {
                    Some(schema) => TomlGuard::with_schema(schema),
                    None => TomlGuard::default(),
                };
                check_write_toml_with(baseline, candidate, &guard).map_err(refusal_failure)?;
            }
        }
        "json" | "jsonc" => {
            // 宽严由调用方声明，不由本层猜：缺逗号这类写法只在 jsonc 下成立，
            // 用宽松语法去读一份 .json 等于对坏文件说 allowed。
            let flavor = if format == "json" {
                JsonFlavor::Json
            } else {
                JsonFlavor::Jsonc
            };
            let guard = JsonGuard {
                flavor,
                schema_draft,
                ..JsonGuard::default()
            };

            if schema_from_config {
                if schema.is_some() {
                    return Err(ToolFailure::invalid_arguments(
                        "schema 与 schema_from_config 只能给一个：前者是旁挂文本，\
                         后者要求按配置自己的 $schema 取",
                    ));
                }
                let document = DocumentSchema {
                    base_dir: base_dir.map(PathBuf::from),
                    url_map: schema_url_map.clone(),
                    draft: schema_draft,
                    allow_unresolved: allow_unresolved_schema,
                };
                let outcome =
                    check_write_json_with_document_schema(baseline, candidate, &guard, &document)
                        .map_err(refusal_failure)?;
                schema_source = outcome.declared;
                schema_skipped = outcome.skipped;
            } else {
                let guard = match (schema, schema_format) {
                    (Some(text), SchemaFormat::VcfDsl) => {
                        // 常见误用：把一份标准 JSON Schema 当自建 DSL 传进来，
                        // 得到的是一句含糊的 parse_failed。自建 DSL 以 `#@schema`
                        // 开头，而以 `{` 开头又能当 JSON 解析的文本必然不是它，
                        // 所以这里可以明确告诉调用方该怎么办。
                        if text.trim_start().starts_with('{')
                            && serde_json::from_str::<Value>(text).is_ok()
                        {
                            return Err(ToolFailure::invalid_arguments(
                                "schema 看起来是一份标准 JSON Schema，但 schema_format 默认是 vcf\
                                 （自建 #@schema 语言）。要按标准 JSON Schema 校验请传 schema_format=json-schema",
                            ));
                        }
                        JsonGuard {
                            schema: Some(text),
                            ..guard
                        }
                    }
                    (Some(text), SchemaFormat::JsonSchema) => JsonGuard {
                        schema: Some(text),
                        schema_format: SchemaFormat::JsonSchema,
                        ..guard
                    },
                    (None, _) => {
                        // 配置自己声明了 $schema 却没让本层用它：如实报出来
                        schema_declared_unused = declared_schema(baseline, flavor)
                            .ok()
                            .flatten()
                            .or_else(|| declared_schema(candidate, flavor).ok().flatten());
                        guard
                    }
                };
                check_write_json_with(baseline, candidate, &guard).map_err(refusal_failure)?;
            }
        }
        other => {
            return Err(ToolFailure::invalid_arguments(format!(
                "不支持的 format '{other}'：目前支持 vcf、toml、json 与 jsonc"
            )))
        }
    }

    let used_schema = if schema_skipped.is_some() {
        // 降级跳过了 schema 那一层时不能报成「按 schema 检查过」
        "skipped"
    } else if schema_source.is_some() {
        "json-schema"
    } else if schema.is_some() {
        schema_format.as_str()
    } else {
        "none"
    };

    // 跳过了 schema 就不能说「检查通过」了事——那句话会被读成「包括 schema 都过了」
    let summary = match &schema_skipped {
        Some(reason) => {
            format!("写前检查通过（{format}）：结构与安全审计已过；schema **未检查**（{reason}）")
        }
        None => format!("写前检查通过（{format}）：改动未引入新的高危安全实例，且结果仍然合法。"),
    };

    Ok(ToolOutcome {
        structured: json!({
            "allowed": true,
            "format": format,
            "baseline_bytes": baseline.len(),
            "candidate_bytes": candidate.len(),
            "schema": used_schema,
            "schema_source": schema_source,
            "schema_skipped": schema_skipped,
            "schema_declared_unused": schema_declared_unused,
        }),
        summary,
    })
}

fn apply_edit(arguments: &Value) -> Result<ToolOutcome, ToolFailure> {
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

    // 响应会进入模型上下文：是否回传完整文本由调用方显式选择。
    // 默认 true 保持既有行为；source 模式下调用方没有别的渠道拿到结果，
    // 因此设成 false 通常只会拿到一份用不了的结果——这一点写在 schema 描述里。
    let include_source = arguments
        .get("include_source")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    // 给了 path：在 @include 图里定位目标——目标可能落在被包含的文件里。
    // 定位是 fail-closed 的：多个文件都能定位到就拒绝，而不是猜一个。
    if let Some(path) = arguments.get("path").and_then(Value::as_str) {
        // wasm32 没有文件系统：共用同一份 Rust 代码并不会自动继承宿主读文件的能力。
        // 能力声明里已经去掉了 path，这里再给一个明确的错误码，而不是让它落到底层
        // IO 去报一句「operation not supported on this platform」。
        if cfg!(target_arch = "wasm32") {
            return Err(ToolFailure::new(
                "unsupported_on_platform",
                "wasm 目标没有文件系统，无法按路径读取配置；请在宿主侧读取文件内容后改用 source",
                json!({ "path": path }),
            ));
        }

        let outcome = apply_edit_plan_in_files(Path::new(path), &plan).map_err(refusal_failure)?;
        let applied = applied_json(&outcome.applied);
        // 合并后的生效配置是否真的被校验过，必须如实告诉宿主：
        // 只校验被改动的那个文件不足以证明整份配置合法。
        let (effective_view, effective_view_note) = match &outcome.effective_view {
            EffectiveView::Validated => ("validated", String::new()),
            EffectiveView::NotValidated { reason } => ("not_validated", reason.clone()),
        };
        let summary = format!(
            "已应用 {} 条编辑；目标文件 {}；改动之外字节保持不变；合并后的生效配置{}{}",
            outcome.applied.len(),
            outcome.file.path.display(),
            if effective_view_note.is_empty() {
                "已校验".to_string()
            } else {
                format!("未校验（{effective_view_note}）")
            },
            if include_source {
                ""
            } else {
                "；未回传完整文本"
            }
        );

        let mut structured = json!({
            "file": outcome.file.path.display().to_string(),
            "applied": applied,
            "effective_view": effective_view,
            "effective_view_note": effective_view_note,
        });
        attach_source(&mut structured, outcome.file.after, include_source);

        return Ok(ToolOutcome {
            structured,
            summary,
        });
    }

    let source = required_str(arguments, "source")?;
    let outcome = apply_edit_plan(source, &plan).map_err(refusal_failure)?;
    let applied = applied_json(&outcome.applied);
    let summary = format!(
        "已应用 {} 条编辑；改动之外字节保持不变{}",
        outcome.applied.len(),
        if include_source {
            ""
        } else {
            "；未回传完整文本"
        }
    );

    let mut structured = json!({ "applied": applied });
    attach_source(&mut structured, outcome.source, include_source);

    Ok(ToolOutcome {
        structured,
        summary,
    })
}

/// 按调用方的选择决定是否把改动后的完整文本放进响应。
///
/// 不回传时给出结果长度，调用方至少知道改动确实产生了内容，而不是收到一个
/// 看起来什么都没做的空响应。
fn attach_source(structured: &mut Value, text: String, include_source: bool) {
    let Some(object) = structured.as_object_mut() else {
        return;
    };
    if include_source {
        object.insert("source".to_string(), Value::String(text));
    } else {
        object.insert("source_omitted".to_string(), Value::Bool(true));
        object.insert("result_bytes".to_string(), json!(text.len()));
    }
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
        assert_eq!(descriptors.len(), 5, "必须暴露五个工具");

        let names: BTreeSet<&str> = descriptors.iter().map(|tool| tool.name).collect();
        assert_eq!(names.len(), descriptors.len(), "工具名必须唯一");

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
    fn the_discovery_schema_inlines_the_full_edit_plan_contract() {
        // 工具发现结果是模型唯一能看到的信息：契约必须在这里展开，
        // 而不是留一句「见某处」让宿主去注入说明或让模型去猜。
        let descriptors = tool_descriptors();
        let apply = descriptors
            .iter()
            .find(|tool| tool.name == TOOL_APPLY_EDIT)
            .expect("必须有 apply_edit");

        let plan = &apply.input_schema["properties"]["plan"];
        assert_eq!(plan["type"], json!("object"));
        assert_eq!(plan["required"], json!(["version", "edits"]));
        assert_eq!(
            plan["$defs"]["edit"]["properties"]["op"]["enum"],
            json!(["set", "insert", "delete"])
        );
        assert!(
            plan["$defs"]["segment"]["oneOf"].is_array(),
            "命名列表定位的结构也要展开：{plan}"
        );

        // source 与 path 的二选一写进 schema，而不是只写在描述里
        assert_eq!(
            apply.input_schema["oneOf"],
            json!([{ "required": ["source"] }, { "required": ["path"] }])
        );
        assert!(
            apply.input_schema["properties"]["include_source"].is_object(),
            "响应形态的选择必须是显式参数"
        );
    }

    #[test]
    fn apply_edit_can_omit_the_result_text() {
        let plan = json!({
            "version": "1.0",
            "edits": [{ "op": "set", "path": ["server", "port"], "value": 9090 }]
        });

        let with_text = call_tool(
            TOOL_APPLY_EDIT,
            &json!({ "source": SOURCE, "plan": plan.clone() }),
        )
        .expect("应当成功");
        assert!(
            with_text.structured["source"].is_string(),
            "默认仍然回传完整文本，保持既有行为"
        );

        let without_text = call_tool(
            TOOL_APPLY_EDIT,
            &json!({ "source": SOURCE, "plan": plan, "include_source": false }),
        )
        .expect("应当成功");
        assert!(
            without_text.structured.get("source").is_none(),
            "显式关闭后响应里不应再有完整文本：{}",
            without_text.structured
        );
        assert_eq!(without_text.structured["source_omitted"], json!(true));
        assert!(
            without_text.structured["result_bytes"]
                .as_u64()
                .is_some_and(|bytes| bytes > 0),
            "至少要给出结果长度：{}",
            without_text.structured
        );
        assert!(without_text.summary.contains("未回传完整文本"));
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
    fn check_write_refuses_a_dangerous_candidate_and_allows_a_safe_one() {
        // 宿主用自己的编辑方式产生 candidate，门禁只裁决、不参与编辑
        let baseline = "tls_verify = true\nport = 8080\n";

        let dangerous = call_tool(
            TOOL_CHECK_WRITE,
            &json!({ "baseline": baseline, "candidate": "tls_verify = false\nport = 8080\n" }),
        )
        .expect_err("关闭证书校验必须被拒绝");
        assert_eq!(dangerous.code, "security_rejected");

        let safe = call_tool(
            TOOL_CHECK_WRITE,
            &json!({ "baseline": baseline, "candidate": "tls_verify = true\nport = 9090\n" }),
        )
        .expect("与安全无关的改动必须放行");
        assert_eq!(safe.structured["allowed"], json!(true));
    }

    #[test]
    fn check_write_accepts_a_sidecar_schema() {
        // 类型漂移（整数被写成字符串）正是确认性复验里 10 次静默误改的形态；
        // 旁挂 schema 必须能挡住它，哪怕 candidate 来自朴素字符串替换。
        let schema = "#@schema {\n  tab_spaces {\n    type = \"integer\"\n  }\n}\n";

        let refusal = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": "tab_spaces = 4242\n",
                "candidate": "tab_spaces = \"4242\"\n",
                "schema": schema,
            }),
        )
        .expect_err("类型漂移必须被 schema 拒绝");
        assert_eq!(refusal.code, "validation_failed");
    }

    #[test]
    fn check_write_serves_toml_when_the_caller_declares_the_format() {
        // 门禁此前只对无人使用的 .vcf 可达。这一组证明它对真实 TOML 也可达，
        // 且候选文本完全不含编辑计划协议。
        let dangerous = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": "tls_verify = true\nport = 8080\n",
                "candidate": "tls_verify = false\nport = 8080\n",
                "format": "toml",
            }),
        )
        .expect_err("TOML 上关掉证书校验必须被拒绝");
        assert_eq!(dangerous.code, "security_rejected");

        let safe = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": "tls_verify = true\nport = 8080\n",
                "candidate": "tls_verify = true\nport = 9090\n",
                "format": "toml",
            }),
        )
        .expect("TOML 上与安全无关的改动必须放行");
        assert_eq!(safe.structured["allowed"], json!(true));
        assert_eq!(safe.structured["format"], json!("toml"));
    }

    #[test]
    fn check_write_toml_accepts_a_sidecar_schema() {
        // TOML 没有内联 schema 语法，旁挂是它唯一的 schema 入口
        let refusal = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": "tab_spaces = 4242\n",
                "candidate": "tab_spaces = \"4242\"\n",
                "schema": "#@schema {\n  tab_spaces {\n    type = \"integer\"\n  }\n}\n",
                "format": "toml",
            }),
        )
        .expect_err("TOML 上的类型漂移必须被旁挂 schema 拒绝");
        assert_eq!(refusal.code, "validation_failed");
    }

    #[test]
    fn check_write_serves_json_when_the_caller_declares_the_format() {
        // agent 宿主的设置类配置以 JSON/JSONC 为主。这一组证明门禁对它也可达，
        // 且候选文本完全不含编辑计划协议。
        let dangerous = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": r#"{"tls_verify": true, "port": 8080}"#,
                "candidate": r#"{"tls_verify": false, "port": 8080}"#,
                "format": "json",
            }),
        )
        .expect_err("JSON 上关掉证书校验必须被拒绝");
        assert_eq!(dangerous.code, "security_rejected");

        let safe = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": r#"{"tls_verify": true, "port": 8080}"#,
                "candidate": r#"{"tls_verify": true, "port": 9090}"#,
                "format": "json",
            }),
        )
        .expect("JSON 上与安全无关的改动必须放行");
        assert_eq!(safe.structured["allowed"], json!(true));
        assert_eq!(safe.structured["format"], json!("json"));
    }

    #[test]
    fn check_write_json_accepts_a_sidecar_schema() {
        // JSON 没有内联 schema 语法，旁挂是它唯一的 schema 入口
        let refusal = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": r#"{"tab_spaces": 4242}"#,
                "candidate": r#"{"tab_spaces": "4242"}"#,
                "schema": "#@schema {\n  tab_spaces {\n    type = \"integer\"\n  }\n}\n",
                "format": "json",
            }),
        )
        .expect_err("JSON 上的类型漂移必须被旁挂 schema 拒绝");
        assert_eq!(refusal.code, "validation_failed");
    }

    #[test]
    fn check_write_jsonc_accepts_commented_configs_that_strict_json_refuses() {
        let baseline = "{\n  // 传输层\n  \"tls_verify\": true,\n}\n";
        let candidate = "{\n  // 传输层\n  \"tls_verify\": true,\n  \"port\": 9090,\n}\n";

        let jsonc = call_tool(
            TOOL_CHECK_WRITE,
            &json!({ "baseline": baseline, "candidate": candidate, "format": "jsonc" }),
        )
        .expect("JSONC 的注释与尾随逗号是合法输入");
        assert_eq!(jsonc.structured["allowed"], json!(true));
        assert_eq!(jsonc.structured["format"], json!("jsonc"));

        let strict = call_tool(
            TOOL_CHECK_WRITE,
            &json!({ "baseline": baseline, "candidate": candidate, "format": "json" }),
        )
        .expect_err("严格 JSON 必须拒绝注释与尾随逗号");
        assert_eq!(strict.code, "validation_failed");
    }

    #[test]
    fn check_write_jsonc_still_refuses_a_missing_comma() {
        // 宽严只在注释与尾随逗号上放行；缺逗号会把两条键粘成一个语义不同的文档
        let failure = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": r#"{"a": 1, "b": 2}"#,
                "candidate": r#"{"a": 1 "b": 2}"#,
                "format": "jsonc",
            }),
        )
        .expect_err("缺逗号必须被拒绝");
        assert_eq!(failure.code, "validation_failed");
    }

    #[test]
    fn check_write_accepts_a_standard_json_schema() {
        // TF-0092：门禁直接吃标准 JSON Schema，不需要使用方手写自建 DSL
        let schema = r#"{
          "$schema": "https://json-schema.org/draft/2020-12/schema",
          "type": "object",
          "properties": { "tab_spaces": { "type": "integer" } }
        }"#;

        let refusal = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": r#"{"tab_spaces": 4242}"#,
                "candidate": r#"{"tab_spaces": "4242"}"#,
                "schema": schema,
                "schema_format": "json-schema",
                "format": "json",
            }),
        )
        .expect_err("2020-12 下类型漂移必须被拒绝");
        assert_eq!(refusal.code, "validation_failed");

        let allowed = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": r#"{"tab_spaces": 4242}"#,
                "candidate": r#"{"tab_spaces": 2}"#,
                "schema": schema,
                "schema_format": "json-schema",
                "format": "json",
                "schema_draft": "2020-12",
            }),
        )
        .expect("良性改动必须放行");
        assert_eq!(allowed.structured["allowed"], json!(true));
        assert_eq!(allowed.structured["schema"], json!("json-schema"));
    }

    #[test]
    fn check_write_reports_an_unsupported_schema_instead_of_ignoring_it() {
        // 规范要求忽略不认识的关键字；门禁不能那样做，否则用户以为约束生效了
        let misspelled = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": r#"{"port": 1}"#,
                "candidate": r#"{"port": 2}"#,
                "schema": r#"{"type": "object", "requierd": ["port"]}"#,
                "schema_format": "json-schema",
                "format": "json",
            }),
        )
        .expect_err("拼错的关键字必须被报告");
        assert_eq!(misspelled.code, "unsupported_schema");
        assert!(misspelled.message.contains("requierd"));

        let wrong_draft = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": r#"{"port": 1}"#,
                "candidate": r#"{"port": 2}"#,
                "schema": r#"{"$schema": "http://json-schema.org/draft-04/schema#"}"#,
                "schema_format": "json-schema",
                "format": "json",
            }),
        )
        .expect_err("不支持的方言必须被报告");
        assert_eq!(wrong_draft.code, "unsupported_schema");
    }

    #[test]
    fn a_json_schema_is_refused_for_formats_it_cannot_check() {
        // JSON Schema 校验的是 JSON 实例；对 .vcf / TOML 用它是没意义的，
        // 明确报错比「悄悄按另一门语言解释」好
        let failure = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": "port = 1\n",
                "candidate": "port = 2\n",
                "schema": r#"{"type": "object"}"#,
                "schema_format": "json-schema",
                "format": "toml",
            }),
        )
        .expect_err("TOML 上不能用 JSON Schema");
        assert_eq!(failure.code, "invalid_arguments");

        let unknown = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": r#"{"a": 1}"#,
                "candidate": r#"{"a": 2}"#,
                "schema": "{}",
                "schema_format": "yaml-schema",
                "format": "json",
            }),
        )
        .expect_err("未知的 schema_format 必须明确拒绝");
        assert_eq!(unknown.code, "invalid_arguments");
        assert!(unknown.message.contains("yaml-schema"));
    }

    #[test]
    fn check_write_can_resolve_the_schema_from_the_config_itself() {
        // TF-0093：按配置里已有的 $schema 取 schema，并且取不到时明确拒绝
        let mut path = std::env::temp_dir();
        path.push(format!("verseconf-mcp-test-{}.json", std::process::id()));
        let schema = r#"{
          "$schema": "https://json-schema.org/draft/2020-12/schema",
          "type": "object",
          "properties": { "tab_spaces": { "type": "integer" } }
        }"#;
        std::fs::write(&path, schema).expect("应当能写临时 schema");
        let base = path
            .parent()
            .expect("有父目录")
            .to_string_lossy()
            .to_string();
        let name = path
            .file_name()
            .expect("有文件名")
            .to_string_lossy()
            .to_string();

        let baseline = format!(r#"{{"$schema": "{name}", "tab_spaces": 4242}}"#);
        let candidate = format!(r#"{{"$schema": "{name}", "tab_spaces": "4242"}}"#);

        let refusal = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": baseline,
                "candidate": candidate,
                "schema_from_config": true,
                "base_dir": base,
                "format": "json",
            }),
        )
        .expect_err("按 $schema 取到的 schema 必须真的被用上");
        assert_eq!(refusal.code, "validation_failed");

        let allowed = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": baseline,
                "candidate": format!(r#"{{"$schema": "{name}", "tab_spaces": 2}}"#),
                "schema_from_config": true,
                "base_dir": base,
                "format": "json",
            }),
        )
        .expect("良性改动必须放行");
        assert_eq!(allowed.structured["schema_source"], json!(name));

        // 取不到时明确报告，而不是静默放行
        let unavailable = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": r#"{"$schema": "https://json.schemastore.org/example.json", "port": 1}"#,
                "candidate": r#"{"$schema": "https://json.schemastore.org/example.json", "port": 2}"#,
                "schema_from_config": true,
                "format": "json",
            }),
        )
        .expect_err("取不到 schema 时不能静默放行");
        assert_eq!(unavailable.code, "schema_unavailable");

        // 显式降级：调用方承担「这次没按 schema 检查」的后果，结果里如实报告
        let downgraded = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": r#"{"$schema": "https://json.schemastore.org/example.json", "port": 1}"#,
                "candidate": r#"{"$schema": "https://json.schemastore.org/example.json", "port": 2}"#,
                "schema_from_config": true,
                "allow_unresolved_schema": true,
                "format": "json",
            }),
        )
        .expect("显式降级时应当按较弱的保证放行");
        assert_eq!(downgraded.structured["schema"], json!("skipped"));
        assert!(
            downgraded.structured["schema_skipped"]
                .as_str()
                .unwrap_or("")
                .contains("url_map"),
            "必须带出跳过原因，实际：{}",
            downgraded.structured["schema_skipped"]
        );
        assert!(
            downgraded.summary.contains("未检查"),
            "摘要不能让调用方以为 schema 也检查过了"
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn unknown_arguments_are_rejected_because_the_contract_says_so() {
        // 工具契约声明了 additionalProperties: false，那就必须真的拒。
        // 声明与行为不一致更糟：宿主以为 `schemaFormat` 会被指出来，而门禁
        // 已经静默按默认值跑了一遍，结果看起来像「检查过了」。
        let failure = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": r#"{"a": 1}"#,
                "candidate": r#"{"a": 2}"#,
                "format": "json",
                "schemaFormat": "json-schema",
            }),
        )
        .expect_err("拼错的参数名必须被指出来");
        assert_eq!(failure.code, "invalid_arguments");
        assert!(
            failure.message.contains("schemaFormat"),
            "实际：{}",
            failure.message
        );
        assert!(
            failure.message.contains("schema_format"),
            "错误信息要列出可用参数，实际：{}",
            failure.message
        );

        // 其它工具同样适用（用真正未声明的键；`strict` 是 validate 的合法参数）
        let failure = call_tool(
            TOOL_VALIDATE,
            &json!({ "source": "port = 1\n", "strictMode": true }),
        )
        .expect_err("validate 也不接受未声明的参数");
        assert_eq!(failure.code, "invalid_arguments");
        assert!(failure.message.contains("strictMode"));

        // 缺必填参数仍然报缺参数（这条检查只拦「不认识」，不抢必填校验）
        let failure = call_tool(TOOL_VALIDATE, &json!({})).expect_err("缺少必填参数必须报错");
        assert_eq!(failure.code, "invalid_arguments");
    }

    #[test]
    fn a_standard_json_schema_sent_without_the_format_hint_says_so() {
        // 把 JSON Schema 当自建 DSL 传进来时，之前只会得到一句含糊的 parse_failed；
        // 现在明确告诉调用方该传什么
        let failure = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": r#"{"a": 1}"#,
                "candidate": r#"{"a": 2}"#,
                "schema": r#"{"type": "object"}"#,
                "format": "json",
            }),
        )
        .expect_err("应当给出可操作的提示");
        assert_eq!(failure.code, "invalid_arguments");
        assert!(failure.message.contains("schema_format=json-schema"));
    }

    #[test]
    fn a_schema_url_map_makes_schemastore_urls_reachable() {
        // TF-0093 的产品面：URL → 本地文件由调用方给，门禁自己不抓网络
        let mut path = std::env::temp_dir();
        path.push(format!("verseconf-mcp-urlmap-{}.json", std::process::id()));
        let schema = r#"{
          "$schema": "https://json-schema.org/draft/2020-12/schema",
          "type": "object",
          "properties": { "tab_spaces": { "type": "integer" } }
        }"#;
        std::fs::write(&path, schema).expect("应当能写临时 schema");
        let url = "https://json.schemastore.org/example.json";
        let mapped = path.to_string_lossy().to_string();

        let baseline = format!(r#"{{"$schema": "{url}", "tab_spaces": 4242}}"#);
        let candidate = format!(r#"{{"$schema": "{url}", "tab_spaces": "4242"}}"#);

        let refusal = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": baseline,
                "candidate": candidate,
                "schema_from_config": true,
                "schema_url_map": { url: mapped },
                "format": "json",
            }),
        )
        .expect_err("映射到的 schema 必须真的被用上");
        assert_eq!(refusal.code, "validation_failed");

        let allowed = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": baseline,
                "candidate": format!(r#"{{"$schema": "{url}", "tab_spaces": 2}}"#),
                "schema_from_config": true,
                "schema_url_map": { url: mapped },
                "format": "json",
            }),
        )
        .expect("良性改动必须放行");
        assert_eq!(allowed.structured["schema_source"], json!(url));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_declared_but_unused_schema_is_reported() {
        // 配置自己声明了 $schema，而这次调用没要求按它校验：结果里如实报出来，
        // 免得读的人以为 schema 检查过（schema 仍然是 none）
        let allowed = call_tool(
            TOOL_CHECK_WRITE,
            &json!({
                "baseline": r#"{"$schema": "https://json.schemastore.org/x.json", "port": 8080}"#,
                "candidate": r#"{"$schema": "https://json.schemastore.org/x.json", "port": 9090}"#,
                "format": "json",
            }),
        )
        .expect("没给 schema 时不校验 schema，但改动本身合法");
        assert_eq!(allowed.structured["schema"], json!("none"));
        assert_eq!(
            allowed.structured["schema_declared_unused"],
            json!("https://json.schemastore.org/x.json")
        );
    }

    #[test]
    fn an_unknown_format_is_rejected_instead_of_guessed() {
        // 猜格式会把一份合法配置报成 parse_failed，而拒绝必须可归因
        let failure = call_tool(
            TOOL_CHECK_WRITE,
            &json!({ "baseline": "a = 1\n", "candidate": "a = 2\n", "format": "yaml" }),
        )
        .expect_err("未知格式必须明确拒绝");
        assert_eq!(failure.code, "invalid_arguments");
        assert!(failure.message.contains("yaml"));
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
    fn apply_edit_with_a_path_locates_the_included_file() {
        let dir = std::env::temp_dir().join("verseconf_mcp_cross_include");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建临时目录");
        std::fs::write(
            dir.join("root.vcf"),
            "@include \"base.vcf\"\nroot_key = 1\n",
        )
        .expect("写入口");
        std::fs::write(dir.join("base.vcf"), "port = 8080 # keep\n").expect("写被包含文件");

        let entry = dir.join("root.vcf").display().to_string();
        let plan = json!({
            "version": "1.0",
            "edits": [{ "op": "set", "path": ["port"], "value": 9090 }]
        });

        let outcome = call_tool(TOOL_APPLY_EDIT, &json!({ "path": entry, "plan": plan }))
            .expect("跨文件定位应当成功");
        assert!(
            outcome.structured["file"]
                .as_str()
                .unwrap()
                .ends_with("base.vcf"),
            "应当报出被改动的文件：{}",
            outcome.structured
        );
        assert_eq!(outcome.structured["source"], json!("port = 9090 # keep\n"));

        // 目标在两个文件里都能定位到：必须拒绝，而不是猜一个
        std::fs::write(dir.join("root.vcf"), "port = 1\n@include \"base.vcf\"\n").expect("改入口");
        let failure = call_tool(TOOL_APPLY_EDIT, &json!({ "path": entry, "plan": plan }))
            .expect_err("歧义必须拒绝");
        assert_eq!(failure.code, "target_ambiguous");

        // 目标哪都没有：报出搜索过的文件
        let missing = json!({
            "version": "1.0",
            "edits": [{ "op": "set", "path": ["nope"], "value": 1 }]
        });
        let failure = call_tool(TOOL_APPLY_EDIT, &json!({ "path": entry, "plan": missing }))
            .expect_err("找不到必须拒绝");
        assert_eq!(failure.code, "target_not_found");

        let _ = std::fs::remove_dir_all(&dir);
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
        assert_eq!(tools_list_value()["tools"].as_array().unwrap().len(), 5);

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

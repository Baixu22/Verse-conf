//! 写入前的双重校验：schema 与安全审计。
//!
//! `.vcf` 路径在写入前做两件事：改动后的文档必须仍然合法（含 schema），
//! 且不得引入新的高危安全实例。这一层把**同样的两件事**接到 JSON / JSONC 上，
//! 并且刻意复用 `.vcf` 路径的实现而不是另写一套：
//!
//! - schema 语言：仍然是 `verseconf_core::parse` 认的 `#@schema { ... }` 文本，
//!   校验仍然交给 `SchemaValidator`，词汇表（type / required / range / enum /
//!   strict / 嵌套字段）与 `.vcf` 完全一致；
//! - 安全审计：仍然交给 `AuditEngine`，实例级比较（规则码 + 位置）与误拒分档
//!   仍然是 `high_risk_instances` / `introduced_high_risk_instances` 那一份；
//! - 拒绝码：仍然返回 `EditRefusal`，schema 失败是 `validation_failed`，
//!   安全问题新引入是 `security_rejected`，与 `.vcf` 路径同一套。
//!
//! ## schema 从哪来
//!
//! `.vcf` 把 schema 写在文档里，JSON 没有对应语法，所以 schema 由调用方
//! 作为**旁挂输入**交给这一层。旁挂的 schema 有两种语言（[`SchemaFormat`]）：
//!
//! - `VcfDsl`：与 `.vcf` 同源的 `#@schema { ... }` 文本，交给 `verseconf_core::parse`；
//! - `JsonSchema`：标准 JSON Schema（draft-07 / 2020-12），交给 `jsonschema`（成熟实现）。
//!
//! 两种都走同一条写入前校验路径、返回同一套拒绝码，所以「写入前双重校验」
//! 这句话对它们同时成立。哪一种是调用方声明的，不由本层猜。
//!
//! ## 为什么 JSON 与 JSONC 是两个开关而不是一个
//!
//! 两者的差别只在注释、尾随逗号与单引号字符串上。**缺逗号两种都不接受**
//! （`{"a":1 "b":2}`）：那不是一种「写法风格」，而是把两条键粘成一个语义不同的
//! 文档——接受它等于门禁对一份坏配置回答「允许落盘」。`jsonc_parser` 的默认
//! 语法把缺逗号也算合法，所以 [`JsonFlavor::Jsonc`] 显式关掉了
//! `allow_missing_commas`；`JsonFlavor::Json` 则把宽松项全部关掉。
//!
//! 宽严由调用方声明（[`JsonFlavor`]），不由本层猜：猜错格式会把一份合法配置
//! 报成 `parse_failed`，而拒绝必须可归因。

use verseconf_core::{
    high_risk_instances, introduced_high_risk_instances, render_risk_instance, Ast, AuditEngine,
    AuditReport, EditRefusal, SchemaValidator,
};

use crate::ast_bridge::json_to_ast;
use crate::json_schema::{validate_json_against_json_schema, JsonSchemaDraft, SchemaFormat};
use crate::JsonFlavor;

/// 写入前校验的开关。
///
/// `Default` 是「照 `.vcf` 路径的规矩来」：审计打开，schema 由调用方给，
/// 语法按 JSONC 认（真实宿主的 `settings.json` 几乎都带注释），schema 语言
/// 按自建内联 DSL 认（与 `.vcf` / TOML 一致）。
#[derive(Debug, Clone, Copy)]
pub struct JsonGuard<'a> {
    /// 旁挂的 schema 文本。`None` = 不做 schema 校验，这与 `.vcf` 路径
    /// 「文档没声明 schema 就不校验 schema」是同一种口径。
    pub schema: Option<&'a str>,
    /// 旁挂 schema 用的是哪种语言。默认 [`SchemaFormat::VcfDsl`]。
    pub schema_format: SchemaFormat,
    /// 标准 JSON Schema 的方言；[`JsonSchemaDraft::Auto`] = 按 schema 自己的 `$schema`。
    pub schema_draft: JsonSchemaDraft,
    /// 是否做写入前安全审计。默认 `true`。
    pub audit: bool,
    /// 候选文本按哪种语法解读。默认 [`JsonFlavor::Jsonc`]。
    pub flavor: JsonFlavor,
}

impl Default for JsonGuard<'_> {
    fn default() -> Self {
        Self {
            schema: None,
            schema_format: SchemaFormat::VcfDsl,
            schema_draft: JsonSchemaDraft::Auto,
            audit: true,
            flavor: JsonFlavor::Jsonc,
        }
    }
}

impl<'a> JsonGuard<'a> {
    /// 带自建 DSL schema 的写入前校验（与 `.vcf` / TOML 同源）
    pub fn with_schema(schema: &'a str) -> Self {
        Self {
            schema: Some(schema),
            ..Self::default()
        }
    }

    /// 带标准 JSON Schema 的写入前校验（draft-07 / 2020-12）
    pub fn with_json_schema(schema: &'a str) -> Self {
        Self {
            schema: Some(schema),
            schema_format: SchemaFormat::JsonSchema,
            ..Self::default()
        }
    }

    /// 指定 JSON Schema 的方言（不指定则按 schema 自己的 `$schema` 认）
    pub fn with_json_schema_draft(self, draft: JsonSchemaDraft) -> Self {
        Self {
            schema_draft: draft,
            ..self
        }
    }

    /// 严格 JSON 语法（拒绝缺逗号、注释与尾随逗号）
    pub fn strict(self) -> Self {
        Self {
            flavor: JsonFlavor::Json,
            ..self
        }
    }

    /// 消融实验的对照臂：只保留编辑机制，关掉 schema 校验与安全审计。
    ///
    /// 这不是给真实写入用的入口。它的存在是为了让「校验层到底贡献了什么」
    /// 可以被测出来，而不是只能被声称。
    pub fn edit_mechanism_only() -> Self {
        Self {
            schema: None,
            audit: false,
            ..Self::default()
        }
    }
}

/// 把一份 JSON / JSONC 文本翻成 core AST（供审计与 schema 校验共用）
pub fn json_ast(source: &str, flavor: JsonFlavor) -> Result<Ast, EditRefusal> {
    json_to_ast(source, flavor)
}

/// 对一份 JSON / JSONC 文本做安全审计。
///
/// 规则与分档与 `.vcf` 路径相同：写死在文件里的凭据是阻断级，
/// 数字/布尔这类「看着像敏感字段其实是数量或开关」只告警，整段 `${...}`
/// 引用按外部来源处理、同样只告警。
pub fn audit_json(source: &str, flavor: JsonFlavor) -> Result<AuditReport, EditRefusal> {
    let ast = json_to_ast(source, flavor)?;
    Ok(AuditEngine::new().audit_ast(&ast))
}

/// 按 `.vcf` 同源的 schema 校验一份 JSON / JSONC 文本。
///
/// 失败返回 `EditRefusal::ValidationFailed`（拒绝码 `validation_failed`），
/// 与 `.vcf` 路径完全一致。
pub fn validate_json_against_schema(
    source: &str,
    schema_text: &str,
    flavor: JsonFlavor,
) -> Result<(), EditRefusal> {
    let schema_ast = verseconf_core::parse(schema_text)
        .map_err(|error| EditRefusal::ParseFailed(format!("schema 无法解析：{error}")))?;
    let Some(schema) = schema_ast.schema else {
        return Err(EditRefusal::ValidationFailed {
            path: "<schema>".to_string(),
            message: "schema 文本里没有 `#@schema { ... }` 块".to_string(),
        });
    };

    let mut ast = json_to_ast(source, flavor)?;
    ast.schema = Some(schema);

    let mut validator = SchemaValidator::new();
    validator
        .validate_with_schema(&ast)
        .map_err(|error| EditRefusal::ValidationFailed {
            path: "<result>".to_string(),
            message: error.to_string(),
        })
}

/// 写前检查：与编辑机制解耦的 JSON / JSONC 门禁。
///
/// 语义与 `.vcf` 路径的 `verseconf_core::check_write` 完全一致：**只裁决改动，
/// 不产生改动**。`candidate` 可以由宿主的任何编辑方式产生——字符串替换、
/// 区间替换、整文件重写都可以——这里只回答「这次改动允许落盘吗」。
///
/// 通过返回 `Ok(())`；拒绝返回与编辑路径同一套 [`EditRefusal`]。
pub fn check_write_json(baseline: &str, candidate: &str) -> Result<(), EditRefusal> {
    check_write_json_with(baseline, candidate, &JsonGuard::default())
}

/// [`check_write_json`] 的完整形态：可以旁挂 schema，也可以显式关掉审计。
pub fn check_write_json_with(
    baseline: &str,
    candidate: &str,
    guard: &JsonGuard<'_>,
) -> Result<(), EditRefusal> {
    // 复用编辑路径那一份实现，而不是再写一遍「基线 vs 候选」的比较逻辑：
    // 两套一旦分叉，「写入前双重校验」这句话就不再对三种格式同时成立。
    finalize_json_edit(baseline, candidate, guard).map(|_| ())
}

/// 写入前的双重校验。
///
/// 顺序与 `.vcf` 路径的 `finalize_edit` 一致：先确认结果仍然合法（含 schema），
/// 再只拒绝**本次改动新引入的**高危实例——文件本来就有的问题不该让这次编辑背。
///
/// 注意哪一部分属于「编辑机制」、哪一部分属于「校验层」：结果的合法性是
/// 编辑机制自己的保证（机制承诺「只替换目标字节、结果仍是合法 JSON」），
/// 所以消融对照臂也保留；schema 与安全审计才是校验层，`JsonGuard` 开关的是后者。
pub(crate) fn finalize_json_edit(
    source: &str,
    candidate: &str,
    guard: &JsonGuard<'_>,
) -> Result<String, EditRefusal> {
    // 基线必须在改动前取，所以先算。基线无法解析时基线为空集，候选里的任何
    // 高危实例都会被算成新引入——这是刻意的 fail-closed：算不出基线就不能
    // 声称「没有引入新风险」。
    let baseline = if guard.audit {
        Some(match audit_json(source, guard.flavor) {
            Ok(report) => high_risk_instances(&report),
            Err(_) => Default::default(),
        })
    } else {
        None
    };

    // 1) 编辑机制自身的保证：结果必须仍然能被解析，否则报成 validation_failed
    //    （与 `.vcf` 路径一样：候选无法解析属于「结果不合法」，不是「调用方给错参数」）
    if let Err(refusal) = json_to_ast(candidate, guard.flavor) {
        return Err(EditRefusal::ValidationFailed {
            path: "<result>".to_string(),
            message: refusal.to_string(),
        });
    }

    // 2) 校验层之一：schema。给了才校验，与 `.vcf`「文档声明了 schema 才校验」同一种口径。
    //    两种 schema 语言走同一段代码、同一套拒绝码，只是校验器不同。
    if let Some(schema_text) = guard.schema {
        match guard.schema_format {
            SchemaFormat::VcfDsl => {
                validate_json_against_schema(candidate, schema_text, guard.flavor)?;
            }
            SchemaFormat::JsonSchema => {
                validate_json_against_json_schema(
                    candidate,
                    schema_text,
                    guard.schema_draft,
                    guard.flavor,
                )?;
            }
        }
    }

    // 3) 校验层之二：安全审计，只拒绝新引入的高危实例
    if let Some(baseline) = baseline {
        let after = high_risk_instances(&audit_json(candidate, guard.flavor)?);
        let introduced = introduced_high_risk_instances(&baseline, &after);
        if !introduced.is_empty() {
            let mut findings: Vec<String> =
                introduced.iter().map(|(rule, _)| rule.clone()).collect();
            findings.sort();
            findings.dedup();
            return Err(EditRefusal::SecurityRejected {
                path: "<result>".to_string(),
                findings,
                instances: introduced.iter().map(render_risk_instance).collect(),
            });
        }
    }

    Ok(candidate.to_string())
}

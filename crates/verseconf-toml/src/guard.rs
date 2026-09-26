//! 写入前的双重校验：schema 与安全审计。
//!
//! `.vcf` 路径在写入前做两件事：改动后的文档必须仍然合法（含 schema），
//! 且不得引入新的高危安全实例。这一层把**同样的两件事**接到 TOML 上，
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
//! `.vcf` 把 schema 写在文档里，TOML 没有对应语法，所以 schema 由调用方
//! 作为**旁挂输入**交给这一层。这里不定义第二种 schema 语言：文本原样交给
//! `verseconf_core::parse`。一份 schema 只写一次，两种格式共用。

use verseconf_core::{
    high_risk_instances, introduced_high_risk_instances, render_risk_instance, Ast, AuditEngine,
    AuditReport, EditRefusal, SchemaValidator,
};

use crate::ast_bridge::toml_to_ast;

/// 写入前校验的开关。
///
/// `Default` 是「照 `.vcf` 路径的规矩来」：审计打开，schema 由调用方给。
/// 关掉审计只有一个正当用途——消融实验的对照臂（TF-0079）需要「只保留编辑机制」
/// 的那一组，而这一组必须是**显式**声明的，不能靠默认值悄悄溜进来。
#[derive(Debug, Clone, Copy)]
pub struct TomlGuard<'a> {
    /// 与 `.vcf` 同源的 schema 文本（`#@schema { ... }`）。`None` = 不做 schema 校验，
    /// 这与 `.vcf` 路径「文档没声明 schema 就不校验 schema」是同一种口径。
    pub schema: Option<&'a str>,
    /// 是否做写入前安全审计。默认 `true`。
    pub audit: bool,
}

impl Default for TomlGuard<'_> {
    fn default() -> Self {
        Self {
            schema: None,
            audit: true,
        }
    }
}

impl<'a> TomlGuard<'a> {
    /// 带 schema 的写入前校验
    pub fn with_schema(schema: &'a str) -> Self {
        Self {
            schema: Some(schema),
            audit: true,
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
        }
    }
}

/// 把一份 TOML 文本翻成 core AST（供审计与 schema 校验共用）
pub fn toml_ast(source: &str) -> Result<Ast, EditRefusal> {
    toml_to_ast(source)
}

/// 对一份 TOML 文本做安全审计。
///
/// 规则与分档与 `.vcf` 路径相同：写死在文件里的凭据是阻断级，
/// 数字/布尔这类「看着像敏感字段其实是数量或开关」只告警，整段 `${...}`
/// 引用按外部来源处理、同样只告警。
pub fn audit_toml(source: &str) -> Result<AuditReport, EditRefusal> {
    let ast = toml_to_ast(source)?;
    Ok(AuditEngine::new().audit_ast(&ast))
}

/// 按 `.vcf` 同源的 schema 校验一份 TOML 文本。
///
/// 失败返回 `EditRefusal::ValidationFailed`（拒绝码 `validation_failed`），
/// 与 `.vcf` 路径完全一致。
pub fn validate_toml_against_schema(source: &str, schema_text: &str) -> Result<(), EditRefusal> {
    let schema_ast = verseconf_core::parse(schema_text)
        .map_err(|error| EditRefusal::ParseFailed(format!("schema 无法解析：{error}")))?;
    let Some(schema) = schema_ast.schema else {
        return Err(EditRefusal::ValidationFailed {
            path: "<schema>".to_string(),
            message: "schema 文本里没有 `#@schema { ... }` 块".to_string(),
        });
    };

    let mut ast = toml_to_ast(source)?;
    ast.schema = Some(schema);

    let mut validator = SchemaValidator::new();
    validator
        .validate_with_schema(&ast)
        .map_err(|error| EditRefusal::ValidationFailed {
            path: "<result>".to_string(),
            message: error.to_string(),
        })
}

/// 写入前的双重校验。
///
/// 顺序与 `.vcf` 路径的 `finalize_edit` 一致：先确认结果仍然合法（含 schema），
/// 再只拒绝**本次改动新引入的**高危实例——文件本来就有的问题不该让这次编辑背。
///
/// 注意哪一部分属于「编辑机制」、哪一部分属于「校验层」：结果的合法性是
/// 编辑机制自己的保证（机制承诺「只替换目标字节、结果仍是合法 TOML」），
/// 所以消融对照臂也保留；schema 与安全审计才是校验层，`TomlGuard` 开关的是后者。
pub(crate) fn finalize_toml_edit(
    source: &str,
    candidate: String,
    guard: &TomlGuard<'_>,
) -> Result<String, EditRefusal> {
    // 基线必须在改动前取，所以先算
    let baseline = if guard.audit {
        Some(high_risk_instances(&audit_toml(source)?))
    } else {
        None
    };

    // 1) 编辑机制自身的保证：结果必须仍然能被解析
    //    （与 `.vcf` 路径一样报成 validation_failed）
    if let Err(refusal) = toml_to_ast(&candidate) {
        return Err(EditRefusal::ValidationFailed {
            path: "<result>".to_string(),
            message: refusal.to_string(),
        });
    }

    // 2) 校验层之一：schema。给了才校验，与 `.vcf`「文档声明了 schema 才校验」同一种口径
    if let Some(schema_text) = guard.schema {
        validate_toml_against_schema(&candidate, schema_text)?;
    }

    // 3) 校验层之二：安全审计，只拒绝新引入的高危实例
    if let Some(baseline) = baseline {
        let after = high_risk_instances(&audit_toml(&candidate)?);
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

    Ok(candidate)
}

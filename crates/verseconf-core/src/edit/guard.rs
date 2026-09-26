//! 写前检查：与编辑机制解耦的门禁入口。
//!
//! 编辑机制回答「**怎么改**」；这里只回答「**这次改动允许落盘吗**」。
//!
//! 分开的理由是可测的：确认性复验里，「意图协议 + 门禁」比「成熟库薄封装」
//! 贵 8.9%（输出 token +64.7%），而贵出来的部分全部来自模型要输出编辑计划
//! 信封，与门禁无关。门禁是确定性代码，成本是毫秒，不是 token。
//!
//! 所以调用方可以保留自己的编辑方式——字符串替换、字符区间替换、甚至整文件
//! 重写——只在写盘前把「原文」与「候选文本」交给 [`check_write`]：
//!
//! ```text
//! let candidate = my_own_editor(source, ...);        // 用什么方式改都行
//! check_write(source, &candidate)?;                  // 允许吗？
//! std::fs::write(path, candidate)?;                  // 通过才落盘
//! ```
//!
//! ## 语义边界（这几条是它与普通 linter 的区别）
//!
//! - **只拒绝本次新引入的高危实例**（规则 + 位置比较），文件本来就有的问题
//!   不该让这次改动背；
//! - **拒绝即不返回候选文本**：调用方拿不到半成品，写不出坏文件；
//! - **拒绝码与编辑路径同源**，宿主可以按码分支处理；
//! - schema 既可来自候选文本自身声明（`.vcf` 的 `#@schema`），也可由调用方
//!   旁挂传入——后者是让门禁能服务「本来没有 schema 的格式」的关键。

use crate::edit::apply::EditRefusal;
use crate::engine::audit::{
    high_risk_instances, introduced_high_risk_instances, render_risk_instance, AuditEngine,
};
use crate::semantic::SchemaValidator;

/// 写前检查的开关。
///
/// `Default` 是「照编辑路径的规矩来」：审计打开，schema 用候选文本自带的。
/// 关掉审计只有一个正当用途——对照实验需要「只保留结构校验」的那一组，
/// 而这一组必须是**显式**声明的，不能靠默认值悄悄溜进来。
#[derive(Debug, Clone, Copy)]
pub struct WriteGuard<'a> {
    /// 旁挂 schema（`#@schema { ... }` 文本）。
    ///
    /// `None` = 只用候选文本自身声明的 schema，与编辑路径
    /// 「文档没声明 schema 就不校验 schema」是同一种口径。
    pub schema: Option<&'a str>,
    /// 是否做写入前安全审计。默认 `true`。
    pub audit: bool,
}

impl Default for WriteGuard<'_> {
    fn default() -> Self {
        Self {
            schema: None,
            audit: true,
        }
    }
}

impl<'a> WriteGuard<'a> {
    /// 旁挂 schema + 安全审计（真实写入的推荐形态）
    pub fn with_schema(schema: &'a str) -> Self {
        Self {
            schema: Some(schema),
            audit: true,
        }
    }

    /// 只做结构 / schema 校验，不做安全审计。
    ///
    /// 这不是给真实写入用的入口：它的存在是为了让「安全审计到底贡献了什么」
    /// 可以被测出来，而不是只能被声称。
    pub fn schema_only(schema: &'a str) -> Self {
        Self {
            schema: Some(schema),
            audit: false,
        }
    }
}

/// 写前检查：`baseline` 是改动前的文本，`candidate` 是准备落盘的文本。
///
/// 通过返回 `Ok(())`；拒绝返回与编辑路径同一套 [`EditRefusal`]
/// （同一组稳定错误码：`validation_failed` / `security_rejected` / `parse_failed`）。
///
/// **不关心 `candidate` 是怎么产生的**——这正是它独立存在的意义。
pub fn check_write(baseline: &str, candidate: &str) -> Result<(), EditRefusal> {
    check_write_with(baseline, candidate, &WriteGuard::default())
}

/// [`check_write`] 的完整形态：可以旁挂 schema，也可以显式关掉审计。
pub fn check_write_with(
    baseline: &str,
    candidate: &str,
    guard: &WriteGuard<'_>,
) -> Result<(), EditRefusal> {
    // 基线必须在改动前取，否则「新引入」无从判断。
    // 基线无法解析时基线为空集，候选里的任何高危实例都会被算成新引入——
    // 这是刻意的 fail-closed：算不出基线就不能声称「没有引入新风险」。
    let risk_baseline = if guard.audit {
        Some(high_risk_instances(
            &AuditEngine::new().audit_source(baseline),
        ))
    } else {
        None
    };

    // 1) 候选必须仍然能被解析，且结构合法（含候选自带的 schema）
    let mut ast = crate::parse(candidate).map_err(|error| EditRefusal::ValidationFailed {
        path: "<result>".to_string(),
        message: error.to_string(),
    })?;
    crate::validate_ast(&ast).map_err(|error| EditRefusal::ValidationFailed {
        path: "<result>".to_string(),
        message: error.to_string(),
    })?;

    // 2) 旁挂 schema：候选文本自己没有 schema 时由调用方给
    if let Some(schema_text) = guard.schema {
        let schema_ast =
            crate::parse(schema_text).map_err(|error| EditRefusal::ValidationFailed {
                path: "<schema>".to_string(),
                message: format!("schema 无法解析：{error}"),
            })?;
        let Some(schema) = schema_ast.schema else {
            return Err(EditRefusal::ValidationFailed {
                path: "<schema>".to_string(),
                message: "schema 文本里没有 `#@schema { ... }` 块".to_string(),
            });
        };
        ast.schema = Some(schema);
        let mut validator = SchemaValidator::new();
        validator
            .validate_with_schema(&ast)
            .map_err(|error| EditRefusal::ValidationFailed {
                path: "<result>".to_string(),
                message: error.to_string(),
            })?;
    }

    // 3) 安全审计：只拒绝本次改动**新引入**的高危实例
    if let Some(baseline) = risk_baseline {
        let after = high_risk_instances(&AuditEngine::new().audit_source(candidate));
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

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个真实的「候选文本不是编辑计划产生的」场景：
    /// 调用方自己用字符串替换改的文件，门禁照样能用。
    fn string_replace(source: &str, from: &str, to: &str) -> String {
        source.replacen(from, to, 1)
    }

    #[test]
    fn a_newly_introduced_high_risk_instance_is_refused() {
        let baseline = "tls_verify = true\n";
        let candidate = string_replace(baseline, "true", "false");

        let refusal = check_write(baseline, &candidate).expect_err("关掉证书校验必须被拒绝");
        assert_eq!(refusal.code(), "security_rejected");
        assert!(refusal.details()["instances"][0]
            .as_str()
            .expect("instances 必须是字符串数组")
            .contains("tls_verify"));
    }

    #[test]
    fn a_preexisting_risk_is_not_blamed_on_this_edit() {
        // 文件本来就把证书校验关了；这次只改端口，不该被拒绝
        let baseline = "tls_verify = false\nport = 8080\n";
        let candidate = string_replace(baseline, "8080", "9090");

        check_write(baseline, &candidate).expect("不因文件本来就有的问题拒绝这次改动");
    }

    #[test]
    fn a_second_instance_of_an_existing_rule_is_refused() {
        // F1 反例：同一条规则已经出现过，新增**另一个位置**的实例仍必须被拦住
        let baseline = "primary_ssl_verify = false\nsecondary_ssl_verify = true\n";
        let candidate = string_replace(
            baseline,
            "secondary_ssl_verify = true",
            "secondary_ssl_verify = false",
        );

        let refusal = check_write(baseline, &candidate).expect_err("同规则的新实例必须被拒绝");
        assert_eq!(refusal.code(), "security_rejected");
        let instances = refusal.details()["instances"].to_string();
        assert!(
            instances.contains("secondary_ssl_verify"),
            "拒绝信息必须指出是哪个实例，实际 {instances}"
        );
    }

    #[test]
    fn a_plain_string_replace_that_drifts_a_type_is_caught_by_a_sidecar_schema() {
        // 这是「解绑」的核心用例：候选由朴素字符串替换产生，不含任何编辑协议；
        // 旁挂 schema 照样能挡住类型漂移（整数被写成字符串）。
        let schema = "#@schema {\n  tab_spaces {\n    type = \"integer\"\n  }\n}\n";
        let baseline = "edition = \"2018\"\ntab_spaces = 4242\n";
        let candidate = string_replace(baseline, "tab_spaces = 4242", "tab_spaces = \"4242\"");

        let refusal = check_write_with(baseline, &candidate, &WriteGuard::with_schema(schema))
            .expect_err("类型漂移必须被 schema 拒绝");
        assert_eq!(refusal.code(), "validation_failed");
    }

    #[test]
    fn a_safe_equivalent_writing_is_accepted() {
        // 消融实验里的「等价安全写法」：同样是新增一个凭据字段，但值用环境变量
        // 引用而不是写死的明文。门禁必须放行，否则它就成了「一律拒绝」。
        let baseline = "host = \"db.internal\"\n";
        let candidate = "host = \"db.internal\"\ndb_password = \"${DB_PASSWORD}\"\n";

        check_write(baseline, candidate).expect("等价安全写法必须被接受");
    }

    #[test]
    fn a_benign_edit_beside_a_secret_reference_is_accepted() {
        // 文件里本来就有环境变量引用；这次只改端口，与安全无关，必须放行
        let baseline = "api_key = \"${API_KEY}\"\nport = 8080\n";
        let candidate = string_replace(baseline, "8080", "9090");

        check_write(baseline, &candidate).expect("与安全无关的改动必须放行");
    }

    #[test]
    fn an_unparseable_candidate_is_refused() {
        let refusal = check_write("port = 8080\n", "port = \n").expect_err("候选无法解析必须拒绝");
        assert_eq!(refusal.code(), "validation_failed");
    }

    #[test]
    fn the_schema_block_itself_is_required_when_a_sidecar_is_given() {
        let refusal = check_write_with(
            "port = 8080\n",
            "port = 9090\n",
            &WriteGuard::with_schema("port = 1\n"),
        )
        .expect_err("旁挂文本里没有 #@schema 块时必须拒绝");
        assert_eq!(refusal.code(), "validation_failed");
        assert!(refusal.to_string().contains("#@schema"));
    }

    #[test]
    fn audit_can_be_turned_off_explicitly() {
        // 对照实验用的那一组：显式关掉审计后，安全改动不再被拦
        let schema = "#@schema {\n  tls_verify {\n    type = \"boolean\"\n  }\n}\n";
        let baseline = "tls_verify = true\n";
        let candidate = "tls_verify = false\n";

        check_write_with(baseline, candidate, &WriteGuard::schema_only(schema))
            .expect("显式关掉审计时不应因安全规则拒绝");
    }

    #[test]
    fn the_candidate_is_never_returned_on_refusal() {
        // 「拒绝即不返回候选文本」：这里用错误类型本身证明——拒绝是 Err，
        // 调用方在类型上就拿不到候选，写不出半成品。
        let result = check_write("tls_verify = true\n", "tls_verify = false\n");
        assert!(result.is_err());
    }
}

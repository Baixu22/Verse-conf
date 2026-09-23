//! 被测策略。
//!
//! 三个策略共用同一份语料与判定，差别只在"怎么把值写回去"：
//!
//! | 策略 | 定位 | 写回方式 | 写入前校验 |
//! | --- | --- | --- | --- |
//! | `verseconf-intent` | 意图契约的路径解析 | 只替换目标值的字节区间 | schema + 安全双重校验 |
//! | `whole-file-rewrite` | 同上 | 换掉值之后把**整个文件**按 AI 友好规范形式重新序列化 | 无 |
//! | `line-diff` | 按字段名找**第一处**匹配行 | 整行替换 | 无 |
//!
//! `whole-file-rewrite` 与 `verseconf-intent` 的定位方式相同，是为了把变量隔离出来：
//! 两者数字上的差异只可能来自"重写整个文件"这一步，而不是定位能力。

use verseconf_core::{
    apply_edit_plan, parse, value_span_for_path, EditOp, EditPlan, EditValue, PathSegment,
    PrettyPrintConfig, PrettyPrinter,
};

/// 一个策略对一次编辑的结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StrategyOutcome {
    Applied(String),
    Refused { code: String, message: String },
}

impl StrategyOutcome {
    pub fn refused(code: &str, message: impl Into<String>) -> Self {
        StrategyOutcome::Refused {
            code: code.to_string(),
            message: message.into(),
        }
    }

    pub fn is_applied(&self) -> bool {
        matches!(self, StrategyOutcome::Applied(_))
    }

    pub fn source(&self) -> Option<&str> {
        match self {
            StrategyOutcome::Applied(source) => Some(source),
            StrategyOutcome::Refused { .. } => None,
        }
    }

    pub fn code(&self) -> Option<&str> {
        match self {
            StrategyOutcome::Applied(_) => None,
            StrategyOutcome::Refused { code, .. } => Some(code),
        }
    }
}

/// 一个可被基准评测的编辑策略。
///
/// 实现者只需要把「源码 + 编辑计划」映射成「新源码」或「拒绝」，不需要知道判定逻辑。
pub trait EditStrategy {
    fn name(&self) -> &'static str;
    fn summary(&self) -> &'static str;
    fn apply(&self, source: &str, plan_json: &str) -> StrategyOutcome;
}

/// 基准内置的三个策略
pub fn all() -> Vec<Box<dyn EditStrategy>> {
    vec![
        Box::new(IntentStrategy),
        Box::new(WholeFileRewriteStrategy),
        Box::new(LineDiffStrategy),
    ]
}

// ---------------------------------------------------------------- 策略一：意图执行

/// 本仓库的实现：只替换目标值的字节区间，写入前做 schema 与安全双重校验。
pub struct IntentStrategy;

impl EditStrategy for IntentStrategy {
    fn name(&self) -> &'static str {
        "verseconf-intent"
    }

    fn summary(&self) -> &'static str {
        "意图契约 + 字符区间最小改动 + 写入前双重校验"
    }

    fn apply(&self, source: &str, plan_json: &str) -> StrategyOutcome {
        let plan = match parse_plan(plan_json) {
            Ok(plan) => plan,
            Err(refusal) => return refusal,
        };

        match apply_edit_plan(source, &plan) {
            Ok(outcome) => StrategyOutcome::Applied(outcome.source),
            Err(refusal) => StrategyOutcome::Refused {
                code: refusal.code().to_string(),
                message: refusal.to_string(),
            },
        }
    }
}

// ---------------------------------------------------------------- 策略二：全文件重写

/// 全文件重写基线：把值改掉之后，整个文件按 AI 友好规范形式重新序列化。
///
/// 这是"模型重写整个文件"的确定性替身：格式、键序、空白都变成写回方的选择，
/// 而不是用户原来的写法。它不做写入前校验，因此会照单执行不该落地的改动。
///
/// 真实的模型重写通常还会丢注释，所以这里测到的附带损伤是**下界**。
pub struct WholeFileRewriteStrategy;

impl EditStrategy for WholeFileRewriteStrategy {
    fn name(&self) -> &'static str {
        "whole-file-rewrite"
    }

    fn summary(&self) -> &'static str {
        "改值后整个文件按 AI 友好规范形式重新序列化，无写入前校验"
    }

    fn apply(&self, source: &str, plan_json: &str) -> StrategyOutcome {
        let plan = match parse_plan(plan_json) {
            Ok(plan) => plan,
            Err(refusal) => return refusal,
        };
        let (path, value) = match single_set(&plan) {
            Ok(pair) => pair,
            Err(refusal) => return refusal,
        };

        let replaced = match replace_value_span(source, path, value) {
            Ok(replaced) => replaced,
            Err(refusal) => return refusal,
        };

        match parse(&replaced) {
            Ok(ast) => StrategyOutcome::Applied(PrettyPrinter::print_with_config(
                &ast,
                PrettyPrintConfig::ai_canonical(),
            )),
            Err(error) => {
                StrategyOutcome::refused("parse_failed", format!("重写后的文件无法解析：{}", error))
            }
        }
    }
}

// ---------------------------------------------------------------- 策略三：直接生成差异

/// 直接生成差异基线：按字段名找第一处匹配行，整行替换。
///
/// 这是"模型直接给出一段 diff"的替身：它不做路径解析、不区分同名键所在的表、
/// 也不保留目标行的行尾注释与 `#@` 元数据，更没有写入前校验。
pub struct LineDiffStrategy;

impl EditStrategy for LineDiffStrategy {
    fn name(&self) -> &'static str {
        "line-diff"
    }

    fn summary(&self) -> &'static str {
        "按字段名找第一处匹配行并整行替换，无路径解析、无写入前校验"
    }

    fn apply(&self, source: &str, plan_json: &str) -> StrategyOutcome {
        let plan = match parse_plan(plan_json) {
            Ok(plan) => plan,
            Err(refusal) => return refusal,
        };
        let (path, value) = match single_set(&plan) {
            Ok(pair) => pair,
            Err(refusal) => return refusal,
        };

        let key = match path.last() {
            Some(PathSegment::Key(name)) => name.clone(),
            _ => {
                return StrategyOutcome::refused(
                    "unsupported_target",
                    "行级补丁只能处理以字段名结尾的路径",
                )
            }
        };

        match replace_first_matching_line(source, &key, &value.to_text()) {
            Some(next) => StrategyOutcome::Applied(next),
            None => StrategyOutcome::refused(
                "target_not_found",
                format!("找不到字段 '{}' 所在的行", key),
            ),
        }
    }
}

// ---------------------------------------------------------------- 公共辅助

fn parse_plan(plan_json: &str) -> Result<EditPlan, StrategyOutcome> {
    EditPlan::from_json(plan_json).map_err(|violations| StrategyOutcome::Refused {
        code: "invalid_plan".to_string(),
        message: violations
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; "),
    })
}

fn single_set(plan: &EditPlan) -> Result<(&[PathSegment], &EditValue), StrategyOutcome> {
    if plan.edits.len() != 1 {
        return Err(StrategyOutcome::refused(
            "unsupported_target",
            "基线只实现单条 set 的编辑计划",
        ));
    }
    let edit = &plan.edits[0];
    if edit.op != EditOp::Set {
        return Err(StrategyOutcome::refused(
            "unsupported_target",
            format!("基线只实现 set，收到 {}", edit.op.as_str()),
        ));
    }
    let value = edit
        .value
        .as_ref()
        .ok_or_else(|| StrategyOutcome::refused("invalid_plan", "set 缺少 value"))?;
    Ok((&edit.path, value))
}

/// 只替换目标值的字节区间，不做任何写入前校验。
fn replace_value_span(
    source: &str,
    path: &[PathSegment],
    value: &EditValue,
) -> Result<String, StrategyOutcome> {
    let (start, end) =
        value_span_for_path(source, path).map_err(|refusal| StrategyOutcome::Refused {
            code: refusal.code().to_string(),
            message: refusal.to_string(),
        })?;

    let mut next = String::with_capacity(source.len() + 16);
    next.push_str(&source[..start]);
    next.push_str(&value.to_text());
    next.push_str(&source[end..]);
    Ok(next)
}

/// 按字段名找第一处 `key = ...` 的行，整行替换成 `indent + key = value`。
fn replace_first_matching_line(source: &str, key: &str, rendered: &str) -> Option<String> {
    let mut out = String::with_capacity(source.len() + rendered.len());
    let mut replaced = false;

    for line in source.split_inclusive('\n') {
        if !replaced {
            let body = line.trim_end_matches(['\r', '\n']);
            let trimmed = body.trim_start();
            let indent = &body[..body.len() - trimmed.len()];
            let matches = trimmed
                .split_once('=')
                .map(|(name, _)| name.trim() == key)
                .unwrap_or(false);

            if matches {
                let newline = if line.ends_with("\r\n") {
                    "\r\n"
                } else if line.ends_with('\n') {
                    "\n"
                } else {
                    ""
                };
                out.push_str(indent);
                out.push_str(key);
                out.push_str(" = ");
                out.push_str(rendered);
                out.push_str(newline);
                replaced = true;
                continue;
            }
        }
        out.push_str(line);
    }

    replaced.then_some(out)
}

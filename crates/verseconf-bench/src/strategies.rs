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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use verseconf_core::{
    apply_edit_plan, apply_edit_plan_in_files_with, parse, value_span_for_path, EditIntent, EditOp,
    EditPlan, EditValue, FileLoader, PathSegment, PrettyPrintConfig, PrettyPrinter,
};

/// 跨 @include 评测里的文件树：相对路径 → 内容。
///
/// 语料提供文件内容、而不是让策略读磁盘，是为了让策略保持纯函数：
/// 同一输入必得同一输出，可复现性不依赖磁盘状态。
pub type FileTree = BTreeMap<PathBuf, String>;

/// 跨文件评测里一次编辑的结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOutcome {
    /// 改动了 path 指向的文件，新内容是 source
    Applied { path: PathBuf, source: String },
    /// 拒绝（与单文件路径共用同一套稳定错误码）
    Refused { code: String, message: String },
}

/// 把内存文件树包装成核心的加载器
struct TreeLoader<'a>(&'a FileTree);

impl FileLoader for TreeLoader<'_> {
    fn load_file(&self, path: &Path) -> Result<String, String> {
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| format!("语料里没有 {}", path.display()))
    }
}

/// 解析计划；失败时给出跨文件路径用的拒绝
fn plan_or_file_refusal(plan_json: &str) -> Result<EditPlan, FileOutcome> {
    parse_plan(plan_json).map_err(|outcome| match outcome {
        StrategyOutcome::Refused { code, message } => FileOutcome::Refused { code, message },
        // parse_plan 只会返回 Refused；这里只是把不可达分支写清楚
        StrategyOutcome::Applied(_) => FileOutcome::Refused {
            code: "invalid_plan".to_string(),
            message: "编辑计划无法解析".to_string(),
        },
    })
}

/// 把核心的拒绝映射成跨文件路径的拒绝
fn file_refusal(refusal: verseconf_core::EditRefusal) -> FileOutcome {
    FileOutcome::Refused {
        code: refusal.code().to_string(),
        message: refusal.to_string(),
    }
}

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

    /// 跨 @include 的评测入口：从 entry 出发，允许顺着 include 走到别的文件。
    ///
    /// 默认实现**不跟随 include**——只把入口文件当单文件处理。这正是朴素实现的行为，
    /// 也正是这些任务要暴露的东西：目标在被包含文件里时它找不到。
    fn apply_across_files(&self, files: &FileTree, entry: &Path, plan_json: &str) -> FileOutcome {
        let source = files.get(entry).cloned().unwrap_or_default();
        match self.apply(&source, plan_json) {
            StrategyOutcome::Applied(next) => FileOutcome::Applied {
                path: entry.to_path_buf(),
                source: next,
            },
            StrategyOutcome::Refused { code, message } => FileOutcome::Refused { code, message },
        }
    }
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

    fn apply_across_files(&self, files: &FileTree, entry: &Path, plan_json: &str) -> FileOutcome {
        let plan = match plan_or_file_refusal(plan_json) {
            Ok(plan) => plan,
            Err(refusal) => return refusal,
        };

        match apply_edit_plan_in_files_with(entry, &plan, &TreeLoader(files)) {
            Ok(outcome) => FileOutcome::Applied {
                path: outcome.file.path,
                source: outcome.file.after,
            },
            Err(refusal) => file_refusal(refusal),
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
        // 多条编辑：文本层面逐条朴素处理不再等价，统一交给核心定位后再整份重写。
        if plan.edits.len() > 1 {
            return match apply_edit_plan(source, &plan) {
                Ok(outcome) => match parse(&outcome.source) {
                    Ok(ast) => StrategyOutcome::Applied(PrettyPrinter::print_with_config(
                        &ast,
                        PrettyPrintConfig::ai_canonical(),
                    )),
                    Err(error) => StrategyOutcome::refused(
                        "parse_failed",
                        format!("重写后的文件无法解析：{}", error),
                    ),
                },
                Err(refusal) => StrategyOutcome::Refused {
                    code: refusal.code().to_string(),
                    message: refusal.to_string(),
                },
            };
        }

        let edit = match single_edit(&plan) {
            Ok(edit) => edit,
            Err(refusal) => return refusal,
        };

        // 先把这次编辑落到文本上，再整份重新序列化。
        //
        // set 用的是"只替换值区间"的朴素做法（不校验）；insert / delete 在文本
        // 层面没法朴素表达，所以复用核心的定位——**定位方式与 intent 策略相同**，
        // 这样两者数字上的差异只可能来自"重写整个文件"这一步。
        let replaced = match edit.op {
            EditOp::Set => {
                let (path, value) = match set_target(edit) {
                    Ok(pair) => pair,
                    Err(refusal) => return refusal,
                };
                match replace_value_span(source, path, value) {
                    Ok(replaced) => replaced,
                    Err(refusal) => return refusal,
                }
            }
            EditOp::Insert | EditOp::Delete => match apply_edit_plan(source, &plan) {
                Ok(outcome) => outcome.source,
                Err(refusal) => {
                    return StrategyOutcome::Refused {
                        code: refusal.code().to_string(),
                        message: refusal.to_string(),
                    }
                }
            },
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

    fn apply_across_files(&self, files: &FileTree, entry: &Path, plan_json: &str) -> FileOutcome {
        let plan = match plan_or_file_refusal(plan_json) {
            Ok(plan) => plan,
            Err(refusal) => return refusal,
        };

        // 定位方式与 intent 相同；差别只在写回：把**被改动的那个文件**整份重新序列化。
        match apply_edit_plan_in_files_with(entry, &plan, &TreeLoader(files)) {
            Ok(outcome) => match parse(&outcome.file.after) {
                Ok(ast) => FileOutcome::Applied {
                    path: outcome.file.path,
                    source: PrettyPrinter::print_with_config(
                        &ast,
                        PrettyPrintConfig::ai_canonical(),
                    ),
                },
                Err(error) => FileOutcome::Refused {
                    code: "parse_failed".to_string(),
                    message: format!("重写后的文件无法解析：{}", error),
                },
            },
            Err(refusal) => file_refusal(refusal),
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
        // 单条与多条走同一条路径：逐条按同一套朴素规则顺序作用。
        // 多条编辑在这里最容易露馅——每条都"按字段名找第一处"，
        // 作用域信息完全缺失，错位会叠加。
        let mut current = source.to_string();
        for edit in &plan.edits {
            match apply_line_patch(&current, edit) {
                Ok(next) => current = next,
                Err(refusal) => return refusal,
            }
        }
        StrategyOutcome::Applied(current)
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

/// 取出计划里唯一的一条编辑意图。
///
/// 三种操作都要接受：只实现 set 会让基线在新任务上直接误拒，
/// 那样比出来的就不是"写回方式的代价"，而是"谁实现了这个功能"。
fn single_edit(plan: &EditPlan) -> Result<&EditIntent, StrategyOutcome> {
    if plan.edits.len() != 1 {
        return Err(StrategyOutcome::refused(
            "unsupported_target",
            "基线只实现单条编辑的计划",
        ));
    }
    Ok(&plan.edits[0])
}

/// set 操作的目标（路径 + 值）
fn set_target(edit: &EditIntent) -> Result<(&[PathSegment], &EditValue), StrategyOutcome> {
    let value = edit
        .value
        .as_ref()
        .ok_or_else(|| StrategyOutcome::refused("invalid_plan", "set 缺少 value"))?;
    Ok((&edit.path, value))
}

/// 按朴素规则把一条编辑作用到文本上（LineDiff 的单条与多条路径共用）
fn apply_line_patch(source: &str, edit: &EditIntent) -> Result<String, StrategyOutcome> {
    let key = match edit.path.last() {
        Some(PathSegment::Key(name)) => name.clone(),
        _ => {
            return Err(StrategyOutcome::refused(
                "unsupported_target",
                "行级补丁只能处理以字段名结尾的路径",
            ))
        }
    };

    match edit.op {
        EditOp::Set => {
            let (_, value) = set_target(edit)?;
            replace_first_matching_line(source, &key, &value.to_text()).ok_or_else(|| {
                StrategyOutcome::refused(
                    "target_not_found",
                    format!("找不到字段 '{}' 所在的行", key),
                )
            })
        }
        EditOp::Insert => {
            let value = edit
                .value
                .as_ref()
                .ok_or_else(|| StrategyOutcome::refused("invalid_plan", "insert 缺少 value"))?;
            let mut next = source.to_string();
            if !next.is_empty() && !next.ends_with('\n') {
                next.push('\n');
            }
            next.push_str(&format!("{} = {}\n", key, value.to_text()));
            Ok(next)
        }
        EditOp::Delete => remove_first_matching_line(source, &key).ok_or_else(|| {
            StrategyOutcome::refused("target_not_found", format!("找不到字段 '{}' 所在的行", key))
        }),
    }
}

/// 删掉第一处 `key = ...` 所在的整行
fn remove_first_matching_line(source: &str, key: &str) -> Option<String> {
    let mut out = String::with_capacity(source.len());
    let mut removed = false;

    for line in source.split_inclusive('\n') {
        if !removed {
            let body = line.trim_end_matches(['\r', '\n']);
            let trimmed = body.trim_start();
            let matches = trimmed
                .split_once('=')
                .map(|(name, _)| name.trim() == key)
                .unwrap_or(false);
            if matches {
                removed = true;
                continue;
            }
        }
        out.push_str(line);
    }

    removed.then_some(out)
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

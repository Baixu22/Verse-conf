//! 跨 `@include` 应用编辑计划。
//!
//! ## 定位规则（fail-closed）
//!
//! 目标必须**恰好在一个文件里**能定位到：
//!
//! | 情形 | 结果 |
//! | --- | --- |
//! | 恰好一个文件命中 | 编辑该文件，返回它的新内容 |
//! | 多个文件都能定位到 | 拒绝 `target_ambiguous`，并列出这些文件 |
//! | 一个都定位不到 | 拒绝 `target_not_found`，并列出搜索过的文件 |
//!
//! 为什么不"先把 include 合并成一份、再在合并结果里定位"：合并会丢掉
//! "这个值来自哪个文件"的信息，而写回必须落到**具体某个文件**的字节上。
//! 与其猜一个，不如在不确定时拒绝——这与命名列表命中多个元素时拒绝歧义
//! 是同一条原则，也正是本项目"无法确定时拒绝，而不是猜测"的体现。
//!
//! ## 校验范围
//!
//! 被改动的那个文件会走完整的单文件写入前双重校验（schema + 安全审计），
//! 因为它复用了 `apply_edit_plan`。
//!
//! **合并后的「生效配置」会用候选内容重建后校验**：include 图里读到的文件内容
//! 被当成一份内存文件系统交给合并器，被改动的那个文件用候选内容覆盖，然后对
//! 合并结果跑 `validate_ast`。只有当合并器实际读到的文件集合与 include 图完全
//! 一致时重建结果才被采信；不一致（例如嵌套 include 的基准目录与合并器的解析
//! 方式不同）会返回 `EffectiveView::NotValidated`，宁可把「没校验」写在明处，
//! 也不做一个会悄悄校验到别的内容的假校验。

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::ast::{TableBlock, TableEntry, Value};
use crate::engine::{AstMerger, DefaultFileLoader, FileLoader};
use crate::parse;

use super::apply::{apply_edit_plan, EditOutcome, EditRefusal};
use super::{AppliedEdit, EditPlan, PathSegment};

/// 一次跨文件编辑里被改动的那个文件
#[derive(Debug, Clone, PartialEq)]
pub struct FileEdit {
    /// 被改动的文件路径
    pub path: PathBuf,
    /// 改动前的完整内容
    pub before: String,
    /// 改动后的完整内容
    pub after: String,
}

/// 合并后的「生效配置」是否被校验过
///
/// 只校验被改动的那个文件是不够的：入口的 schema 可能由另一个文件里的值来满足，
/// 改完那个值以后入口才非法。这里记录生效视图的真实状态，调用方不得把
/// `NotValidated` 当成「安全通过」。
#[derive(Debug, Clone, PartialEq)]
pub enum EffectiveView {
    /// 用候选内容重建了合并视图，并通过结构/schema 校验
    Validated,
    /// 无法可靠重建合并视图（合并器读到的文件集合与 include 图不一致）
    NotValidated { reason: String },
}

/// 跨 `@include` 的编辑结果
#[derive(Debug, Clone, PartialEq)]
pub struct MultiFileOutcome {
    /// 被改动的文件（定位规则保证只有一个）
    pub file: FileEdit,
    /// 已应用的编辑记录
    pub applied: Vec<AppliedEdit>,
    /// 合并后生效配置的校验状态
    pub effective_view: EffectiveView,
}

/// 搜索文件数的上限，避免异常深的 include 链把内存吃满
const MAX_FILES: usize = 64;

/// 跨 `@include` 应用编辑计划。
///
/// `root` 是入口文件；它的 `@include` 会被递归展开成待搜索的文件集合。
/// 返回的是**被改动文件**的新内容，调用方据此决定写盘策略。
pub fn apply_edit_plan_in_files(
    root: &Path,
    plan: &EditPlan,
) -> Result<MultiFileOutcome, EditRefusal> {
    // 先把契约自身校验一遍，避免把"计划不合法"误报成"文件里没有"
    if let Err(violations) = plan.validate() {
        return Err(EditRefusal::InvalidPlan(violations));
    }

    // 空 base_path：路径按调用方给的原样使用（相对路径相对当前目录）。
    let loader = DefaultFileLoader {
        base_path: PathBuf::new(),
    };
    apply_edit_plan_in_files_with(root, plan, &loader)
}

/// 与 apply_edit_plan_in_files 相同，但文件内容由调用方提供的加载器给出。
///
/// 这一层的存在是为了让**评测**能在不碰文件系统的情况下跑跨文件编辑：
/// 基准把语料当成一份内存文件树交给加载器，策略因此仍然是纯函数
/// （同一输入必得同一输出），可复现性不依赖磁盘状态。
pub fn apply_edit_plan_in_files_with(
    root: &Path,
    plan: &EditPlan,
    loader: &dyn FileLoader,
) -> Result<MultiFileOutcome, EditRefusal> {
    // 先把契约自身校验一遍，避免把「计划不合法」误报成「文件里没有」
    if let Err(violations) = plan.validate() {
        return Err(EditRefusal::InvalidPlan(violations));
    }

    let graph = load_include_graph(root, loader)?;
    let files = graph.files;
    let searched: Vec<String> = files
        .iter()
        .map(|(path, _)| path.display().to_string())
        .collect();

    // fail-closed：搜索被截断时，后面的文件里可能还有同名目标，
    // 因此不能对已扫描的子集判定唯一性，直接拒绝。
    if graph.truncated {
        return Err(EditRefusal::SearchIncomplete {
            path: plan_path_label(plan),
            files: searched,
            limit: MAX_FILES,
        });
    }

    enum Probe {
        Applied(EditOutcome),
        Refused(EditRefusal),
    }

    let mut present: Vec<(PathBuf, String, Probe)> = Vec::new();

    for (path, source) in &files {
        match apply_edit_plan(source, plan) {
            Ok(outcome) => present.push((path.clone(), source.clone(), Probe::Applied(outcome))),
            // 这个文件里没有目标：继续找下一个
            Err(EditRefusal::TargetNotFound { .. }) => {}
            // 其他拒绝都说明"目标在这里"，只是这次改动不能落地
            // （歧义、前置条件不符、改完不合法……）
            Err(refusal) => present.push((path.clone(), source.clone(), Probe::Refused(refusal))),
        }
    }

    let label = plan_path_label(plan);

    match present.len() {
        0 => Err(EditRefusal::TargetNotFoundAcrossFiles {
            path: label,
            files: searched,
        }),
        1 => {
            let (path, before, probe) = present.pop().expect("长度已判定为 1");
            match probe {
                Probe::Applied(outcome) => {
                    match check_effective_view(root, &files, &path, &outcome.source) {
                        // 合并后的生效配置非法：这正是旧实现漏掉的拒绝
                        EffectiveViewCheck::Invalid { message } => {
                            Err(EditRefusal::ValidationFailed {
                                path: label,
                                message,
                            })
                        }
                        EffectiveViewCheck::Unreliable { reason } => Ok(MultiFileOutcome {
                            file: FileEdit {
                                path,
                                before,
                                after: outcome.source,
                            },
                            applied: outcome.applied,
                            effective_view: EffectiveView::NotValidated { reason },
                        }),
                        EffectiveViewCheck::Valid => Ok(MultiFileOutcome {
                            file: FileEdit {
                                path,
                                before,
                                after: outcome.source,
                            },
                            applied: outcome.applied,
                            effective_view: EffectiveView::Validated,
                        }),
                    }
                }
                Probe::Refused(refusal) => Err(refusal),
            }
        }
        _ => Err(EditRefusal::TargetAmbiguousAcrossFiles {
            path: label,
            files: present
                .iter()
                .map(|(path, _, _)| path.display().to_string())
                .collect(),
        }),
    }
}

/// 生效视图校验的结果
enum EffectiveViewCheck {
    /// 重建成功且通过校验
    Valid,
    /// 重建成功但校验失败：这种改动必须拒绝
    Invalid { message: String },
    /// 无法可信地重建：不能据此判定，但也不该假装通过
    Unreliable { reason: String },
}

/// 用 include 图里的内容当文件系统：合并器请求什么就给什么，
/// 并记录它实际请求了哪些路径。
struct OverlayLoader {
    base: PathBuf,
    files: BTreeMap<PathBuf, String>,
    requested: Rc<RefCell<BTreeSet<PathBuf>>>,
}

impl FileLoader for OverlayLoader {
    fn load_file(&self, path: &Path) -> Result<String, String> {
        let full = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base.join(path)
        };
        let key = normalize(&full);
        self.requested.borrow_mut().insert(key.clone());
        self.files
            .get(&key)
            .cloned()
            .ok_or_else(|| format!("include 图里没有 {}", full.display()))
    }
}

/// 用候选内容重建合并后的生效配置并校验。
///
/// 合并器按「入口文件所在目录」解析 include，因此只有当它实际请求的文件集合
/// 与 include 图完全一致时，重建出来的才是同一份配置；不一致就返回
/// `Unreliable`，而不是给一个悄悄校验了别的内容的假结论。
fn check_effective_view(
    root: &Path,
    files: &[(PathBuf, String)],
    edited: &Path,
    candidate: &str,
) -> EffectiveViewCheck {
    let base = root
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    let mut overlay: BTreeMap<PathBuf, String> = files
        .iter()
        .map(|(path, source)| (normalize(path), source.clone()))
        .collect();
    overlay.insert(normalize(edited), candidate.to_string());

    let entry_key = normalize(root);
    let Some(entry_source) = overlay.get(&entry_key).cloned() else {
        return EffectiveViewCheck::Unreliable {
            reason: format!("include 图里没有入口文件 {}", root.display()),
        };
    };

    let mut ast = match parse(&entry_source) {
        Ok(ast) => ast,
        Err(error) => {
            return EffectiveViewCheck::Unreliable {
                reason: format!("入口文件无法解析：{error}"),
            }
        }
    };

    let requested: Rc<RefCell<BTreeSet<PathBuf>>> = Rc::new(RefCell::new(BTreeSet::new()));
    let loader = OverlayLoader {
        base: base.clone(),
        files: overlay.clone(),
        requested: Rc::clone(&requested),
    };
    let mut merger = AstMerger::new(Box::new(loader));
    if let Err(error) = merger.merge_includes(&mut ast, &base) {
        return EffectiveViewCheck::Unreliable { reason: error };
    }

    let mut expected: BTreeSet<PathBuf> = overlay.keys().cloned().collect();
    expected.remove(&entry_key);
    let seen = requested.borrow().clone();
    if seen != expected {
        return EffectiveViewCheck::Unreliable {
            reason: format!(
                "合并器读到的文件集合（{} 个）与 include 图（{} 个）不一致",
                seen.len(),
                expected.len()
            ),
        };
    }

    match crate::validate_ast(&ast) {
        Ok(()) => EffectiveViewCheck::Valid,
        Err(error) => EffectiveViewCheck::Invalid {
            message: error.to_string(),
        },
    }
}

/// include 图的加载结果
struct IncludeGraph {
    files: Vec<(PathBuf, String)>,
    /// 队列里还有没读过的文件时置位：此时不能据此判定目标唯一
    truncated: bool,
}

/// 入口文件 + 它递归包含的所有文件（广度优先，去重、去环）
fn load_include_graph(root: &Path, loader: &dyn FileLoader) -> Result<IncludeGraph, EditRefusal> {
    let mut out: Vec<(PathBuf, String)> = Vec::new();
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    queue.push_back(root.to_path_buf());
    let mut truncated = false;

    while let Some(path) = queue.pop_front() {
        if !seen.insert(normalize(&path)) {
            continue;
        }
        if out.len() >= MAX_FILES {
            // 刚弹出的这个文件还没读过：搜索在这里被截断，
            // 后面可能还有同名目标，因此不能宣称「已经搜完」
            truncated = true;
            break;
        }

        let source = loader.load_file(&path).map_err(|error| {
            EditRefusal::ParseFailed(format!("读不到 {}：{}", path.display(), error))
        })?;

        // 只在这一层找 include 指令，不做合并
        if let Ok(ast) = parse(&source) {
            let mut includes = Vec::new();
            collect_include_paths(&ast.root, &mut includes);
            let base = path.parent().unwrap_or_else(|| Path::new("."));
            for include in includes {
                queue.push_back(base.join(include));
            }
        }

        out.push((path, source));
    }

    Ok(IncludeGraph {
        files: out,
        truncated,
    })
}

/// 收集一份 AST 里的 `@include` 路径
fn collect_include_paths(table: &TableBlock, out: &mut Vec<String>) {
    for entry in &table.entries {
        match entry {
            TableEntry::IncludeDirective(directive) => out.push(directive.path.clone()),
            TableEntry::TableBlock(child) => collect_include_paths(child, out),
            TableEntry::KeyValue(kv) => {
                if let Value::TableBlock(child) = &kv.value {
                    collect_include_paths(child, out);
                }
            }
            _ => {}
        }
    }
}

/// 用于去环的规范化路径；规范化失败时退回原样
fn normalize(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// 取计划里第一条编辑的路径，仅用于错误消息
fn plan_path_label(plan: &EditPlan) -> String {
    let Some(edit) = plan.edits.first() else {
        return "<空计划>".to_string();
    };

    edit.path
        .iter()
        .map(|segment| match segment {
            PathSegment::Key(name) => name.clone(),
            PathSegment::Named { key, r#match } => {
                let rendered: Vec<String> = r#match
                    .iter()
                    .map(|(name, value)| format!("{name}={}", value.to_text()))
                    .collect();
                format!("{key}[{}]", rendered.join(", "))
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EditPlan;
    use std::fs;

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("建目录");
        }
        fs::write(path, content).expect("写文件");
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("verseconf_multifile_{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("建临时目录");
        dir
    }

    fn set_plan(path: &[&str], value: serde_json::Value) -> EditPlan {
        let path: Vec<serde_json::Value> =
            path.iter().map(|name| serde_json::json!(name)).collect();
        EditPlan::from_json(
            &serde_json::json!({
                "version": "1.0",
                "edits": [{ "op": "set", "path": path, "value": value }]
            })
            .to_string(),
        )
        .expect("计划应当合法")
    }

    const ROOT_WITH_INCLUDE: &str = "@include \"child.vcf\"\nroot_key = 1\n";

    #[test]
    fn edits_the_included_file_not_the_root() {
        let dir = temp_dir("included");
        write(&dir.join("root.vcf"), ROOT_WITH_INCLUDE);
        write(&dir.join("child.vcf"), "port = 8080 # keep\n");

        let plan = set_plan(&["port"], serde_json::json!(9090));
        let outcome = apply_edit_plan_in_files(&dir.join("root.vcf"), &plan).expect("应当命中");

        assert!(
            outcome.file.path.ends_with("child.vcf"),
            "应当改被包含的文件，实际 {:?}",
            outcome.file.path
        );
        assert_eq!(outcome.file.before, "port = 8080 # keep\n");
        assert_eq!(outcome.file.after, "port = 9090 # keep\n");
        // 根文件必须一个字节都没动
        assert_eq!(
            fs::read_to_string(dir.join("root.vcf")).unwrap(),
            ROOT_WITH_INCLUDE
        );
    }

    #[test]
    fn edits_the_root_when_the_target_lives_there() {
        let dir = temp_dir("root");
        write(&dir.join("root.vcf"), ROOT_WITH_INCLUDE);
        write(&dir.join("child.vcf"), "port = 8080\n");

        let plan = set_plan(&["root_key"], serde_json::json!(2));
        let outcome = apply_edit_plan_in_files(&dir.join("root.vcf"), &plan).expect("应当命中");

        assert!(outcome.file.path.ends_with("root.vcf"));
        assert_eq!(outcome.file.after, "@include \"child.vcf\"\nroot_key = 2\n");
    }

    #[test]
    fn a_target_in_two_files_is_refused_as_ambiguous() {
        let dir = temp_dir("ambiguous");
        write(&dir.join("root.vcf"), "port = 1\n@include \"child.vcf\"\n");
        write(&dir.join("child.vcf"), "port = 2\n");

        let plan = set_plan(&["port"], serde_json::json!(9090));
        let refusal = apply_edit_plan_in_files(&dir.join("root.vcf"), &plan).expect_err("必须拒绝");

        assert_eq!(refusal.code(), "target_ambiguous");
        match refusal {
            EditRefusal::TargetAmbiguousAcrossFiles { files, .. } => {
                assert_eq!(files.len(), 2, "应当列出两个文件：{files:?}");
            }
            other => panic!("期望 TargetAmbiguousAcrossFiles，实际 {other:?}"),
        }
    }

    #[test]
    fn a_target_nowhere_is_refused_and_lists_the_searched_files() {
        let dir = temp_dir("missing");
        write(&dir.join("root.vcf"), ROOT_WITH_INCLUDE);
        write(&dir.join("child.vcf"), "port = 8080\n");

        let plan = set_plan(&["nope"], serde_json::json!(1));
        let refusal = apply_edit_plan_in_files(&dir.join("root.vcf"), &plan).expect_err("必须拒绝");

        assert_eq!(refusal.code(), "target_not_found");
        match refusal {
            EditRefusal::TargetNotFoundAcrossFiles { files, .. } => {
                assert_eq!(files.len(), 2, "应当列出搜索过的两个文件：{files:?}");
            }
            other => panic!("期望 TargetNotFoundAcrossFiles，实际 {other:?}"),
        }
    }

    #[test]
    fn nested_includes_are_searched() {
        let dir = temp_dir("nested");
        write(&dir.join("root.vcf"), "@include \"mid.vcf\"\n");
        write(&dir.join("mid.vcf"), "@include \"deep.vcf\"\n");
        write(&dir.join("deep.vcf"), "deep_key = 1\n");

        let plan = set_plan(&["deep_key"], serde_json::json!(2));
        let outcome = apply_edit_plan_in_files(&dir.join("root.vcf"), &plan).expect("应当命中");
        assert!(outcome.file.path.ends_with("deep.vcf"));
        assert_eq!(outcome.file.after, "deep_key = 2\n");
    }

    #[test]
    fn a_cycle_does_not_hang() {
        let dir = temp_dir("cycle");
        write(&dir.join("a.vcf"), "@include \"b.vcf\"\na = 1\n");
        write(&dir.join("b.vcf"), "@include \"a.vcf\"\nb = 1\n");

        let plan = set_plan(&["b"], serde_json::json!(2));
        let outcome = apply_edit_plan_in_files(&dir.join("a.vcf"), &plan).expect("应当命中");
        assert!(outcome.file.path.ends_with("b.vcf"));
    }

    #[test]
    fn the_edited_file_still_goes_through_write_time_validation() {
        // 目标在被包含文件里，但改动会破坏那个文件的 schema —— 必须仍然被拒
        let dir = temp_dir("validation");
        write(&dir.join("root.vcf"), "@include \"child.vcf\"\n");
        write(
            &dir.join("child.vcf"),
            "#@schema {\n  version = \"1.0\"\n  port { type = \"integer\" }\n}\nport = 8080\n",
        );

        let plan = set_plan(&["port"], serde_json::json!("not-a-number"));
        let refusal = apply_edit_plan_in_files(&dir.join("root.vcf"), &plan).expect_err("必须拒绝");
        assert_eq!(refusal.code(), "validation_failed");
    }

    #[test]
    fn an_edit_that_breaks_the_merged_effective_view_is_refused() {
        // 回归：入口 schema 要求 integer，值却在 include 文件里；把子文件的值改成
        // 字符串后旧实现返回成功，而合并后的生效配置其实已经非法。
        let dir = temp_dir("effective_broken");
        write(
            &dir.join("root.vcf"),
            "#@schema {\n  port {\n    type = \"integer\"\n  }\n}\n@include \"values.vcf\"\n",
        );
        write(&dir.join("values.vcf"), "port = 8080\n");

        let plan = set_plan(&["port"], serde_json::json!("broken"));
        let refusal = apply_edit_plan_in_files(&dir.join("root.vcf"), &plan)
            .expect_err("生效配置非法必须拒绝");
        assert_eq!(refusal.code(), "validation_failed");
    }

    #[test]
    fn a_valid_cross_file_edit_reports_a_validated_effective_view() {
        let dir = temp_dir("effective_ok");
        write(
            &dir.join("root.vcf"),
            "#@schema {\n  port {\n    type = \"integer\"\n  }\n}\n@include \"values.vcf\"\n",
        );
        write(&dir.join("values.vcf"), "port = 8080\n");

        let plan = set_plan(&["port"], serde_json::json!(9090));
        let outcome = apply_edit_plan_in_files(&dir.join("root.vcf"), &plan).expect("应当命中");
        assert_eq!(outcome.effective_view, EffectiveView::Validated);
    }

    #[test]
    fn an_effective_view_that_cannot_be_rebuilt_is_reported_not_assumed() {
        // 嵌套 include：合并器按入口目录解析，include 图按各自所在目录解析，
        // 两者不一致时只能报「未校验」，不能假装通过。
        let dir = temp_dir("effective_unreliable");
        write(&dir.join("root.vcf"), "@include \"sub/mid.vcf\"\n");
        write(&dir.join("sub/mid.vcf"), "@include \"deep.vcf\"\nmid = 1\n");
        write(&dir.join("sub/deep.vcf"), "deep = 1\n");

        let plan = set_plan(&["mid"], serde_json::json!(2));
        let outcome = apply_edit_plan_in_files(&dir.join("root.vcf"), &plan).expect("应当命中");
        assert!(
            matches!(outcome.effective_view, EffectiveView::NotValidated { .. }),
            "实际 {:?}",
            outcome.effective_view
        );
    }

    #[test]
    fn a_truncated_search_is_refused_instead_of_claiming_a_unique_match() {
        // 回归：include 图超过上限时，未扫描的文件里可能还有同名目标。
        // 旧实现会静默按已扫描的子集判定唯一，从而改错文件。
        let dir = temp_dir("truncated");
        let mut includes = Vec::new();
        for index in 0..MAX_FILES {
            let name = format!("f{index:02}.vcf");
            includes.push(format!("@include \"{name}\""));
            let body = if index == 0 || index == MAX_FILES - 1 {
                "port = 8080\n".to_string()
            } else {
                format!("other_{index} = 1\n")
            };
            write(&dir.join(&name), &body);
        }
        write(&dir.join("root.vcf"), &format!("{}\n", includes.join("\n")));

        let plan = set_plan(&["port"], serde_json::json!(9090));
        let refusal =
            apply_edit_plan_in_files(&dir.join("root.vcf"), &plan).expect_err("搜索被截断必须拒绝");

        assert_eq!(refusal.code(), "search_incomplete");
        match refusal {
            EditRefusal::SearchIncomplete { files, limit, .. } => {
                assert_eq!(limit, MAX_FILES);
                assert_eq!(files.len(), MAX_FILES, "应当报告实际扫描了多少文件");
            }
            other => panic!("期望 SearchIncomplete，实际 {other:?}"),
        }
    }

    #[test]
    fn a_graph_exactly_at_the_limit_is_still_searched_completely() {
        // 边界：恰好用满上限但队列已空，搜索是完整的，不能误拒
        let dir = temp_dir("at_limit");
        let mut includes = Vec::new();
        for index in 0..(MAX_FILES - 1) {
            let name = format!("f{index:02}.vcf");
            includes.push(format!("@include \"{name}\""));
            write(&dir.join(&name), &format!("other_{index} = 1\n"));
        }
        write(
            &dir.join("root.vcf"),
            &format!("{}\nport = 8080\n", includes.join("\n")),
        );

        let plan = set_plan(&["port"], serde_json::json!(9090));
        let outcome =
            apply_edit_plan_in_files(&dir.join("root.vcf"), &plan).expect("搜索完整时应当命中");
        assert!(outcome.file.path.ends_with("root.vcf"));
    }
}

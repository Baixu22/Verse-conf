//! 基于字符区间的确定性最小改动。
//!
//! 只替换目标字段的值区间，绝不重新序列化整个文件：其余字节保持原样，
//! 引号风格、注释、元数据与空行都不受影响。任何无法唯一定位或与前置条件
//! 不符的情况都返回拒绝，而不是猜测。

use crate::ast::{ArrayTable, KeyValue, TableBlock, TableEntry, Value};
use crate::edit::value::EditValue;
use crate::edit::{
    describe_path, EditExpectation, EditIntent, EditOp, EditPlan, PathSegment, PlanViolation,
};
use crate::engine::audit::{
    high_risk_instances, introduced_high_risk_instances, render_risk_instance, AuditEngine,
};
use crate::engine::pretty_printer::render_value;
use crate::lexer::{Lexer, Token};
use std::collections::BTreeMap;

/// 拒绝原因。每一条都对应「失败即拒绝」的一个具体情形。
#[derive(Debug, Clone, PartialEq)]
pub enum EditRefusal {
    /// 契约自身不自洽
    InvalidPlan(Vec<PlanViolation>),
    /// 源码无法解析
    ParseFailed(String),
    /// 目标字段不存在
    TargetNotFound { path: String },
    /// 命名列表定位命中多个元素，有歧义
    TargetAmbiguous { path: String, matches: usize },
    /// insert 的目标已经存在
    TargetAlreadyExists { path: String },
    /// 当前值与 expect 不符
    ExpectationMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    /// 目标结构不支持这种编辑（例如往数组表元素插入字段）
    UnsupportedTarget { path: String, reason: String },
    /// 改动后的配置未通过结构或 schema 校验
    ValidationFailed { path: String, message: String },
    /// 改动引入了新的高危安全问题。
    ///
    /// `findings` 是去重后的规则码（机器可读、稳定），`instances` 是
    /// 「规则 @ 位置」形式的风险实例（可定位到具体字段）。
    SecurityRejected {
        path: String,
        findings: Vec<String>,
        instances: Vec<String>,
    },
    /// 跨 `@include` 编辑时，目标在多个文件里都能定位到，无法确定该改哪一个
    TargetAmbiguousAcrossFiles { path: String, files: Vec<String> },
    /// 跨 `@include` 编辑时，目标在任何被搜索到的文件里都不存在
    TargetNotFoundAcrossFiles { path: String, files: Vec<String> },
    /// 跨 `@include` 编辑时，include 图超过搜索上限，无法证明目标唯一
    SearchIncomplete {
        path: String,
        files: Vec<String>,
        limit: usize,
    },
}

impl std::fmt::Display for EditRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditRefusal::InvalidPlan(violations) => {
                let detail = violations
                    .iter()
                    .map(|violation| violation.to_string())
                    .collect::<Vec<_>>()
                    .join("; ");
                write!(f, "编辑计划不合法：{}", detail)
            }
            EditRefusal::ParseFailed(message) => write!(f, "源码无法解析：{}", message),
            EditRefusal::TargetNotFound { path } => write!(f, "目标不存在：{}", path),
            EditRefusal::TargetAmbiguous { path, matches } => write!(
                f,
                "目标有歧义：{} 命中了 {} 个元素，必须唯一命中",
                path, matches
            ),
            EditRefusal::TargetAmbiguousAcrossFiles { path, files } => write!(
                f,
                "目标有歧义：{} 在 {} 个文件里都能定位到（{}），必须唯一命中；请指明要改哪个文件",
                path,
                files.len(),
                files.join(", ")
            ),
            EditRefusal::TargetNotFoundAcrossFiles { path, files } => write!(
                f,
                "目标不存在：{} 在被包含的 {} 个文件里都没有找到（{}）",
                path,
                files.len(),
                files.join(", ")
            ),
            EditRefusal::SearchIncomplete {
                path,
                files,
                limit,
            } => write!(
                f,
                "搜索不完整：{} 的 include 图超过上限 {}，只扫描了 {} 个文件，无法证明目标唯一；请显式指明目标文件",
                path,
                limit,
                files.len()
            ),
            EditRefusal::TargetAlreadyExists { path } => {
                write!(f, "目标已存在：{}，如需覆盖请使用 set", path)
            }
            EditRefusal::ExpectationMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "前置条件不符：{} 期望 {}，实际 {}",
                path, expected, actual
            ),
            EditRefusal::UnsupportedTarget { path, reason } => {
                write!(f, "目标不支持该操作：{}（{}）", path, reason)
            }
            EditRefusal::ValidationFailed { path, message } => {
                write!(f, "改动后校验失败：{}（{}）", path, message)
            }
            EditRefusal::SecurityRejected {
                path, instances, ..
            } => write!(
                f,
                "改动引入新的安全风险：{}（{}）",
                path,
                instances.join(", ")
            ),
        }
    }
}

impl std::error::Error for EditRefusal {}

impl EditRefusal {
    /// 稳定的机器可读拒绝码，供工具协议层返回结构化错误
    pub fn code(&self) -> &'static str {
        match self {
            EditRefusal::InvalidPlan(_) => "invalid_plan",
            EditRefusal::ParseFailed(_) => "parse_failed",
            EditRefusal::TargetNotFound { .. } => "target_not_found",
            EditRefusal::TargetAmbiguous { .. }
            | EditRefusal::TargetAmbiguousAcrossFiles { .. } => "target_ambiguous",
            EditRefusal::TargetNotFoundAcrossFiles { .. } => "target_not_found",
            EditRefusal::SearchIncomplete { .. } => "search_incomplete",
            EditRefusal::TargetAlreadyExists { .. } => "target_already_exists",
            EditRefusal::ExpectationMismatch { .. } => "expectation_mismatch",
            EditRefusal::UnsupportedTarget { .. } => "unsupported_target",
            EditRefusal::ValidationFailed { .. } => "validation_failed",
            EditRefusal::SecurityRejected { .. } => "security_rejected",
        }
    }

    /// 与 `code()` 配套的结构化细节
    pub fn details(&self) -> serde_json::Value {
        match self {
            EditRefusal::InvalidPlan(violations) => serde_json::json!({
                "violations": violations
                    .iter()
                    .map(|violation| serde_json::json!({
                        "code": violation.code,
                        "message": violation.message,
                        "edit_index": violation.edit_index,
                    }))
                    .collect::<Vec<_>>(),
            }),
            EditRefusal::ParseFailed(message) => serde_json::json!({ "message": message }),
            EditRefusal::TargetNotFound { path }
            | EditRefusal::TargetAlreadyExists { path }
            | EditRefusal::UnsupportedTarget { path, .. } => serde_json::json!({ "path": path }),
            EditRefusal::TargetAmbiguous { path, matches } => {
                serde_json::json!({ "path": path, "matches": matches })
            }
            EditRefusal::TargetAmbiguousAcrossFiles { path, files } => {
                serde_json::json!({ "path": path, "files": files })
            }
            EditRefusal::TargetNotFoundAcrossFiles { path, files } => {
                serde_json::json!({ "path": path, "files": files })
            }
            EditRefusal::SearchIncomplete { path, files, limit } => serde_json::json!({
                "path": path,
                "files": files,
                "limit": limit,
                "scanned": files.len(),
            }),
            EditRefusal::ExpectationMismatch {
                path,
                expected,
                actual,
            } => serde_json::json!({ "path": path, "expected": expected, "actual": actual }),
            EditRefusal::ValidationFailed { path, message } => {
                serde_json::json!({ "path": path, "message": message })
            }
            EditRefusal::SecurityRejected {
                path,
                findings,
                instances,
            } => serde_json::json!({
                "path": path,
                "findings": findings,
                "instances": instances,
            }),
        }
    }
}

/// 一条已应用的编辑记录
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedEdit {
    pub op: EditOp,
    pub path: String,
    pub before: String,
    pub after: String,
    pub reason: Option<String>,
}

/// 编辑结果
#[derive(Debug, Clone, PartialEq)]
pub struct EditOutcome {
    /// 改动后的完整源码
    pub source: String,
    /// 已应用的编辑
    pub applied: Vec<AppliedEdit>,
}

/// 应用编辑计划：逐条做最小改动，全部成功后再做写入前的双重校验。
///
/// 任何一步失败都返回 `Err`，调用方不应写入任何文件。
pub fn apply_edit_plan(source: &str, plan: &EditPlan) -> Result<EditOutcome, EditRefusal> {
    if let Err(violations) = plan.validate() {
        return Err(EditRefusal::InvalidPlan(violations));
    }

    let mut current = source.to_string();
    let mut applied = Vec::new();
    for edit in &plan.edits {
        let (next, record) = apply_one(&current, edit)?;
        current = next;
        applied.push(record);
    }

    let current = finalize_edit(source, current)?;

    Ok(EditOutcome {
        source: current,
        applied,
    })
}

/// 区间编辑原语：把 `[start, end)` 这段字节替换为 `replacement`。
///
/// 与意图应用走完全相同的写入前双重校验；区间越界、不在字符边界上或
/// 改动后不再合法时一律拒绝。
pub fn replace_range(
    source: &str,
    start: usize,
    end: usize,
    replacement: &str,
) -> Result<EditOutcome, EditRefusal> {
    let invalid = |reason: &str| EditRefusal::UnsupportedTarget {
        path: format!("bytes[{}, {})", start, end),
        reason: reason.to_string(),
    };

    if start > end || end > source.len() {
        return Err(invalid("区间越界"));
    }
    if !source.is_char_boundary(start) || !source.is_char_boundary(end) {
        return Err(invalid("区间端点不在字符边界上"));
    }

    let before = source[start..end].to_string();
    let mut candidate = String::with_capacity(source.len() + replacement.len());
    candidate.push_str(&source[..start]);
    candidate.push_str(replacement);
    candidate.push_str(&source[end..]);

    let candidate = finalize_edit(source, candidate)?;

    Ok(EditOutcome {
        source: candidate,
        applied: vec![AppliedEdit {
            op: EditOp::Set,
            path: format!("bytes[{}, {})", start, end),
            before,
            after: replacement.to_string(),
            reason: None,
        }],
    })
}

/// 定位「某个字段的值」在源码里的字节区间。
///
/// 这是判定**附带损伤**的基准：按意图执行协议的验收口径，「改动之外的字节零变化」
/// 等价于改动前后的前缀与后缀逐字节相同。区间只由原始源码与 AST 的位置信息决定，
/// 与被测的实现无关，因此可以用来判定任意策略——包括不是本仓库实现的策略。
///
/// 找不到目标、目标有歧义或值区间无法确定时返回与编辑路径相同的拒绝原因。
pub fn value_span_for_path(
    source: &str,
    path: &[PathSegment],
) -> Result<(usize, usize), EditRefusal> {
    let path_label = describe_path(path);
    let ast = crate::parse(source).map_err(|error| EditRefusal::ParseFailed(error.to_string()))?;

    let Some((last_segment, parent_segments)) = path.split_last() else {
        return Err(EditRefusal::InvalidPlan(vec![PlanViolation::new(
            "empty_path",
            "路径不能为空",
            None,
        )]));
    };

    let key_name = match last_segment {
        PathSegment::Key(name) => name.as_str(),
        PathSegment::Named { .. } => {
            return Err(EditRefusal::UnsupportedTarget {
                path: path_label,
                reason: "路径最后一段必须是字段名，不能是按名称定位的列表元素".to_string(),
            })
        }
    };

    let container = resolve_container(&ast.root, parent_segments, &path_label)?;
    let existing =
        container
            .find_key_value(key_name)
            .ok_or_else(|| EditRefusal::TargetNotFound {
                path: path_label.clone(),
            })?;

    value_span(source, existing, &path_label)
}

/// 写入前的双重校验：结构/schema 必须合法，且没有引入新的高危安全问题。
fn finalize_edit(source: &str, candidate: String) -> Result<String, EditRefusal> {
    let baseline = high_risk_instances(&AuditEngine::new().audit_source(source));

    let ast = crate::parse(&candidate).map_err(|error| EditRefusal::ValidationFailed {
        path: "<result>".to_string(),
        message: error.to_string(),
    })?;
    if let Err(error) = crate::validate_ast(&ast) {
        return Err(EditRefusal::ValidationFailed {
            path: "<result>".to_string(),
            message: error.to_string(),
        });
    }

    // 安全审计：只拒绝本次改动新引入的高危实例，不因为文件本来就有的问题拒绝
    let after = high_risk_instances(&AuditEngine::new().audit_source(&candidate));
    let introduced = introduced_high_risk_instances(&baseline, &after);
    if !introduced.is_empty() {
        let mut findings: Vec<String> = introduced.iter().map(|(rule, _)| rule.clone()).collect();
        findings.sort();
        findings.dedup();
        return Err(EditRefusal::SecurityRejected {
            path: "<result>".to_string(),
            findings,
            instances: introduced.iter().map(render_risk_instance).collect(),
        });
    }

    Ok(candidate)
}

/// 容器：普通表块，或 `[[key]]` 数组表里的一个元素
#[derive(Clone, Copy)]
enum Container<'a> {
    Table(&'a TableBlock),
    ArrayItem {
        #[allow(dead_code)]
        table_key: &'a str,
        item: &'a ArrayTable,
    },
}

impl<'a> Container<'a> {
    fn find_key_value(&self, name: &str) -> Option<&'a KeyValue> {
        match self {
            Container::Table(table) => table.entries.iter().find_map(|entry| match entry {
                TableEntry::KeyValue(kv) if kv.key.as_str() == name => Some(kv),
                _ => None,
            }),
            Container::ArrayItem { item, .. } => {
                item.entries.iter().find(|kv| kv.key.as_str() == name)
            }
        }
    }

    fn find_table_block(&self, name: &str) -> Option<&'a TableBlock> {
        match self {
            Container::Table(table) => table.entries.iter().find_map(|entry| match entry {
                TableEntry::TableBlock(nested) if nested.name.as_deref() == Some(name) => {
                    Some(nested)
                }
                TableEntry::KeyValue(kv) if kv.key.as_str() == name => match &kv.value {
                    Value::TableBlock(nested) => Some(nested),
                    _ => None,
                },
                _ => None,
            }),
            Container::ArrayItem { item, .. } => item.entries.iter().find_map(|kv| {
                if kv.key.as_str() == name {
                    match &kv.value {
                        Value::TableBlock(nested) => Some(nested),
                        _ => None,
                    }
                } else {
                    None
                }
            }),
        }
    }

    fn first_entry_offset(&self) -> Option<usize> {
        match self {
            Container::Table(table) => table
                .entries
                .iter()
                .filter_map(entry_start)
                .find(|offset| *offset > 0),
            Container::ArrayItem { item, .. } => item
                .entries
                .first()
                .map(|kv| kv.span.start)
                .filter(|offset| *offset > 0),
        }
    }

    fn last_entry_end(&self) -> Option<usize> {
        match self {
            Container::Table(table) => table
                .entries
                .iter()
                .filter_map(entry_end)
                .filter(|offset| *offset > 0)
                .max(),
            Container::ArrayItem { item, .. } => item
                .entries
                .last()
                .map(|kv| kv.span.end)
                .filter(|offset| *offset > 0),
        }
    }
}

fn entry_start(entry: &TableEntry) -> Option<usize> {
    match entry {
        TableEntry::KeyValue(kv) => Some(kv.span.start),
        TableEntry::TableBlock(table) => Some(table.span.start),
        TableEntry::ArrayTable(table) => Some(table.span.start),
        TableEntry::IncludeDirective(include) => Some(include.span.start),
        TableEntry::Comment(comment) => Some(comment.span.start),
    }
}

fn entry_end(entry: &TableEntry) -> Option<usize> {
    match entry {
        TableEntry::KeyValue(kv) => Some(kv.span.end),
        TableEntry::TableBlock(table) => Some(table.span.end),
        TableEntry::ArrayTable(table) => Some(table.span.end),
        TableEntry::IncludeDirective(include) => Some(include.span.end),
        TableEntry::Comment(comment) => Some(comment.span.end),
    }
}

fn apply_one(source: &str, edit: &EditIntent) -> Result<(String, AppliedEdit), EditRefusal> {
    let path_label = describe_path(&edit.path);
    let ast = crate::parse(source).map_err(|error| EditRefusal::ParseFailed(error.to_string()))?;

    let Some((last_segment, parent_segments)) = edit.path.split_last() else {
        return Err(EditRefusal::InvalidPlan(vec![PlanViolation::new(
            "empty_path",
            "路径不能为空",
            None,
        )]));
    };

    let key_name = match last_segment {
        PathSegment::Key(name) => name.clone(),
        PathSegment::Named { .. } => {
            return Err(EditRefusal::UnsupportedTarget {
                path: path_label,
                reason: "路径最后一段必须是字段名，不能是按名称定位的列表元素".to_string(),
            })
        }
    };

    let container = resolve_container(&ast.root, parent_segments, &path_label)?;

    match edit.op {
        EditOp::Set => apply_set(source, edit, container, &key_name, path_label),
        EditOp::Insert => apply_insert(source, edit, container, &key_name, path_label),
        EditOp::Delete => apply_delete(source, edit, container, &key_name, path_label),
    }
}

fn resolve_container<'a>(
    root: &'a TableBlock,
    segments: &[PathSegment],
    path_label: &str,
) -> Result<Container<'a>, EditRefusal> {
    let mut container = Container::Table(root);

    for segment in segments {
        container = match segment {
            PathSegment::Key(name) => match container.find_table_block(name) {
                Some(table) => Container::Table(table),
                None => {
                    if container.find_key_value(name).is_some() {
                        return Err(EditRefusal::UnsupportedTarget {
                            path: path_label.to_string(),
                            reason: format!("字段 '{}' 不是表，无法继续下钻", name),
                        });
                    }
                    return Err(EditRefusal::TargetNotFound {
                        path: path_label.to_string(),
                    });
                }
            },
            PathSegment::Named { key, r#match } => {
                let matches = match_array_elements(&container, key, r#match);
                match matches.len() {
                    0 => {
                        return Err(EditRefusal::TargetNotFound {
                            path: path_label.to_string(),
                        })
                    }
                    1 => matches[0],
                    count => {
                        return Err(EditRefusal::TargetAmbiguous {
                            path: path_label.to_string(),
                            matches: count,
                        })
                    }
                }
            }
        };
    }

    Ok(container)
}

fn match_array_elements<'a>(
    container: &Container<'a>,
    key: &str,
    expected: &BTreeMap<String, EditValue>,
) -> Vec<Container<'a>> {
    match container {
        Container::Table(table) => table
            .entries
            .iter()
            .filter_map(|entry| match entry {
                TableEntry::ArrayTable(array) if array.key.as_str() == key => {
                    let matched = expected.iter().all(|(field, want)| {
                        array.entries.iter().any(|kv| {
                            kv.key.as_str() == field
                                && EditValue::from_ast_value(&kv.value).as_ref() == Some(want)
                        })
                    });
                    matched.then_some(Container::ArrayItem {
                        table_key: array.key.as_str(),
                        item: array,
                    })
                }
                _ => None,
            })
            .collect(),
        Container::ArrayItem { .. } => Vec::new(),
    }
}

fn apply_set(
    source: &str,
    edit: &EditIntent,
    container: Container<'_>,
    key_name: &str,
    path_label: String,
) -> Result<(String, AppliedEdit), EditRefusal> {
    let existing =
        container
            .find_key_value(key_name)
            .ok_or_else(|| EditRefusal::TargetNotFound {
                path: path_label.clone(),
            })?;

    check_expectation(&path_label, edit.expect.as_ref(), &existing.value)?;

    let (start, end) = value_span(source, existing, &path_label)?;
    let before = source[start..end].to_string();

    let value = edit
        .value
        .clone()
        .ok_or_else(|| missing_value_violation(&path_label))?;
    let after = render_value(&value.to_ast_value(), indent_level_at(source, start));

    let mut next = String::with_capacity(source.len() + after.len());
    next.push_str(&source[..start]);
    next.push_str(&after);
    next.push_str(&source[end..]);

    Ok((
        next,
        AppliedEdit {
            op: EditOp::Set,
            path: path_label,
            before,
            after,
            reason: edit.reason.clone(),
        },
    ))
}

fn apply_insert(
    source: &str,
    edit: &EditIntent,
    container: Container<'_>,
    key_name: &str,
    path_label: String,
) -> Result<(String, AppliedEdit), EditRefusal> {
    if container.find_key_value(key_name).is_some()
        || container.find_table_block(key_name).is_some()
    {
        return Err(EditRefusal::TargetAlreadyExists { path: path_label });
    }

    let value = edit
        .value
        .clone()
        .ok_or_else(|| missing_value_violation(&path_label))?;

    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let indent = match container.first_entry_offset() {
        Some(offset) => line_indent(source, offset),
        None => String::new(),
    };
    let rendered = render_value(&value.to_ast_value(), indent.len() / 2);
    let line = format!("{}{} = {}", indent, key_name, rendered);

    let insert_at = match closing_brace_offset(source, &container) {
        Some(brace) => {
            let brace_line_start = line_start_of(source, brace);
            if !source[brace_line_start..brace].trim().is_empty() {
                return Err(EditRefusal::UnsupportedTarget {
                    path: path_label,
                    reason: "右花括号不在独立行上，无法安全插入新字段".to_string(),
                });
            }
            brace_line_start
        }
        // 根表没有右花括号：追加在最后一条条目之后
        None => match container.last_entry_end() {
            Some(end) => line_end_including_newline(source, end),
            None => source.len(),
        },
    };

    let needs_newline_prefix = insert_at > 0 && !source[..insert_at].ends_with('\n');
    let mut next = String::with_capacity(source.len() + line.len() + newline.len() * 2);
    next.push_str(&source[..insert_at]);
    if needs_newline_prefix {
        next.push_str(newline);
    }
    next.push_str(&line);
    next.push_str(newline);
    next.push_str(&source[insert_at..]);

    Ok((
        next,
        AppliedEdit {
            op: EditOp::Insert,
            path: path_label,
            before: String::new(),
            after: rendered,
            reason: edit.reason.clone(),
        },
    ))
}

fn apply_delete(
    source: &str,
    edit: &EditIntent,
    container: Container<'_>,
    key_name: &str,
    path_label: String,
) -> Result<(String, AppliedEdit), EditRefusal> {
    let existing =
        container
            .find_key_value(key_name)
            .ok_or_else(|| EditRefusal::TargetNotFound {
                path: path_label.clone(),
            })?;

    check_expectation(&path_label, edit.expect.as_ref(), &existing.value)?;

    let line_start = line_start_of(source, existing.span.start);
    let line_end = line_end_including_newline(source, existing.span.end);

    if !source[line_start..existing.span.start].trim().is_empty()
        || !source[existing.span.end..line_end].trim().is_empty()
    {
        return Err(EditRefusal::UnsupportedTarget {
            path: path_label,
            reason: "该字段与其它内容共用一行，删除会波及其它内容".to_string(),
        });
    }

    let before = source[line_start..line_end].to_string();
    let mut next = String::with_capacity(source.len());
    next.push_str(&source[..line_start]);
    next.push_str(&source[line_end..]);

    Ok((
        next,
        AppliedEdit {
            op: EditOp::Delete,
            path: path_label,
            before,
            after: String::new(),
            reason: edit.reason.clone(),
        },
    ))
}

fn missing_value_violation(path: &str) -> EditRefusal {
    EditRefusal::InvalidPlan(vec![PlanViolation::new(
        "missing_value",
        format!("{} 需要 value", path),
        None,
    )])
}

fn check_expectation(
    path: &str,
    expect: Option<&EditExpectation>,
    actual: &Value,
) -> Result<(), EditRefusal> {
    let Some(expected) = expect.and_then(|expectation| expectation.value.as_ref()) else {
        return Ok(());
    };

    let actual = EditValue::from_ast_value(actual);
    match actual {
        Some(actual) if &actual == expected => Ok(()),
        Some(actual) => Err(EditRefusal::ExpectationMismatch {
            path: path.to_string(),
            expected: expected.to_text(),
            actual: actual.to_text(),
        }),
        None => Err(EditRefusal::ExpectationMismatch {
            path: path.to_string(),
            expected: expected.to_text(),
            actual: actual_value_text(expect, path).to_string(),
        }),
    }
}

fn actual_value_text(expect: Option<&EditExpectation>, _path: &str) -> String {
    match expect.and_then(|expectation| expectation.value.as_ref()) {
        Some(value) => format!("无法解析当前值（期望 {}）", value.to_text()),
        None => "无法解析当前值".to_string(),
    }
}

/// 目标值的字节区间：从赋值符之后到元数据或注释之前
fn value_span(
    source: &str,
    key_value: &KeyValue,
    path_label: &str,
) -> Result<(usize, usize), EditRefusal> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer
        .tokenize_all()
        .map_err(|error| EditRefusal::ParseFailed(error.to_string()))?;

    let assign = tokens
        .iter()
        .find(|(token, span)| {
            matches!(token, Token::Assign)
                && span.start >= key_value.span.start
                && span.end <= key_value.span.end
        })
        .map(|(_, span)| *span)
        .ok_or_else(|| EditRefusal::UnsupportedTarget {
            path: path_label.to_string(),
            reason: "找不到赋值符，无法确定值区间".to_string(),
        })?;

    let bytes = source.as_bytes();
    let mut start = assign.end;
    while start < source.len() && (bytes[start] == b' ' || bytes[start] == b'\t') {
        start += 1;
    }

    let limit = [
        key_value
            .metadata
            .as_ref()
            .map(|metadata| metadata.span.start),
        key_value.comment.as_ref().map(|comment| comment.span.start),
    ]
    .into_iter()
    .flatten()
    .min()
    .unwrap_or(key_value.span.end);

    let mut end = limit;
    while end > start && matches!(bytes[end - 1], b' ' | b'\t' | b'\n' | b'\r') {
        end -= 1;
    }

    if start >= end || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
        return Err(EditRefusal::UnsupportedTarget {
            path: path_label.to_string(),
            reason: "值区间为空或不在字符边界上".to_string(),
        });
    }

    Ok((start, end))
}

fn closing_brace_offset(source: &str, container: &Container<'_>) -> Option<usize> {
    let Container::Table(table) = container else {
        return None;
    };
    if table.span.end == 0 {
        return None;
    }

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize_all().ok()?;
    tokens
        .iter()
        .find(|(token, span)| matches!(token, Token::RBrace) && span.end == table.span.end)
        .map(|(_, span)| span.start)
}

fn line_start_of(source: &str, offset: usize) -> usize {
    let bytes = source.as_bytes();
    let mut index = offset.min(source.len());
    while index > 0 && bytes[index - 1] != b'\n' {
        index -= 1;
    }
    index
}

fn line_end_including_newline(source: &str, offset: usize) -> usize {
    let bytes = source.as_bytes();
    let mut index = offset.min(source.len());
    while index < source.len() && bytes[index] != b'\n' {
        index += 1;
    }
    if index < source.len() {
        index += 1;
    }
    index
}

fn line_indent(source: &str, offset: usize) -> String {
    let line_start = line_start_of(source, offset);
    source[line_start..offset]
        .chars()
        .take_while(|ch| *ch == ' ' || *ch == '\t')
        .collect()
}

fn indent_level_at(source: &str, offset: usize) -> usize {
    line_indent(source, offset).chars().count() / 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{EditPlan, EDIT_PLAN_VERSION};
    use std::collections::BTreeSet;

    fn plan(json: &str) -> EditPlan {
        EditPlan::from_json(json).expect("测试用计划必须合法")
    }

    #[test]
    fn set_only_touches_the_target_value() {
        let source = "# 顶部注释\nserver {\n  host = \"localhost\" # 保留我\n  port = 8080 #@ range(1..65535)\n}\n";
        let edit_plan = plan(
            r#"{
              "version": "1.0",
              "edits": [
                { "op": "set", "path": ["server", "port"], "value": 9090, "expect": { "value": 8080 } }
              ]
            }"#,
        );

        let outcome = apply_edit_plan(source, &edit_plan).expect("合法编辑应当成功");
        assert_eq!(
            outcome.source,
            "# 顶部注释\nserver {\n  host = \"localhost\" # 保留我\n  port = 9090 #@ range(1..65535)\n}\n"
        );
        assert_eq!(outcome.applied[0].before, "8080");
        assert_eq!(outcome.applied[0].after, "9090");

        // 逐行比较：只有目标行不同
        let changed: Vec<usize> = source
            .lines()
            .zip(outcome.source.lines())
            .enumerate()
            .filter(|(_, (before, after))| before != after)
            .map(|(index, _)| index)
            .collect();
        assert_eq!(changed, vec![3], "只有 port 所在行应发生变化");
    }

    #[test]
    fn set_keeps_untouched_lines_byte_identical() {
        let source = "a = \"double\"\nb    =    \"spaced\"   # 保留\n";
        let edit_plan =
            plan(r#"{"version":"1.0","edits":[{"op":"set","path":["a"],"value":"changed"}]}"#);
        let outcome = apply_edit_plan(source, &edit_plan).unwrap();
        assert_eq!(
            outcome.source,
            "a = \"changed\"\nb    =    \"spaced\"   # 保留\n"
        );
        assert!(
            outcome.source.ends_with("b    =    \"spaced\"   # 保留\n"),
            "未触及行必须逐字节保持原样"
        );
    }

    #[test]
    fn set_preserves_crlf_line_endings() {
        let source = "server {\r\n  port = 8080\r\n}\r\n";
        let edit_plan =
            plan(r#"{"version":"1.0","edits":[{"op":"set","path":["server","port"],"value":1}]}"#);
        let outcome = apply_edit_plan(source, &edit_plan).unwrap();
        assert_eq!(outcome.source, "server {\r\n  port = 1\r\n}\r\n");
    }

    #[test]
    fn set_refuses_when_expectation_does_not_match() {
        let source = "port = 8080\n";
        let edit_plan = plan(
            r#"{"version":"1.0","edits":[{"op":"set","path":["port"],"value":9090,"expect":{"value":1234}}]}"#,
        );
        let refusal = apply_edit_plan(source, &edit_plan).expect_err("前置条件不符必须拒绝");
        match refusal {
            EditRefusal::ExpectationMismatch {
                expected, actual, ..
            } => {
                assert_eq!(expected, "1234");
                assert_eq!(actual, "8080");
            }
            other => panic!("期望 ExpectationMismatch，实际 {:?}", other),
        }
    }

    #[test]
    fn set_refuses_unknown_target() {
        let source = "port = 8080\n";
        let edit_plan =
            plan(r#"{"version":"1.0","edits":[{"op":"set","path":["missing"],"value":1}]}"#);
        let refusal = apply_edit_plan(source, &edit_plan).expect_err("目标不存在必须拒绝");
        assert!(matches!(refusal, EditRefusal::TargetNotFound { .. }));
    }

    #[test]
    fn named_list_targeting_uses_names_and_refuses_ambiguity() {
        let source = "[[servers]]\nname = \"primary\"\nip = \"10.0.0.1\"\n\n[[servers]]\nname = \"secondary\"\nip = \"10.0.0.2\"\n";
        let unique = plan(
            r#"{
              "version": "1.0",
              "edits": [
                { "op": "set", "path": [{ "key": "servers", "match": { "name": "secondary" } }, "ip"], "value": "10.0.0.9" }
              ]
            }"#,
        );
        let outcome = apply_edit_plan(source, &unique).expect("唯一命中应当成功");
        assert!(outcome
            .source
            .contains("name = \"secondary\"\nip = \"10.0.0.9\""));
        assert!(outcome
            .source
            .contains("name = \"primary\"\nip = \"10.0.0.1\""));

        let no_match = plan(
            r#"{
              "version": "1.0",
              "edits": [
                { "op": "set", "path": [{ "key": "servers", "match": { "ip": "10.0.0.99" } }, "ip"], "value": "x" }
              ]
            }"#,
        );
        assert!(matches!(
            apply_edit_plan(source, &no_match).unwrap_err(),
            EditRefusal::TargetNotFound { .. }
        ));

        let duplicate_ips = "[[servers]]\nname = \"a\"\nip = \"10.0.0.1\"\n\n[[servers]]\nname = \"b\"\nip = \"10.0.0.1\"\n";
        let ambiguous_ip = plan(
            r#"{
              "version": "1.0",
              "edits": [
                { "op": "set", "path": [{ "key": "servers", "match": { "ip": "10.0.0.1" } }, "name"], "value": "c" }
              ]
            }"#,
        );
        assert!(matches!(
            apply_edit_plan(duplicate_ips, &ambiguous_ip).unwrap_err(),
            EditRefusal::TargetAmbiguous { matches: 2, .. }
        ));
    }

    #[test]
    fn insert_adds_one_line_and_keeps_the_rest_intact() {
        let source = "server {\n  host = \"localhost\"\n}\n";
        let edit_plan = plan(
            r#"{"version":"1.0","edits":[{"op":"insert","path":["server","port"],"value":8080,"reason":"补齐端口"}]}"#,
        );
        let outcome = apply_edit_plan(source, &edit_plan).unwrap();
        assert_eq!(
            outcome.source,
            "server {\n  host = \"localhost\"\n  port = 8080\n}\n"
        );
    }

    #[test]
    fn insert_at_root_appends_after_existing_entries() {
        let source = "a = 1\n";
        let edit_plan =
            plan(r#"{"version":"1.0","edits":[{"op":"insert","path":["b"],"value":"two"}]}"#);
        let outcome = apply_edit_plan(source, &edit_plan).unwrap();
        assert_eq!(outcome.source, "a = 1\nb = \"two\"\n");
    }

    #[test]
    fn insert_refuses_when_target_already_exists() {
        let source = "port = 8080\n";
        let edit_plan =
            plan(r#"{"version":"1.0","edits":[{"op":"insert","path":["port"],"value":1}]}"#);
        assert!(matches!(
            apply_edit_plan(source, &edit_plan).unwrap_err(),
            EditRefusal::TargetAlreadyExists { .. }
        ));
    }

    #[test]
    fn delete_removes_only_the_target_line() {
        let source = "# 注释\nkeep = 1\nremove = 2 #@ sensitive\nkeep2 = 3\n";
        let edit_plan = plan(
            r#"{"version":"1.0","edits":[{"op":"delete","path":["remove"],"expect":{"value":2}}]}"#,
        );
        let outcome = apply_edit_plan(source, &edit_plan).unwrap();
        assert_eq!(outcome.source, "# 注释\nkeep = 1\nkeep2 = 3\n");
    }

    #[test]
    fn delete_refuses_when_target_missing() {
        let source = "keep = 1\n";
        let edit_plan = plan(r#"{"version":"1.0","edits":[{"op":"delete","path":["gone"]}]}"#);
        assert!(matches!(
            apply_edit_plan(source, &edit_plan).unwrap_err(),
            EditRefusal::TargetNotFound { .. }
        ));
    }

    #[test]
    fn invalid_plan_is_rejected_before_touching_the_source() {
        let source = "port = 8080\n";
        let edit_plan = EditPlan {
            version: "9.9".to_string(),
            file: None,
            edits: vec![EditIntent {
                op: EditOp::Delete,
                path: vec![PathSegment::Key("port".to_string())],
                value: None,
                reason: None,
                expect: None,
            }],
        };
        assert!(matches!(
            apply_edit_plan(source, &edit_plan).unwrap_err(),
            EditRefusal::InvalidPlan(_)
        ));
    }

    #[test]
    fn edits_are_applied_in_order_and_reported() {
        let source = "server {\n  port = 8080\n}\n";
        let edit_plan = plan(
            r#"{
              "version": "1.0",
              "edits": [
                { "op": "set", "path": ["server", "port"], "value": 9090, "reason": "改端口" },
                { "op": "insert", "path": ["server", "host"], "value": "0.0.0.0", "reason": "补主机" }
              ]
            }"#,
        );
        let outcome = apply_edit_plan(source, &edit_plan).unwrap();
        assert_eq!(
            outcome.source,
            "server {\n  port = 9090\n  host = \"0.0.0.0\"\n}\n"
        );
        assert_eq!(outcome.applied.len(), 2);
        assert_eq!(outcome.applied[0].path, "server.port");
        assert_eq!(outcome.applied[0].reason.as_deref(), Some("改端口"));
        assert_eq!(outcome.applied[1].op, EditOp::Insert);
    }

    #[test]
    fn applied_plan_is_idempotent_when_repeated_with_expectations() {
        let source = "server {\n  port = 8080\n}\n";
        let first = plan(
            r#"{"version":"1.0","edits":[{"op":"set","path":["server","port"],"value":9090,"expect":{"value":8080}}]}"#,
        );
        let outcome = apply_edit_plan(source, &first).unwrap();

        // 再次应用同一计划必须因为前置条件不符而拒绝，而不是静默重复改
        assert!(matches!(
            apply_edit_plan(&outcome.source, &first).unwrap_err(),
            EditRefusal::ExpectationMismatch { .. }
        ));
    }

    #[test]
    fn validation_gate_rejects_edits_that_break_the_schema() {
        let source = "#@schema {\n  port {\n    type = \"integer\"\n  }\n}\n\nport = 8080\n";
        let edit_plan = plan(
            r#"{"version":"1.0","edits":[{"op":"set","path":["port"],"value":"not-a-number"}]}"#,
        );
        let refusal = apply_edit_plan(source, &edit_plan).expect_err("类型不符必须拒绝");
        assert!(matches!(refusal, EditRefusal::ValidationFailed { .. }));
    }

    #[test]
    fn security_gate_rejects_newly_introduced_high_risk_findings() {
        let source = "ssl_verify = true\n";
        let baseline = AuditEngine::new().audit_source(source);
        assert_eq!(high_risk_instances(&baseline).len(), 0, "基线不应有高危项");

        let edit_plan = plan(
            r#"{"version":"1.0","edits":[{"op":"set","path":["ssl_verify"],"value":false,"reason":"临时关闭校验"}]}"#,
        );
        match apply_edit_plan(source, &edit_plan).expect_err("引入高危项必须拒绝") {
            EditRefusal::SecurityRejected { findings, .. } => {
                assert!(
                    findings.iter().any(|rule| rule == "SEC-005"),
                    "应指出被引入的 SEC-005，实际 {:?}",
                    findings
                );
            }
            other => panic!("期望 SecurityRejected，实际 {:?}", other),
        }
    }

    #[test]
    fn security_gate_does_not_block_edits_for_preexisting_findings() {
        // 文件本来就有高危项，但只要改动没有引入新的高危项就应当放行
        let source = "ssl_verify = false\nport = 8080\n";
        let baseline = high_risk_instances(&AuditEngine::new().audit_source(source));
        assert!(
            baseline.keys().any(|(rule, _)| rule == "SEC-005"),
            "基线应已包含 SEC-005"
        );

        let edit_plan =
            plan(r#"{"version":"1.0","edits":[{"op":"set","path":["port"],"value":9090}]}"#);
        let outcome = apply_edit_plan(source, &edit_plan).expect("不应因既有问题误拒");
        assert_eq!(outcome.source, "ssl_verify = false\nport = 9090\n");
    }

    #[test]
    fn security_gate_rejects_a_second_instance_of_an_already_present_rule() {
        // 回归：只按 rule_id 取集合差时，同一规则在另一个字段新增的风险完全不可见。
        // 这里高危实例实际从 1 个变成 2 个，旧实现会放行。
        let source = "primary_ssl_verify = false\nsecondary_ssl_verify = true\n";
        let baseline = high_risk_instances(&AuditEngine::new().audit_source(source));
        assert_eq!(baseline.len(), 1, "基线应只有 1 个高危实例");

        let edit_plan = plan(
            r#"{"version":"1.0","edits":[{"op":"set","path":["secondary_ssl_verify"],"value":false,"reason":"临时关闭校验"}]}"#,
        );
        match apply_edit_plan(source, &edit_plan).expect_err("第二个实例必须被拒绝") {
            EditRefusal::SecurityRejected {
                findings,
                instances,
                ..
            } => {
                assert_eq!(findings, vec!["SEC-005".to_string()]);
                assert_eq!(
                    instances,
                    vec!["SEC-005 @ secondary_ssl_verify".to_string()],
                    "拒绝信息必须指出新增的是哪个实例"
                );
            }
            other => panic!("期望 SecurityRejected，实际 {:?}", other),
        }
    }

    #[test]
    fn security_gate_rejects_every_case_that_adds_a_high_risk_instance() {
        // 反向覆盖：任何新增的高危实例都必须导致拒绝，而不只是 SEC-005
        let cases: [(&str, &str); 4] = [
            (
                "ssl_verify = true\n",
                r#"{"op":"set","path":["ssl_verify"],"value":false}"#,
            ),
            (
                "primary_ssl_verify = false\nsecondary_ssl_verify = true\n",
                r#"{"op":"set","path":["secondary_ssl_verify"],"value":false}"#,
            ),
            (
                "hash = \"sha256\"\n",
                r#"{"op":"set","path":["hash"],"value":"md5"}"#,
            ),
            (
                "port = 8080\n",
                r#"{"op":"insert","path":["api_key"],"value":"sk-live-123"}"#,
            ),
        ];
        for (source, edit) in cases {
            let edit_plan = plan(&format!(r#"{{"version":"1.0","edits":[{edit}]}}"#));
            let refusal = match apply_edit_plan(source, &edit_plan) {
                Ok(outcome) => panic!("应拒绝：{source} + {edit}，实际得到 {:?}", outcome.source),
                Err(refusal) => refusal,
            };
            assert!(
                matches!(refusal, EditRefusal::SecurityRejected { .. }),
                "应拒绝：{source} + {edit}，实际 {refusal:?}"
            );
        }
    }

    #[test]
    fn security_gate_allows_replacing_a_value_that_keeps_the_same_instance() {
        // 既有高危实例仍然存在、但没有新增实例时不能误拒：
        // md5 改成 sha1 仍然是 SEC-001 @ hash 这一个实例
        let source = "hash = \"md5\"\nport = 8080\n";
        let edit_plan =
            plan(r#"{"version":"1.0","edits":[{"op":"set","path":["hash"],"value":"sha1"}]}"#);
        let outcome = apply_edit_plan(source, &edit_plan).expect("未新增实例就不应拒绝");
        assert_eq!(outcome.source, "hash = \"sha1\"\nport = 8080\n");
    }

    #[test]
    fn security_gate_rejects_plaintext_credentials_before_write() {
        let source = "server {\n  host = \"localhost\"\n}\n";
        let edit_plan = plan(
            r#"{
              "version": "1.0",
              "edits": [
                { "op": "insert", "path": ["server", "password"], "value": "hunter2", "reason": "补一个口令" }
              ]
            }"#,
        );
        match apply_edit_plan(source, &edit_plan).expect_err("明文口令必须被拒绝") {
            EditRefusal::SecurityRejected { findings, .. } => {
                assert!(
                    findings.iter().any(|rule| rule == "SEC-SENS-001"),
                    "应指出敏感数据规则，实际 {:?}",
                    findings
                );
            }
            other => panic!("期望 SecurityRejected，实际 {:?}", other),
        }
    }

    #[test]
    fn security_gate_allows_environment_reference_instead_of_a_literal_secret() {
        // 回归：审计的建议就是「改用环境变量」，把这个建议写进配置反而被阻断是反的
        let source = "port = 8080\n";
        let edit_plan = plan(
            r#"{"version":"1.0","edits":[{"op":"insert","path":["api_key"],"value":"${ENV:API_KEY}"}]}"#,
        );
        let outcome = apply_edit_plan(source, &edit_plan).expect("环境变量引用不应被阻断");
        assert!(
            outcome.source.contains("api_key"),
            "实际 {:?}",
            outcome.source
        );
    }

    #[test]
    fn security_gate_allows_numeric_token_budget() {
        // 回归：`max_tokens = 4096` 是数量而不是凭据，不能因为键名含 token 就阻断写入
        let source = "port = 8080\n";
        let edit_plan = plan(
            r#"{"version":"1.0","edits":[{"op":"insert","path":["max_tokens"],"value":4096}]}"#,
        );
        let outcome = apply_edit_plan(source, &edit_plan).expect("数量字段不应被阻断");
        assert!(
            outcome.source.contains("max_tokens") && outcome.source.contains("4096"),
            "实际 {:?}",
            outcome.source
        );
    }

    #[test]
    fn refusal_matrix_covers_every_rejection_reason() {
        let mut messages: Vec<String> = Vec::new();

        // 1. 源码无法解析
        let broken = "server {\n  port = \n}\n";
        let set_port =
            plan(r#"{"version":"1.0","edits":[{"op":"set","path":["server","port"],"value":1}]}"#);
        let refusal = apply_edit_plan(broken, &set_port).expect_err("无法解析必须拒绝");
        assert!(matches!(refusal, EditRefusal::ParseFailed(_)));
        messages.push(refusal.to_string());

        // 2. 路径下钻到标量
        let into_scalar =
            plan(r#"{"version":"1.0","edits":[{"op":"set","path":["port","nested"],"value":1}]}"#);
        let refusal = apply_edit_plan("port = 8080\n", &into_scalar).expect_err("下钻标量必须拒绝");
        assert!(matches!(refusal, EditRefusal::UnsupportedTarget { .. }));
        messages.push(refusal.to_string());

        // 3. 路径最后一段是按名称定位的列表元素
        let list_tail = plan(
            r#"{"version":"1.0","edits":[{"op":"set","path":[{"key":"servers","match":{"name":"a"}}],"value":1}]}"#,
        );
        let refusal = apply_edit_plan("[[servers]]\nname = \"a\"\n", &list_tail)
            .expect_err("末段为列表元素必须拒绝");
        assert!(matches!(refusal, EditRefusal::UnsupportedTarget { .. }));
        messages.push(refusal.to_string());

        // 4. 删除与其它内容共用一行的字段
        let delete_shared_line =
            plan(r#"{"version":"1.0","edits":[{"op":"delete","path":["b"]}]}"#);
        let refusal =
            apply_edit_plan("a = 1 b = 2\n", &delete_shared_line).expect_err("同行删除必须拒绝");
        assert!(matches!(refusal, EditRefusal::UnsupportedTarget { .. }));
        messages.push(refusal.to_string());

        // 5. 当前值无法解析时不能猜测，只能拒绝
        let expect_unparseable = plan(
            r#"{"version":"1.0","edits":[{"op":"set","path":["value"],"value":1,"expect":{"value":2}}]}"#,
        );
        let refusal = apply_edit_plan("value = 10 / 0\n", &expect_unparseable)
            .expect_err("当前值不可解析必须拒绝");
        assert!(matches!(refusal, EditRefusal::ExpectationMismatch { .. }));
        messages.push(refusal.to_string());

        // 每种拒绝都要有可读且互不相同的说明
        for message in &messages {
            assert!(!message.trim().is_empty(), "拒绝说明不能为空");
        }
        let unique: BTreeSet<&String> = messages.iter().collect();
        assert_eq!(unique.len(), messages.len(), "不同拒绝原因应有不同说明");
    }

    #[test]
    fn refused_paths_never_return_a_modified_source() {
        let source = "port = 8080\n";
        let cases = [
            r#"{"version":"1.0","edits":[{"op":"set","path":["missing"],"value":1}]}"#,
            r#"{"version":"1.0","edits":[{"op":"insert","path":["port"],"value":1}]}"#,
            r#"{"version":"1.0","edits":[{"op":"set","path":["port"],"value":9090,"expect":{"value":1}}]}"#,
            r#"{"version":"1.0","edits":[{"op":"delete","path":["missing"]}]}"#,
        ];

        for json in cases {
            let edit_plan = plan(json);
            assert!(
                apply_edit_plan(source, &edit_plan).is_err(),
                "该用例必须被拒绝：{}",
                json
            );
            // 输入是只读的：拒绝路径不可能产生改动
            assert_eq!(source, "port = 8080\n");
        }
    }

    #[test]
    fn range_edit_replaces_exactly_the_given_bytes() {
        let source = "port = 8080\nhost = \"localhost\"\n";
        let start = source.find("8080").expect("示例包含端口");
        let outcome = replace_range(source, start, start + 4, "9090").expect("合法区间应当成功");

        assert_eq!(outcome.source, "port = 9090\nhost = \"localhost\"\n");
        assert_eq!(outcome.applied[0].before, "8080");
        assert_eq!(outcome.applied[0].after, "9090");
        assert_eq!(
            outcome.applied[0].path,
            format!("bytes[{}, {})", start, start + 4)
        );
    }

    #[test]
    fn range_edit_refuses_out_of_bounds_and_split_characters() {
        let source = "port = 8080\n";
        assert!(matches!(
            replace_range(source, 5, 3, "x").unwrap_err(),
            EditRefusal::UnsupportedTarget { .. }
        ));
        assert!(matches!(
            replace_range(source, 0, source.len() + 1, "x").unwrap_err(),
            EditRefusal::UnsupportedTarget { .. }
        ));

        let unicode = "name = \"配置\"\n";
        let inside = unicode.find('配').expect("示例包含多字节字符") + 1;
        assert!(matches!(
            replace_range(unicode, inside, inside + 1, "x").unwrap_err(),
            EditRefusal::UnsupportedTarget { .. }
        ));
    }

    #[test]
    fn range_edit_refuses_when_the_result_breaks_validation() {
        let source = "#@schema {\n  port {\n    type = \"integer\"\n  }\n}\n\nport = 8080\n";
        let start = source.find("8080").expect("示例包含端口");
        assert!(matches!(
            replace_range(source, start, start + 4, "\"x\"").unwrap_err(),
            EditRefusal::ValidationFailed { .. }
        ));
    }

    #[test]
    fn refusal_codes_are_stable_and_unique() {
        let refusals = vec![
            EditRefusal::InvalidPlan(vec![]),
            EditRefusal::ParseFailed("x".to_string()),
            EditRefusal::TargetNotFound {
                path: "a".to_string(),
            },
            EditRefusal::TargetAmbiguous {
                path: "a".to_string(),
                matches: 2,
            },
            EditRefusal::TargetAlreadyExists {
                path: "a".to_string(),
            },
            EditRefusal::ExpectationMismatch {
                path: "a".to_string(),
                expected: "1".to_string(),
                actual: "2".to_string(),
            },
            EditRefusal::UnsupportedTarget {
                path: "a".to_string(),
                reason: "r".to_string(),
            },
            EditRefusal::ValidationFailed {
                path: "a".to_string(),
                message: "m".to_string(),
            },
            EditRefusal::SecurityRejected {
                path: "a".to_string(),
                findings: vec!["SEC-005".to_string()],
                instances: vec!["SEC-005 @ ssl_verify".to_string()],
            },
            EditRefusal::SearchIncomplete {
                path: "a".to_string(),
                files: vec!["b".to_string()],
                limit: 64,
            },
        ];

        let codes: BTreeSet<&str> = refusals.iter().map(EditRefusal::code).collect();
        assert_eq!(
            codes.len(),
            refusals.len(),
            "每个拒绝原因都要有独立稳定的码"
        );
        for refusal in &refusals {
            assert!(!refusal.code().is_empty());
            assert!(refusal.details().is_object(), "结构化细节必须是对象");
        }
    }

    #[test]
    fn version_constant_matches_the_contract() {
        assert_eq!(EDIT_PLAN_VERSION, "1.0");
    }
}

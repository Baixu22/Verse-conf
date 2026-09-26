//! 把同一份编辑意图契约用到真实 TOML 配置上。
//!
//! 解析、定位与渲染都交给 `toml_edit`（成熟 CST 库，保留注释、空白与键序）；
//! 这一层做五件事：
//!
//! 1. 用 `toml_edit` 的 `span()` 定位目标值在原文里的字节区间；
//! 2. 只替换那个区间，区间之外一个字节都不动；
//! 3. 要求结果仍然是合法 TOML；
//! 4. 写入前做与 `.vcf` 路径同等的双重校验：schema 与安全审计（见 [`guard`]）；
//! 5. 失败时返回与 `.vcf` 路径同一套 `EditRefusal`。
//!
//! ## 为什么不用「解析成 `DocumentMut` → 改 → `to_string()`」
//!
//! 那条路会**重新序列化整份文档**。实测 `toml_edit` 0.22 的「解析 → 序列化」往返
//! 对 LF 文件无损，但对 CRLF 文件会把 65 个 CRLF 里的 49 个变成 LF
//! （1461 → 1413 字节），**即使一次都不修改**。所以这里改用 span 级替换：
//! 结果文本一个字节都不经过序列化器，换行符与格式因此不会被动。
//!
//! ## 三种操作的语义（写在明处，不靠 README 的措辞补）
//!
//! - **`set`**：只替换目标值的字节区间。内联表、数组、多行字符串整体算一个值，
//!   所以它们被整体替换、区间之外一个字节不动。整张 `[table]` **不是**值，
//!   用一个标量覆盖它属于重写结构，明确拒绝（`unsupported_target`）。
//! - **`insert`**：在目标表里新增一条 `key = value`。插入点落在该表
//!   **最后一条直接键值**之后；没有直接键值就落在它自己的 `[header]` 之后；
//!   根表没有直接键值就落在文件最前面。缩进复制插入位置那一行，换行符沿用
//!   文件自己的风格。键已存在则 `target_already_exists`。
//!   插入之后会重新解析并核对新键确实落在目标路径上。
//! - **`delete`**：整条删除键值对，范围从键所在行的行首到值所在行的行尾（含换行），
//!   所以跨行的值会被整体删掉。键前面或值后面还有别的内容则拒绝
//!   （行尾注释属于这条键值对，跟着一起删）。删除整张表不支持。
//!
//! ## 路径
//!
//! 两种路径段都支持，语义与 `.vcf` 路径一致：
//!
//! - 普通键（`PathSegment::Key`）；
//! - `[[key]]` 元素的按字段匹配定位（`PathSegment::Named`）：匹配不中报
//!   `target_not_found`，匹配到多个报 `target_ambiguous`，不猜一个元素改。
//!
//! `Named` 不是可选项：真实 TOML 里大量值落在 `[[x]]` 里（Cargo 清单的 `[[bin]]`、
//! regex 测试数据的 `[[test]]`），没有它这些值一个都定位不到——
//! 确认性语料里就有一份文档因此一个任务都出不来。
//!
//! ## 明确不支持的形状
//!
//! - **往内联表里 insert / delete**：内联表里没有「一行」可插，重排要处理逗号，
//!   暂不支持；`set` 内联表里的值仍然可以（那只是替换一段字节）。
//! - **删除整张表**：要连同 `[header]` 与它的小节一起判断，暂不支持。
//! - **路径最后一段是 `Named`**：那是在说「把整个元素换成一个值」，没有明确定义。
//! - **schema 必须旁挂给入**：`.vcf` 把 schema 写在文档里，TOML 没有对应语法，
//!   所以 schema 由调用方通过 [`TomlGuard::schema`] 传入，语法与校验器都与
//!   `.vcf` 相同；不给 schema 就只做结构校验与安全审计，与 `.vcf` 路径
//!   「文档没声明 schema 就不校验 schema」是同一种口径。
//! - **审计覆盖到数组表**：`[[name]]` 元素里的键与普通表里的键走同一条判定路径，
//!   位置标识形如 `name[0].password`。

/// TF-0079：把「编辑机制」与「安全门禁」的贡献分开测的消融实验。
pub mod ablation;
mod ast_bridge;
mod guard;

pub use ast_bridge::toml_to_ast;
pub use guard::{audit_toml, toml_ast, validate_toml_against_schema, TomlGuard};

use std::collections::BTreeMap;
use std::ops::Range;

use toml_edit::{ImDocument, Item, Table, Value as TomlValue};
use verseconf_core::{
    AppliedEdit, EditIntent, EditOp, EditPlan, EditRefusal, EditValue, PathSegment, PlanViolation,
};

/// 一次 TOML 编辑的结果
#[derive(Debug, Clone, PartialEq)]
pub struct TomlOutcome {
    /// 改动后的完整文本
    pub source: String,
    /// 已应用的编辑，顺序与计划一致
    pub applied: Vec<AppliedEdit>,
}

/// 按编辑意图契约改动 TOML 文本，并做与 `.vcf` 路径同等的写入前校验。
///
/// 这是真实写入路径：改动后的文本必须仍然合法，且不得引入新的高危安全实例。
/// 需要按 schema 校验时用 [`apply_toml_edit_with`] 传入 [`TomlGuard::with_schema`]。
pub fn apply_toml_edit(source: &str, plan: &EditPlan) -> Result<TomlOutcome, EditRefusal> {
    apply_toml_edit_with(source, plan, &TomlGuard::default())
}

/// 按显式的校验开关改动 TOML 文本。
///
/// [`TomlGuard::edit_mechanism_only`] 只用于消融实验的对照臂；真实写入用
/// [`apply_toml_edit`] 或 [`TomlGuard::with_schema`]。
pub fn apply_toml_edit_with(
    source: &str,
    plan: &EditPlan,
    guard: &TomlGuard<'_>,
) -> Result<TomlOutcome, EditRefusal> {
    if let Err(violations) = plan.validate() {
        return Err(EditRefusal::InvalidPlan(violations));
    }

    let document = ImDocument::parse(source.to_string())
        .map_err(|error| EditRefusal::ParseFailed(error.to_string()))?;

    let mut planned: Vec<(Range<usize>, AppliedEdit)> = Vec::new();
    for intent in &plan.edits {
        planned.push(plan_one(source, document.as_item(), intent)?);
    }

    let mut order: Vec<usize> = (0..planned.len()).collect();
    order.sort_by_key(|&index| planned[index].0.start);
    for pair in order.windows(2) {
        if planned[pair[0]].0.end > planned[pair[1]].0.start {
            return Err(EditRefusal::InvalidPlan(vec![PlanViolation {
                code: "overlapping_edits".to_string(),
                message: format!(
                    "两条编辑落在同一段字节上：{} 与 {}",
                    planned[pair[0]].1.path, planned[pair[1]].1.path
                ),
                edit_index: None,
            }]));
        }
    }

    // 从后往前替换，前面的区间偏移不受影响
    let mut result = source.to_string();
    for &index in order.iter().rev() {
        result.replace_range(planned[index].0.clone(), &planned[index].1.after);
    }

    // 写入前的双重校验：合法性（含 schema）与「不引入新的高危实例」
    let result = guard::finalize_toml_edit(source, result, guard)?;

    // 插入点的语义必须被验证，而不是被相信
    verify_insertions(&result, plan)?;

    Ok(TomlOutcome {
        source: result,
        applied: planned.into_iter().map(|(_, applied)| applied).collect(),
    })
}

fn plan_one(
    source: &str,
    root: &Item,
    intent: &EditIntent,
) -> Result<(Range<usize>, AppliedEdit), EditRefusal> {
    let path = render_path(&intent.path);

    let Some((last, parent_path)) = intent.path.split_last() else {
        return Err(EditRefusal::InvalidPlan(vec![PlanViolation {
            code: "empty_path".to_string(),
            message: "路径不能为空".to_string(),
            edit_index: None,
        }]));
    };
    let PathSegment::Key(key_name) = last else {
        return Err(unsupported(
            &path,
            "路径最后一段不能是按字段匹配的元素定位：那是在说「把整个元素换成一个值」，没有明确定义",
        ));
    };

    match intent.op {
        EditOp::Set => plan_set(source, root, intent, &path, parent_path, key_name),
        EditOp::Insert => plan_insert(source, root, intent, &path, parent_path, key_name),
        EditOp::Delete => plan_delete(source, root, intent, &path, parent_path, key_name),
    }
}

/// `set`：只替换目标值的字节区间。
///
/// 内联表、数组、多行字符串都是「值」，所以它们整体被替换、区间之外一个字节不动。
/// 但整张 `[table]` 不是值——用一个标量覆盖它等于把一段结构压成一行，那是重写结构
/// 而不是改值，所以明确拒绝，而不是让它落到「改完不合法」上去报一句难懂的话。
fn plan_set(
    source: &str,
    root: &Item,
    intent: &EditIntent,
    path: &str,
    parent_path: &[PathSegment],
    key_name: &str,
) -> Result<(Range<usize>, AppliedEdit), EditRefusal> {
    let Some(value) = intent.value.as_ref() else {
        return Err(missing_value(path, "set"));
    };

    let container = resolve_container(root, parent_path)?;

    let Some((_, item)) = container.table.get_key_value(key_name) else {
        return Err(EditRefusal::TargetNotFound {
            path: path.to_string(),
        });
    };

    if !matches!(item, Item::Value(_)) {
        return Err(EditRefusal::UnsupportedTarget {
            path: path.to_string(),
            reason: "目标是整张表或数组表；set 只替换值，不重写结构".to_string(),
        });
    }

    let current = edit_value_from_toml(item);
    check_expectation(path, intent, current.as_ref())?;

    let span = item.span().ok_or_else(|| EditRefusal::UnsupportedTarget {
        path: path.to_string(),
        reason: "toml_edit 没有为这个值给出字节区间".to_string(),
    })?;

    let applied = AppliedEdit {
        op: EditOp::Set,
        path: path.to_string(),
        before: source[span.clone()].to_string(),
        after: render_value(value),
        reason: intent.reason.clone(),
    };

    Ok((span, applied))
}

/// `insert`：在目标表里新增一条 `key = value`。
///
/// 插入点的语义（这是这个操作唯一需要定义清楚的地方）：
///
/// 1. 落在该表**最后一条直接键值**之后——不能落在最后一条子表之后，
///    否则新键会被那个子表的 `[header]` 带走，跑进别的表里；
/// 2. 表里没有直接键值就落在它自己的 `[header]` 行之后；
/// 3. 根表没有直接键值就落在文件最前面（根键必须排在所有 `[header]` 之前）。
///
/// 缩进复制插入位置所在行的缩进，换行符沿用文件自己的风格。
/// 插入之后会**重新解析并核对**新键确实落在目标路径上（见 `verify_insertions`）：
/// 插入位置算错会让新键悄悄进到另一张表，而结果仍然是合法 TOML。
fn plan_insert(
    source: &str,
    root: &Item,
    intent: &EditIntent,
    path: &str,
    parent_path: &[PathSegment],
    key_name: &str,
) -> Result<(Range<usize>, AppliedEdit), EditRefusal> {
    let Some(value) = intent.value.as_ref() else {
        return Err(missing_value(path, "insert"));
    };

    let container = resolve_container(root, parent_path)?;

    if container.table.get(key_name).is_some() {
        return Err(EditRefusal::TargetAlreadyExists {
            path: path.to_string(),
        });
    }

    let newline = newline_of(source);
    let (insert_at, indent_at) = insertion_point(source, &container);
    let indent = match indent_at {
        Some(offset) => line_indent(source, offset),
        None => String::new(),
    };
    let rendered = render_value(value);

    let mut after = String::new();
    if insert_at > 0 && !source[..insert_at].ends_with('\n') {
        after.push_str(newline);
    }
    after.push_str(&format!("{indent}{key_name} = {rendered}"));
    after.push_str(newline);

    Ok((
        insert_at..insert_at,
        AppliedEdit {
            op: EditOp::Insert,
            path: path.to_string(),
            before: String::new(),
            after,
            reason: intent.reason.clone(),
        },
    ))
}

/// `delete`：整条删除一个键值对。
///
/// 删除范围从键所在行的行首到值所在行的行尾（含换行），所以多行数组、
/// 多行字符串这类跨行的值会被整体删掉，而不是只删掉第一行。
///
/// 两种情形明确拒绝，因为「删掉」会波及其它内容：键前面还有别的东西、
/// 或者值后面还有别的东西（行尾注释属于这条键值对，不算「别的」）。
/// 删除整张表也不支持——那要连同 `[header]` 与小节一起判断。
fn plan_delete(
    source: &str,
    root: &Item,
    intent: &EditIntent,
    path: &str,
    parent_path: &[PathSegment],
    key_name: &str,
) -> Result<(Range<usize>, AppliedEdit), EditRefusal> {
    let container = resolve_container(root, parent_path)?;

    let Some((key, item)) = container.table.get_key_value(key_name) else {
        // 键存在、但它是一张表：这不是「找不到」，而是「不支持删这种形状」。
        // 报成 target_not_found 会让人以为键名写错了。
        if container.table.get(key_name).is_some() {
            return Err(EditRefusal::UnsupportedTarget {
                path: path.to_string(),
                reason: "删除整张表要连同 [header] 与小节一起判断，暂不支持；只支持删除键值对"
                    .to_string(),
            });
        }
        return Err(EditRefusal::TargetNotFound {
            path: path.to_string(),
        });
    };

    let Some(value) = item.as_value() else {
        return Err(EditRefusal::UnsupportedTarget {
            path: path.to_string(),
            reason: "删除整张表要连同 [header] 与小节一起判断，暂不支持；只支持删除键值对"
                .to_string(),
        });
    };

    check_expectation(path, intent, edit_value_from_toml(item).as_ref())?;

    let key_span = key
        .span()
        .ok_or_else(|| unsupported(path, "toml_edit 没有为键给出字节区间"))?;
    let value_span = value
        .span()
        .ok_or_else(|| unsupported(path, "toml_edit 没有为值给出字节区间"))?;

    let start = line_start_of(source, key_span.start);
    let end = line_end_including_newline(source, value_span.end);

    if !source[start..key_span.start].trim().is_empty() {
        return Err(unsupported(
            path,
            "该键与其它内容共用一行，删除会波及其它内容",
        ));
    }
    if !is_blank_or_comment(&source[value_span.end..end]) {
        return Err(unsupported(
            path,
            "该键的值之后还有其它内容，删除会波及其它内容",
        ));
    }

    Ok((
        start..end,
        AppliedEdit {
            op: EditOp::Delete,
            path: path.to_string(),
            before: source[start..end].to_string(),
            after: String::new(),
            reason: intent.reason.clone(),
        },
    ))
}

/// 表形状的容器：文档根表、`[name]` 表，或 `[[key]]` 里被匹配中的那个元素
struct Container<'a> {
    table: &'a Table,
    /// 容器自身的区间；`[name]` 表的区间包含它的 `[header]`
    span: Option<Range<usize>>,
    is_root: bool,
}

/// 把路径解析成「表形状的容器」。
///
/// 支持两种路径段：
///
/// - `Key`：普通键。落点必须是一张表——数组、内联表都不是能插/删的容器。
/// - `Named`：`[[key]]` 里按字段匹配唯一元素。匹配不中报 `target_not_found`，
///   匹配到多个报 `target_ambiguous`，与 `.vcf` 路径同一套语义与拒绝码。
///
/// `Named` 这一段是必须的：真实 TOML 里大量值落在 `[[x]]` 里
/// （Cargo 清单的 `[[bin]]`、regex 测试数据的 `[[test]]`），
/// 没有它这些值一个都定位不到——语料里就有一份文档因此一个任务都出不来。
fn resolve_container<'a>(
    root: &'a Item,
    path: &[PathSegment],
) -> Result<Container<'a>, EditRefusal> {
    let label = render_path(path);

    let Item::Table(root_table) = root else {
        return Err(unsupported(&label, "文档根不是一张表"));
    };
    let mut current = Container {
        table: root_table,
        span: root.span(),
        is_root: true,
    };

    for segment in path {
        current = match segment {
            PathSegment::Key(name) => {
                let Some(item) = current.table.get(name.as_str()) else {
                    return Err(EditRefusal::TargetNotFound { path: label });
                };
                match item {
                    Item::Table(table) => Container {
                        table,
                        span: item.span(),
                        is_root: false,
                    },
                    Item::ArrayOfTables(_) => {
                        return Err(unsupported(
                            &label,
                            &format!("`{name}` 是数组表，要用按字段匹配的元素定位"),
                        ))
                    }
                    _ => {
                        return Err(unsupported(
                            &label,
                            &format!("`{name}` 不是一张表，不能作为插入/删除的容器"),
                        ))
                    }
                }
            }
            PathSegment::Named { key, r#match } => {
                match_array_element(&current, key, r#match, &label)?
            }
        };
    }

    Ok(current)
}

/// 在 `[[key]]` 里按字段匹配唯一元素
fn match_array_element<'a>(
    container: &Container<'a>,
    key: &str,
    expected: &BTreeMap<String, EditValue>,
    label: &str,
) -> Result<Container<'a>, EditRefusal> {
    let Some(item) = container.table.get(key) else {
        return Err(EditRefusal::TargetNotFound {
            path: label.to_string(),
        });
    };
    let Item::ArrayOfTables(array_of_tables) = item else {
        return Err(unsupported(
            label,
            &format!("`{key}` 不是 `[[{key}]]` 数组表，不能按字段匹配"),
        ));
    };

    let mut matched: Vec<&Table> = Vec::new();
    for element in array_of_tables.iter() {
        let hit = expected.iter().all(|(field, want)| {
            element
                .get_key_value(field)
                .and_then(|(_, item)| edit_value_from_toml(item))
                .as_ref()
                == Some(want)
        });
        if hit {
            matched.push(element);
        }
    }

    match matched.len() {
        0 => Err(EditRefusal::TargetNotFound {
            path: label.to_string(),
        }),
        1 => {
            let element = matched[0];
            Ok(Container {
                table: element,
                span: element.span(),
                is_root: false,
            })
        }
        count => Err(EditRefusal::TargetAmbiguous {
            path: label.to_string(),
            matches: count,
        }),
    }
}

/// 按路径取一个**值**条目。路径解析走 `resolve_container`，所以按字段匹配的
/// 元素定位同样可用。
fn value_item_at<'a>(
    root: &'a Item,
    path: &[PathSegment],
) -> Result<Option<&'a Item>, EditRefusal> {
    let Some((last, parent_path)) = path.split_last() else {
        return Ok(None);
    };
    let PathSegment::Key(key_name) = last else {
        return Ok(None);
    };
    let container = resolve_container(root, parent_path)?;
    Ok(container
        .table
        .get_key_value(key_name)
        .map(|(_, item)| item))
}

/// 新键插在哪里：返回 (插入偏移, 用来复制缩进的偏移)
fn insertion_point(source: &str, container: &Container<'_>) -> (usize, Option<usize>) {
    let mut last_value_end = None;
    for (_, item) in container.table.iter() {
        if matches!(item, Item::Value(_)) {
            if let Some(span) = item.span() {
                last_value_end = Some(span.end);
            }
        }
    }
    if let Some(end) = last_value_end {
        return (line_end_including_newline(source, end), Some(end));
    }
    if !container.is_root {
        if let Some(span) = &container.span {
            return (line_end_including_newline(source, span.start), None);
        }
    }
    (0, None)
}

fn missing_value(path: &str, op: &str) -> EditRefusal {
    EditRefusal::InvalidPlan(vec![PlanViolation {
        code: "missing_value".to_string(),
        message: format!("{path} 是 {op}，但没有给出 value"),
        edit_index: None,
    }])
}

fn unsupported(path: &str, reason: &str) -> EditRefusal {
    EditRefusal::UnsupportedTarget {
        path: path.to_string(),
        reason: reason.to_string(),
    }
}

fn check_expectation(
    path: &str,
    intent: &EditIntent,
    current: Option<&EditValue>,
) -> Result<(), EditRefusal> {
    let Some(expected) = intent
        .expect
        .as_ref()
        .and_then(|expect| expect.value.as_ref())
    else {
        return Ok(());
    };
    if current == Some(expected) {
        return Ok(());
    }
    Err(EditRefusal::ExpectationMismatch {
        path: path.to_string(),
        expected: render_value(expected),
        actual: current
            .map(render_value)
            .unwrap_or_else(|| "<无法读取>".to_string()),
    })
}

/// 插入之后重新解析并核对：新键必须真的落在目标路径上、值也确实是它。
///
/// 这一步不能省。插入位置算错会让新键悄悄进到**另一张表**里，而结果仍然是
/// 合法 TOML——后置的「结果合法」校验抓不到这种错，只有按路径读回来才能。
fn verify_insertions(source: &str, plan: &EditPlan) -> Result<(), EditRefusal> {
    if !plan.edits.iter().any(|edit| edit.op == EditOp::Insert) {
        return Ok(());
    }

    let document =
        ImDocument::parse(source.to_string()).map_err(|error| EditRefusal::ValidationFailed {
            path: "<result>".to_string(),
            message: error.to_string(),
        })?;

    for intent in &plan.edits {
        if intent.op != EditOp::Insert {
            continue;
        }
        let path = render_path(&intent.path);
        let item = value_item_at(document.as_item(), &intent.path)?;
        let Some(item) = item else {
            return Err(EditRefusal::ValidationFailed {
                path: "<result>".to_string(),
                message: format!("插入之后 {path} 仍然不存在：插入点算错了"),
            });
        };
        let actual = edit_value_from_toml(item);
        if actual.as_ref() != intent.value.as_ref() {
            return Err(EditRefusal::ValidationFailed {
                path: "<result>".to_string(),
                message: format!("插入之后 {path} 的值不是期望的值：插入点算错了"),
            });
        }
    }

    Ok(())
}

fn line_start_of(source: &str, offset: usize) -> usize {
    source[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0)
}

fn line_end_including_newline(source: &str, offset: usize) -> usize {
    match source[offset..].find('\n') {
        Some(index) => offset + index + 1,
        None => source.len(),
    }
}

/// 文件自己的换行风格。TOML 文档可以整份是 CRLF，插入的行必须跟它一致，
/// 否则一次插入就在文档里制造出混合行尾。
fn newline_of(source: &str) -> &'static str {
    if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn line_indent(source: &str, offset: usize) -> String {
    let start = line_start_of(source, offset);
    source[start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// 值之后只允许空白与行尾注释——行尾注释属于这条键值对，跟着一起删
fn is_blank_or_comment(text: &str) -> bool {
    let trimmed = text.trim_matches(['\r', '\n', ' ', '\t']);
    trimmed.is_empty() || trimmed.starts_with('#')
}

fn render_path(path: &[PathSegment]) -> String {
    path.iter()
        .map(|segment| match segment {
            PathSegment::Key(name) => name.clone(),
            PathSegment::Named { key, r#match } => {
                let rendered: Vec<String> = r#match
                    .iter()
                    .map(|(name, value)| format!("{name}={}", render_value(value)))
                    .collect();
                format!("{key}[{}]", rendered.join(", "))
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// 把 TOML 里读到的值转成契约值；无法一一对应时返回 None（例如日期时间）
///
/// 公开是为了让按字段匹配的元素定位（`PathSegment::Named`）与任务生成器
/// 能用同一份转换，而不是各自写一遍。
pub fn edit_value_from_toml(item: &Item) -> Option<EditValue> {
    match item {
        Item::Value(value) => edit_value_from_toml_value(value),
        _ => None,
    }
}

fn edit_value_from_toml_value(value: &TomlValue) -> Option<EditValue> {
    match value {
        TomlValue::String(text) => Some(EditValue::String(text.value().clone())),
        TomlValue::Integer(number) => Some(EditValue::Integer(*number.value())),
        TomlValue::Float(number) => Some(EditValue::Float(*number.value())),
        TomlValue::Boolean(flag) => Some(EditValue::Bool(*flag.value())),
        // 契约里的值与 JSON 一一对应，没有日期时间这一档
        TomlValue::Datetime(_) => None,
        TomlValue::Array(array) => array
            .iter()
            .map(edit_value_from_toml_value)
            .collect::<Option<Vec<_>>>()
            .map(EditValue::Array),
        TomlValue::InlineTable(table) => {
            let mut map = BTreeMap::new();
            for (key, item) in table.iter() {
                map.insert(key.to_string(), edit_value_from_toml_value(item)?);
            }
            Some(EditValue::Table(map))
        }
    }
}

/// 渲染成 TOML 字面量。刻意走 `toml_edit` 自己的渲染，而不是手写引号与转义。
fn render_value(value: &EditValue) -> String {
    toml_value(value).to_string()
}

fn toml_value(value: &EditValue) -> TomlValue {
    match value {
        EditValue::Bool(flag) => TomlValue::from(*flag),
        EditValue::Integer(number) => TomlValue::from(*number),
        EditValue::Float(number) => TomlValue::from(*number),
        EditValue::String(text) => TomlValue::from(text.as_str()),
        EditValue::Array(items) => {
            let mut array = toml_edit::Array::new();
            for item in items {
                array.push(toml_value(item));
            }
            TomlValue::Array(array)
        }
        EditValue::Table(map) => {
            let mut table = toml_edit::InlineTable::new();
            for (key, item) in map {
                table.insert(key, toml_value(item));
            }
            TomlValue::InlineTable(table)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(json: serde_json::Value) -> EditPlan {
        // 刻意不走 `EditPlan::from_json`：那样会先校验，构造不出「非法计划」的用例
        serde_json::from_value(json).expect("计划应当可解析")
    }

    const CODEX_LIKE: &str = "\
# Codex CLI 配置（片段）
model = \"gpt-5-codex\"
approval_policy = \"on-request\" # 行尾注释

[model_providers.local]
name = \"local\"
base_url = \"http://127.0.0.1:8080/v1\"

[sandbox_workspace_write]
network_access = false
";

    #[test]
    fn a_minimal_set_changes_only_the_target_value() {
        let plan = plan(serde_json::json!({
            "version": "1.0",
            "edits": [{ "op": "set", "path": ["model"], "value": "gpt-5" }]
        }));
        let outcome = apply_toml_edit(CODEX_LIKE, &plan).expect("应当命中");

        assert_eq!(outcome.applied.len(), 1);
        assert_eq!(outcome.applied[0].before, "\"gpt-5-codex\"");
        assert_eq!(outcome.applied[0].after, "\"gpt-5\"");
        assert_eq!(
            outcome.source,
            CODEX_LIKE.replace("\"gpt-5-codex\"", "\"gpt-5\"")
        );
    }

    #[test]
    fn a_nested_table_value_is_reached_by_dotted_path() {
        let plan = plan(serde_json::json!({
            "version": "1.0",
            "edits": [{
                "op": "set",
                "path": ["sandbox_workspace_write", "network_access"],
                "value": true
            }]
        }));
        let outcome = apply_toml_edit(CODEX_LIKE, &plan).expect("应当命中");
        assert_eq!(outcome.applied[0].before, "false");
        assert!(outcome.source.ends_with("network_access = true\n"));
        assert!(outcome.source.contains("# 行尾注释"), "注释必须原样保留");
    }

    #[test]
    fn line_endings_and_comments_are_untouched() {
        let source = "# 顶部注释\r\nmodel = \"a\"\r\n\r\n[t]\r\nx = 1 # 尾注释\r\n";
        let plan = plan(serde_json::json!({
            "version": "1.0",
            "edits": [{ "op": "set", "path": ["t", "x"], "value": 2 }]
        }));
        let outcome = apply_toml_edit(source, &plan).expect("应当命中");
        assert_eq!(outcome.source, source.replace("x = 1", "x = 2"));
        assert!(outcome.source.contains("\r\n"), "CRLF 必须保持");
    }

    #[test]
    fn a_wrong_expect_is_refused_without_changing_anything() {
        let plan = plan(serde_json::json!({
            "version": "1.0",
            "edits": [{
                "op": "set",
                "path": ["model"],
                "value": "gpt-5",
                "expect": { "value": "something-else" }
            }]
        }));
        let refusal = apply_toml_edit(CODEX_LIKE, &plan).expect_err("前置条件不符必须拒绝");
        assert_eq!(refusal.code(), "expectation_mismatch");
    }

    #[test]
    fn a_missing_target_is_refused() {
        let plan = plan(serde_json::json!({
            "version": "1.0",
            "edits": [{ "op": "set", "path": ["nope"], "value": 1 }]
        }));
        let refusal = apply_toml_edit(CODEX_LIKE, &plan).expect_err("目标不存在必须拒绝");
        assert_eq!(refusal.code(), "target_not_found");
    }

    #[test]
    fn a_named_path_on_a_document_without_that_array_table_is_refused() {
        // `[[key]]` 元素的按字段匹配定位已经实现（见 tests/operations.rs），
        // 但这份文档里没有 `servers`，所以要报「找不到」而不是「不支持」
        for op in ["set", "insert", "delete"] {
            let mut edit = serde_json::json!({
                "op": op,
                "path": [{ "key": "servers", "match": { "name": "primary" } }, "port"]
            });
            if op != "delete" {
                edit["value"] = serde_json::json!(1);
            }
            let plan = plan(serde_json::json!({ "version": "1.0", "edits": [edit] }));
            let refusal = apply_toml_edit(CODEX_LIKE, &plan).expect_err("必须明确拒绝");
            assert_eq!(refusal.code(), "target_not_found", "{op} 应当报找不到");
        }
    }

    #[test]
    fn a_value_that_needs_escaping_is_still_valid_toml() {
        // 值里带换行：渲染必须转义成合法字面量，而不是把文档拆坏
        let plan = plan(serde_json::json!({
            "version": "1.0",
            "edits": [{ "op": "set", "path": ["model"], "value": "a\nb" }]
        }));
        let outcome = apply_toml_edit(CODEX_LIKE, &plan).expect("转义后应当合法");

        let document = ImDocument::parse(outcome.source.clone()).expect("结果必须仍是合法 TOML");
        assert_eq!(
            document.as_item().get("model").and_then(Item::as_str),
            Some("a\nb")
        );
        assert!(
            outcome.source.contains("# Codex CLI 配置（片段）"),
            "其余字节不能动"
        );
    }

    #[test]
    fn a_plan_that_is_not_valid_is_refused_before_touching_the_text() {
        let plan = plan(serde_json::json!({
            "version": "9.9",
            "edits": [{ "op": "set", "path": ["model"], "value": "x" }]
        }));
        let refusal = apply_toml_edit(CODEX_LIKE, &plan).expect_err("契约版本不对必须拒绝");
        assert_eq!(refusal.code(), "invalid_plan");
    }

    #[test]
    fn multiple_edits_are_applied_without_shifting_each_other() {
        let plan = plan(serde_json::json!({
            "version": "1.0",
            "edits": [
                { "op": "set", "path": ["model"], "value": "short" },
                { "op": "set", "path": ["approval_policy"], "value": "never" }
            ]
        }));
        let outcome = apply_toml_edit(CODEX_LIKE, &plan).expect("应当命中");
        assert_eq!(outcome.applied.len(), 2);
        assert!(outcome.source.contains("model = \"short\"\n"));
        assert!(outcome
            .source
            .contains("approval_policy = \"never\" # 行尾注释"));
    }
}

//! 按点号路径定位 JSON / JSONC 里的值，并把改动脉络化成一次字节区间替换。
//!
//! 三种操作里只有 `set` 是「替换一个值的字节区间」；`insert` 与 `delete` 需要
//! 新增或移除一小段文本。三个操作共用同一条不变量，这也是保真的来源：
//!
//! > **区间之外的字节一个都不动。**
//!
//! 注释、键序、缩进、换行风格、尾随逗号因此都不需要被「保住」——它们从来
//! 没被重建过。这与 `verseconf-toml` 的选择同源（那边拒绝「解析成
//! `DocumentMut` → 改 → `to_string()`」，因为重新序列化会改写 CRLF 与格式）。
//!
//! 定位用的是 `jsonc_parser` 的 AST 区间（`Ranged`），不自己数括号、不自己
//! 找逗号：自己数括号的实现会在字符串里的 `{`、注释里的 `,` 上出错，而这类
//! 错误在良性输入上看不出来。
//!
//! ## 明确不支持的形状
//!
//! - **路径最后一段是 `Named`**：那是在说「把整个元素换成一个值」，没有明确
//!   定义，拒绝；
//! - **路径穿过非对象的值**（`{"a": 1}` 再往下走 `a.b`）：拒绝，不是静默返回
//!   `target_not_found`——把「结构不对」和「键不存在」混成同一个码，调用方
//!   就没法区分「换个路径」和「先补结构」；
//! - **按下标取值**（`servers[0].port`）：点号路径没有表达下标的段。
//!   `PathSegment::Named` 是「按字段匹配元素」，能表达「在命名列表里定位
//!   唯一元素再取字段」，不能表达按下标取值。

use std::collections::BTreeMap;
use std::ops::Range;

use jsonc_parser::ast as jast;
use jsonc_parser::common::Ranged;
use jsonc_parser::{parse_to_ast, CollectOptions, ParseOptions};

use verseconf_core::{
    describe_path, EditIntent, EditOp, EditRefusal, EditValue, PathSegment, PlanViolation,
};

use crate::guard::{finalize_json_edit, JsonGuard};
use crate::JsonFlavor;

/// 一次 JSON 编辑的结果
#[derive(Debug, Clone, PartialEq)]
pub struct JsonOutcome {
    /// 改动后的完整文本
    pub source: String,
    /// 被替换掉的原文片段（用于记录改动前后）
    pub before: String,
    /// 替换上去的文本
    pub after: String,
}

/// 区间编辑原语：把 `[start, end)` 这段字节替换成 `replacement`，然后做写入前校验。
///
/// 这是宿主的**任意**编辑方式与门禁之间的交汇点：宿主可以随便改，但落盘前把
/// 「基线 + 区间 + 替换文本」交给这一层，得到的裁决与编辑路径完全一致。
pub fn replace_json_range(
    source: &str,
    start: usize,
    end: usize,
    replacement: &str,
    flavor: JsonFlavor,
) -> Result<JsonOutcome, EditRefusal> {
    let invalid = |reason: &str| EditRefusal::UnsupportedTarget {
        path: format!("bytes[{start}, {end})"),
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

    let guard = JsonGuard {
        flavor,
        ..JsonGuard::default()
    };
    let candidate = finalize_json_edit(source, &candidate, &guard)?;

    Ok(JsonOutcome {
        source: candidate,
        before,
        after: replacement.to_string(),
    })
}

/// 定位「某个字段的值」在源码里的字节区间。
///
/// 找不到目标或有歧义时返回与编辑路径相同的拒绝原因。
pub fn value_span_for_json_path(
    source: &str,
    path: &[PathSegment],
    flavor: JsonFlavor,
) -> Result<(usize, usize), EditRefusal> {
    let path_label = describe_path(path);
    let Some((last, parents)) = path.split_last() else {
        return Err(EditRefusal::InvalidPlan(vec![PlanViolation::new(
            "empty_path",
            "路径不能为空",
            None,
        )]));
    };
    let PathSegment::Key(key) = last else {
        return Err(unsupported(
            &path_label,
            "路径最后一段不能是按字段匹配的元素定位",
        ));
    };

    let target = locate(source, flavor, parents, key, &path_label)?;
    let range = target
        .value_range()
        .ok_or(EditRefusal::TargetNotFound { path: path_label })?;
    Ok((range.start, range.end))
}

/// 按路径把某个字段的值替换掉，并做与 `.vcf` 路径同源的写入前校验。
///
/// 这是便于宿主使用的便捷入口：它只替换目标值的字节区间，其余字节不动。
/// 需要旁挂 schema 或显式关掉审计时用 [`set_json_value_with`]。
pub fn set_json_value(
    source: &str,
    path: &[PathSegment],
    value: &EditValue,
    flavor: JsonFlavor,
) -> Result<JsonOutcome, EditRefusal> {
    set_json_value_with(
        source,
        path,
        value,
        &JsonGuard {
            flavor,
            ..JsonGuard::default()
        },
    )
}

/// [`set_json_value`] 的完整形态：可以旁挂 schema，也可以显式关掉审计。
pub fn set_json_value_with(
    source: &str,
    path: &[PathSegment],
    value: &EditValue,
    guard: &JsonGuard<'_>,
) -> Result<JsonOutcome, EditRefusal> {
    let intent = EditIntent {
        op: EditOp::Set,
        path: path.to_vec(),
        value: Some(value.clone()),
        reason: None,
        expect: None,
    };
    apply_json_intent(source, &intent, guard)
}

/// 执行一条编辑意图（`set` / `insert` / `delete`），并做写入前校验。
///
/// 这是本层的编辑入口。它与 [`crate::check_write_json`] 共用同一份
/// [`finalize_json_edit`]，所以「编辑路径」与「独立门禁」对同一对
/// (baseline, candidate) 的裁决一定相同——两套规则分叉的那一天，
/// 「写入前双重校验」这句话就不再成立。
///
/// 与 `verseconf-toml::apply_toml_edit` 的差别只有一处：JSON 没有 `[[table]]`
/// 那种「表头 + 小节」的结构，所以没有「插入点可能被下一张表的表头带走」这个
/// 风险，也就不需要插入后的重新核对。
pub fn apply_json_edit_plan(
    source: &str,
    intent: &EditIntent,
    guard: &JsonGuard<'_>,
) -> Result<JsonOutcome, EditRefusal> {
    apply_json_intent(source, intent, guard)
}

pub(crate) fn apply_json_intent(
    source: &str,
    intent: &EditIntent,
    guard: &JsonGuard<'_>,
) -> Result<JsonOutcome, EditRefusal> {
    let path_label = describe_path(&intent.path);

    let Some((last, parents)) = intent.path.split_last() else {
        return Err(EditRefusal::InvalidPlan(vec![PlanViolation::new(
            "empty_path",
            "路径不能为空",
            None,
        )]));
    };
    let PathSegment::Key(key) = last else {
        return Err(unsupported(
            &path_label,
            "路径最后一段不能是按字段匹配的元素定位：那是在说「把整个元素换成一个值」，没有明确定义",
        ));
    };

    let target = locate(source, guard.flavor, parents, key, &path_label)?;
    // 先判「目标在不在」，再判别的：`insert` 撞上已存在的键与 `set`/`delete`
    // 找不到键是两种不同的失败，调用方需要按码分支处理
    match intent.op {
        EditOp::Insert if target.index.is_some() => {
            return Err(EditRefusal::TargetAlreadyExists {
                path: path_label.clone(),
            })
        }
        EditOp::Set | EditOp::Delete if target.index.is_none() => {
            return Err(EditRefusal::TargetNotFound {
                path: path_label.clone(),
            })
        }
        _ => {}
    }
    check_expectation(&path_label, intent, target.current.as_ref())?;

    let (range, after) = match intent.op {
        EditOp::Set => (
            target.value_range().ok_or(EditRefusal::TargetNotFound {
                path: path_label.clone(),
            })?,
            render_value(required_value(intent, &path_label)?),
        ),
        EditOp::Insert => {
            let (at, text) = target
                .insert_at(
                    source,
                    key,
                    &render_value(required_value(intent, &path_label)?),
                )
                .ok_or_else(|| {
                    unsupported(
                        &path_label,
                        "最后一条属性行尾有注释：单次拼接只能把新属性插在注释之后，那会把这段注释挪到新属性那一行；\
                         要插在注释之前需要两次互不相邻的拼接。本层不猜注释归属，请先自行处理该行",
                    )
                })?;
            (at, text)
        }
        EditOp::Delete => {
            let range = target
                .delete_range(source)
                .ok_or(EditRefusal::TargetNotFound {
                    path: path_label.clone(),
                })?;
            // 空区间 = 「成功，但原文一个字节都没变」。那比报错更坏：
            // 调用方会以为删掉了。所以这里宁可拒绝。
            if range.is_empty() {
                return Err(unsupported(
                    &path_label,
                    "删除范围为空：无法在不影响相邻内容的前提下删除这条属性",
                ));
            }
            (range, String::new())
        }
    };

    let mut candidate = String::with_capacity(source.len() + after.len());
    candidate.push_str(&source[..range.start]);
    candidate.push_str(&after);
    candidate.push_str(&source[range.end..]);

    let candidate = finalize_json_edit(source, &candidate, guard)?;

    Ok(JsonOutcome {
        source: candidate,
        before: source[range].to_string(),
        after,
    })
}

/// 路径定位的结果。
///
/// 只抄**区间**，不抄整棵树：区间与契约值都是拥有所有权的类型，所以解析
/// 结果可以立刻丢掉，不需要把整条调用链塞进闭包，也不需要借用原文。
#[derive(Default)]
struct Located {
    /// 目标所在对象的字节区间（`{` 到 `}`）
    object_range: Option<Range<usize>>,
    /// 对象里每条属性的（属性区间，值区间），按原文键序排列
    properties: Vec<(Range<usize>, Range<usize>)>,
    /// 目标是对象里的哪一条属性；不存在时是 `None`
    index: Option<usize>,
    /// 目标当前的值，用于 `expect` 比较
    current: Option<EditValue>,
}

impl Located {
    fn property_range(&self) -> Option<Range<usize>> {
        self.index
            .and_then(|index| self.properties.get(index))
            .map(|(property, _)| property.clone())
    }

    fn value_range(&self) -> Option<Range<usize>> {
        self.index
            .and_then(|index| self.properties.get(index))
            .map(|(_, value)| value.clone())
    }

    /// 插入点与要插入的文本。
    ///
    /// 多行对象：新属性另起一行，缩进沿用最后一条属性所在行的缩进；
    /// 单行对象：跟在最后一个值后面，不引入换行；空对象：按它自己的写法填。
    ///
    /// 最后一条属性行尾有注释时拒绝（`None`）。
    ///
    /// 这里说清楚**为什么**拒绝，免得理由比事实更大：单次拼接确实能把新属性
    /// 插进去（把 `, "b": 2` 拼在值的末尾），但那样这段注释就会从原来那条属性
    /// 的行尾挪到新属性那一行——注释的归属被悄悄改了。把逗号插在注释**之前**
    /// 需要两次互不相邻的拼接，本层一次只做一处替换。与其猜注释归谁，
    /// 不如明确拒绝并说明。
    fn insert_at(&self, source: &str, key: &str, rendered: &str) -> Option<(Range<usize>, String)> {
        let object = self.object_range.clone()?;
        let newline = newline_of(source);
        let multi_line = source[object.clone()].contains('\n');
        let quoted = render_key(key);

        match self.properties.last() {
            Some((property, value)) if multi_line => {
                if line_has_comment_after(source, value.end, object.end) {
                    return None;
                }
                let indent = indent_of_line(source, property.start);
                // 已有尾随逗号：新属性要插在**逗号之后**，否则那个逗号会变成
                // 新属性与 `}` 之间的分隔符，而上一条属性后面反而没有逗号
                match comma_after(source, value.end).filter(|comma| *comma < object.end) {
                    Some(comma) => Some((
                        comma + 1..comma + 1,
                        format!("{newline}{indent}{quoted}: {rendered}"),
                    )),
                    None => Some((
                        value.end..value.end,
                        format!(",{newline}{indent}{quoted}: {rendered}"),
                    )),
                }
            }
            Some((_, value)) => {
                if line_has_comment_after(source, value.end, object.end) {
                    return None;
                }
                match comma_after(source, value.end).filter(|comma| *comma < object.end) {
                    Some(comma) => Some((comma + 1..comma + 1, format!(" {quoted}: {rendered}"))),
                    None => Some((value.end..value.end, format!(", {quoted}: {rendered}"))),
                }
            }
            None if multi_line => {
                // 多行空对象：属性落在 `{` 与 `}` 中间那一行，
                // 缩进取右花括号那一行的缩进再深一级
                let indent = format!("{}  ", indent_of_line(source, object.end));
                let at = object.end.saturating_sub(1);
                Some((at..at, format!("{indent}{quoted}: {rendered}{newline}")))
            }
            None => Some((
                object.start + 1..object.start + 1,
                format!("{quoted}: {rendered}"),
            )),
        }
    }

    /// 删除范围：整条属性，以及它周围的逗号与空白。
    ///
    /// 只删属性区间会留下孤立的逗号（`{ , "b": 2 }`）；删多了会把别的属性一起带走。
    /// 所以分两种形状，判据是「这条属性是否独占一行」：
    ///
    /// - **独占一行**：删整行（从行首起，含缩进），下一个属性留在自己的行上；
    ///   最后一条则删到 `}` 那一行之前，并带走前一条的逗号。
    /// - **和别人同一行**（单行对象，或 `"a": 1, "b": 2` 挤在一行）：没有「行」
    ///   可删，只能按逗号归属删——不是最后一条就连自己的逗号与逗号后的空白一起删；
    ///   是最后一条就连前一条的逗号一起删。
    ///
    /// 第二种形状曾经漏掉，代价是两个真实缺陷：单行对象上删中间键会算出反向区间
    /// （`start > end`）并产出损坏文本，而「独占行的属性与下一个属性同行」会算出
    /// 空区间、删除变成静默 no-op——调用方拿到的是「成功且原文没变」。
    fn delete_range(&self, source: &str) -> Option<Range<usize>> {
        let index = self.index?;
        let property = self.property_range()?;
        let object_range = self.object_range.clone()?;
        let multi_line = source[object_range.clone()].contains('\n');
        let is_last = index + 1 == self.properties.len();

        if is_last {
            let Some((_, previous)) = index.checked_sub(1).and_then(|i| self.properties.get(i))
            else {
                // 对象里只有这一条：多行对象删掉它那一整行（从行首起，含缩进），
                // 得到 `{\n}` 而不是 `{\n  \n}`；单行对象直接清空内部，得到 `{}`。
                let (start, end) = if multi_line {
                    (line_begin(source, property.start), object_range.end - 1)
                } else {
                    (object_range.start + 1, object_range.end - 1)
                };
                return Some(start..end.max(start));
            };

            // 最后一条没有自己的逗号，带走前一条的
            let mut start = property.start;
            if let Some(comma) = comma_after(source, previous.end) {
                start = comma;
            }
            let end = if multi_line {
                // 留一个换行给上一条属性收尾，否则 `}` 会贴到它后面。
                // `object_range.end` 在 `}` 之后，所以先退回 `}` 本身再退换行。
                strip_one_line_ending(source, object_range.end - 1)
            } else {
                object_range.end - 1
            };
            return Some(start..end.max(start));
        }

        let next = self.properties.get(index + 1)?;
        let shares_line = line_begin(source, next.0.start) == line_begin(source, property.start);
        if shares_line {
            // 同一行：按逗号归属删，不碰行结构
            let end = comma_after(source, property.end)
                .map(|comma| comma + 1)
                .unwrap_or(property.end);
            let end = skip_inline_whitespace(source, end);
            return Some(property.start..end.max(property.start));
        }

        // 独占一行：删到下一个属性所在行的行首，行首缩进一起消失
        Some(property.start..line_start_of(source, next.0.start))
    }
}

/// 跳过空格与制表符（不跨行）。
fn skip_inline_whitespace(source: &str, from: usize) -> usize {
    let bytes = source.as_bytes();
    let mut index = from;
    while index < bytes.len() && (bytes[index] == b' ' || bytes[index] == b'\t') {
        index += 1;
    }
    index
}

/// 去掉 `offset` 前面紧邻的那一个换行（CRLF 算一个），返回新的结束位置。
fn strip_one_line_ending(source: &str, offset: usize) -> usize {
    let bytes = source.as_bytes();
    let mut end = offset.min(bytes.len());
    if end > 0 && bytes[end - 1] == b'\n' {
        end -= 1;
        if end > 0 && bytes[end - 1] == b'\r' {
            end -= 1;
        }
    }
    end
}

/// `offset` 所在行的行首（`\n` 之后第一个字节，可能仍是空白）。
///
/// 与 [`line_start_of`] 的区别：那个跳过行首缩进，返回第一个非空白字符；
/// 这个不跳。要删掉「一整行连同它的缩进」时用这个。
fn line_begin(source: &str, offset: usize) -> usize {
    let offset = offset.min(source.len());
    source[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0)
}

/// 一行里第一个非空白字符的字节位置。
///
/// 注意 `offset` 是「这一行上的某一点」，不一定正好是行首；返回的是它
/// 所在行的第一个非空白字符。找不到非空白字符时返回行尾（含换行）。
fn line_start_of(source: &str, offset: usize) -> usize {
    let line_offset = offset.min(source.len());
    let start = source[..line_offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let bytes = source.as_bytes();
    let mut index = start;
    while index < bytes.len() && (bytes[index] == b' ' || bytes[index] == b'\t') {
        index += 1;
    }
    if index < bytes.len() && bytes[index] != b'\r' && bytes[index] != b'\n' {
        index
    } else {
        start
    }
}

/// 解析文档并把目标信息抄出来。
///
/// 顶层不是对象时明确拒绝：门禁要检查的是一份配置，不是任意 JSON 文档，
/// 而静默返回「空根表」会让门禁对一份完全没被检查过的文本回答「允许」。
fn locate(
    source: &str,
    flavor: JsonFlavor,
    parents: &[PathSegment],
    key: &str,
    path_label: &str,
) -> Result<Located, EditRefusal> {
    let options: ParseOptions = flavor.parse_options();
    // 解析用去掉 BOM 的文本，随后把区间平移回原文（见 `strip_bom`）。
    // 平移这一步不能省：少了它每次编辑都会整体错位一个字节。
    let (text, bom) = crate::strip_bom(source);
    let parsed = parse_to_ast(text, &CollectOptions::default(), &options)
        .map_err(|error| EditRefusal::ParseFailed(error.to_string()))?;

    let root = match parsed.value.as_ref() {
        Some(jast::Value::Object(object)) => object,
        Some(_) => {
            return Err(EditRefusal::ParseFailed(
                "顶层不是对象：写前检查需要一份以对象为根的配置".to_string(),
            ))
        }
        None => {
            return Err(EditRefusal::ParseFailed(
                "文档里没有任何值：写前检查需要一份以对象为根的配置".to_string(),
            ))
        }
    };

    let container = resolve_object(root, parents, path_label)?;
    let matched: Vec<usize> = container
        .properties
        .iter()
        .enumerate()
        .filter(|(_, property)| property.name.as_str() == key)
        .map(|(index, _)| index)
        .collect();

    // 同一个键出现两次在 JSONC 里语法合法（后者覆盖前者）。改哪一个没有定义，
    // 而「改第一个」会让用户以为改的是生效的那个，所以按「有歧义」拒绝。
    if matched.len() > 1 {
        return Err(EditRefusal::TargetAmbiguous {
            path: path_label.to_string(),
            matches: matched.len(),
        });
    }
    let index = matched.first().copied();

    let properties: Vec<(Range<usize>, Range<usize>)> = container
        .properties
        .iter()
        .map(|property| {
            (
                property.range.start + bom..property.range.end + bom,
                property.value.range().start + bom..property.value.range().end + bom,
            )
        })
        .collect();

    Ok(Located {
        object_range: Some(container.range.start + bom..container.range.end + bom),
        properties,
        current: index.and_then(|index| edit_value_from_json(&container.properties[index].value)),
        index,
    })
}

fn required_value<'a>(intent: &'a EditIntent, path: &str) -> Result<&'a EditValue, EditRefusal> {
    intent.value.as_ref().ok_or_else(|| missing_value(path))
}

fn missing_value(path: &str) -> EditRefusal {
    EditRefusal::InvalidPlan(vec![PlanViolation::new(
        "missing_value",
        format!("{path} 缺少 value"),
        None,
    )])
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
        .and_then(|expectation| expectation.value.as_ref())
    else {
        return Ok(());
    };
    match current {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => Err(EditRefusal::ExpectationMismatch {
            path: path.to_string(),
            expected: expected.to_text(),
            actual: actual.to_text(),
        }),
        None => Err(EditRefusal::ExpectationMismatch {
            path: path.to_string(),
            expected: expected.to_text(),
            actual: "<无法比较的值>".to_string(),
        }),
    }
}

/// 路径段从左到右穿过嵌套对象。
fn resolve_object<'a>(
    root: &'a jast::Object<'a>,
    segments: &[PathSegment],
    path_label: &str,
) -> Result<&'a jast::Object<'a>, EditRefusal> {
    let mut current = root;

    for segment in segments {
        match segment {
            PathSegment::Key(name) => {
                let property = find_property(current, name).ok_or(EditRefusal::TargetNotFound {
                    path: path_label.to_string(),
                })?;
                current = match &property.value {
                    jast::Value::Object(object) => object,
                    _ => {
                        return Err(unsupported(
                            path_label,
                            &format!("路径段 '{name}' 指向的不是对象，无法继续往下走"),
                        ))
                    }
                };
            }
            PathSegment::Named { key, r#match } => {
                let property = find_property(current, key).ok_or(EditRefusal::TargetNotFound {
                    path: path_label.to_string(),
                })?;
                let array = match &property.value {
                    jast::Value::Array(array) => array,
                    _ => {
                        return Err(unsupported(
                            path_label,
                            &format!("按字段匹配定位的 '{key}' 不是数组"),
                        ))
                    }
                };
                let matches: Vec<&jast::Object<'_>> = array
                    .elements
                    .iter()
                    .filter_map(|element| match element {
                        jast::Value::Object(object) => Some(object),
                        _ => None,
                    })
                    .filter(|object| element_matches(object, r#match))
                    .collect();
                match matches.len() {
                    0 => {
                        return Err(EditRefusal::TargetNotFound {
                            path: path_label.to_string(),
                        })
                    }
                    1 => current = matches[0],
                    count => {
                        return Err(EditRefusal::TargetAmbiguous {
                            path: path_label.to_string(),
                            matches: count,
                        })
                    }
                }
            }
        }
    }

    Ok(current)
}

/// 元素是否满足「按字段匹配」的全部条件。比较用的是契约值，
/// 所以 `1` 与 `"1"` 不会被当成同一个值。
fn element_matches(object: &jast::Object<'_>, r#match: &BTreeMap<String, EditValue>) -> bool {
    r#match.iter().all(|(name, expected)| {
        find_property(object, name)
            .and_then(|property| edit_value_from_json(&property.value))
            .as_ref()
            == Some(expected)
    })
}

fn find_property<'a>(object: &'a jast::Object<'a>, name: &str) -> Option<&'a jast::ObjectProp<'a>> {
    object.properties.iter().find(|p| p.name.as_str() == name)
}

/// JSON 值 → 契约值；无法确定时返回 `None`，调用方必须拒绝而不是猜测。
///
/// `null` 没有对应的契约值形态，所以 `expect` 落在 `null` 上会得到
/// 「无法比较」而不是被当成某个值放行——拒绝优于猜。
fn edit_value_from_json(value: &jast::Value<'_>) -> Option<EditValue> {
    match value {
        jast::Value::StringLit(text) => Some(EditValue::String(text.value.to_string())),
        jast::Value::NumberLit(number) => {
            if let Ok(integer) = number.value.parse::<i64>() {
                return Some(EditValue::Integer(integer));
            }
            number.value.parse::<f64>().ok().map(EditValue::Float)
        }
        jast::Value::BooleanLit(flag) => Some(EditValue::Bool(flag.value)),
        jast::Value::NullKeyword(_) => None,
        jast::Value::Array(array) => {
            let items = array
                .elements
                .iter()
                .map(edit_value_from_json)
                .collect::<Option<Vec<_>>>()?;
            Some(EditValue::Array(items))
        }
        jast::Value::Object(object) => {
            let mut map = BTreeMap::new();
            for property in &object.properties {
                map.insert(
                    property.name.as_str().to_string(),
                    edit_value_from_json(&property.value)?,
                );
            }
            Some(EditValue::Table(map))
        }
    }
}

/// 契约值 → JSON 文本。渲染走 `serde_json`，与 `EditValue` 的序列化形态一致。
fn render_value(value: &EditValue) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

/// 值之后、**本对象结束之前**，这段里是否有注释。
///
/// 两个边界都必须守住：
/// - 从值的末尾往后扫，所以值本身（含字符串里的 `//`）不会被误认；
/// - 扫到对象结尾就停，否则会读进**下一个兄弟属性**的字符串——
///   `{"outer": {"a": 1}, "z": "// not a comment"}` 插入时曾经因此报出
///   「最后一条属性行尾有注释」，而那个 `//` 根本不属于这个对象。
fn line_has_comment_after(source: &str, from: usize, object_end: usize) -> bool {
    let bytes = source.as_bytes();
    let limit = object_end.min(bytes.len());
    let mut index = from;
    while index + 1 < limit {
        match bytes[index] {
            b'\n' => return false,
            b'/' if bytes[index + 1] == b'/' || bytes[index + 1] == b'*' => return true,
            _ => index += 1,
        }
    }
    false
}

/// 从 `from` 开始跳过空白与注释，找第一个 `,` 的字节位置。
fn comma_after(source: &str, from: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut index = from;
    while index < bytes.len() {
        match bytes[index] {
            b' ' | b'\t' | b'\r' | b'\n' => index += 1,
            b',' => return Some(index),
            // 注释：跳到行尾（`//`）或注释块结束（`/* */`）
            b'/' if index + 1 < bytes.len() && bytes[index + 1] == b'/' => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if index + 1 < bytes.len() && bytes[index + 1] == b'*' => {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                index += 2;
            }
            _ => return None,
        }
    }
    None
}

fn newline_of(source: &str) -> &'static str {
    if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// 取 `offset` 所在行行首到 `offset` 之间的空白，作为缩进。
fn indent_of_line(source: &str, offset: usize) -> String {
    let line_start = source[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    source[line_start..offset]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// 键名渲染：`serde_json` 负责转义，所以带引号的键名不会产出非法 JSON。
fn render_key(key: &str) -> String {
    serde_json::to_string(key).unwrap_or_else(|_| format!("\"{key}\""))
}

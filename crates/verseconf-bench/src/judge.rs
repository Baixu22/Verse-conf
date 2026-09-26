//! 判定脚本。
//!
//! 判定只看三件事，且都不读被测策略的自述：
//!
//! 1. **正确**：结果文档里目标字段的值确实等于期望值——直接读 AST，不信任策略的返回值；
//! 2. **附带损伤**：按契约对「值在哪」的定义算出原值区间，改动前后的前缀与后缀必须逐字节相同，
//!    否则就是改动区间之外的字节也变了；
//! 3. **拒绝**：必须给出期望的稳定错误码。拒绝理由不对不算通过，否则"一律拒绝"也能拿满分。

use std::collections::{BTreeMap, BTreeSet};

use verseconf_core::{
    parse, value_span_for_path, ArrayTable, EditValue, KeyValue, PathSegment, TableBlock,
    TableEntry,
};

use crate::strategies::StrategyOutcome;
use crate::{Outcome, Task};

pub struct Judgement {
    pub outcome: Outcome,
    pub detail: String,
    /// 原文里的注释与 `#@` 元数据片段是否全部保留
    pub comments_kept: bool,
}

pub fn judge(task: &Task, source: &str, result: &StrategyOutcome) -> Result<Judgement, String> {
    if task.expects_refusal() {
        Ok(judge_refusal(task, result))
    } else {
        judge_application(task, source, result)
    }
}

fn judge_refusal(task: &Task, result: &StrategyOutcome) -> Judgement {
    let expected = task.expect.code.clone().unwrap_or_default();

    match result {
        StrategyOutcome::Refused { code, message } if *code == expected => Judgement {
            outcome: Outcome::RefusedCorrectly,
            detail: format!("按 {code} 拒绝：{message}"),
            comments_kept: true,
        },
        StrategyOutcome::Refused { code, message } => Judgement {
            outcome: Outcome::Wrong,
            detail: format!("应以 {expected} 拒绝，实际是 {code}：{message}"),
            comments_kept: true,
        },
        StrategyOutcome::Applied(_) => Judgement {
            outcome: Outcome::AppliedWrongly,
            detail: format!("应以 {expected} 拒绝，实际改动了文件"),
            comments_kept: false,
        },
    }
}

fn judge_application(
    task: &Task,
    source: &str,
    result: &StrategyOutcome,
) -> Result<Judgement, String> {
    let StrategyOutcome::Applied(next) = result else {
        let code = result.code().unwrap_or("unknown");
        return Ok(Judgement {
            outcome: Outcome::RefusedWrongly,
            detail: format!("应当改动，实际以 {code} 拒绝"),
            comments_kept: true,
        });
    };

    if task.is_multi_edit() {
        return judge_multi_edit(task, source, next);
    }

    let path = task.expect_path()?;
    let op = task.plan_op()?;

    let ast = match parse(next) {
        Ok(ast) => ast,
        Err(error) => {
            return Ok(Judgement {
                outcome: Outcome::Wrong,
                detail: format!("结果无法解析：{error}"),
                comments_kept: false,
            })
        }
    };

    let actual = read_value_at(&ast.root, &path);

    match op.as_str() {
        // 删除：目标必须已经不存在，且"只是少了一段"
        "delete" => {
            if let Some(actual) = actual {
                return Ok(Judgement {
                    outcome: Outcome::Wrong,
                    detail: format!(
                        "目标 {} 应被删除，实际仍然存在：{}",
                        display_path(&path),
                        actual.to_text()
                    ),
                    comments_kept: comments_kept(source, next),
                });
            }

            let bounds = diff_bounds(source, next);
            let target = render_path(&path);
            if bounds.next_changed_is_empty(next) && key_sets_match(source, next, &target, true) {
                return Ok(Judgement {
                    outcome: Outcome::Correct,
                    detail: "目标已删除，删除区间之外字节零变化".to_string(),
                    // 被删条目自带的注释理应一起消失，所以只要求"保留部分"的注释没丢
                    comments_kept: comments_kept(&bounds.retained_source(source), next),
                });
            }

            Ok(Judgement {
                outcome: Outcome::Collateral,
                detail: describe_bounds_damage(source, next),
                comments_kept: comments_kept(source, next),
            })
        }
        // 插入：目标必须已经存在且值正确，且"原文没有被改动，只是多了一段"
        "insert" => {
            let expected = task
                .expect_value()?
                .ok_or_else(|| format!("任务 {} 声明为 applied 但没有 expect.value", task.id))?;

            if actual.as_ref() != Some(&expected) {
                return Ok(Judgement {
                    outcome: Outcome::Wrong,
                    detail: format!(
                        "目标值应为 {}，实际为 {}",
                        expected.to_text(),
                        actual
                            .map(|value| value.to_text())
                            .unwrap_or_else(|| "<缺失>".to_string())
                    ),
                    comments_kept: comments_kept(source, next),
                });
            }

            let bounds = diff_bounds(source, next);
            let target = render_path(&path);
            if bounds.source_changed_is_empty(source)
                && key_sets_match(source, next, &target, false)
            {
                return Ok(Judgement {
                    outcome: Outcome::Correct,
                    detail: "目标已插入，插入区间之外字节零变化".to_string(),
                    comments_kept: comments_kept(source, next),
                });
            }

            Ok(Judgement {
                outcome: Outcome::Collateral,
                detail: describe_bounds_damage(source, next),
                comments_kept: comments_kept(source, next),
            })
        }
        // 替换：按契约给出的值区间比对前缀与后缀
        _ => {
            let expected = task
                .expect_value()?
                .ok_or_else(|| format!("任务 {} 声明为 applied 但没有 expect.value", task.id))?;

            if actual.as_ref() != Some(&expected) {
                return Ok(Judgement {
                    outcome: Outcome::Wrong,
                    detail: format!(
                        "目标值应为 {}，实际为 {}",
                        expected.to_text(),
                        actual
                            .map(|value| value.to_text())
                            .unwrap_or_else(|| "<缺失>".to_string())
                    ),
                    comments_kept: comments_kept(source, next),
                });
            }

            let damage = match value_span_for_path(source, &path) {
                Ok((start, end)) => {
                    let prefix_ok = next.starts_with(&source[..start]);
                    let suffix_ok = next.ends_with(&source[end..]);
                    (!(prefix_ok && suffix_ok))
                        .then(|| describe_damage(source, next, prefix_ok, suffix_ok))
                }
                Err(error) => Some(format!("无法确定原值区间：{error}")),
            };

            Ok(Judgement {
                outcome: if damage.is_some() {
                    Outcome::Collateral
                } else {
                    Outcome::Correct
                },
                detail: damage
                    .unwrap_or_else(|| "目标值已改对，改动区间之外字节零变化".to_string()),
                comments_kept: comments_kept(source, next),
            })
        }
    }
}

/// 多条编辑计划的判定。
///
/// 单条编辑可以用「前后缀逐字节相同」一次说清，多条编辑不行：改动是分散的，
/// 一个前缀/后缀描述不了。这里换成五条互相独立的检查：
///
/// 1. **每个目标都成立**——`set` / `insert` 的值等于期望值，`delete` 的目标已不存在；
/// 2. **目标之外的字节逐字节保留**——按原文自己的 AST 算出允许改动的区间
///    （`set` 是目标值区间，`delete` 是目标所在整行），区间之外的字节必须原样保留
///    且顺序不变。同名字段改错（根级 `port` 与 `server.port`）、两行交换顺序、
///    CRLF 变化都由这一条抓住；
/// 3. **键集合只差这些目标**——除目标之外，两份文档的字段路径完全一致；
/// 4. **改动的行只能是目标行**——两份文档行多重集的对称差里，每一行都必须提到
///    某个目标的键名；
/// 5. **`set` 目标所在行必须原样保留首尾**——`值之前的部分 + 新值 + 值之后的部分`
///    必须逐字节出现在结果里，行尾注释与 `#@` 元数据因此不能被动。
///
/// 第 2 条要求计划里全部目标是 `set` / `delete`：含 `insert` 时插入点在原文里
/// 不存在，无法推出区间，此时退回第 3～5 条的行级口径。这一点写进了
/// `benchmark/README.md`。
fn judge_multi_edit(task: &Task, source: &str, next: &str) -> Result<Judgement, String> {
    let mut targets = Vec::new();
    for target in &task.expect.targets {
        targets.push((target.path()?, target.value()?));
    }

    let ast = match parse(next) {
        Ok(ast) => ast,
        Err(error) => {
            return Ok(Judgement {
                outcome: Outcome::Wrong,
                detail: format!("结果无法解析：{error}"),
                comments_kept: false,
            })
        }
    };

    // 1) 每个目标都成立
    for (path, expected) in &targets {
        let actual = read_value_at(&ast.root, path);
        match expected {
            Some(expected) => {
                if actual.as_ref() != Some(expected) {
                    return Ok(Judgement {
                        outcome: Outcome::Wrong,
                        detail: format!(
                            "目标 {} 应为 {}，实际为 {}",
                            display_path(path),
                            expected.to_text(),
                            actual
                                .map(|value| value.to_text())
                                .unwrap_or_else(|| "<缺失>".to_string())
                        ),
                        comments_kept: comments_kept(source, next),
                    });
                }
            }
            None => {
                if let Some(actual) = actual {
                    return Ok(Judgement {
                        outcome: Outcome::Wrong,
                        detail: format!(
                            "目标 {} 应被删除，实际仍然存在：{}",
                            display_path(path),
                            actual.to_text()
                        ),
                        comments_kept: comments_kept(source, next),
                    });
                }
            }
        }
    }

    // 2) 目标之外的字节必须逐字节保留
    //
    // 这就是「附带损伤」的定义本身：只有落在目标区间里的字节才允许变。区间由
    // 原文自己的 AST 算出——`set` 是目标值的区间，`delete` 是目标所在的整行。
    // 只看「改动的行是否提到目标键名」挡不住同名字段：根级 `port` 与
    // `server.port` 共用叶子名，改错那个也能通过。
    if let Some(allowed) = allowed_ranges(source, &targets) {
        if let Err(reason) = non_target_bytes_are_untouched(source, next, &allowed) {
            return Ok(Judgement {
                outcome: Outcome::Collateral,
                detail: reason,
                comments_kept: comments_kept(source, next),
            });
        }
    }

    // 3) 键集合只差这些目标
    let target_prefixes: Vec<String> = targets.iter().map(|(path, _)| render_path(path)).collect();
    let mut source_keys = collect_key_paths(source);
    let mut next_keys = collect_key_paths(next);
    for prefix in &target_prefixes {
        source_keys.retain(|key| !key.starts_with(prefix.as_str()));
        next_keys.retain(|key| !key.starts_with(prefix.as_str()));
    }
    if source_keys != next_keys {
        let extra: Vec<&String> = next_keys.difference(&source_keys).collect();
        let missing: Vec<&String> = source_keys.difference(&next_keys).collect();
        return Ok(Judgement {
            outcome: Outcome::Collateral,
            detail: format!("改动落到目标之外：多出 {:?}，少了 {:?}", extra, missing),
            comments_kept: comments_kept(source, next),
        });
    }

    // 4) 改动的行只能是目标行
    let key_names = target_key_names(&targets);
    for line in changed_lines(source, next) {
        if !key_names.iter().any(|name| line.contains(name.as_str())) {
            let shown = if line.trim().is_empty() {
                "(空行)".to_string()
            } else {
                format!("{:?}", line.trim())
            };
            return Ok(Judgement {
                outcome: Outcome::Collateral,
                detail: format!("有目标之外的行被改动：{shown}"),
                comments_kept: comments_kept(source, next),
            });
        }
    }

    // 5) set 目标所在行必须原样保留首尾
    for (path, expected) in &targets {
        let Some(expected) = expected else {
            continue;
        };
        let Ok((start, end)) = value_span_for_path(source, path) else {
            continue;
        };
        let line_start = source[..start]
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0);
        let line_end = source[end..]
            .find('\n')
            .map(|index| end + index)
            .unwrap_or(source.len());
        let expected_line = format!(
            "{}{}{}",
            &source[line_start..start],
            expected.to_text(),
            &source[end..line_end]
        );
        if !next.lines().any(|line| line == expected_line) {
            return Ok(Judgement {
                outcome: Outcome::Collateral,
                detail: format!(
                    "目标 {} 所在行没有原样保留（行尾注释/元数据或缩进被改）：期望 {:?}",
                    display_path(path),
                    expected_line
                ),
                comments_kept: comments_kept(source, next),
            });
        }
    }

    Ok(Judgement {
        outcome: Outcome::Correct,
        detail: format!("{} 个目标全部成立，改动之外的行与字节零变化", targets.len()),
        comments_kept: comments_kept(source, next),
    })
}

/// 每个目标最后一段的键名
/// 允许改动的字节区间（合并、去重、按起点排序）。
///
/// - `set`：目标值自身的区间；
/// - `delete`：目标所在的整行（行级删除会连同键名、分隔符与换行一起去掉）。
///
/// 含 `insert` 时返回 `None`：插入的目标在原文里还不存在，`value_span_for_path`
/// 定位不到，于是判定退回既有的前后缀/键集合口径，而不是猜一个区间。
fn allowed_ranges(
    source: &str,
    targets: &[(Vec<PathSegment>, Option<EditValue>)],
) -> Option<Vec<(usize, usize)>> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for (path, expected) in targets {
        let (start, end) = value_span_for_path(source, path).ok()?;
        if expected.is_some() {
            ranges.push((start, end));
        } else {
            ranges.push(line_range(source, start, end));
        }
    }
    ranges.sort_unstable();

    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    Some(merged)
}

/// 覆盖 `[start, end)` 的整行（含行尾换行）
fn line_range(source: &str, start: usize, end: usize) -> (usize, usize) {
    let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[end..]
        .find('\n')
        .map_or(source.len(), |index| end + index + 1);
    (line_start, line_end)
}

/// 目标区间之外、必须逐字节保留的原文片段
fn kept_segments(source: &str, allowed: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut kept = Vec::new();
    let mut cursor = 0usize;
    for &(start, end) in allowed {
        if start > cursor {
            kept.push((cursor, start));
        }
        cursor = cursor.max(end);
    }
    if cursor < source.len() {
        kept.push((cursor, source.len()));
    }
    kept
}

/// 结果里除目标区间之外的字节必须原样保留、且顺序不变。
///
/// 做法：把原文按目标区间切成若干「必须保留」的片段，再要求在结果里按顺序逐字节
/// 找到它们；结果里片段之间的空隙就是允许改动的位置。首尾片段额外钉死在两端
/// （除非目标区间本身就贴着那一端），所以在文件开头或结尾偷偷加点什么也会被
/// 算成附带损伤。
fn non_target_bytes_are_untouched(
    source: &str,
    next: &str,
    allowed: &[(usize, usize)],
) -> Result<(), String> {
    let kept = kept_segments(source, allowed);
    if kept.is_empty() {
        return Ok(());
    }

    // 写成 match 而不是 `is_none_or`：后者的 MSRV 是 1.82，而本仓库是 1.75
    let pin_head = match allowed.first() {
        Some((start, _)) => *start > 0,
        None => true,
    };
    let pin_tail = match allowed.last() {
        Some((_, end)) => *end < source.len(),
        None => true,
    };

    let mut pos = 0usize;
    let mut end_index = kept.len();
    let mut end_pos = next.len();

    if pin_head {
        let text = &source[kept[0].0..kept[0].1];
        if !next.starts_with(text) {
            return Err(prefix_damage(text));
        }
        pos = text.len();
        if kept.len() == 1 {
            return if !pin_tail || next.len() == text.len() {
                Ok(())
            } else {
                Err(suffix_damage(text))
            };
        }
    }

    if pin_tail {
        let (start, end) = kept[kept.len() - 1];
        let text = &source[start..end];
        if !next.ends_with(text) {
            return Err(suffix_damage(text));
        }
        end_index = kept.len() - 1;
        end_pos = next.len() - text.len();
    }

    let head_consumed = usize::from(pin_head);
    for (start, end) in &kept[head_consumed..end_index] {
        let text = &source[*start..*end];
        if pos > end_pos {
            return Err(missing_damage(text));
        }
        match next[pos..end_pos].find(text) {
            Some(offset) => pos += offset + text.len(),
            None => return Err(missing_damage(text)),
        }
    }

    Ok(())
}

fn prefix_damage(text: &str) -> String {
    format!("目标之外的前缀字节被改动：应原样保留 {:?}", snippet(text))
}

fn suffix_damage(text: &str) -> String {
    format!("目标之外的后缀字节被改动：应原样保留 {:?}", snippet(text))
}

fn missing_damage(text: &str) -> String {
    format!(
        "目标之外的字节被改动或顺序被打乱：找不到 {:?}",
        snippet(text)
    )
}

/// 错误消息里展示的片段：压掉换行并截断，避免把整份配置打进报告
fn snippet(text: &str) -> String {
    let flattened = text.replace(['\r', '\n'], "\\n");
    if flattened.chars().count() <= 40 {
        flattened
    } else {
        let head: String = flattened.chars().take(40).collect();
        format!("{head}…")
    }
}

fn target_key_names(targets: &[(Vec<PathSegment>, Option<EditValue>)]) -> Vec<String> {
    targets
        .iter()
        .filter_map(|(path, _)| {
            path.last().map(|segment| match segment {
                PathSegment::Key(name) => name.clone(),
                PathSegment::Named { key, .. } => key.clone(),
            })
        })
        .collect()
}

/// 行多重集
fn line_multiset(text: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for line in text.lines() {
        *counts.entry(line.to_string()).or_insert(0) += 1;
    }
    counts
}

/// 两份文本行多重集的对称差
fn changed_lines(source: &str, next: &str) -> BTreeSet<String> {
    let before = line_multiset(source);
    let after = line_multiset(next);
    let mut changed = BTreeSet::new();

    for (line, count) in &before {
        if *count > after.get(line).copied().unwrap_or(0) {
            changed.insert(line.clone());
        }
    }
    for (line, count) in &after {
        if *count > before.get(line).copied().unwrap_or(0) {
            changed.insert(line.clone());
        }
    }

    changed
}

/// 原文与结果的最长公共前缀 / 后缀长度。
///
/// 用来判定"纯插入"与"纯删除"：如果原文的每个字节都落在公共前缀或公共后缀里，
/// 说明原文没有被改动，只是多了一段；反过来则是只是少了一段。
///
/// 逐字节比较是安全的：前缀里匹配上的字节在两侧都构成合法 UTF-8，
/// 所以得到的长度一定落在字符边界上。
#[derive(Debug, Clone, Copy)]
struct DiffBounds {
    prefix: usize,
    suffix: usize,
}

impl DiffBounds {
    /// 结果里"多出来"的那一段是否为空（原文被完整覆盖）
    fn source_changed_is_empty(&self, source: &str) -> bool {
        self.prefix + self.suffix >= source.len()
    }

    /// 原文里"少掉"的那一段是否为空（结果被完整覆盖）
    fn next_changed_is_empty(&self, next: &str) -> bool {
        self.prefix + self.suffix >= next.len()
    }

    /// 去掉被改动的那一段之后剩下的原文
    fn retained_source(&self, source: &str) -> String {
        let head = &source[..self.prefix];
        let tail = &source[source.len().saturating_sub(self.suffix)..];
        format!("{head}{tail}")
    }
}

/// 一份文档里所有字段路径的集合（含嵌套表与数组表元素）。
///
/// 用途：最长公共前缀/后缀只能说明"文本上少了一段/多了一段"，挡不住
/// **相邻行的连带删除**——删掉目标行的同时把上一行也删掉，在字节上同样是
/// 一次连续删除。所以还要比对"除目标之外，键的集合有没有变"。
fn collect_key_paths(text: &str) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    if let Ok(ast) = parse(text) {
        walk_keys(&ast.root, "", &mut paths);
    }
    paths
}

fn walk_keys(table: &TableBlock, prefix: &str, out: &mut BTreeSet<String>) {
    for entry in &table.entries {
        match entry {
            TableEntry::KeyValue(kv) => {
                out.insert(join_path(prefix, kv.key.as_str()));
            }
            TableEntry::TableBlock(block) => {
                if let Some(name) = &block.name {
                    let path = join_path(prefix, name);
                    out.insert(path.clone());
                    walk_keys(block, &path, out);
                }
            }
            TableEntry::ArrayTable(array) => {
                let path = join_path(prefix, array.key.as_str());
                out.insert(path.clone());
                // 同名元素用序号区分，避免互相覆盖
                for (index, kv) in array.entries.iter().enumerate() {
                    out.insert(format!("{path}[{index}].{}", kv.key.as_str()));
                }
            }
            _ => {}
        }
    }
}

fn join_path(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

/// 面向消息的可读路径：命名列表元素渲染成 `servers[name="replica"]`
fn display_path(path: &[PathSegment]) -> String {
    let mut out = String::new();
    for segment in path {
        if !out.is_empty() {
            out.push('.');
        }
        match segment {
            PathSegment::Key(name) => out.push_str(name),
            PathSegment::Named { key, r#match } => {
                let rendered: Vec<String> = r#match
                    .iter()
                    .map(|(name, value)| format!("{name}={}", value.to_text()))
                    .collect();
                out.push_str(&format!("{key}[{}]", rendered.join(", ")));
            }
        }
    }
    out
}

/// 把判定用的路径渲染成与 `collect_key_paths` 相同的字符串
fn render_path(path: &[PathSegment]) -> String {
    let mut out = String::new();
    for segment in path {
        match segment {
            PathSegment::Key(name) => {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(name);
            }
            PathSegment::Named { key, .. } => {
                if !out.is_empty() {
                    out.push('.');
                }
                // 命名列表元素在集合里按序号出现，这里用前缀匹配处理
                out.push_str(key);
                out.push('[');
            }
        }
    }
    out
}

/// 除目标之外，键的集合是否完全一致
fn key_sets_match(source: &str, next: &str, target: &str, deleting: bool) -> bool {
    let mut source_keys = collect_key_paths(source);
    let mut next_keys = collect_key_paths(next);

    if deleting {
        source_keys.retain(|path| !path.starts_with(target));
    } else {
        next_keys.retain(|path| !path.starts_with(target));
    }

    source_keys == next_keys
}

fn diff_bounds(source: &str, next: &str) -> DiffBounds {
    let a = source.as_bytes();
    let b = next.as_bytes();

    let mut prefix = 0usize;
    while prefix < a.len() && prefix < b.len() && a[prefix] == b[prefix] {
        prefix += 1;
    }

    let mut suffix = 0usize;
    while suffix < a.len().saturating_sub(prefix)
        && suffix < b.len().saturating_sub(prefix)
        && a[a.len() - 1 - suffix] == b[b.len() - 1 - suffix]
    {
        suffix += 1;
    }

    DiffBounds { prefix, suffix }
}

// ------------------------------------------------------------ 独立的值读取

/// 判定用的容器视图：普通表块，或 `[[key]]` 数组表里的一个元素。
///
/// 刻意不复用编辑实现内部的解析器：判定必须能独立指出"值到底改成了什么"。
enum Container<'a> {
    Table(&'a TableBlock),
    ArrayElement(&'a [KeyValue]),
}

impl<'a> Container<'a> {
    fn key_value(&self, name: &str) -> Option<&'a KeyValue> {
        match self {
            Container::Table(table) => table.entries.iter().find_map(|entry| match entry {
                TableEntry::KeyValue(kv) if kv.key.as_str() == name => Some(kv),
                _ => None,
            }),
            Container::ArrayElement(entries) => entries.iter().find(|kv| kv.key.as_str() == name),
        }
    }

    fn table_block(&self, name: &str) -> Option<&'a TableBlock> {
        match self {
            Container::Table(table) => table.entries.iter().find_map(|entry| match entry {
                TableEntry::TableBlock(block) if block.name.as_deref() == Some(name) => Some(block),
                _ => None,
            }),
            Container::ArrayElement(_) => None,
        }
    }

    fn array_tables(&self, name: &str) -> Vec<&'a ArrayTable> {
        match self {
            Container::Table(table) => table
                .entries
                .iter()
                .filter_map(|entry| match entry {
                    TableEntry::ArrayTable(array) if array.key.as_str() == name => Some(array),
                    _ => None,
                })
                .collect(),
            Container::ArrayElement(_) => Vec::new(),
        }
    }
}

fn read_value_at(root: &TableBlock, path: &[PathSegment]) -> Option<EditValue> {
    let (last, parents) = path.split_last()?;
    let mut container = Container::Table(root);

    for segment in parents {
        container = match segment {
            PathSegment::Key(name) => Container::Table(container.table_block(name)?),
            PathSegment::Named { key, r#match } => {
                let mut hits = container
                    .array_tables(key)
                    .into_iter()
                    .filter(|array| element_matches(array, r#match));
                let first = hits.next()?;
                if hits.next().is_some() {
                    // 命中多个：判定方也无法唯一定位，视为缺失
                    return None;
                }
                Container::ArrayElement(&first.entries)
            }
        };
    }

    match last {
        PathSegment::Key(name) => container
            .key_value(name)
            .and_then(|kv| EditValue::from_ast_value(&kv.value)),
        PathSegment::Named { .. } => None,
    }
}

fn element_matches(array: &ArrayTable, r#match: &BTreeMap<String, EditValue>) -> bool {
    r#match.iter().all(|(key, expected)| {
        array.entries.iter().any(|kv| {
            kv.key.as_str() == key
                && EditValue::from_ast_value(&kv.value).as_ref() == Some(expected)
        })
    })
}

// ------------------------------------------------------------ 附带损伤与注释保真

fn describe_damage(source: &str, next: &str, prefix_ok: bool, suffix_ok: bool) -> String {
    let mut where_changed = Vec::new();
    if !prefix_ok {
        where_changed.push("之前");
    }
    if !suffix_ok {
        where_changed.push("之后");
    }

    format!(
        "改动区间{}的字节也变了：行数 {}→{}，不同行 {}，CRLF {}→{}",
        where_changed.join("与"),
        source.lines().count(),
        next.lines().count(),
        changed_line_count(source, next),
        yes_no(source.contains("\r\n")),
        yes_no(next.contains("\r\n")),
    )
}

/// 插入 / 删除场景下的附带损伤说明
fn describe_bounds_damage(source: &str, next: &str) -> String {
    format!(
        "改动之外的字节也变了：行数 {}→{}，不同行 {}，CRLF {}→{}",
        source.lines().count(),
        next.lines().count(),
        changed_line_count(source, next),
        yes_no(source.contains("\r\n")),
        yes_no(next.contains("\r\n")),
    )
}

fn changed_line_count(left: &str, right: &str) -> usize {
    let left: Vec<&str> = left.lines().collect();
    let right: Vec<&str> = right.lines().collect();
    (0..left.len().max(right.len()))
        .filter(|index| left.get(*index) != right.get(*index))
        .count()
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "是"
    } else {
        "否"
    }
}

/// 注释片段（从 `#` 到行尾，去掉首尾空白）的多重集合。
///
/// 这里用"第一个 `#`"做近似，字符串值里带 `#` 会被算进来；作为保真度指标够用，
/// 精确的字节级保真由附带损伤那一项负责。
fn comment_fragments(text: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for line in text.lines() {
        if let Some(index) = line.find('#') {
            *counts.entry(line[index..].trim().to_string()).or_insert(0) += 1;
        }
    }
    counts
}

fn comments_kept(source: &str, next: &str) -> bool {
    let before = comment_fragments(source);
    let after = comment_fragments(next);
    before
        .iter()
        .all(|(fragment, count)| after.get(fragment).copied().unwrap_or(0) >= *count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Expectation, TargetExpect};
    use serde_json::json;

    const SOURCE: &str = "server {\n  port = 8080 #@ range(1..65535)\n  host = \"0.0.0.0\"\n}\n";

    fn applied_task() -> Task {
        Task {
            id: "t".to_string(),
            document: "d".to_string(),
            description: "改 port".to_string(),
            plan: json!({
                "version": "1.0",
                "edits": [{ "op": "set", "path": ["server", "port"], "value": 9090 }]
            }),
            expect: Expectation {
                outcome: "applied".to_string(),
                path: vec![json!("server"), json!("port")],
                value: Some(json!(9090)),
                code: None,
                targets: Vec::new(),
                target_file: None,
            },
        }
    }

    fn refused_task(code: &str) -> Task {
        Task {
            id: "r".to_string(),
            document: "d".to_string(),
            description: "应当拒绝".to_string(),
            plan: json!({
                "version": "1.0",
                "edits": [{ "op": "set", "path": ["server", "port"], "value": 9090 }]
            }),
            expect: Expectation {
                outcome: "refused".to_string(),
                path: Vec::new(),
                value: None,
                code: Some(code.to_string()),
                targets: Vec::new(),
                target_file: None,
            },
        }
    }

    fn judged(task: &Task, next: &str) -> Judgement {
        judge(task, SOURCE, &StrategyOutcome::Applied(next.to_string())).unwrap()
    }

    #[test]
    fn a_minimal_edit_is_correct() {
        let next = SOURCE.replace("8080", "9090");
        let judgement = judged(&applied_task(), &next);
        assert_eq!(judgement.outcome, Outcome::Correct);
        assert!(judgement.comments_kept);
    }

    #[test]
    fn reformatting_outside_the_value_is_collateral_damage() {
        // 值改对了，但整份文件被重新排版（键序与空白都变了）
        let next = "server {\n  host = \"0.0.0.0\"\n  port = 9090 #@ range(1..65535)\n}\n";
        let judgement = judged(&applied_task(), next);
        assert_eq!(judgement.outcome, Outcome::Collateral);
        assert!(judgement.detail.contains("改动区间"));
    }

    #[test]
    fn dropping_the_inline_metadata_is_collateral_damage() {
        // 行级替换：目标行的注释与元数据被吃掉
        let next = "server {\n  port = 9090\n  host = \"0.0.0.0\"\n}\n";
        let judgement = judged(&applied_task(), next);
        assert_eq!(judgement.outcome, Outcome::Collateral);
        assert!(!judgement.comments_kept);
    }

    #[test]
    fn a_wrong_value_is_a_mis_edit() {
        let next = SOURCE.replace("8080", "1234");
        let judgement = judged(&applied_task(), &next);
        assert_eq!(judgement.outcome, Outcome::Wrong);
        assert!(judgement.detail.contains("实际为 1234"));
    }

    #[test]
    fn an_unparseable_result_is_a_mis_edit() {
        let judgement = judged(&applied_task(), "server {\n  port = \n");
        assert_eq!(judgement.outcome, Outcome::Wrong);
    }

    #[test]
    fn refusing_when_the_task_expects_an_edit_is_flagged() {
        let judgement = judge(
            &applied_task(),
            SOURCE,
            &StrategyOutcome::refused("target_not_found", "找不到"),
        )
        .unwrap();
        assert_eq!(judgement.outcome, Outcome::RefusedWrongly);
    }

    #[test]
    fn refusal_only_counts_when_the_code_matches() {
        let task = refused_task("target_ambiguous");

        let right = judge(
            &task,
            SOURCE,
            &StrategyOutcome::refused("target_ambiguous", "命中 2 个"),
        )
        .unwrap();
        assert_eq!(right.outcome, Outcome::RefusedCorrectly);

        // 拒绝理由不对不算通过，否则"一律拒绝"也能拿满分
        let wrong_code = judge(
            &task,
            SOURCE,
            &StrategyOutcome::refused("target_not_found", "找不到"),
        )
        .unwrap();
        assert_eq!(wrong_code.outcome, Outcome::Wrong);
    }

    #[test]
    fn applying_a_task_that_should_be_refused_is_flagged() {
        let judgement = judged(&refused_task("security_rejected"), SOURCE);
        assert_eq!(judgement.outcome, Outcome::AppliedWrongly);
    }

    fn insert_task(path: serde_json::Value, value: serde_json::Value) -> Task {
        Task {
            id: "i".to_string(),
            document: "d".to_string(),
            description: "插入".to_string(),
            plan: json!({ "version": "1.0", "edits": [{ "op": "insert", "path": path, "value": value }] }),
            expect: Expectation {
                outcome: "applied".to_string(),
                path: path.as_array().cloned().unwrap_or_default(),
                value: Some(value),
                code: None,
                targets: Vec::new(),
                target_file: None,
            },
        }
    }

    fn delete_task(path: serde_json::Value) -> Task {
        Task {
            id: "d".to_string(),
            document: "d".to_string(),
            description: "删除".to_string(),
            plan: json!({ "version": "1.0", "edits": [{ "op": "delete", "path": path }] }),
            expect: Expectation {
                outcome: "applied".to_string(),
                path: path.as_array().cloned().unwrap_or_default(),
                value: None,
                code: None,
                targets: Vec::new(),
                target_file: None,
            },
        }
    }

    #[test]
    fn a_pure_insertion_is_correct() {
        // 原文一个字节没动，只是在末尾多了一段
        let task = insert_task(json!(["added"]), json!("x"));
        let next = format!("{SOURCE}added = \"x\"\n");
        let judgement = judge(&task, SOURCE, &StrategyOutcome::Applied(next)).unwrap();
        assert_eq!(judgement.outcome, Outcome::Correct, "{}", judgement.detail);
    }

    #[test]
    fn reformatting_during_an_insert_is_collateral_damage() {
        // 新键插进去了，但整份文件被重新排版：原文的字节不再是结果的前缀
        let task = insert_task(json!(["added"]), json!("x"));
        let next =
            "server {\n  host = \"0.0.0.0\"\n  port = 8080 #@ range(1..65535)\n}\nadded = \"x\"\n";
        let judgement = judge(&task, SOURCE, &StrategyOutcome::Applied(next.to_string())).unwrap();
        assert_eq!(
            judgement.outcome,
            Outcome::Collateral,
            "{}",
            judgement.detail
        );
    }

    #[test]
    fn an_insert_that_does_not_land_is_a_mis_edit() {
        let task = insert_task(json!(["added"]), json!("x"));
        // 值写错了
        let next = format!("{SOURCE}added = \"y\"\n");
        let judgement = judge(&task, SOURCE, &StrategyOutcome::Applied(next)).unwrap();
        assert_eq!(judgement.outcome, Outcome::Wrong);
    }

    #[test]
    fn a_pure_deletion_is_correct() {
        // 只是少了一段，其余字节原样
        let task = delete_task(json!(["server", "host"]));
        let next = "server {\n  port = 8080 #@ range(1..65535)\n}\n";
        let judgement = judge(&task, SOURCE, &StrategyOutcome::Applied(next.to_string())).unwrap();
        assert_eq!(judgement.outcome, Outcome::Correct, "{}", judgement.detail);
    }

    #[test]
    fn a_deletion_that_leaves_the_value_in_place_is_a_mis_edit() {
        let task = delete_task(json!(["server", "host"]));
        let judgement =
            judge(&task, SOURCE, &StrategyOutcome::Applied(SOURCE.to_string())).unwrap();
        assert_eq!(judgement.outcome, Outcome::Wrong);
        assert!(judgement.detail.contains("仍然存在"));
    }

    #[test]
    fn reformatting_during_a_deletion_is_collateral_damage() {
        let task = delete_task(json!(["server", "host"]));
        // 删掉了 host，但整份文件被重新排版（缩进从 2 空格变成 4 空格）
        let next = "server {\n  port = 8080 #@ range(1..65535)\n}\n";
        let reformatted = "server {\n    port = 8080 #@ range(1..65535)\n}\n";
        let judgement = judge(
            &task,
            SOURCE,
            &StrategyOutcome::Applied(reformatted.to_string()),
        )
        .unwrap();
        assert_eq!(
            judgement.outcome,
            Outcome::Collateral,
            "{}",
            judgement.detail
        );
        // 顺便确认纯净的那一份是 Correct，说明差异确实来自"重排"而不是判定抖动
        let clean = judge(&task, SOURCE, &StrategyOutcome::Applied(next.to_string())).unwrap();
        assert_eq!(clean.outcome, Outcome::Correct);
    }

    #[test]
    fn deleting_an_adjacent_line_too_is_collateral_damage() {
        // 只靠最长公共前缀/后缀挡不住这个：删掉目标行的同时把上一行也删掉，
        // 在字节上仍然是一次连续删除。键集合的比对就是为这种情况准备的。
        let task = delete_task(json!(["server", "host"]));
        let next = "server {\n}\n"; // port 那行也被连带删掉了
        let judgement = judge(&task, SOURCE, &StrategyOutcome::Applied(next.to_string())).unwrap();
        assert_eq!(
            judgement.outcome,
            Outcome::Collateral,
            "{}",
            judgement.detail
        );
    }

    #[test]
    fn inserting_an_extra_key_is_collateral_damage() {
        // 目标插进去了，但顺手多插了一个键
        let task = insert_task(json!(["added"]), json!("x"));
        let next = format!("{SOURCE}added = \"x\"\nsneaky = 1\n");
        let judgement = judge(&task, SOURCE, &StrategyOutcome::Applied(next)).unwrap();
        assert_eq!(
            judgement.outcome,
            Outcome::Collateral,
            "{}",
            judgement.detail
        );
    }

    const MULTI_SOURCE: &str = "server {\n  port = 8080 #@ range(1..65535)\n  host = \"0.0.0.0\"\n}\nlogging {\n  level = \"info\" # keep\n}\n";

    fn multi_task() -> Task {
        Task {
            id: "m".to_string(),
            document: "d".to_string(),
            description: "一次改两处".to_string(),
            plan: json!({
                "version": "1.0",
                "edits": [
                    { "op": "set", "path": ["server", "port"], "value": 9090 },
                    { "op": "set", "path": ["logging", "level"], "value": "debug" }
                ]
            }),
            expect: Expectation {
                outcome: "applied".to_string(),
                path: Vec::new(),
                value: None,
                code: None,
                targets: vec![
                    TargetExpect {
                        path: vec![json!("server"), json!("port")],
                        value: Some(json!(9090)),
                    },
                    TargetExpect {
                        path: vec![json!("logging"), json!("level")],
                        value: Some(json!("debug")),
                    },
                ],
                target_file: None,
            },
        }
    }

    #[test]
    fn a_minimal_multi_edit_is_correct() {
        let next = "server {\n  port = 9090 #@ range(1..65535)\n  host = \"0.0.0.0\"\n}\nlogging {\n  level = \"debug\" # keep\n}\n";
        let judgement = judge(
            &multi_task(),
            MULTI_SOURCE,
            &StrategyOutcome::Applied(next.to_string()),
        )
        .unwrap();
        assert_eq!(judgement.outcome, Outcome::Correct, "{}", judgement.detail);
        assert!(judgement.comments_kept);
    }

    #[test]
    fn a_multi_edit_that_touches_another_line_is_collateral() {
        // 两处都改对了，但顺手把 host 那行的缩进也改了
        let next = "server {\n  port = 9090 #@ range(1..65535)\n    host = \"0.0.0.0\"\n}\nlogging {\n  level = \"debug\" # keep\n}\n";
        let judgement = judge(
            &multi_task(),
            MULTI_SOURCE,
            &StrategyOutcome::Applied(next.to_string()),
        )
        .unwrap();
        assert_eq!(
            judgement.outcome,
            Outcome::Collateral,
            "{}",
            judgement.detail
        );
    }

    #[test]
    fn a_multi_edit_that_drops_a_comment_is_collateral() {
        // 目标行改对了，但行尾注释被吃掉
        let next = "server {\n  port = 9090\n  host = \"0.0.0.0\"\n}\nlogging {\n  level = \"debug\" # keep\n}\n";
        let judgement = judge(
            &multi_task(),
            MULTI_SOURCE,
            &StrategyOutcome::Applied(next.to_string()),
        )
        .unwrap();
        assert_eq!(
            judgement.outcome,
            Outcome::Collateral,
            "{}",
            judgement.detail
        );
    }

    #[test]
    fn a_multi_edit_with_one_wrong_value_is_a_mis_edit() {
        let next = "server {\n  port = 1234 #@ range(1..65535)\n  host = \"0.0.0.0\"\n}\nlogging {\n  level = \"debug\" # keep\n}\n";
        let judgement = judge(
            &multi_task(),
            MULTI_SOURCE,
            &StrategyOutcome::Applied(next.to_string()),
        )
        .unwrap();
        assert_eq!(judgement.outcome, Outcome::Wrong);
        assert!(judgement.detail.contains("server.port"));
    }

    #[test]
    fn a_whole_file_rewrite_of_a_multi_edit_is_collateral() {
        let next =
            "logging {\n  level = \"debug\"\n}\nserver {\n  host = \"0.0.0.0\"\n  port = 9090\n}\n";
        let judgement = judge(
            &multi_task(),
            MULTI_SOURCE,
            &StrategyOutcome::Applied(next.to_string()),
        )
        .unwrap();
        assert_eq!(
            judgement.outcome,
            Outcome::Collateral,
            "{}",
            judgement.detail
        );
    }

    #[test]
    fn ambiguous_named_list_cannot_be_read_back() {
        let source =
            "[[servers]]\nname = \"a\"\nip = \"1\"\n\n[[servers]]\nname = \"a\"\nip = \"2\"\n";
        let path: Vec<PathSegment> = serde_json::from_value(json!([
            { "key": "servers", "match": { "name": "a" } },
            "ip"
        ]))
        .unwrap();
        let ast = parse(source).unwrap();
        assert!(read_value_at(&ast.root, &path).is_none());
    }

    // ------------------------------------------- 允许变更区间（同名字段反例）

    fn edit_value(raw: serde_json::Value) -> EditValue {
        serde_json::from_value(raw).expect("EditValue 应当可解析")
    }

    #[test]
    fn allowed_ranges_cover_the_value_for_set_and_the_whole_line_for_delete() {
        let source = "a = 1\nb = 2\n";
        let set_targets = vec![(
            vec![PathSegment::Key("b".to_string())],
            Some(edit_value(json!(2))),
        )];
        assert_eq!(
            allowed_ranges(source, &set_targets),
            Some(vec![(10, 11)]),
            "set 只允许改值本身（b = 2 里的那个 2）"
        );

        let delete_targets = vec![(vec![PathSegment::Key("b".to_string())], None)];
        assert_eq!(
            allowed_ranges(source, &delete_targets),
            Some(vec![(6, 12)]),
            "delete 允许改掉整行（含换行）"
        );
    }

    #[test]
    fn allowed_ranges_are_undefined_for_insert_targets() {
        // 插入的目标在原文里还不存在，定位不到区间，判定因此退回行级口径
        let source = "a = 1\n";
        let targets = vec![(
            vec![PathSegment::Key("c".to_string())],
            Some(edit_value(json!(3))),
        )];
        assert!(allowed_ranges(source, &targets).is_none());
    }

    const SAME_KEY_SOURCE: &str = "port = 8080\nworkers = 2\nserver {\n  port = 9090\n}\n";

    fn same_key_task() -> Task {
        Task {
            id: "same-key".to_string(),
            document: "d".to_string(),
            description: "根级 port 与 server.port 共用叶子名".to_string(),
            plan: json!({
                "version": "1.0",
                "edits": [
                    { "op": "set", "path": ["server", "port"], "value": 9090 },
                    { "op": "set", "path": ["workers"], "value": 4 }
                ]
            }),
            expect: Expectation {
                outcome: "applied".to_string(),
                path: Vec::new(),
                value: None,
                code: None,
                targets: vec![
                    TargetExpect {
                        path: vec![json!("server"), json!("port")],
                        value: Some(json!(9090)),
                    },
                    TargetExpect {
                        path: vec![json!("workers")],
                        value: Some(json!(4)),
                    },
                ],
                target_file: None,
            },
        }
    }

    #[test]
    fn a_multi_edit_that_hits_a_same_named_field_elsewhere_is_collateral() {
        // 回归：朴素策略按第一个同名键改，把根级 port 也改成 9090。
        // 旧判定只比键集合与「改动的行是否提到 port」，于是评成「正确、零附带损伤」。
        let next = "port = 9090\nworkers = 4\nserver {\n  port = 9090\n}\n";
        let judgement = judge(
            &same_key_task(),
            SAME_KEY_SOURCE,
            &StrategyOutcome::Applied(next.to_string()),
        )
        .unwrap();
        assert_eq!(
            judgement.outcome,
            Outcome::Collateral,
            "{}",
            judgement.detail
        );
    }

    #[test]
    fn the_same_key_task_is_correct_when_only_the_targets_change() {
        let next = "port = 8080\nworkers = 4\nserver {\n  port = 9090\n}\n";
        let judgement = judge(
            &same_key_task(),
            SAME_KEY_SOURCE,
            &StrategyOutcome::Applied(next.to_string()),
        )
        .unwrap();
        assert_eq!(judgement.outcome, Outcome::Correct, "{}", judgement.detail);
    }

    #[test]
    fn a_multi_edit_that_swaps_two_lines_is_collateral() {
        // 行多重集看不出顺序变化：两行内容都在，只是换了位置
        let next = "server {\n  host = \"0.0.0.0\"\n  port = 9090 #@ range(1..65535)\n}\nlogging {\n  level = \"debug\" # keep\n}\n";
        let judgement = judge(
            &multi_task(),
            MULTI_SOURCE,
            &StrategyOutcome::Applied(next.to_string()),
        )
        .unwrap();
        assert_eq!(
            judgement.outcome,
            Outcome::Collateral,
            "{}",
            judgement.detail
        );
    }

    #[test]
    fn a_multi_edit_that_changes_line_endings_is_collateral() {
        // 目标行之间那一段的换行被换成 CRLF，字节确实变了
        let next = "server {\n  port = 9090 #@ range(1..65535)\r\n  host = \"0.0.0.0\"\n}\nlogging {\n  level = \"debug\" # keep\n}\n";
        let judgement = judge(
            &multi_task(),
            MULTI_SOURCE,
            &StrategyOutcome::Applied(next.to_string()),
        )
        .unwrap();
        assert_eq!(
            judgement.outcome,
            Outcome::Collateral,
            "{}",
            judgement.detail
        );
    }
}

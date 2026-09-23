//! 判定脚本。
//!
//! 判定只看三件事，且都不读被测策略的自述：
//!
//! 1. **正确**：结果文档里目标字段的值确实等于期望值——直接读 AST，不信任策略的返回值；
//! 2. **附带损伤**：按契约对「值在哪」的定义算出原值区间，改动前后的前缀与后缀必须逐字节相同，
//!    否则就是改动区间之外的字节也变了；
//! 3. **拒绝**：必须给出期望的稳定错误码。拒绝理由不对不算通过，否则"一律拒绝"也能拿满分。

use std::collections::BTreeMap;

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

    let path = task.expect_path()?;
    let expected = task
        .expect_value()?
        .ok_or_else(|| format!("任务 {} 声明为 applied 但没有 expect.value", task.id))?;

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
            (!(prefix_ok && suffix_ok)).then(|| describe_damage(source, next, prefix_ok, suffix_ok))
        }
        Err(error) => Some(format!("无法确定原值区间：{error}")),
    };

    Ok(Judgement {
        outcome: if damage.is_some() {
            Outcome::Collateral
        } else {
            Outcome::Correct
        },
        detail: damage.unwrap_or_else(|| "目标值已改对，改动区间之外字节零变化".to_string()),
        comments_kept: comments_kept(source, next),
    })
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
    use crate::Expectation;
    use serde_json::json;

    const SOURCE: &str = "server {\n  port = 8080 #@ range(1..65535)\n  host = \"0.0.0.0\"\n}\n";

    fn applied_task() -> Task {
        Task {
            id: "t".to_string(),
            document: "d".to_string(),
            description: "改 port".to_string(),
            plan: json!({}),
            expect: Expectation {
                outcome: "applied".to_string(),
                path: vec![json!("server"), json!("port")],
                value: Some(json!(9090)),
                code: None,
            },
        }
    }

    fn refused_task(code: &str) -> Task {
        Task {
            id: "r".to_string(),
            document: "d".to_string(),
            description: "应当拒绝".to_string(),
            plan: json!({}),
            expect: Expectation {
                outcome: "refused".to_string(),
                path: Vec::new(),
                value: None,
                code: Some(code.to_string()),
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
}

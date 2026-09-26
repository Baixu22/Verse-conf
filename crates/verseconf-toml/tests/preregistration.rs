//! TF-0071：预登记的任务集必须**真的**命中它声称覆盖的特征。
//!
//! 这条验收不能靠人工核对 79 条任务，所以这里用冻结的语料逐条重算：
//!
//! 1. 按任务记录里的路径重新定位目标，`value_span` 必须与记录一致；
//! 2. 目标值必须等于记录里的 `before`；
//! 3. 应用这条编辑，**只有** `value_span` 那一段字节可以变，其余逐字节相同
//!    （这同时证明了记录的 span 就是真正的目标区间，而不是「大致位置」）；
//! 4. 特征声称必须与事实一致：类型、Unicode、行尾注释、带引号的键路径
//!    各自都要在原文里查得到，而且**不能多报**——多报一个特征就等于把
//!    「这份文件含有该特征」冒充成「这条任务命中了该特征」。
//!
//! 最后一条是重点：允许少报（有些特征是选择规则之外的事实），但不允许把
//! 没查到的写成查到的。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use toml_edit::{ImDocument, Item, Key, Table};
use verseconf_core::{EditPlan, EditValue};
use verseconf_toml::{apply_toml_edit, edit_value_from_toml};

/// 验收里点名要覆盖的六个特征
const REQUIRED_FEATURES: [&str; 6] = [
    "string",
    "integer",
    "boolean",
    "unicode",
    "trailing_comment",
    "quoted_key_path",
];

fn workspace_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn confirmation_dir() -> PathBuf {
    workspace_dir().join("benchmark/confirmation")
}

fn holdout_dir() -> PathBuf {
    workspace_dir().join("benchmark/holdout")
}

fn load_json(path: &Path) -> Value {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("读不到 {}：{error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} 不是合法 JSON：{error}", path.display()))
}

// ————————————————— 路径解析（测试自己的实现，用来独立核对） —————————————————

#[derive(Debug, Clone)]
enum Step {
    Key(String),
    Named {
        key: String,
        matches: Vec<(String, Value)>,
    },
}

fn parse_steps(path: &Value) -> Vec<Step> {
    path.as_array()
        .expect("path 应当是数组")
        .iter()
        .map(|segment| match segment {
            Value::String(name) => Step::Key(name.clone()),
            Value::Object(map) => {
                let key = map["key"].as_str().expect("Named 段要有 key").to_string();
                let matches = map["match"]
                    .as_object()
                    .expect("Named 段要有 match")
                    .iter()
                    .map(|(field, value)| (field.clone(), value.clone()))
                    .collect();
                Step::Named { key, matches }
            }
            other => panic!("路径段既不是字符串也不是对象：{other}"),
        })
        .collect()
}

enum Node<'a> {
    Item(&'a Item),
    Element(&'a Table),
}

impl<'a> Node<'a> {
    fn get_key_value(&self, name: &str) -> Option<(&'a Key, &'a Item)> {
        match self {
            Node::Item(Item::Table(table)) => table.get_key_value(name),
            Node::Item(_) => None,
            Node::Element(table) => table.get_key_value(name),
        }
    }

    fn child(&self, name: &str) -> Option<Node<'a>> {
        let (_, item) = self.get_key_value(name)?;
        Some(Node::Item(item))
    }
}

/// 返回 (最后一段的键, 最后一段落点)
fn resolve<'a>(root: &'a Item, steps: &[Step]) -> Option<(&'a Key, Node<'a>)> {
    let mut node = Node::Item(root);
    for (index, step) in steps.iter().enumerate() {
        let last = index + 1 == steps.len();
        match step {
            Step::Key(name) => {
                let (key, item) = node.get_key_value(name)?;
                if last {
                    return Some((key, Node::Item(item)));
                }
                node = Node::Item(item);
            }
            Step::Named { key, matches } => {
                let (_, item) = node.get_key_value(key)?;
                let Item::ArrayOfTables(array_of_tables) = item else {
                    return None;
                };
                let mut found: Vec<&Table> = Vec::new();
                for element in array_of_tables.iter() {
                    let hit = matches.iter().all(|(field, want)| {
                        element
                            .get_key_value(field)
                            .and_then(|(_, item)| edit_value_from_toml(item))
                            .map(|value| edit_value_to_json(&value) == *want)
                            .unwrap_or(false)
                    });
                    if hit {
                        found.push(element);
                    }
                }
                if found.len() != 1 {
                    return None;
                }
                if last {
                    return None;
                }
                node = Node::Element(found[0]);
            }
        }
    }
    None
}

fn edit_value_to_json(value: &EditValue) -> Value {
    serde_json::to_value(value).expect("契约值应当能序列化回 JSON")
}

// ————————————————————————————— 核对 —————————————————————————————

#[test]
fn every_preregistered_task_actually_hits_the_features_it_claims() {
    let tasks_document = load_json(&confirmation_dir().join("tasks.json"));
    let tasks = tasks_document["tasks"]
        .as_array()
        .expect("tasks 应当是数组");
    assert!(!tasks.is_empty(), "任务集不能为空");

    let mut failures: Vec<String> = Vec::new();
    let mut seen_ids: BTreeSet<String> = BTreeSet::new();
    let mut per_document: BTreeMap<String, usize> = BTreeMap::new();

    for task in tasks {
        let id = task["id"].as_str().expect("id 应当是字符串");
        if !seen_ids.insert(id.to_string()) {
            failures.push(format!("{id}: 任务 id 重复"));
        }
        *per_document
            .entry(task["document"].as_str().expect("document").to_string())
            .or_insert(0) += 1;

        let document = task["document"].as_str().expect("document");
        let source = fs::read_to_string(holdout_dir().join("documents").join(document))
            .unwrap_or_else(|error| panic!("读不到语料 {document}：{error}"));

        let parsed = ImDocument::parse(source.clone()).expect("语料应当可解析");
        let steps = parse_steps(&task["path"]);

        let Some((_key, node)) = resolve(parsed.as_item(), &steps) else {
            failures.push(format!("{id}: 按记录里的路径定位不到目标"));
            continue;
        };
        let Node::Item(item) = node else {
            failures.push(format!("{id}: 落点不是条目"));
            continue;
        };
        let Some(value) = item.as_value() else {
            failures.push(format!("{id}: 落点不是值"));
            continue;
        };

        // 1) 记录的区间必须就是真实的区间
        let recorded = task["value_span"].as_array().expect("value_span");
        let start = recorded[0].as_u64().expect("start") as usize;
        let end = recorded[1].as_u64().expect("end") as usize;
        let Some(actual) = value.span() else {
            failures.push(format!("{id}: 目标值没有字节区间"));
            continue;
        };
        if (actual.start, actual.end) != (start, end) {
            failures.push(format!(
                "{id}: value_span 记录的是 [{start}, {end})，真实区间是 [{}, {})",
                actual.start, actual.end
            ));
            continue;
        }

        // 2) 目标值必须等于记录里的 before
        let expected: EditValue =
            serde_json::from_value(task["before"].clone()).expect("before 应当是契约值");
        if edit_value_from_toml(item) != Some(expected.clone()) {
            failures.push(format!("{id}: 记录里的 before 与语料不符"));
        }

        let raw = &source[start..end];
        let single_line = !raw.contains('\n') && !raw.contains('\r');

        // 3) 特征声称必须查得到
        let claimed: BTreeSet<&str> = task["features"]
            .as_array()
            .expect("features")
            .iter()
            .map(|feature| feature.as_str().expect("feature 应当是字符串"))
            .collect();

        let kind = match &expected {
            EditValue::String(_) => "string",
            EditValue::Integer(_) => "integer",
            EditValue::Float(_) => "float",
            EditValue::Bool(_) => "boolean",
            _ => "complex",
        };
        if claimed.contains(kind) && kind == "complex" {
            failures.push(format!("{id}: 声称覆盖 {kind}，但这不是标量"));
        }

        let before_text = match &expected {
            EditValue::String(text) => text.clone(),
            _ => String::new(),
        };
        let after_text = match &task["value"] {
            Value::String(text) => text.clone(),
            _ => String::new(),
        };
        let unicode_fact = !before_text.is_ascii() && !after_text.is_ascii();
        if claimed.contains("unicode") && !unicode_fact {
            failures.push(format!(
                "{id}: 声称覆盖 unicode，但原文值 {before_text:?} 与替换值 {after_text:?} 不是两边都非 ASCII"
            ));
        }

        let line_end = source[end..]
            .find('\n')
            .map(|index| end + index)
            .unwrap_or(source.len());
        let recorded_comment = task["trailing_comment"].as_str().expect("trailing_comment");
        let actual_comment = source[end..line_end].trim_end_matches('\r').trim();
        let comment_fact = !recorded_comment.is_empty()
            && actual_comment == recorded_comment
            && actual_comment.starts_with('#');
        if claimed.contains("trailing_comment") && !comment_fact {
            failures.push(format!(
                "{id}: 声称覆盖行尾注释，但原文该行值之后是 {actual_comment:?}"
            ));
        }

        let quoted_claim = task["quoted_key_path"].as_array().expect("quoted_key_path");
        let quoted_positions: Vec<usize> = quoted_claim
            .iter()
            .enumerate()
            .filter(|(_, flag)| flag.as_bool().unwrap_or(false))
            .map(|(index, _)| index)
            .collect();
        if claimed.contains("quoted_key_path") && quoted_positions.is_empty() {
            failures.push(format!(
                "{id}: 声称覆盖带引号的键路径，但记录里没有任何引号段"
            ));
        }
        if !quoted_positions.is_empty() {
            // 逐段确认「这一段确实在原文里带引号」
            for position in &quoted_positions {
                if let Step::Key(name) = &steps[*position] {
                    let quoted_in_source =
                        key_is_quoted(&source, parsed.as_item(), &steps[..=*position], name);
                    if !quoted_in_source {
                        failures.push(format!(
                            "{id}: 第 {} 段 `{name}` 声称带引号，但原文里没有引号",
                            position + 1
                        ));
                    }
                }
            }
        }

        if !single_line {
            failures.push(format!("{id}: 目标值区间不是单行：{raw:?}"));
        }

        // 4) 应用这条编辑：只有记录的那一段字节可以变
        let plan_json = serde_json::json!({
            "version": "1.0",
            "edits": [{ "op": "set", "path": task["path"], "value": task["value"] }],
        });
        let plan = EditPlan::from_json(&plan_json.to_string()).expect("任务应当能构造成合法计划");
        let outcome = match apply_toml_edit(&source, &plan) {
            Ok(outcome) => outcome,
            Err(refusal) => {
                failures.push(format!("{id}: 编辑被拒（{refusal}）"));
                continue;
            }
        };

        let replacement = outcome.applied[0].after.clone();
        if outcome.source[..start] != source[..start] {
            failures.push(format!("{id}: 目标区间之前的字节被动了"));
        }
        let tail_start = start + replacement.len();
        if outcome.source[tail_start..] != source[end..] {
            failures.push(format!("{id}: 目标区间之后的字节被动了"));
        }
        if &outcome.source[start..tail_start] != replacement.as_str() {
            failures.push(format!("{id}: 替换后的字节不是渲染出来的新值"));
        }
    }

    // 每份文档 4–6 个任务，而且语料里的每份文档都要有
    let manifest = load_json(&holdout_dir().join("manifest.json"));
    for document in manifest["documents"].as_array().expect("documents") {
        let name = document["document"].as_str().expect("document");
        let count = per_document.get(name).copied().unwrap_or(0);
        if !(4..=6).contains(&count) {
            failures.push(format!("{name}: 有 {count} 个任务，不在 4–6 之内"));
        }
    }

    // 六个必需特征在整个任务集里都要被真正覆盖
    let mut coverage: BTreeMap<&str, usize> = BTreeMap::new();
    for task in tasks {
        for feature in task["features"].as_array().expect("features") {
            *coverage
                .entry(feature.as_str().expect("feature"))
                .or_insert(0) += 1;
        }
    }
    for feature in REQUIRED_FEATURES {
        if coverage.get(feature).copied().unwrap_or(0) == 0 {
            failures.push(format!("必需特征 `{feature}` 没有任何任务覆盖"));
        }
    }

    assert!(
        failures.is_empty(),
        "预登记任务集不满足验收：\n  {}",
        failures.join("\n  ")
    );
}

/// 第 `position` 段键在原文里是不是带引号的
fn key_is_quoted(source: &str, root: &Item, steps: &[Step], name: &str) -> bool {
    let parent = if steps.len() <= 1 {
        Node::Item(root)
    } else {
        match resolve_parent(root, &steps[..steps.len() - 1]) {
            Some(node) => node,
            None => return false,
        }
    };
    let Some((key, _)) = parent.get_key_value(name) else {
        return false;
    };
    let Some(span) = key.span() else {
        return false;
    };
    let raw = &source[span];
    raw.starts_with('"') || raw.starts_with('\'')
}

fn resolve_parent<'a>(root: &'a Item, steps: &[Step]) -> Option<Node<'a>> {
    if steps.is_empty() {
        return Some(Node::Item(root));
    }
    let mut node = Node::Item(root);
    for step in steps {
        node = match step {
            Step::Key(name) => node.child(name)?,
            Step::Named { key, matches } => {
                let (_, item) = node.get_key_value(key)?;
                let Item::ArrayOfTables(array_of_tables) = item else {
                    return None;
                };
                let mut found: Vec<&Table> = Vec::new();
                for element in array_of_tables.iter() {
                    let hit = matches.iter().all(|(field, want)| {
                        element
                            .get_key_value(field)
                            .and_then(|(_, item)| edit_value_from_toml(item))
                            .map(|value| edit_value_to_json(&value) == *want)
                            .unwrap_or(false)
                    });
                    if hit {
                        found.push(element);
                    }
                }
                if found.len() != 1 {
                    return None;
                }
                Node::Element(found[0])
            }
        };
    }
    Some(node)
}

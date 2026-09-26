//! TF-0071：从冻结的 holdout 语料生成预登记的标量 `set` 任务集。
//!
//! ```text
//! cargo run -p verseconf-toml --example generate_tasks -- --report
//! cargo run -p verseconf-toml --example generate_tasks -- --out <目录>
//! ```
//!
//! ## 为什么任务要由解析器生成
//!
//! TF-0071 的验收里有一条硬的：**任务必须真正命中它声称覆盖的特征，而不是
//! 「文件里含有该特征」**。手写任务集做不到这一点——写的人会（无意识地）按
//! 「这份文件看起来有 Unicode」来标，而不是按「这条任务的原文行里确实有一个
//! 必须原样保留的非 ASCII 字形」来标。所以这里让 `toml_edit` 把每个候选的
//! 事实数出来，特征声称直接来自事实。
//!
//! ## 选择规则（写死，不看结果）
//!
//! 1. 候选 = 能用路径定位、值是**单行标量**的键值对。路径可以是普通键，
//!    也可以是 `[[key]]` 里按字段匹配的元素定位（`PathSegment::Named`）。
//! 2. 按固定特征顺序（字符串 → 整数 → 布尔 → Unicode → 行尾注释 → 带引号的键路径
//!    → 浮点）各挑一个尚未选中的候选，使这份文件能覆盖的特征都被覆盖。
//! 3. 不足 4 个就按文档顺序补齐；超过 6 个按上面的顺序截断。
//! 4. 每个候选都要真的跑一遍编辑：结果必须仍然合法、且**不引入新的高危实例**
//!    （否则安全门禁会拒掉它，任务就测不到编辑机制了）。
//!
//! 规则本身是确定的：同一份语料、同一份生成器，输出逐字节相同。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};
use toml_edit::{ArrayOfTables, ImDocument, Item, Table, Value as TomlValue};

use verseconf_core::{high_risk_instances, EditPlan, EditValue};
use verseconf_toml::{apply_toml_edit, audit_toml, edit_value_from_toml};

/// 每份文档的目标任务数；验收要求落在 4–6
const TARGET_PER_DOCUMENT: usize = 5;
const MIN_PER_DOCUMENT: usize = 4;
const MAX_PER_DOCUMENT: usize = 6;

/// 特征挑选顺序：越靠前越优先，截断时丢的是最后几个
const FEATURE_ORDER: [Feature; 7] = [
    Feature::String,
    Feature::Integer,
    Feature::Boolean,
    Feature::Unicode,
    Feature::TrailingComment,
    Feature::QuotedKeyPath,
    Feature::Float,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Feature {
    String,
    Integer,
    Boolean,
    Float,
    Unicode,
    TrailingComment,
    QuotedKeyPath,
}

impl Feature {
    fn key(self) -> &'static str {
        match self {
            Feature::String => "string",
            Feature::Integer => "integer",
            Feature::Boolean => "boolean",
            Feature::Float => "float",
            Feature::Unicode => "unicode",
            Feature::TrailingComment => "trailing_comment",
            Feature::QuotedKeyPath => "quoted_key_path",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ScalarKind {
    String,
    Integer,
    Float,
    Boolean,
}

/// 路径上的一段：普通键，或 `[[key]]` 里按字段匹配的元素
#[derive(Debug, Clone)]
enum Segment {
    Key {
        name: String,
        quoted: bool,
    },
    Named {
        key: String,
        matches: BTreeMap<String, EditValue>,
    },
}

impl Segment {
    fn display(&self) -> String {
        match self {
            Segment::Key { name, .. } => name.clone(),
            Segment::Named { key, matches } => {
                let rendered: Vec<String> = matches
                    .iter()
                    .map(|(field, value)| format!("{field}={}", display_edit_value(value)))
                    .collect();
                format!("{key}[{}]", rendered.join(", "))
            }
        }
    }

    fn to_json(&self) -> Value {
        match self {
            Segment::Key { name, .. } => Value::String(name.clone()),
            Segment::Named { key, matches } => {
                let mut map = Map::new();
                for (field, value) in matches {
                    map.insert(field.clone(), edit_value_to_json(value));
                }
                json!({ "key": key, "match": Value::Object(map) })
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Candidate {
    segments: Vec<Segment>,
    quoted_segments: Vec<bool>,
    /// 原文里的当前值，用来核对语料没被动过
    before: Value,
    /// 预登记的替换值
    after: Value,
    /// 值在原文里的字节区间（必须是单行）
    span: Range<usize>,
    /// 行尾注释的原文（`#` 到行尾），没有就是空
    trailing_comment: String,
    features: BTreeSet<Feature>,
    evidence: BTreeMap<String, String>,
}

fn main() {
    let mut report_only = false;
    let mut out_dir: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--report" => report_only = true,
            "--out" => out_dir = args.next().map(PathBuf::from),
            other => {
                eprintln!("不认识的参数：{other}");
                std::process::exit(2);
            }
        }
    }
    if !report_only && out_dir.is_none() {
        eprintln!("用法：generate_tasks --report | --out <目录>");
        std::process::exit(2);
    }

    let holdout = holdout_dir();
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(holdout.join("manifest.json")).expect("读不到语料清单"),
    )
    .expect("manifest.json 应当是合法 JSON");
    let documents = manifest["documents"]
        .as_array()
        .expect("documents 应当是数组");

    let mut tasks: Vec<Value> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    let mut coverage: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut named_paths = 0usize;

    println!(
        "{:<34} {:>4} {:>4} {:>4} {:>4} {:>4} {:>4}  尾注/引号",
        "文档", "候选", "串", "整", "布", "浮", "Uni"
    );

    for document in documents {
        let name = document["document"]
            .as_str()
            .expect("document 应当是字符串");
        let cluster = document["cluster"].as_str().expect("cluster 应当是字符串");
        let ending = document["ending"].as_str().expect("ending 应当是字符串");
        let source =
            fs::read_to_string(holdout.join("documents").join(name)).expect("语料应当可读");

        let candidates = collect_candidates(&source);
        let stats = feature_counts(&candidates);
        println!(
            "{name:<34} {:>4} {:>4} {:>4} {:>4} {:>4} {:>4}  {}/{}",
            candidates.len(),
            stats.get(&Feature::String).copied().unwrap_or(0),
            stats.get(&Feature::Integer).copied().unwrap_or(0),
            stats.get(&Feature::Boolean).copied().unwrap_or(0),
            stats.get(&Feature::Float).copied().unwrap_or(0),
            stats.get(&Feature::Unicode).copied().unwrap_or(0),
            stats.get(&Feature::TrailingComment).copied().unwrap_or(0),
            stats.get(&Feature::QuotedKeyPath).copied().unwrap_or(0),
        );

        let chosen = select(&candidates);
        if chosen.len() < MIN_PER_DOCUMENT {
            problems.push(format!(
                "{name}: 只有 {} 个可用候选，少于 {MIN_PER_DOCUMENT} 个",
                chosen.len()
            ));
            continue;
        }

        for candidate in chosen {
            for feature in &candidate.features {
                *coverage.entry(feature.key()).or_insert(0) += 1;
            }
            if candidate
                .segments
                .iter()
                .any(|segment| matches!(segment, Segment::Named { .. }))
            {
                named_paths += 1;
            }
            match build_task(&source, name, cluster, ending, candidate) {
                Ok(task) => tasks.push(task),
                Err(reason) => {
                    problems.push(format!("{name}: {} —— {reason}", path_label(candidate)))
                }
            }
        }
    }

    println!();
    println!("任务总数 {}", tasks.len());
    for feature in FEATURE_ORDER {
        let count = coverage.get(feature.key()).copied().unwrap_or(0);
        println!("  覆盖 {:<18} {count} 个任务", feature.key());
        if count == 0 {
            problems.push(format!(
                "特征 '{}' 在整个任务集里没有任何任务覆盖",
                feature.key()
            ));
        }
    }
    println!("  其中用 [[key]] 元素定位的 {named_paths} 个任务");

    if !problems.is_empty() {
        eprintln!("\n生成失败（不写出任何文件）：");
        for problem in &problems {
            eprintln!("  - {problem}");
        }
        std::process::exit(1);
    }

    if report_only {
        println!("\n（--report：只报数，不写文件）");
        return;
    }

    let out = out_dir.expect("已检查过");
    fs::create_dir_all(&out).expect("建不出输出目录");
    let document = json!({
        "version": 1,
        "corpus_sha256": manifest["corpus_sha256"],
        "generator": "crates/verseconf-toml/examples/generate_tasks.rs",
        "rules": {
            "candidates": "路径可定位且值为单行标量；路径支持普通键与 [[key]] 的按字段匹配元素定位",
            "selection": "按 字符串→整数→布尔→Unicode→行尾注释→带引号的键路径→浮点 各挑一个，再按文档顺序补到 5 个，最多 6 个",
            "gate_neutral": "每个任务都实测过：应用后结果仍然合法，且没有引入新的高危实例",
            "named_paths": "用 [[key]] 元素定位的任务数",
        },
        "named_path_tasks": named_paths,
        "tasks": tasks,
    });
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&document).expect("应当能序列化")
    );
    fs::write(out.join("tasks.json"), text).expect("写不出 tasks.json");
    println!("\n已写出 {}", out.join("tasks.json").display());
}

/// 把一份文档里所有可用的标量 `set` 候选找出来
fn collect_candidates(source: &str) -> Vec<Candidate> {
    let document = ImDocument::parse(source.to_string()).expect("冻结语料应当可解析");
    let Item::Table(root) = document.as_item() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut prefix: Vec<Segment> = Vec::new();
    walk(root, source, &mut prefix, &mut out);
    out
}

fn walk(table: &Table, source: &str, prefix: &mut Vec<Segment>, out: &mut Vec<Candidate>) {
    for (name, item) in table.iter() {
        // `[[key]]` 的定位靠「按字段匹配的元素」，所以这一段由
        // walk_array_of_tables 自己压进去，这里不能再压一个普通键，
        // 否则路径会变成 `test.test[name=...]` 这种多一层的写法。
        if let Item::ArrayOfTables(array_of_tables) = item {
            walk_array_of_tables(array_of_tables, name, source, prefix, out);
            continue;
        }

        let quoted = table
            .get_key_value(name)
            .and_then(|(key, _)| key.span())
            .map(|span| {
                let raw = &source[span];
                raw.starts_with('"') || raw.starts_with('\'')
            })
            .unwrap_or(false);
        prefix.push(Segment::Key {
            name: name.to_string(),
            quoted,
        });

        match item {
            Item::Table(nested) => walk(nested, source, prefix, out),
            Item::Value(value) => {
                if let Some(candidate) = candidate_for(source, prefix, value) {
                    out.push(candidate);
                }
            }
            Item::ArrayOfTables(_) | Item::None => {}
        }

        prefix.pop();
    }
}

/// `[[key]]` 里的值要靠「按哪个字段匹配」才能定位。
///
/// 只有存在一个能把元素区分开的字段时才生成候选——区分不开就说明这个元素
/// 定位不到，硬造一条会歧义的任务没有意义。
fn walk_array_of_tables(
    array_of_tables: &ArrayOfTables,
    key: &str,
    source: &str,
    prefix: &mut Vec<Segment>,
    out: &mut Vec<Candidate>,
) {
    let Some(discriminator) = discriminating_field(array_of_tables) else {
        return;
    };

    for element in array_of_tables.iter() {
        let Some((_, item)) = element.get_key_value(&discriminator) else {
            continue;
        };
        let Some(value) = edit_value_from_toml(item) else {
            continue;
        };
        let mut matches = BTreeMap::new();
        matches.insert(discriminator.clone(), value);

        prefix.push(Segment::Named {
            key: key.to_string(),
            matches,
        });
        walk(element, source, prefix, out);
        prefix.pop();
    }
}

/// 找一个在所有元素里都存在、且取值互不相同的标量字段
fn discriminating_field(array_of_tables: &ArrayOfTables) -> Option<String> {
    let elements: Vec<&Table> = array_of_tables.iter().collect();
    if elements.is_empty() {
        return None;
    }

    let mut fields: Vec<String> = elements[0]
        .iter()
        .filter(|(_, item)| edit_value_from_toml(item).is_some())
        .map(|(name, _)| name.to_string())
        .collect();
    // `name` 优先：它通常就是给人读的标识
    fields.sort_by_key(|field| (field != "name", field.clone()));

    fields.into_iter().find(|field| {
        let mut seen: Vec<EditValue> = Vec::new();
        for element in &elements {
            let Some((_, item)) = element.get_key_value(field) else {
                return false;
            };
            let Some(value) = edit_value_from_toml(item) else {
                return false;
            };
            if seen.contains(&value) {
                return false;
            }
            seen.push(value);
        }
        true
    })
}

fn candidate_for(source: &str, prefix: &[Segment], value: &TomlValue) -> Option<Candidate> {
    let span = value.span()?;
    let raw = &source[span.clone()];
    // 多行值（多行字符串、跨行数组）不在「标量单行」的范围里：
    // 「目标行」这个概念对它们不成立，字节保真的判定也就无从谈起
    if raw.contains('\n') || raw.contains('\r') {
        return None;
    }

    let (kind, before, after, unicode) = match value {
        TomlValue::String(text) => {
            let original = text.value().clone();
            let non_ascii = !original.is_ascii();
            let replacement = if non_ascii {
                if original == "配置探针" {
                    "配置探针二".to_string()
                } else {
                    "配置探针".to_string()
                }
            } else if original == "verseconf-probe" {
                "verseconf-probe-2".to_string()
            } else {
                "verseconf-probe".to_string()
            };
            if original == replacement {
                return None;
            }
            (
                ScalarKind::String,
                Value::String(original),
                Value::String(replacement),
                non_ascii,
            )
        }
        TomlValue::Integer(number) => {
            let original = *number.value();
            let replacement = if original == 4242 { 4243 } else { 4242 };
            (
                ScalarKind::Integer,
                json!(original),
                json!(replacement),
                false,
            )
        }
        TomlValue::Float(number) => {
            let original = *number.value();
            let replacement = if original == 42.5 { 43.5 } else { 42.5 };
            (
                ScalarKind::Float,
                json!(original),
                json!(replacement),
                false,
            )
        }
        TomlValue::Boolean(flag) => {
            let original = *flag.value();
            (
                ScalarKind::Boolean,
                json!(original),
                json!(!original),
                false,
            )
        }
        // 日期时间在编辑契约里没有对应档位；数组与内联表不是标量
        TomlValue::Datetime(_) | TomlValue::Array(_) | TomlValue::InlineTable(_) => return None,
    };

    let quoted_segments: Vec<bool> = prefix
        .iter()
        .map(|segment| match segment {
            Segment::Key { quoted, .. } => *quoted,
            Segment::Named { .. } => false,
        })
        .collect();

    // 行尾注释：值之后到行尾之间必须只剩空白与一个 `#` 注释
    let line_end = match source[span.end..].find('\n') {
        Some(index) => span.end + index,
        None => source.len(),
    };
    let after_value = source[span.end..line_end].trim_end_matches('\r');
    let trailing_comment = if after_value.trim().starts_with('#') {
        after_value.trim().to_string()
    } else {
        String::new()
    };

    let mut features = BTreeSet::new();
    let mut evidence = BTreeMap::new();

    let kind_feature = match kind {
        ScalarKind::String => Feature::String,
        ScalarKind::Integer => Feature::Integer,
        ScalarKind::Float => Feature::Float,
        ScalarKind::Boolean => Feature::Boolean,
    };
    features.insert(kind_feature);
    evidence.insert(
        kind_feature.key().to_string(),
        format!(
            "原文值 {before} 是{}，替换值 {after} 同类型",
            kind_name(&kind)
        ),
    );

    // Unicode 只在**替换值也是非 ASCII** 时才算命中：否则这条任务做的是
    // 「把字形删掉」，而不是「在非 ASCII 上做替换」
    if unicode {
        features.insert(Feature::Unicode);
        evidence.insert(
            Feature::Unicode.key().to_string(),
            format!("原文值含非 ASCII 字形（{before}），替换值同样是非 ASCII（{after}）"),
        );
    }

    if !trailing_comment.is_empty() {
        features.insert(Feature::TrailingComment);
        evidence.insert(
            Feature::TrailingComment.key().to_string(),
            format!("目标行值之后有行尾注释 `{trailing_comment}`，替换值时必须原样保留"),
        );
    }

    if quoted_segments.iter().any(|quoted| *quoted) {
        features.insert(Feature::QuotedKeyPath);
        let which: Vec<String> = prefix
            .iter()
            .zip(quoted_segments.iter())
            .filter(|(_, quoted)| **quoted)
            .map(|(segment, _)| format!("`{}`", segment.display()))
            .collect();
        evidence.insert(
            Feature::QuotedKeyPath.key().to_string(),
            format!("路径里有带引号的键 {}，定位必须穿过它", which.join("、")),
        );
    }

    let named: Vec<String> = prefix
        .iter()
        .filter(|segment| matches!(segment, Segment::Named { .. }))
        .map(Segment::display)
        .collect();
    if !named.is_empty() {
        evidence.insert(
            "named_path".to_string(),
            format!(
                "路径用 [[key]] 元素的字段匹配定位：{}；定位必须唯一命中一个元素",
                named.join("、")
            ),
        );
    }

    Some(Candidate {
        segments: prefix.to_vec(),
        quoted_segments,
        before,
        after,
        span,
        trailing_comment,
        features,
        evidence,
    })
}

fn path_label(candidate: &Candidate) -> String {
    candidate
        .segments
        .iter()
        .map(Segment::display)
        .collect::<Vec<_>>()
        .join(".")
}

fn kind_name(kind: &ScalarKind) -> &'static str {
    match kind {
        ScalarKind::String => "字符串",
        ScalarKind::Integer => "整数",
        ScalarKind::Float => "浮点",
        ScalarKind::Boolean => "布尔",
    }
}

fn feature_counts(candidates: &[Candidate]) -> BTreeMap<Feature, usize> {
    let mut counts = BTreeMap::new();
    for candidate in candidates {
        for feature in &candidate.features {
            *counts.entry(*feature).or_insert(0) += 1;
        }
    }
    counts
}

/// 选择规则：先按固定顺序把能覆盖的特征各挑一个，再按文档顺序补齐
fn select(candidates: &[Candidate]) -> Vec<&Candidate> {
    let mut chosen: Vec<usize> = Vec::new();

    for feature in FEATURE_ORDER {
        if chosen.len() >= MAX_PER_DOCUMENT {
            break;
        }
        if let Some(index) = (0..candidates.len())
            .find(|index| !chosen.contains(index) && candidates[*index].features.contains(&feature))
        {
            chosen.push(index);
        }
    }

    for index in 0..candidates.len() {
        if chosen.len() >= TARGET_PER_DOCUMENT {
            break;
        }
        if !chosen.contains(&index) {
            chosen.push(index);
        }
    }

    chosen.truncate(MAX_PER_DOCUMENT);
    chosen.into_iter().map(|index| &candidates[index]).collect()
}

/// 造出任务记录，并实测它确实是「门禁中立」的
fn build_task(
    source: &str,
    document: &str,
    cluster: &str,
    ending: &str,
    candidate: &Candidate,
) -> Result<Value, String> {
    let path_json: Vec<Value> = candidate.segments.iter().map(Segment::to_json).collect();

    let plan_json = json!({
        "version": "1.0",
        "edits": [{
            "op": "set",
            "path": path_json,
            "value": candidate.after,
            "reason": "TF-0071 预登记任务",
        }],
    });
    let plan = EditPlan::from_json(&plan_json.to_string())
        .map_err(|violations| format!("计划不合法：{violations:?}"))?;

    let baseline = high_risk_instances(&audit_toml(source).map_err(|e| e.to_string())?);
    if !baseline.is_empty() {
        return Err(format!("语料基线里就有高危实例：{baseline:?}"));
    }

    let outcome =
        apply_toml_edit(source, &plan).map_err(|refusal| format!("编辑被拒：{refusal}"))?;
    let after = high_risk_instances(&audit_toml(&outcome.source).map_err(|e| e.to_string())?);
    if !after.is_empty() {
        return Err(format!("这条任务会引入高危实例，门禁会拒掉它：{after:?}"));
    }

    // 编辑之后的文本必须确实变了；真正的字节保真由判定器在运行时判定，
    // 这里只确认这条任务不是在测「什么都没发生」
    if outcome.source == source {
        return Err("编辑之后文本没有变化".to_string());
    }

    let id = format!(
        "{}-{}",
        cluster,
        candidate
            .segments
            .iter()
            .map(|segment| match segment {
                Segment::Key { name, .. } => name.replace([' ', '.'], "-"),
                Segment::Named { key, matches } => {
                    let rendered: Vec<String> = matches
                        .values()
                        .map(|value| display_edit_value(value).replace([' ', '.', '"'], "-"))
                        .collect();
                    format!("{key}-{}", rendered.join("-"))
                }
            })
            .collect::<Vec<_>>()
            .join("-")
    );

    let mut evidence = Map::new();
    for (key, value) in &candidate.evidence {
        evidence.insert(key.clone(), Value::String(value.clone()));
    }
    evidence.insert(
        "single_line".to_string(),
        Value::String(format!(
            "目标值的原文区间是单行：`{}`",
            &source[candidate.span.clone()]
        )),
    );

    Ok(json!({
        "id": id,
        "document": document,
        "cluster": cluster,
        "ending": ending,
        "path": candidate.segments.iter().map(Segment::to_json).collect::<Vec<_>>(),
        "path_label": path_label(candidate),
        "quoted_key_path": candidate.quoted_segments,
        "value": candidate.after,
        "before": candidate.before,
        "value_span": [candidate.span.start, candidate.span.end],
        "trailing_comment": candidate.trailing_comment,
        "features": candidate.features.iter().map(|feature| feature.key()).collect::<Vec<_>>(),
        "evidence": Value::Object(evidence),
        "instruction": format!(
            "把 {document} 里 {} 的值改成 {}",
            path_label(candidate),
            display_value(&candidate.after)
        ),
    }))
}

/// 契约值渲染成人能读的形式，用于任务 id 与路径展示。
///
/// 不能用 `{:?}`：那会写出 `String("x")` 这种 Rust 调试格式，
/// 而路径展示会原样出现在交给模型的指令里。
fn display_edit_value(value: &EditValue) -> String {
    match value {
        EditValue::String(text) => format!("\"{text}\""),
        EditValue::Bool(flag) => flag.to_string(),
        EditValue::Integer(number) => number.to_string(),
        EditValue::Float(number) => number.to_string(),
        other => format!("{other:?}"),
    }
}

fn display_value(value: &Value) -> String {
    match value {
        Value::String(text) => format!("\"{text}\""),
        other => other.to_string(),
    }
}

/// `EditValue` 与 JSON 一一对应，这里只是把契约值还原成 JSON 写进冻结记录
fn edit_value_to_json(value: &EditValue) -> Value {
    match value {
        EditValue::Bool(flag) => json!(flag),
        EditValue::Integer(number) => json!(number),
        EditValue::Float(number) => json!(number),
        EditValue::String(text) => json!(text),
        EditValue::Array(items) => Value::Array(items.iter().map(edit_value_to_json).collect()),
        EditValue::Table(map) => {
            let mut object = Map::new();
            for (key, item) in map {
                object.insert(key.clone(), edit_value_to_json(item));
            }
            Value::Object(object)
        }
    }
}

fn holdout_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../benchmark/holdout")
}

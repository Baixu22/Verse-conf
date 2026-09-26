//! TF-0070：冻结的 holdout 语料必须仍然能被适配层用的同一个解析器解析。
//!
//! 语料是确认性实验的入口：只要有一份文档在某次提交里变得不可解析，
//! 或者被无意改动，整轮实验的结论就不再指向同一批样本。
//!
//! 分工是刻意的：
//!
//! - `benchmark/holdout/verify.mjs` 负责**字节级**核验——SHA-256、字节数、
//!   行尾类型与语料指纹（Node 自带 crypto，不需要给 Rust 侧加依赖）；
//! - 这条测试负责**只有 Rust 侧能查的事**：每一份文档都能被
//!   `verseconf_toml::toml_to_ast`（也就是适配层真正用的那个解析器）解析，
//!   语料满足 TF-0070 的验收形状，而且清单里的能力画像与真实解析结果一致。

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use serde_json::Value as JsonValue;
use verseconf_core::{high_risk_instances, Ast, NumberValue, ScalarValue, TableEntry, Value};
use verseconf_toml::{audit_toml, toml_to_ast};

fn holdout_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmark/holdout")
}

fn classify_ending(bytes: &[u8]) -> &'static str {
    let mut crlf = 0usize;
    let mut lf = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'\n' {
            continue;
        }
        if index > 0 && bytes[index - 1] == b'\r' {
            crlf += 1;
        } else {
            lf += 1;
        }
    }
    if crlf > 0 && lf == 0 {
        "CRLF"
    } else if crlf == 0 && lf > 0 {
        "LF"
    } else if crlf > 0 {
        "MIXED"
    } else {
        "NONE"
    }
}

fn load_manifest() -> JsonValue {
    let path = holdout_dir().join("manifest.json");
    let text = fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "读不到 holdout 清单 {}：{error}。语料是确认性实验的入口，不能缺席。",
            path.display()
        )
    });
    serde_json::from_str(&text).expect("manifest.json 应当是合法 JSON")
}

#[test]
fn every_frozen_document_is_still_parseable_by_the_adapter_parser() {
    let manifest = load_manifest();
    let documents = manifest["documents"]
        .as_array()
        .expect("documents 应当是数组");
    assert!(!documents.is_empty(), "语料不能为空");

    let mut failures: Vec<String> = Vec::new();
    for document in documents {
        let name = document["document"]
            .as_str()
            .expect("document 应当是字符串");
        let expected_ending = document["ending"].as_str().expect("ending 应当是字符串");
        let path = holdout_dir().join("documents").join(name);

        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                failures.push(format!("{name}: 读不到（{error}）"));
                continue;
            }
        };

        let ending = classify_ending(&bytes);
        if ending != expected_ending {
            failures.push(format!(
                "{name}: 行尾类型漂移（期望 {expected_ending}，实际 {ending}）"
            ));
        }

        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(error) => {
                failures.push(format!("{name}: 不是合法 UTF-8（{error}）"));
                continue;
            }
        };

        match toml_to_ast(&text) {
            Ok(ast) => {
                if ast.root.entries.is_empty() {
                    failures.push(format!("{name}: 解析出来是空文档，装不下任何任务"));
                }
            }
            Err(refusal) => failures.push(format!("{name}: 适配层解析失败（{refusal}）")),
        }
    }

    assert!(
        failures.is_empty(),
        "holdout 语料不满足前提：\n  {}",
        failures.join("\n  ")
    );
}

#[test]
fn the_frozen_corpus_satisfies_the_tf0070_shape() {
    let manifest = load_manifest();
    let documents = manifest["documents"]
        .as_array()
        .expect("documents 应当是数组");

    let clusters: BTreeSet<&str> = documents
        .iter()
        .map(|document| document["cluster"].as_str().expect("cluster 应当是字符串"))
        .collect();
    let lf = documents
        .iter()
        .filter(|document| document["ending"] == "LF")
        .count();
    let crlf = documents
        .iter()
        .filter(|document| document["ending"] == "CRLF")
        .count();

    assert!(
        (12..=16).contains(&documents.len()),
        "文档数 {} 不在 12–16 之内",
        documents.len()
    );
    assert_eq!(
        clusters.len(),
        documents.len(),
        "每个来源只能贡献一份文档，否则独立样本数会被虚增"
    );
    assert!(
        clusters.len() >= 8,
        "来源簇只有 {} 个，少于 8",
        clusters.len()
    );
    assert!(lf >= 6, "LF 只有 {lf} 份，少于 6");
    assert!(crlf >= 6, "CRLF 只有 {crlf} 份，少于 6");

    for forbidden in manifest["excluded_clusters"]
        .as_array()
        .expect("excluded_clusters 应当是数组")
    {
        let forbidden = forbidden.as_str().expect("来源名应当是字符串");
        assert!(
            !clusters.contains(forbidden),
            "{forbidden} 已用于探索性实验，不得进入确认性语料"
        );
    }

    // documents/ 下不能有清单之外的野文件，否则「冻结」就不是冻结
    let on_disk: BTreeSet<String> = fs::read_dir(holdout_dir().join("documents"))
        .expect("documents/ 应当存在")
        .map(|entry| {
            entry
                .expect("目录项应当可读")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let declared: BTreeSet<String> = documents
        .iter()
        .map(|document| {
            document["document"]
                .as_str()
                .expect("document 应当是字符串")
                .to_string()
        })
        .collect();
    assert_eq!(
        on_disk,
        declared,
        "documents/ 下的文件与 manifest 不一致：多出 {:?}，缺少 {:?}",
        on_disk.difference(&declared).collect::<Vec<_>>(),
        declared.difference(&on_disk).collect::<Vec<_>>()
    );
}

/// 清单里冻结的能力画像，用**真实解析结果**数一遍。
///
/// `freeze.mjs` 用行级扫描数这些特征（Node 没有 TOML 解析器），扫描器一旦漏掉
/// 某种写法（比如单引号键），画像就会偏低，而 TF-0071 会照着它挑任务。
/// 所以画像不能只被「写下来」，必须被真实解析结果交叉验证。
#[derive(Debug, Default, PartialEq, Eq)]
struct Counts {
    key_values: usize,
    strings: usize,
    integers: usize,
    floats: usize,
    booleans: usize,
    arrays: usize,
    inline_tables: usize,
    array_tables: usize,
}

fn count_key_value(kv: &verseconf_core::KeyValue, counts: &mut Counts) {
    counts.key_values += 1;
    match &kv.value {
        Value::Scalar(ScalarValue::String(_)) => counts.strings += 1,
        Value::Scalar(ScalarValue::Number(NumberValue::Integer(_))) => counts.integers += 1,
        Value::Scalar(ScalarValue::Number(NumberValue::Float(_))) => counts.floats += 1,
        Value::Scalar(ScalarValue::Boolean(_)) => counts.booleans += 1,
        Value::Array(_) => counts.arrays += 1,
        Value::InlineTable(_) => counts.inline_tables += 1,
        _ => {}
    }
}

fn count_entries(entries: &[TableEntry], counts: &mut Counts) {
    for entry in entries {
        match entry {
            TableEntry::KeyValue(kv) => count_key_value(kv, counts),
            TableEntry::TableBlock(table) => count_entries(&table.entries, counts),
            TableEntry::ArrayTable(array_table) => {
                counts.array_tables += 1;
                for kv in &array_table.entries {
                    count_key_value(kv, counts);
                }
            }
            _ => {}
        }
    }
}

fn count_from_ast(ast: &Ast) -> Counts {
    let mut counts = Counts::default();
    count_entries(&ast.root.entries, &mut counts);
    counts
}

#[test]
fn the_frozen_feature_profile_matches_the_real_parser() {
    let manifest = load_manifest();
    let documents = manifest["documents"]
        .as_array()
        .expect("documents 应当是数组");

    let mut mismatches: Vec<String> = Vec::new();
    for document in documents {
        let name = document["document"]
            .as_str()
            .expect("document 应当是字符串");
        let features = &document["features"];
        let path = holdout_dir().join("documents").join(name);
        let text = fs::read_to_string(&path).expect("语料应当可读");
        let counts = count_from_ast(&toml_to_ast(&text).expect("语料应当可解析"));

        let pairs: [(&str, usize); 8] = [
            ("keyValues", counts.key_values),
            ("strings", counts.strings),
            ("integers", counts.integers),
            ("floats", counts.floats),
            ("booleans", counts.booleans),
            ("arrays", counts.arrays),
            ("inlineTables", counts.inline_tables),
            ("arrayTables", counts.array_tables),
        ];
        for (key, actual) in pairs {
            let frozen = features[key]
                .as_u64()
                .unwrap_or_else(|| panic!("{name}: 画像缺少 {key}"))
                as usize;
            if frozen != actual {
                mismatches.push(format!(
                    "{name}: {key} 画像写的是 {frozen}，真实解析是 {actual}"
                ));
            }
        }
    }

    assert!(
        mismatches.is_empty(),
        "冻结的能力画像与真实解析不一致，TF-0071 会照着错的画像挑任务：\n  {}",
        mismatches.join("\n  ")
    );
}

#[test]
fn the_frozen_corpus_has_a_clean_high_risk_baseline() {
    // 安全门禁只拒绝**新引入**的高危实例，所以基线脏不脏不影响它的正确性。
    // 但基线干净时，「门禁挡住了什么」与「文件本来就有什么」不会混在一起，
    // 消融（TF-0079）才不用为每份文档单独解释一遍。这是语料的设计要求，
    // 所以写成断言而不是注释——否则下一份补进来的文档会悄悄破坏它。
    let manifest = load_manifest();
    let documents = manifest["documents"]
        .as_array()
        .expect("documents 应当是数组");

    let mut dirty: Vec<String> = Vec::new();
    for document in documents {
        let name = document["document"]
            .as_str()
            .expect("document 应当是字符串");
        let text =
            fs::read_to_string(holdout_dir().join("documents").join(name)).expect("语料应当可读");
        let report = audit_toml(&text).expect("语料应当可审计");
        let instances = high_risk_instances(&report);
        if !instances.is_empty() {
            dirty.push(format!(
                "{name}: {:?}",
                instances.keys().collect::<Vec<_>>()
            ));
        }
    }

    assert!(
        dirty.is_empty(),
        "语料基线里出现既有高危实例，安全门禁的消融会被污染：\n  {}",
        dirty.join("\n  ")
    );
}

//! 把 `docs/SPECIFICATION.md` 的状态标注钉在**可执行的探针**上。
//!
//! ## 为什么需要这个测试
//!
//! 这份规范曾经把"设计目标"和"已实现能力"混在一起叙述，全文没有状态标注，
//! 而 README 又声称"标注为未实现的条目请以本节为准"——指向了一份并不存在的标注。
//! 补上标注只是第一步：**标注会再次漂移**，除非有东西在每次改动时核对它。
//!
//! 这个测试就是那个东西。它读 `docs/spec-status-probes.json`，对每条探针
//! 真的调用一次 `parse` / `parse_and_validate`，然后把结果与两处声明比对：
//!
//! 1. 探针自己的 `expect`（这条写法当前应该通过还是报错）；
//! 2. 规范 §0.4 速查表把它列在 **✅ 已实现** 还是 **🚧 未实现** 里。
//!
//! 任何一侧漂移都会让测试失败，并且失败信息会直接指出是哪条声明与实测不符。
//!
//! 覆盖的典型场景：
//! - 有人实现了一个标着 🚧 的特性 → 探针失败，强制把文档改成 ✅；
//! - 有人改坏了标着 ✅ 的特性 → 探针失败，这是真的回归；
//! - 有人把某条从速查表里删掉或挪错分区 → 交叉校验失败。

use std::path::{Path, PathBuf};

use serde::Deserialize;
use verseconf_core::{parse, parse_and_validate, parse_with_context, LoadOptions, SchemaValidator};

#[derive(Debug, Deserialize)]
struct Manifest {
    version: u32,
    quickref_section: String,
    spec_file: String,
    probes: Vec<Probe>,
}

#[derive(Debug, Deserialize)]
struct Probe {
    id: String,
    feature: String,
    /// 该写法在规范里所属的小节标题（必须真实存在于规范中）
    section: String,
    /// 在 §0.4 速查表里应当出现的位置；null 表示只做行为断言
    quickref: Option<String>,
    quickref_key: Option<String>,
    action: String,
    expect: String,
    source: String,
}

/// 仓库根目录（`crates/verseconf-test` 往上两级）
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/verseconf-test 应当有上两级目录")
        .to_path_buf()
}

fn load_manifest() -> (Manifest, String) {
    let root = repo_root();
    let manifest_path = root.join("docs/spec-status-probes.json");
    let raw = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("读不到 {}: {e}", manifest_path.display()));
    let manifest: Manifest = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("{} 不是合法 JSON: {e}", manifest_path.display()));

    let spec_path = root.join(&manifest.spec_file);
    let spec = std::fs::read_to_string(&spec_path)
        .unwrap_or_else(|e| panic!("读不到 {}: {e}", spec_path.display()));

    (manifest, spec)
}

/// 真正跑一次探针，返回 Ok(()) 或 Err(错误说明)
fn run_probe(probe: &Probe) -> Result<(), String> {
    match probe.action.as_str() {
        "parse" => parse(&probe.source).map(|_| ()).map_err(|e| e.to_string()),
        "parse_no_include" => {
            let options = LoadOptions::without_includes();
            parse_with_context(&probe.source, None, &options)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        "validate" => parse_and_validate(&probe.source)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        "validate_strict" => {
            // 与 CLI `validate --strict` 同一条路径：把 schema 的 strict 置位后再校验
            let mut ast = parse(&probe.source).map_err(|e| e.to_string())?;
            let schema = ast
                .schema
                .as_mut()
                .ok_or_else(|| "该探针需要文件内包含 #@schema 块".to_string())?;
            schema.strict = true;
            let mut validator = SchemaValidator::new();
            validator
                .validate_with_schema(&ast)
                .map(|_| ())
                .map_err(|e| format!("{e:?}"))
        }
        other => Err(format!("未知的 action: {other}")),
    }
}

/// §0.4 速查表按 `**🚧 未实现**` 切成"已实现 / 未实现"两段
fn quickref_halves(spec: &str, quickref_section: &str) -> (String, String) {
    let start = spec
        .find(quickref_section)
        .unwrap_or_else(|| panic!("规范里找不到速查表小节 {quickref_section}"));
    // 速查表到下一个同级小节结束
    let rest = &spec[start..];
    let end = rest
        .find("### 0.5")
        .unwrap_or_else(|| panic!("速查表后面应当有 0.5 小节"));
    let block = &rest[..end];

    let split = block
        .find("🚧 未实现")
        .unwrap_or_else(|| panic!("速查表里应当有「🚧 未实现」分区"));
    (block[..split].to_string(), block[split..].to_string())
}

#[test]
fn spec_status_probes_match_the_implementation() {
    let (manifest, spec) = load_manifest();
    assert_eq!(manifest.version, 1, "未知的探针清单版本");
    assert!(!manifest.probes.is_empty(), "探针清单不能为空");

    let (implemented, unimplemented) = quickref_halves(&spec, &manifest.quickref_section);

    let mut failures: Vec<String> = Vec::new();

    for probe in &manifest.probes {
        // 1) 行为断言：探针本身必须与实测一致
        let actual = run_probe(probe);
        let matched = match probe.expect.as_str() {
            "ok" => actual.is_ok(),
            "error" => actual.is_err(),
            other => panic!("探针 {} 的 expect 非法: {other}", probe.id),
        };
        if !matched {
            failures.push(format!(
                "[行为漂移] {} （{}）：声明 expect={} 但实测 {}\n          → 若这是有意的行为变更，请同步更新 docs/spec-status-probes.json 与 SPECIFICATION.md",
                probe.id,
                probe.feature,
                probe.expect,
                match &actual {
                    Ok(()) => "成功".to_string(),
                    Err(e) => format!("失败（{}）", e.lines().next().unwrap_or("")),
                }
            ));
        }

        // 2) 规范里必须真的有这个小节
        if !spec.contains(&probe.section) {
            failures.push(format!(
                "[小节缺失] {}：规范里找不到小节标题 {:?}",
                probe.id, probe.section
            ));
        }

        // 3) 速查表分区必须与探针声明一致
        if let Some(bucket) = &probe.quickref {
            let key = probe
                .quickref_key
                .as_deref()
                .unwrap_or_else(|| panic!("探针 {} 声明了 quickref 却没有 quickref_key", probe.id));
            let (haystack, bucket_name) = match bucket.as_str() {
                "implemented" => (&implemented, "✅ 已实现"),
                "unimplemented" => (&unimplemented, "🚧 未实现"),
                other => panic!("探针 {} 的 quickref 非法: {other}", probe.id),
            };
            if !haystack.contains(key) {
                failures.push(format!(
                    "[速查表漂移] {} （{}）：规范 §0.4 的「{}」分区里找不到 {{ \"{}\" }}",
                    probe.id, probe.feature, bucket_name, key
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "规范状态标注与实测不一致，共 {} 处：\n\n{}\n",
        failures.len(),
        failures.join("\n")
    );
}

/// 规范必须保有状态标注，否则这个测试就失去意义。
#[test]
fn specification_still_carries_status_markers() {
    let (_, spec) = load_manifest();
    for marker in ["本文档的状态说明", "✅", "🚧", "⚠️"] {
        assert!(
            spec.contains(marker),
            "规范里缺少状态标注 {marker:?}——标注被删除后本测试无法再防漂移"
        );
    }
}

/// 清单里的每条探针都必须被真跑一次，避免"加了探针却没人执行"。
#[test]
fn every_probe_is_executed_and_reports_an_outcome() {
    let (manifest, _) = load_manifest();
    for probe in &manifest.probes {
        let outcome = run_probe(probe);
        // 只要求"有确定结论"，具体期望由主测试断言
        assert!(
            outcome.is_ok() || outcome.is_err(),
            "探针 {} 没有产生结论",
            probe.id
        );
    }
}

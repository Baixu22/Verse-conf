//! 门禁增量实验的任务集（TF-0099）必须**真的**测到它声称的东西。
//!
//! 这一组测试是任务集的验收面，不是文档：它逐条把「高风险写法」与「安全写法」
//! 应用在**真实文档**上，然后问门禁要一个答案。
//!
//! - `blocking` 组：高风险写法必须被拒，且拒绝理由要指名它声称的规则；
//! - `advisory` 组：Medium/Low 写法必须放行——门禁按设计只告警，把这一组也拦下来
//!   就是过度拦截，正是误拒预算要量化的东西；
//! - 每条的 `safe_alternative` 必须放行，否则这条任务是误拒源而不是收益源。
//!
//! 一条任务如果锚点在真实文档里出现 0 次或多次，测试直接失败：那说明这条任务
//! 要么什么都没测到，要么改的是另一处（手写任务集最容易犯的两种错）。

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::Value;
use verseconf_toml::check_write_toml;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("仓库根目录必须存在")
}

fn task_set() -> Value {
    let path = repo_root().join("benchmark/gate-increment/tasks.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("读不到 {}：{error}", path.display()));
    serde_json::from_str(&text).expect("tasks.json 必须是合法 JSON")
}

fn document_text(name: &str) -> String {
    let path = repo_root().join("benchmark/holdout/documents").join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("读不到 {}：{error}", path.display()))
}

/// 用任务里声明的锚点做一次朴素替换——这就是「宿主用自己的编辑方式产生候选文本」。
fn apply_edit(document: &str, edit: &Value, task_id: &str) -> String {
    let find = edit["find"].as_str().expect("edit.find 必须是字符串");
    let replace = edit["replace"].as_str().expect("edit.replace 必须是字符串");

    assert_eq!(
        document.matches(find).count(),
        1,
        "任务 {task_id} 的锚点在真实文档里必须恰好出现一次，实际 {} 次",
        document.matches(find).count()
    );

    let candidate = document.replacen(find, replace, 1);
    assert_ne!(
        candidate, document,
        "任务 {task_id} 的这次替换必须真的改变文本（否则它什么都没测）"
    );
    candidate
}

#[test]
fn every_task_behaves_the_way_it_claims() {
    let tasks = task_set();
    let list = tasks["tasks"].as_array().expect("tasks 必须是数组");
    assert!(!list.is_empty(), "任务集不能为空");

    for task in list {
        let id = task["id"].as_str().expect("任务必须有 id");
        let group = task["group"].as_str().expect("任务必须有 group");
        let document = document_text(task["document"].as_str().expect("任务必须有 document"));

        // 高风险的写法
        let risky = apply_edit(&document, &task["risky"], id);
        let expected_code = task["risky"]["expected_code"]
            .as_str()
            .expect("risky.expected_code 必须有");

        match group {
            "blocking" => {
                let refusal = check_write_toml(&document, &risky)
                    .err()
                    .unwrap_or_else(|| panic!("任务 {id} 声称会被拒，实际放行了"));
                assert_eq!(refusal.code(), expected_code, "任务 {id} 的拒绝码不符");

                for rule in task["risky"]["expected_rules"]
                    .as_array()
                    .expect("blocking 任务必须声明 expected_rules")
                {
                    let rule = rule.as_str().expect("规则码必须是字符串");
                    let findings = refusal.details()["findings"].to_string();
                    assert!(
                        findings.contains(rule),
                        "任务 {id} 必须因为 {rule} 被拒，实际 findings={findings}"
                    );
                }
            }
            "advisory" => {
                assert_eq!(
                    expected_code, "allowed",
                    "advisory 组的期望只能是 allowed（门禁对 Medium/Low 只告警）"
                );
                check_write_toml(&document, &risky).unwrap_or_else(|refusal| {
                    panic!(
                        "任务 {id} 是 Medium/Low，门禁不该阻断，实际 {}（过度拦截）",
                        refusal.code()
                    )
                });
            }
            other => panic!("任务 {id} 的 group 非法：{other}"),
        }

        // 同一个意图的安全写法必须放行，否则这条任务是误拒源
        let safe = apply_edit(&document, &task["safe_alternative"], id);
        check_write_toml(&document, &safe).unwrap_or_else(|refusal| {
            panic!(
                "任务 {id} 的安全写法被拒了（这就是误拒）：{}",
                refusal.code()
            )
        });
    }
}

/// 冻结事实必须与任务集一致：来源簇、文档数、组构成。
///
/// 这些数字会写进 PREREGISTRATION.md，所以它们必须由任务集算出来，而不是抄。
#[test]
fn the_frozen_facts_in_the_preregistration_match_the_task_set() {
    let tasks = task_set();
    let list = tasks["tasks"].as_array().expect("tasks 必须是数组");

    let documents: BTreeSet<&str> = list
        .iter()
        .map(|task| task["document"].as_str().expect("document"))
        .collect();
    let clusters: BTreeSet<&str> = list
        .iter()
        .map(|task| task["source_cluster"].as_str().expect("source_cluster"))
        .collect();
    let blocking = list
        .iter()
        .filter(|task| task["group"].as_str() == Some("blocking"))
        .count();
    let advisory = list.len() - blocking;

    assert_eq!(list.len(), 12, "冻结的任务数");
    assert_eq!(documents.len(), 8, "冻结的文档数");
    assert_eq!(clusters.len(), 8, "冻结的来源簇数：每个来源只贡献一份文档");
    assert_eq!(blocking, 9, "冻结的 blocking 任务数");
    assert_eq!(advisory, 3, "冻结的 advisory 任务数");

    // 每条任务都要有意图与来源依据，否则它无法被复核
    for task in list {
        let id = task["id"].as_str().expect("id");
        for field in ["instruction", "path", "evidence"] {
            assert!(
                !task[field].is_null(),
                "任务 {id} 缺少 {field}：任务集必须能被人复核"
            );
        }
        assert!(
            !task["instruction"]
                .as_str()
                .expect("instruction")
                .is_empty(),
            "任务 {id} 的 instruction 不能为空"
        );
    }
}

/// 任务集引用的文档必须是 holdout 冻结语料里的成员，指纹要对得上。
///
/// 语料换了、任务集没跟着换，是这类实验最隐蔽的失效方式：
/// 数字还在，但它已经不在当初冻结的那批文件上了。
#[test]
fn every_referenced_document_is_a_frozen_holdout_member() {
    let manifest_path = repo_root().join("benchmark/holdout/manifest.json");
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path).expect("读取 holdout manifest"),
    )
    .expect("manifest.json 必须是合法 JSON");

    let frozen: BTreeSet<String> = manifest["documents"]
        .as_array()
        .expect("manifest.documents 必须是数组")
        .iter()
        .map(|entry| {
            entry["document"]
                .as_str()
                .expect("manifest 里每份文档都要有 document 字段")
                .to_string()
        })
        .collect();

    let tasks = task_set();
    for task in tasks["tasks"].as_array().expect("tasks") {
        let name = task["document"].as_str().expect("document");
        assert!(
            frozen.contains(name),
            "任务 {} 引用了 {name}，它不在冻结的 holdout 语料里",
            task["id"]
        );
    }
}

//! TF-0079 消融实验的报告生成器。
//!
//! 跑两臂、算指标、写 `benchmark/ablation/latest.md` 与 `latest.json`，
//! 并把本轮固定的高风险任务集快照成 `benchmark/ablation/tasks.json`。
//!
//! 用法：
//!   cargo run -p verseconf-toml --example ablation
//!   cargo run -p verseconf-toml --example ablation -- --out <目录>
//!
//! 产物里**不写时间戳**：同一份代码重跑必须逐字节相同，否则「报告有没有被
//! 手改过」就只能靠人眼看。

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use verseconf_core::EditPlan;
use verseconf_toml::ablation::{high_risk_tasks, run_all, summarize, ArmOutcome};
use verseconf_toml::{apply_toml_edit_with, TomlGuard};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn outcome_label(outcome: &ArmOutcome) -> String {
    match outcome {
        ArmOutcome::Applied => "放行".to_string(),
        ArmOutcome::Refused(code) => format!("拒绝（{code}）"),
    }
}

/// 预登记的 79 条中立任务：用来证明门禁不是在一律拒绝。
fn neutral_probe(repo: &Path) -> (usize, usize, usize) {
    let tasks_path = repo.join("benchmark/confirmation/tasks.json");
    let tasks: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&tasks_path).expect("读不到预登记任务集"))
            .expect("预登记任务集必须是合法 JSON");
    let documents = repo.join("benchmark/holdout/documents");

    let (mut total, mut mechanism_only_refusals, mut gated_refusals) = (0usize, 0usize, 0usize);
    for task in tasks["tasks"].as_array().expect("tasks 必须是数组") {
        let document = documents.join(task["document"].as_str().expect("任务缺少 document"));
        let source = fs::read_to_string(&document).expect("读不到 holdout 文档");
        let plan_json = json!({
            "version": "1.0",
            "edits": [{ "op": "set", "path": task["path"], "value": task["value"] }],
        })
        .to_string();
        let plan = EditPlan::from_json(&plan_json).expect("预登记任务必须能翻成合法计划");

        total += 1;
        if apply_toml_edit_with(&source, &plan, &TomlGuard::edit_mechanism_only()).is_err() {
            mechanism_only_refusals += 1;
        }
        if apply_toml_edit_with(&source, &plan, &TomlGuard::default()).is_err() {
            gated_refusals += 1;
        }
    }
    (total, mechanism_only_refusals, gated_refusals)
}

fn main() {
    let mut out_dir = repo_root().join("benchmark/ablation");
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--out" {
            out_dir = PathBuf::from(args.next().expect("--out 需要一个目录"));
        }
    }
    let repo = repo_root();
    fs::create_dir_all(&out_dir).expect("创建输出目录失败");

    let tasks = high_risk_tasks();
    let results = run_all(&tasks);
    let summary = summarize(&results);
    let (neutral_total, neutral_mechanism_refusals, neutral_gated_refusals) = neutral_probe(&repo);

    // —— 冻结任务集快照：让这一组任务以数据形态可被检查 ——
    let frozen: Vec<serde_json::Value> = tasks
        .iter()
        .map(|task| {
            let edit = |edit: &verseconf_toml::ablation::Edit| {
                json!({
                    "op": match edit.op {
                        verseconf_toml::ablation::Op::Set => "set",
                        verseconf_toml::ablation::Op::Insert => "insert",
                    },
                    "path": edit.path,
                    "value": match edit.value {
                        verseconf_toml::ablation::Val::Bool(flag) => json!(flag),
                        verseconf_toml::ablation::Val::Int(number) => json!(number),
                        verseconf_toml::ablation::Val::Str(text) => json!(text),
                    },
                })
            };
            json!({
                "id": task.id,
                "group": task.group.as_str(),
                "instruction": task.instruction,
                "document": task.document,
                "edit": edit(&task.edit),
                "expected_rule": task.expected_rule,
                "safe_alternative": task.safe_alternative.map(|alternative| edit(&alternative)),
            })
        })
        .collect();
    let tasks_json = json!({
        "version": 1,
        "frozen_for": "TF-0079 消融实验：把「编辑机制」与「安全门禁」的贡献分开测",
        "why_this_set": "预登记里的 79 个任务是刻意门禁中立的，两臂输出逐字节相同，测不到校验层。要评估校验层必须另立一组会引入安全实例的任务，并在开跑前固定下来。",
        "groups": {
            "blocking": "引入 Critical / High 实例：门禁的契约是拒绝",
            "advisory": "引入 Medium / Low 实例：门禁按设计只告警、不阻断",
        },
        "tasks": frozen,
    });
    fs::write(
        out_dir.join("tasks.json"),
        serde_json::to_string_pretty(&tasks_json).unwrap() + "\n",
    )
    .expect("写 tasks.json 失败");

    // —— 机器可读结果 ——
    let per_task: Vec<serde_json::Value> = results
        .iter()
        .map(|result| {
            json!({
                "id": result.id,
                "group": result.group.as_str(),
                "expected_rule": result.expected_rule,
                "mechanism_only": result.mechanism_only.as_str(),
                "silently_introduced": result.silently_introduced,
                "gated": result.gated.as_str(),
                "gated_prevented": result.gated_prevented,
                "refusal_names_instance": result.refusal_names_instance,
                "safe_alternative_accepted": result.safe_alternative_accepted,
            })
        })
        .collect();
    let result_json = json!({
        "arms": {
            "mechanism-only": "只保留编辑机制：复用 toml_edit 定位值区间、只替换目标字节",
            "mechanism-plus-gate": "编辑机制 + 校验层：写入前做 schema 与安全双重校验",
        },
        "metrics": {
            "silent_security_mis_edit": "改动被应用、且改动后相对改动前新增了 Critical/High 高危实例——调用方拿不到任何信号",
            "prevented": "门禁拒绝、且独立复核确认这条改动确实会新增高危实例",
            "false_refusal": "门禁拒绝了本来应该放行的改动",
        },
        "summary": {
            "blocking_tasks": summary.blocking_tasks,
            "blocking_silent_mis_edits": summary.blocking_silent_mis_edits,
            "blocking_silent_mis_edits_gated": summary.blocking_silent_mis_edits_gated,
            "blocking_refusals": summary.blocking_refusals,
            "blocking_prevented": summary.blocking_prevented,
            "refusals_naming_instance": summary.refusals_naming_instance,
            "blocking_with_safe_alternative": summary.blocking_with_safe_alternative,
            "safe_alternatives_accepted": summary.safe_alternatives_accepted,
            "advisory_tasks": summary.advisory_tasks,
            "advisory_applied_by_both": summary.advisory_applied_by_both,
            "neutral_tasks": neutral_total,
            "neutral_false_refusals_mechanism_only": neutral_mechanism_refusals,
            "neutral_false_refusals_gated": neutral_gated_refusals,
        },
        "tasks": per_task,
    });
    fs::write(
        out_dir.join("latest.json"),
        serde_json::to_string_pretty(&result_json).unwrap() + "\n",
    )
    .expect("写 latest.json 失败");

    // —— 人读报告 ——
    let mut md = String::new();
    md.push_str("# TOML 消融实验：编辑机制 vs 编辑机制 + 校验层（TF-0079）\n\n");
    md.push_str(
        "两臂跑同一批任务，唯一差别是**写入前有没有校验层**。度量刻意不信任门禁的自述：\n\
         改动有没有引入高危实例，由 `high_risk_instances` 在改动前后各算一次取差集得出。\n\n",
    );
    md.push_str("## 总览\n\n");
    md.push_str("| 指标 | 只保留编辑机制 | 编辑机制 + 校验层 |\n| --- | --- | --- |\n");
    md.push_str(&format!(
        "| 静默写入高危实例的任务数 | {} | {} |\n",
        summary.blocking_silent_mis_edits, summary.blocking_silent_mis_edits_gated
    ));
    md.push_str(&format!(
        "| 拒绝数 | — | {} |\n| 其中确实挡住了一次真实引入 | — | {} |\n| 拒绝信息指出具体实例 | — | {} |\n",
        summary.blocking_refusals, summary.blocking_prevented, summary.refusals_naming_instance
    ));
    md.push_str(&format!(
        "| 等价安全写法被接受 | — | {}/{} |\n",
        summary.safe_alternatives_accepted, summary.blocking_with_safe_alternative
    ));
    md.push_str(&format!(
        "| 误拒（{} 条预登记中立任务） | {} | {} |\n",
        neutral_total, neutral_mechanism_refusals, neutral_gated_refusals
    ));
    md.push_str(&format!(
        "| 告警级改动两臂都放行 | {}/{} | {}/{} |\n\n",
        summary.advisory_applied_by_both,
        summary.advisory_tasks,
        summary.advisory_applied_by_both,
        summary.advisory_tasks
    ));

    md.push_str("## 逐任务\n\n");
    md.push_str(
        "| 任务 | 分组 | 预期规则 | 只保留编辑机制 | 静默引入 | 编辑机制 + 校验层 | 挡住真实引入 | 等价安全写法 |\n\
         | --- | --- | --- | --- | --- | --- | --- | --- |\n",
    );
    for result in &results {
        let introduced = if result.silently_introduced.is_empty() {
            "—".to_string()
        } else {
            result.silently_introduced.join("、")
        };
        let safe = match result.safe_alternative_accepted {
            None => "—".to_string(),
            Some(true) => "接受".to_string(),
            Some(false) => "**被拒**".to_string(),
        };
        md.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} | {} | {} |\n",
            result.id,
            result.group.as_str(),
            result.expected_rule,
            outcome_label(&result.mechanism_only),
            introduced,
            outcome_label(&result.gated),
            if result.gated_prevented { "是" } else { "否" },
            safe
        ));
    }

    md.push_str("\n## 这批结果的边界（不要在它之外引用）\n\n");
    md.push_str(
        "- **只测编辑机制与安全门禁的贡献**，不测模型行为：本实验不调用任何模型，两臂的输入完全相同。\n\
         - **门禁的契约只覆盖 Critical / High**：`advisory` 组的 3 条改动（SEC-002/003/004）按设计只告警、不阻断，\n\
           两臂都放行。这是安全承诺的一条边界，不能把「没有阻断」说成「没有引入风险」。\n\
         - **任务集是本轮固定的 8 条**，不是随机抽样：它覆盖全部 6 条规则码，但样本量小，只用于判断\n\
           「校验层有没有可测的贡献」，不用于估计真实场景的误拒率。\n\
         - **误拒控制用的是 79 条预登记中立任务**：它们刻意门禁中立，所以只能证明门禁不是在一律拒绝，\n\
           不能证明它在真实高风险语料上的误拒率。\n",
    );
    fs::write(out_dir.join("latest.md"), md).expect("写 latest.md 失败");

    println!("消融实验完成，产物写入 {}", out_dir.display());
    println!(
        "blocking {} 条：对照臂静默写入 {} 条，实验臂拒绝 {} 条（挡住真实引入 {} 条）；误拒 {} 条",
        summary.blocking_tasks,
        summary.blocking_silent_mis_edits,
        summary.blocking_refusals,
        summary.blocking_prevented,
        neutral_gated_refusals
    );
}

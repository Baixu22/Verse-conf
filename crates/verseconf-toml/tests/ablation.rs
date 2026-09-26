//! TF-0079：消融实验的不变量。
//!
//! 这份测试回答的是项目最该被追问的那个问题：**校验层到底贡献了什么**。
//! 它把两臂跑在同一批任务上，并且刻意用三条互相独立的检查把结论钉住：
//!
//! 1. 对照臂（只保留编辑机制）必须**静默**写入高危实例——如果它不写，
//!    说明这批任务根本没命中风险，后面的收益就是假的；
//! 2. 实验臂（编辑机制 + 校验层）必须拒绝，而且拒绝信息要指出具体实例；
//! 3. 拒绝不能是死路：同一个意图的等价安全写法必须被接受；
//!    同时 79 条预登记的中立任务两臂都要放行——否则「拒绝得多」会被误当成安全收益。
//!
//! 度量不信任门禁的自述：有没有引入高危实例由 `high_risk_instances` 在改动前后
//! 各算一次取差集得出。

use std::fs;
use std::path::PathBuf;

use serde_json::json;
use verseconf_core::EditPlan;
use verseconf_toml::ablation::{
    high_risk_tasks, plan_of, run_all, run_task, summarize, ArmOutcome, TaskGroup,
};
use verseconf_toml::{apply_toml_edit_with, audit_toml, TomlGuard};

fn benchmark_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmark")
}

/// 预登记的 79 条任务：刻意门禁中立，用来测「门禁是不是在一律拒绝」。
fn neutral_probe() -> (usize, usize, usize) {
    let tasks: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(benchmark_dir().join("confirmation/tasks.json")).unwrap(),
    )
    .unwrap();
    let documents = benchmark_dir().join("holdout/documents");

    let mut total = 0usize;
    let mut mechanism_only_refusals = 0usize;
    let mut gated_refusals = 0usize;

    for task in tasks["tasks"].as_array().unwrap() {
        let document = documents.join(task["document"].as_str().unwrap());
        let source = fs::read_to_string(&document).unwrap();
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

#[test]
fn every_high_risk_task_really_hits_the_rule_it_claims() {
    for task in high_risk_tasks() {
        let result = run_task(&task);
        match task.group {
            TaskGroup::Blocking => {
                assert!(
                    !result.silently_introduced.is_empty(),
                    "任务 {} 声称引入 {}，但只保留编辑机制时并没有新增任何高危实例——\
                     这批任务没命中风险，消融结论就不成立。实际引入：{:?}",
                    task.id,
                    task.expected_rule,
                    result.silently_introduced
                );
                assert!(
                    result
                        .silently_introduced
                        .iter()
                        .any(|instance| instance.starts_with(task.expected_rule)),
                    "任务 {} 声称引入 {}，实际引入的是 {:?}",
                    task.id,
                    task.expected_rule,
                    result.silently_introduced
                );
            }
            TaskGroup::Advisory => {
                // 告警级规则不进高危实例集合，但必须真的出现在审计结果里，
                // 而且改动前不存在——否则「门禁没拦」测的是另一回事。
                let outcome = apply_toml_edit_with(
                    task.document,
                    &plan_of(&task.edit),
                    &TomlGuard::edit_mechanism_only(),
                )
                .expect("对照臂必须放行告警级改动");
                let before = audit_toml(task.document).expect("源文档必须可审计");
                let after = audit_toml(&outcome.source).expect("结果必须可审计");
                let count = |report: &verseconf_core::AuditReport| {
                    report
                        .findings
                        .iter()
                        .filter(|finding| finding.rule_id == task.expected_rule)
                        .count()
                };
                assert!(
                    count(&after) > count(&before),
                    "任务 {} 声称引入 {}，但审计结果里这条规则没有变多",
                    task.id,
                    task.expected_rule
                );
            }
        }
    }
}

#[test]
fn the_mechanism_alone_writes_high_risk_instances_silently() {
    let results = run_all(&high_risk_tasks());
    let summary = summarize(&results);
    assert_eq!(
        summary.blocking_silent_mis_edits, summary.blocking_tasks,
        "每一条 blocking 任务在对照臂下都必须是静默误写，实际 {}/{}",
        summary.blocking_silent_mis_edits, summary.blocking_tasks
    );
    assert!(
        summary.blocking_tasks >= 5,
        "blocking 任务太少，结论不可外推"
    );
}

#[test]
fn the_gate_refuses_every_introduction_it_would_have_missed() {
    let results = run_all(&high_risk_tasks());
    let summary = summarize(&results);

    assert_eq!(
        summary.blocking_silent_mis_edits_gated, 0,
        "加上校验层后不允许再出现静默写入的高危实例"
    );
    assert_eq!(
        summary.blocking_refusals, summary.blocking_tasks,
        "每一条 blocking 任务都必须被门禁拒绝"
    );
    assert_eq!(
        summary.blocking_prevented, summary.blocking_tasks,
        "每一次拒绝都必须确实挡住了一次真实引入——拒绝次数本身不算收益"
    );
    assert_eq!(
        summary.refusals_naming_instance, summary.blocking_tasks,
        "拒绝信息必须指出是哪个实例，而不只是哪条规则"
    );

    for result in &results {
        if result.group == TaskGroup::Blocking {
            assert!(
                result.gated.refused_with("security_rejected"),
                "任务 {} 的拒绝码应当是 security_rejected，实际 {}",
                result.id,
                result.gated.as_str()
            );
            assert_eq!(
                result.mechanism_only,
                ArmOutcome::Applied,
                "任务 {} 的对照臂必须放行，否则这条任务测不出机制与门禁的差别",
                result.id
            );
        }
    }
}

#[test]
fn a_refusal_is_not_a_dead_end() {
    let results = run_all(&high_risk_tasks());
    let summary = summarize(&results);

    assert_eq!(
        summary.safe_alternatives_accepted, summary.blocking_with_safe_alternative,
        "每一条声明了等价安全写法的任务，那个写法都必须被门禁接受"
    );
    assert!(
        summary.blocking_with_safe_alternative >= 5,
        "至少要给每条 blocking 任务配一个等价安全写法，否则无法区分「拒绝」与「死路」"
    );

    for task in high_risk_tasks() {
        if task.group != TaskGroup::Blocking {
            continue;
        }
        let alternative = task
            .safe_alternative
            .expect("blocking 任务必须给出等价安全写法");
        assert!(
            apply_toml_edit_with(task.document, &plan_of(&alternative), &TomlGuard::default())
                .is_ok(),
            "任务 {} 的等价安全写法被门禁拒绝了，拒绝就成了死路",
            task.id
        );
    }
}

#[test]
fn the_gate_does_not_refuse_the_preregistered_neutral_tasks() {
    let (total, mechanism_only_refusals, gated_refusals) = neutral_probe();

    assert_eq!(total, 79, "预登记任务数变了，说明冻结物被改过");
    assert_eq!(mechanism_only_refusals, 0, "对照臂不该拒绝中立任务");
    assert_eq!(
        gated_refusals, 0,
        "门禁拒绝了 {gated_refusals} 条刻意门禁中立的任务——这不是安全收益，是误拒"
    );
}

#[test]
fn advisory_rules_are_warned_about_but_not_blocked() {
    let results = run_all(&high_risk_tasks());
    let summary = summarize(&results);

    assert_eq!(
        summary.advisory_tasks, summary.advisory_applied_by_both,
        "告警级（Medium / Low）改动按设计两臂都放行；这条边界必须被测出来，\
         不能把「没有阻断」说成「没有引入风险」"
    );
    assert!(summary.advisory_tasks >= 3, "告警级样本太少");
}

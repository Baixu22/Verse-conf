//! TF-0079：消融实验——把「编辑机制」与「安全门禁」的贡献分开测。
//!
//! ## 为什么要有这一层
//!
//! 预登记（`benchmark/confirmation/PREREGISTRATION.md`）里的 79 个任务是**刻意门禁中立**的：
//! 两臂输出逐字节相同，所以那份实验测不到校验层。要评估校验层的贡献，必须另立一组
//! **会引入安全实例**的任务，并在开跑之前把这一组固定下来——否则「哪条算高风险」会变成
//! 看完结果再挑。
//!
//! ## 两组任务，两种预期
//!
//! - `Blocking`：改动引入的是 **Critical / High** 实例。门禁的契约是拒绝这类改动，
//!   所以对照臂会**静默**写入一个高危项，实验臂会拒绝。
//! - `Advisory`：改动引入的是 **Medium / Low** 实例。门禁按设计只告警、不阻断
//!   （TF-0053 的误拒分档），所以两臂都会应用。这是安全承诺的一条边界，必须如实报告，
//!   不能把「没有阻断」说成「没有引入风险」。
//!
//! ## 度量口径
//!
//! 关键是**不信任门禁的自述**：改动有没有引入高危实例，由实验代码用
//! `high_risk_instances` 在改动前后各算一次、再取差集得出，而不是看门禁有没有报错。
//! 门禁自己报错不算收益——只有「拒绝确实挡住了一次真实引入」才算。

use verseconf_core::{
    high_risk_instances, introduced_high_risk_instances, render_risk_instance, EditPlan,
};

use crate::{apply_toml_edit_with, audit_toml, TomlGuard};

/// 任务分组：按引入实例的严重度分，而不是按「我觉得危不危险」分。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskGroup {
    /// 引入 Critical / High 实例：门禁的契约是拒绝
    Blocking,
    /// 引入 Medium / Low 实例：门禁按设计只告警
    Advisory,
}

impl TaskGroup {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskGroup::Blocking => "blocking",
            TaskGroup::Advisory => "advisory",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Set,
    Insert,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Val {
    Bool(bool),
    Int(i64),
    Str(&'static str),
}

/// 一条编辑意图（消融任务只需要 set / insert 单值这一种形态）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edit {
    pub op: Op,
    pub path: &'static [&'static str],
    pub value: Val,
}

/// 一条高风险任务：它请求的改动**本身就是**引入风险的改动。
#[derive(Debug, Clone, Copy)]
pub struct AblationTask {
    pub id: &'static str,
    pub group: TaskGroup,
    /// 真实场景里调用方会说的话
    pub instruction: &'static str,
    pub document: &'static str,
    pub edit: Edit,
    /// 这条改动预期引入的规则码，用于交叉核对「任务真的命中了它声称的特征」
    pub expected_rule: &'static str,
    /// 同一个意图的等价安全写法；门禁必须接受它，否则拒绝就是死路
    pub safe_alternative: Option<Edit>,
}

/// 消融实验固定的高风险任务集。
///
/// 这一组在开跑前写死；改了它就必须作废本轮数据重新跑。
pub fn high_risk_tasks() -> Vec<AblationTask> {
    vec![
        // ——— Blocking：Critical / High，门禁契约是拒绝 ———
        AblationTask {
            id: "disable-tls-verification",
            group: TaskGroup::Blocking,
            instruction: "把 tls_verify 改成 false",
            document: "tls_verify = true\n",
            edit: Edit {
                op: Op::Set,
                path: &["tls_verify"],
                value: Val::Bool(false),
            },
            expected_rule: "SEC-005",
            safe_alternative: Some(Edit {
                op: Op::Set,
                path: &["tls_verify"],
                value: Val::Bool(true),
            }),
        },
        AblationTask {
            id: "second-instance-of-an-existing-rule",
            group: TaskGroup::Blocking,
            instruction: "文件里已经关了一处证书校验，再把 tls.verify 也关掉",
            document: "ssl_verify = false\n\n[tls]\nverify = true\n",
            edit: Edit {
                op: Op::Set,
                path: &["tls", "verify"],
                value: Val::Bool(false),
            },
            expected_rule: "SEC-005",
            safe_alternative: Some(Edit {
                op: Op::Set,
                path: &["tls", "verify"],
                value: Val::Bool(true),
            }),
        },
        AblationTask {
            id: "insert-weak-hash-algorithm",
            group: TaskGroup::Blocking,
            instruction: "新增 algorithm = \"md5\"",
            document: "name = \"signer\"\n",
            edit: Edit {
                op: Op::Insert,
                path: &["algorithm"],
                value: Val::Str("md5"),
            },
            expected_rule: "SEC-001",
            safe_alternative: Some(Edit {
                op: Op::Insert,
                path: &["algorithm"],
                value: Val::Str("sha256"),
            }),
        },
        AblationTask {
            id: "insert-plaintext-password",
            group: TaskGroup::Blocking,
            instruction: "新增 db_password = \"hunter2\"",
            document: "host = \"db.internal\"\n",
            edit: Edit {
                op: Op::Insert,
                path: &["db_password"],
                value: Val::Str("hunter2"),
            },
            expected_rule: "SEC-SENS-001",
            safe_alternative: Some(Edit {
                op: Op::Insert,
                path: &["db_password"],
                value: Val::Str("${DB_PASSWORD}"),
            }),
        },
        AblationTask {
            id: "replace-env-reference-with-live-key",
            group: TaskGroup::Blocking,
            instruction: "把 api_key 从环境变量引用改成写死的真实密钥",
            document: "api_key = \"${API_KEY}\"\n",
            edit: Edit {
                op: Op::Set,
                path: &["api_key"],
                value: Val::Str("sk-live-9f8e7d6c5b4a3210"),
            },
            expected_rule: "SEC-SENS-001",
            safe_alternative: Some(Edit {
                op: Op::Set,
                path: &["api_key"],
                value: Val::Str("${API_KEY}"),
            }),
        },
        // ——— Advisory：Medium / Low，门禁按设计只告警 ———
        AblationTask {
            id: "insert-telnet-port",
            group: TaskGroup::Advisory,
            instruction: "新增 port = 23（telnet）",
            document: "name = \"svc\"\n",
            edit: Edit {
                op: Op::Insert,
                path: &["port"],
                value: Val::Int(23),
            },
            expected_rule: "SEC-002",
            safe_alternative: None,
        },
        AblationTask {
            id: "enable-debug-mode",
            group: TaskGroup::Advisory,
            instruction: "把 debug 改成 true",
            document: "debug = false\n",
            edit: Edit {
                op: Op::Set,
                path: &["debug"],
                value: Val::Bool(true),
            },
            expected_rule: "SEC-003",
            safe_alternative: None,
        },
        AblationTask {
            id: "bind-wildcard-host",
            group: TaskGroup::Advisory,
            instruction: "把 host 改成 0.0.0.0",
            document: "host = \"127.0.0.1\"\n",
            edit: Edit {
                op: Op::Set,
                path: &["host"],
                value: Val::Str("0.0.0.0"),
            },
            expected_rule: "SEC-004",
            safe_alternative: None,
        },
    ]
}

fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// 把一条消融编辑翻成契约计划。这里手工拼 JSON 是为了不给这个 crate 增加运行时依赖：
/// `serde_json` 只在 dev-dependencies 里，真实写入路径不需要它。
pub fn plan_of(edit: &Edit) -> EditPlan {
    let op = match edit.op {
        Op::Set => "set",
        Op::Insert => "insert",
    };
    let path = edit
        .path
        .iter()
        .map(|segment| json_string(segment))
        .collect::<Vec<_>>()
        .join(",");
    let value = match edit.value {
        Val::Bool(flag) => flag.to_string(),
        Val::Int(number) => number.to_string(),
        Val::Str(text) => json_string(text),
    };
    let json =
        format!(r#"{{"version":"1.0","edits":[{{"op":"{op}","path":[{path}],"value":{value}}}]}}"#);
    EditPlan::from_json(&json).expect("消融任务里的计划必须合法")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArmOutcome {
    Applied,
    Refused(String),
}

impl ArmOutcome {
    pub fn as_str(&self) -> String {
        match self {
            ArmOutcome::Applied => "applied".to_string(),
            ArmOutcome::Refused(code) => format!("refused:{code}"),
        }
    }

    pub fn applied(&self) -> bool {
        matches!(self, ArmOutcome::Applied)
    }

    pub fn refused_with(&self, code: &str) -> bool {
        matches!(self, ArmOutcome::Refused(found) if found == code)
    }
}

#[derive(Debug, Clone)]
pub struct TaskResult {
    pub id: String,
    pub group: TaskGroup,
    pub expected_rule: String,
    /// 只保留编辑机制时的结果
    pub mechanism_only: ArmOutcome,
    /// 只保留编辑机制时**静默**引入的高危实例（独立复核得出，不信任门禁）
    pub silently_introduced: Vec<String>,
    /// 编辑机制 + 校验层时的结果
    pub gated: ArmOutcome,
    /// 门禁的拒绝是否真的挡住了一次引入（拒绝 ≠ 收益，只有挡住才算）
    pub gated_prevented: bool,
    /// 拒绝码是否指出是哪个实例
    pub refusal_names_instance: bool,
    /// 等价安全写法是否被门禁接受（None = 这条任务没有等价安全写法）
    pub safe_alternative_accepted: Option<bool>,
}

/// 独立复核：改动前后各算一次高危实例，取新增。
fn introduced_instances(source: &str, after: &str) -> Vec<String> {
    let Ok(before_report) = audit_toml(source) else {
        return Vec::new();
    };
    let Ok(after_report) = audit_toml(after) else {
        return Vec::new();
    };
    let before = high_risk_instances(&before_report);
    let after = high_risk_instances(&after_report);
    introduced_high_risk_instances(&before, &after)
        .iter()
        .map(render_risk_instance)
        .collect()
}

pub fn run_task(task: &AblationTask) -> TaskResult {
    let plan = plan_of(&task.edit);

    let mechanism_only =
        match apply_toml_edit_with(task.document, &plan, &TomlGuard::edit_mechanism_only()) {
            Ok(outcome) => {
                let silently_introduced = introduced_instances(task.document, &outcome.source);
                (ArmOutcome::Applied, silently_introduced)
            }
            Err(refusal) => (ArmOutcome::Refused(refusal.code().to_string()), Vec::new()),
        };

    let mut refusal_names_instance = false;
    let gated = match apply_toml_edit_with(task.document, &plan, &TomlGuard::default()) {
        Ok(_) => ArmOutcome::Applied,
        Err(refusal) => {
            if let verseconf_core::EditRefusal::SecurityRejected { instances, .. } = &refusal {
                refusal_names_instance = !instances.is_empty();
            }
            ArmOutcome::Refused(refusal.code().to_string())
        }
    };

    let gated_prevented = !gated.applied() && !mechanism_only.1.is_empty();

    let safe_alternative_accepted = task.safe_alternative.map(|alternative| {
        apply_toml_edit_with(task.document, &plan_of(&alternative), &TomlGuard::default()).is_ok()
    });

    TaskResult {
        id: task.id.to_string(),
        group: task.group,
        expected_rule: task.expected_rule.to_string(),
        mechanism_only: mechanism_only.0,
        silently_introduced: mechanism_only.1,
        gated,
        gated_prevented,
        refusal_names_instance,
        safe_alternative_accepted,
    }
}

pub fn run_all(tasks: &[AblationTask]) -> Vec<TaskResult> {
    tasks.iter().map(run_task).collect()
}

/// 消融的汇总数字。每一个都对应验收标准里的一句话。
#[derive(Debug, Clone, Default)]
pub struct AblationSummary {
    pub blocking_tasks: usize,
    /// 只保留编辑机制时，静默写入高危实例的任务数
    pub blocking_silent_mis_edits: usize,
    /// 加上校验层后仍然静默写入高危实例的任务数
    pub blocking_silent_mis_edits_gated: usize,
    /// 门禁拒绝的任务数
    pub blocking_refusals: usize,
    /// 拒绝中确实挡住了一次真实引入的数量（拒绝 ≠ 收益）
    pub blocking_prevented: usize,
    /// 拒绝信息能指出具体实例的数量
    pub refusals_naming_instance: usize,
    pub blocking_with_safe_alternative: usize,
    pub safe_alternatives_accepted: usize,
    pub advisory_tasks: usize,
    /// 告警级改动两臂都放行的数量（门禁按设计只告警）
    pub advisory_applied_by_both: usize,
}

pub fn summarize(results: &[TaskResult]) -> AblationSummary {
    let mut summary = AblationSummary::default();
    for result in results {
        match result.group {
            TaskGroup::Blocking => {
                summary.blocking_tasks += 1;
                if !result.silently_introduced.is_empty() && result.mechanism_only.applied() {
                    summary.blocking_silent_mis_edits += 1;
                }
                if !result.silently_introduced.is_empty() && result.gated.applied() {
                    summary.blocking_silent_mis_edits_gated += 1;
                }
                if !result.gated.applied() {
                    summary.blocking_refusals += 1;
                }
                if result.gated_prevented {
                    summary.blocking_prevented += 1;
                }
                if result.refusal_names_instance {
                    summary.refusals_naming_instance += 1;
                }
                if let Some(accepted) = result.safe_alternative_accepted {
                    summary.blocking_with_safe_alternative += 1;
                    if accepted {
                        summary.safe_alternatives_accepted += 1;
                    }
                }
            }
            TaskGroup::Advisory => {
                summary.advisory_tasks += 1;
                if result.mechanism_only.applied() && result.gated.applied() {
                    summary.advisory_applied_by_both += 1;
                }
            }
        }
    }
    summary
}

//! Agent 编辑保真度基准。
//!
//! 固定语料 + 固定判定脚本 + 可插拔策略。任何人把语料与判定跑一遍都会得到同一组数字，
//! 因为判定不依赖任何被测实现：
//!
//! - **附带损伤**的定义直接来自意图执行协议的验收口径「改动之外的字节零变化」，
//!   只由原始源码与改动区域决定。三种操作各有其形态：
//!   - `set`：原值区间的**前缀与后缀**必须逐字节不变；
//!   - `insert`：**原文的每一个字节**都必须落在结果的最长公共前缀或后缀里
//!     （即原文没有被改动，只是多了一段）；
//!   - `delete`：**结果的每一个字节**都必须落在原文的最长公共前缀或后缀里
//!     （即只是少了一段，其余没被动过）；
//! - **正确**的定义是「目标字段的值确实变成了期望值」；
//! - **拒绝**必须给出期望的稳定错误码，拒绝理由不对不算通过——否则"一律拒绝"也能拿满分。

pub mod judge;
pub mod report;
pub mod strategies;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use verseconf_core::{EditValue, PathSegment};

/// 判定口径的版本。判定规则变化时必须递增，否则历史结果不可比。
pub const METHODOLOGY_VERSION: &str = "1.2";

/// 固定语料：一组文档 + 一组编辑任务
#[derive(Debug, Clone, Deserialize)]
pub struct Corpus {
    pub version: String,
    pub documents: BTreeMap<String, String>,
    pub tasks: Vec<Task>,
}

/// 一个编辑任务
#[derive(Debug, Clone, Deserialize)]
pub struct Task {
    pub id: String,
    pub document: String,
    pub description: String,
    /// 编辑意图契约（原始 JSON，交给策略自己解析）
    pub plan: serde_json::Value,
    pub expect: Expectation,
}

/// 期望结果
#[derive(Debug, Clone, Deserialize)]
pub struct Expectation {
    /// `applied` 或 `refused`
    pub outcome: String,
    /// `applied` 时目标字段的路径（单条编辑）
    #[serde(default)]
    pub path: Vec<serde_json::Value>,
    /// `applied` 时目标字段的期望值（单条编辑）
    #[serde(default)]
    pub value: Option<serde_json::Value>,
    /// `refused` 时期望的稳定错误码
    #[serde(default)]
    pub code: Option<String>,
    /// 跨文件任务：期望被改动的文件（文档键）。
    ///
    /// 非空时该任务走跨 @include 的评测路径：策略从 document 指向的入口文件出发，
    /// 允许顺着 include 走到别的文件；判定拿**这个文件**的内容做附带损伤比对，
    /// 并且要求策略改动的正是这个文件。
    #[serde(default)]
    pub target_file: Option<String>,
    /// **多条编辑**计划里每个目标的期望。
    ///
    /// 非空时走多编辑判定：每个目标都要成立，且改动必须**只**落在这些目标上。
    /// 为空时沿用单条编辑的判定路径，历史任务的语义与数字都不变。
    #[serde(default)]
    pub targets: Vec<TargetExpect>,
}

/// 多编辑计划里的一个目标
#[derive(Debug, Clone, Deserialize)]
pub struct TargetExpect {
    /// 目标字段的路径
    pub path: Vec<serde_json::Value>,
    /// 期望值；`delete` 目标省略
    #[serde(default)]
    pub value: Option<serde_json::Value>,
}

impl TargetExpect {
    pub fn path(&self) -> Result<Vec<PathSegment>, String> {
        serde_json::from_value(serde_json::Value::Array(self.path.clone()))
            .map_err(|error| format!("expect.targets 里的 path 无法解析：{error}"))
    }

    pub fn value(&self) -> Result<Option<EditValue>, String> {
        match &self.value {
            None => Ok(None),
            Some(raw) => serde_json::from_value(raw.clone())
                .map(Some)
                .map_err(|error| format!("expect.targets 里的 value 无法解析：{error}")),
        }
    }
}

impl Task {
    pub fn expects_refusal(&self) -> bool {
        self.expect.outcome == "refused"
    }

    pub fn plan_json(&self) -> String {
        self.plan.to_string()
    }

    /// 编辑计划里的操作类型（`set` / `insert` / `delete`）。
    ///
    /// 判定需要知道是哪种操作，因为"改动之外字节零变化"在三种操作下的
    /// 形态不同：替换是「前后缀不变」，插入是「原文全被前后缀覆盖」，
    /// 删除是「新文全被前后缀覆盖」。
    pub fn plan_op(&self) -> Result<String, String> {
        self.plan
            .get("edits")
            .and_then(|edits| edits.as_array())
            .and_then(|edits| edits.first())
            .and_then(|edit| edit.get("op"))
            .and_then(|op| op.as_str())
            .map(str::to_string)
            .ok_or_else(|| format!("任务 {} 的 plan 里读不到 edits[0].op", self.id))
    }

    /// 是否是多条编辑的任务
    pub fn is_multi_edit(&self) -> bool {
        !self.expect.targets.is_empty()
    }

    pub fn expect_path(&self) -> Result<Vec<PathSegment>, String> {
        serde_json::from_value(serde_json::Value::Array(self.expect.path.clone()))
            .map_err(|error| format!("任务 {} 的 expect.path 无法解析：{}", self.id, error))
    }

    pub fn expect_value(&self) -> Result<Option<EditValue>, String> {
        match &self.expect.value {
            None => Ok(None),
            Some(raw) => serde_json::from_value(raw.clone())
                .map(Some)
                .map_err(|error| format!("任务 {} 的 expect.value 无法解析：{}", self.id, error)),
        }
    }
}

impl Corpus {
    pub fn load(corpus_dir: &Path) -> Result<Self, String> {
        let tasks_path = corpus_dir.join("tasks.json");
        let text = std::fs::read_to_string(&tasks_path)
            .map_err(|error| format!("读不到 {}：{}", tasks_path.display(), error))?;
        let corpus: Corpus = serde_json::from_str(&text)
            .map_err(|error| format!("{} 不是合法语料：{}", tasks_path.display(), error))?;

        if corpus.version != METHODOLOGY_VERSION {
            return Err(format!(
                "语料版本 {} 与判定版本 {} 不一致；口径变化后必须同步更新，否则结果不可比",
                corpus.version, METHODOLOGY_VERSION
            ));
        }
        if corpus.tasks.is_empty() {
            return Err("语料里没有任何任务".to_string());
        }
        Ok(corpus)
    }

    pub fn document_path(&self, corpus_dir: &Path, key: &str) -> Result<PathBuf, String> {
        self.documents
            .get(key)
            .map(|relative| corpus_dir.join(relative))
            .ok_or_else(|| format!("语料里没有文档 '{}'", key))
    }

    pub fn source(&self, corpus_dir: &Path, key: &str) -> Result<String, String> {
        let path = self.document_path(corpus_dir, key)?;
        std::fs::read_to_string(&path)
            .map_err(|error| format!("读不到 {}：{}", path.display(), error))
    }

    /// 语料指纹（FNV-1a 64）：所有文档字节 + 任务定义。
    ///
    /// 第三方重跑时用它确认跑的是同一份语料；数字对不上时先比对指纹。
    pub fn fingerprint(&self, corpus_dir: &Path) -> Result<String, String> {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for key in self.documents.keys() {
            hash = fnv1a64(key.as_bytes(), hash);
            hash = fnv1a64(self.source(corpus_dir, key)?.as_bytes(), hash);
        }
        let tasks_path = corpus_dir.join("tasks.json");
        let tasks = std::fs::read(&tasks_path)
            .map_err(|error| format!("读不到 {}：{}", tasks_path.display(), error))?;
        hash = fnv1a64(&tasks, hash);
        Ok(format!("{hash:016x}"))
    }
}

fn fnv1a64(bytes: &[u8], mut hash: u64) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// 一个任务在一个策略下的判定结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// 改对了，且改动区间之外的字节零变化
    Correct,
    /// 改对了，但改动区间之外的字节也变了
    Collateral,
    /// 值不对、目标不对，或结果无法解析
    Wrong,
    /// 该拒的拒了，且错误码正确
    RefusedCorrectly,
    /// 该改的拒了（误拒）
    RefusedWrongly,
    /// 该拒的改了（该拒未拒）
    AppliedWrongly,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Correct => "正确",
            Outcome::Collateral => "附带损伤",
            Outcome::Wrong => "误改",
            Outcome::RefusedCorrectly => "正确拒绝",
            Outcome::RefusedWrongly => "误拒",
            Outcome::AppliedWrongly => "该拒未拒",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskResult {
    pub task_id: String,
    pub description: String,
    pub expected: String,
    pub outcome: Outcome,
    pub detail: String,
    /// 注释与 #@ 元数据片段是否全部保留
    pub comments_kept: bool,
    /// 同一任务重复运行与逆序运行的结果是否逐字节一致
    pub deterministic: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Metrics {
    pub applied_tasks: usize,
    pub refused_tasks: usize,
    pub correct: usize,
    pub collateral: usize,
    pub wrong: usize,
    pub refused_correctly: usize,
    pub refused_wrongly: usize,
    pub applied_wrongly: usize,
    pub comments_kept: usize,
    pub deterministic: bool,
}

impl Metrics {
    pub fn correct_rate(&self) -> f64 {
        ratio(self.correct, self.applied_tasks)
    }

    pub fn collateral_rate(&self) -> f64 {
        ratio(self.collateral, self.applied_tasks)
    }

    pub fn wrong_rate(&self) -> f64 {
        ratio(
            self.wrong + self.applied_wrongly,
            self.applied_tasks + self.refused_tasks,
        )
    }

    pub fn refusal_accuracy(&self) -> f64 {
        ratio(self.refused_correctly, self.refused_tasks)
    }

    pub fn comment_fidelity(&self) -> f64 {
        ratio(self.comments_kept, self.applied_tasks)
    }
}

fn ratio(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategyReport {
    pub name: String,
    pub summary: String,
    pub metrics: Metrics,
    pub results: Vec<TaskResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunReport {
    pub methodology_version: String,
    pub corpus_fingerprint: String,
    pub document_count: usize,
    pub task_count: usize,
    pub strategies: Vec<StrategyReport>,
}

/// 跑一遍完整基准。
///
/// `repeat` 是每个任务的重复次数（至少 2）：同一输入重复跑、以及把任务顺序倒过来跑，
/// 结果都必须逐字节一致，否则该策略的确定性记为 false。
pub fn run(corpus_dir: &Path, repeat: usize) -> Result<RunReport, String> {
    let corpus = Corpus::load(corpus_dir)?;
    let repeat = repeat.max(2);

    // 先把所有源码读出来：语料是固定的，策略不该碰文件系统。
    let mut sources: BTreeMap<String, String> = BTreeMap::new();
    for key in corpus.documents.keys() {
        sources.insert(key.clone(), corpus.source(corpus_dir, key)?);
    }

    // 跨文件任务需要的文件树：相对路径 → 内容。仍然不碰磁盘——语料就是文件树。
    let tree: strategies::FileTree = corpus
        .documents
        .iter()
        .filter_map(|(key, relative)| {
            sources
                .get(key)
                .map(|source| (PathBuf::from(relative), source.clone()))
        })
        .collect();

    let mut strategies = Vec::new();
    for strategy in strategies::all() {
        let mut results = Vec::new();
        let mut forward: Vec<RunOutcome> = Vec::new();

        for task in &corpus.tasks {
            let first = evaluate(strategy.as_ref(), task, &corpus, &sources, &tree)?;
            let mut deterministic = true;
            for _ in 1..repeat {
                if evaluate(strategy.as_ref(), task, &corpus, &sources, &tree)? != first {
                    deterministic = false;
                }
            }

            let judgement = judge_task(task, &first, &corpus, &sources)?;
            results.push(TaskResult {
                task_id: task.id.clone(),
                description: task.description.clone(),
                expected: if task.expects_refusal() {
                    format!("refused({})", task.expect.code.clone().unwrap_or_default())
                } else {
                    "applied".to_string()
                },
                outcome: judgement.outcome,
                detail: judgement.detail,
                comments_kept: judgement.comments_kept,
                deterministic,
            });
            forward.push(first);
        }

        // 逆序再跑一遍：捕捉策略内部的顺序依赖或共享状态。
        let mut reverse: Vec<RunOutcome> = Vec::new();
        for task in corpus.tasks.iter().rev() {
            reverse.push(evaluate(strategy.as_ref(), task, &corpus, &sources, &tree)?);
        }
        reverse.reverse();
        let order_independent = forward == reverse;

        let metrics = summarize(&corpus, &results, order_independent);
        strategies.push(StrategyReport {
            name: strategy.name().to_string(),
            summary: strategy.summary().to_string(),
            metrics,
            results,
        });
    }

    Ok(RunReport {
        methodology_version: METHODOLOGY_VERSION.to_string(),
        corpus_fingerprint: corpus.fingerprint(corpus_dir)?,
        document_count: corpus.documents.len(),
        task_count: corpus.tasks.len(),
        strategies,
    })
}

/// 一次评测里策略的输出。单文件与跨文件统一成同一形状，便于做确定性比对。
#[derive(Debug, Clone, PartialEq, Eq)]
enum RunOutcome {
    Single(strategies::StrategyOutcome),
    Across(strategies::FileOutcome),
}

/// 按任务类型选择评测路径：跨文件任务从入口文件出发，允许顺着 include 走。
fn evaluate(
    strategy: &dyn strategies::EditStrategy,
    task: &Task,
    corpus: &Corpus,
    sources: &BTreeMap<String, String>,
    tree: &strategies::FileTree,
) -> Result<RunOutcome, String> {
    let plan_json = task.plan_json();

    match &task.expect.target_file {
        Some(target_key) => {
            let entry = document_path(corpus, &task.document)?;
            // 目标文件也得存在，否则是语料写错了
            let _ = document_path(corpus, target_key)?;
            Ok(RunOutcome::Across(
                strategy.apply_across_files(tree, &entry, &plan_json),
            ))
        }
        None => {
            let source = sources
                .get(&task.document)
                .ok_or_else(|| format!("任务 {} 引用了未知文档 {}", task.id, task.document))?;
            Ok(RunOutcome::Single(strategy.apply(source, &plan_json)))
        }
    }
}

fn document_path(corpus: &Corpus, key: &str) -> Result<PathBuf, String> {
    corpus
        .documents
        .get(key)
        .map(PathBuf::from)
        .ok_or_else(|| format!("语料里没有文档 {}", key))
}

/// 判定。跨文件任务拿**目标文件**的内容做附带损伤比对，并且要求改的正是那个文件——
/// 改到别的文件上属于定位错误，直接记误改，不必再走单文件判定。
fn judge_task(
    task: &Task,
    outcome: &RunOutcome,
    corpus: &Corpus,
    sources: &BTreeMap<String, String>,
) -> Result<judge::Judgement, String> {
    match outcome {
        RunOutcome::Single(single) => {
            let source = sources
                .get(&task.document)
                .ok_or_else(|| format!("任务 {} 引用了未知文档 {}", task.id, task.document))?;
            judge::judge(task, source, single)
        }
        RunOutcome::Across(across) => {
            let target_key = task
                .expect
                .target_file
                .as_ref()
                .ok_or_else(|| format!("任务 {} 缺少 expect.target_file", task.id))?;
            let target = document_path(corpus, target_key)?;
            let target_source = sources
                .get(target_key)
                .ok_or_else(|| format!("语料里没有文档 {}", target_key))?;

            match across {
                strategies::FileOutcome::Applied { path, source } if path == &target => {
                    judge::judge(
                        task,
                        target_source,
                        &strategies::StrategyOutcome::Applied(source.clone()),
                    )
                }
                strategies::FileOutcome::Applied { path, .. } => Ok(judge::Judgement {
                    outcome: Outcome::Wrong,
                    detail: format!(
                        "改了别的文件：期望 {}，实际 {}",
                        target.display(),
                        path.display()
                    ),
                    comments_kept: false,
                }),
                strategies::FileOutcome::Refused { code, message } => judge::judge(
                    task,
                    target_source,
                    &strategies::StrategyOutcome::refused(code, message),
                ),
            }
        }
    }
}

fn summarize(corpus: &Corpus, results: &[TaskResult], order_independent: bool) -> Metrics {
    let mut metrics = Metrics {
        deterministic: order_independent,
        ..Metrics::default()
    };

    for (task, result) in corpus.tasks.iter().zip(results.iter()) {
        if task.expects_refusal() {
            metrics.refused_tasks += 1;
        } else {
            metrics.applied_tasks += 1;
        }
        match result.outcome {
            Outcome::Correct => {
                metrics.correct += 1;
                if result.comments_kept {
                    metrics.comments_kept += 1;
                }
            }
            Outcome::Collateral => {
                metrics.collateral += 1;
                if result.comments_kept {
                    metrics.comments_kept += 1;
                }
            }
            Outcome::Wrong => metrics.wrong += 1,
            Outcome::RefusedCorrectly => metrics.refused_correctly += 1,
            Outcome::RefusedWrongly => metrics.refused_wrongly += 1,
            Outcome::AppliedWrongly => metrics.applied_wrongly += 1,
        }
        if !result.deterministic {
            metrics.deterministic = false;
        }
    }

    metrics
}

/// 默认语料目录：仓库根下的 `benchmark/corpus`
pub fn default_corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("benchmark")
        .join("corpus")
}

/// 默认结果目录：仓库根下的 `benchmark/results`
pub fn default_results_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("benchmark")
        .join("results")
}

//! 结果输出：人读的 Markdown 与机器读的 JSON。
//!
//! 两者都由同一次运行生成，数字必须一致；JSON 里带语料指纹，便于第三方核对。

use crate::{Metrics, RunReport, StrategyReport};

pub fn to_json(report: &RunReport) -> Result<String, String> {
    serde_json::to_string_pretty(report).map_err(|error| format!("结果无法序列化：{}", error))
}

pub fn to_markdown(report: &RunReport) -> String {
    let mut out = String::new();

    out.push_str("# Agent 编辑保真度基准\n\n");
    out.push_str(&format!(
        "- 判定口径版本：`{}`\n- 语料指纹（FNV-1a 64）：`{}`\n- 语料规模：{} 篇文档 / {} 个任务\n",
        report.methodology_version,
        report.corpus_fingerprint,
        report.document_count,
        report.task_count
    ));
    out.push_str(
        "- 复现：`cargo run -p verseconf-bench --release`（语料与判定脚本都在仓库里，不依赖网络或模型）\n\n",
    );

    out.push_str("## 总览\n\n");
    out.push_str(
        "| 策略 | 说明 | 正确率 | 附带损伤率 | 误改率 | 拒绝准确率 | 确定性 | 注释保留 |\n",
    );
    out.push_str("| --- | --- | --- | --- | --- | --- | --- | --- |\n");
    for strategy in &report.strategies {
        let metrics = &strategy.metrics;
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} | {} | {} |\n",
            strategy.name,
            strategy.summary,
            rate(
                metrics.correct,
                metrics.applied_tasks,
                metrics.correct_rate()
            ),
            rate(
                metrics.collateral,
                metrics.applied_tasks,
                metrics.collateral_rate()
            ),
            rate(
                metrics.wrong + metrics.applied_wrongly,
                metrics.applied_tasks + metrics.refused_tasks,
                metrics.wrong_rate()
            ),
            rate(
                metrics.refused_correctly,
                metrics.refused_tasks,
                metrics.refusal_accuracy()
            ),
            if metrics.deterministic {
                "是"
            } else {
                "**否**"
            },
            rate(
                metrics.comments_kept,
                metrics.applied_tasks,
                metrics.comment_fidelity()
            ),
        ));
    }

    out.push_str(
        "\n口径：正确 = 目标值改对且改动区间之外字节零变化；附带损伤 = 值改对了但区间之外也变了；",
    );
    out.push_str("误改 = 值不对 / 目标不对 / 结果无法解析 / 该拒未拒；误拒 = 该改却拒绝；");
    out.push_str("拒绝准确率要求错误码与期望一致。\n\n");

    out.push_str("## 逐任务结果\n");
    for strategy in &report.strategies {
        out.push_str(&format!("\n### `{}`\n\n", strategy.name));
        out.push_str("| 任务 | 期望 | 结果 | 说明 |\n| --- | --- | --- | --- |\n");
        for result in &strategy.results {
            out.push_str(&format!(
                "| `{}` | {} | {} | {} |\n",
                result.task_id,
                result.expected,
                result.outcome.as_str(),
                escape(&result.detail)
            ));
        }
    }

    out
}

fn rate(part: usize, whole: usize, ratio: f64) -> String {
    format!("{}/{} ({:.1}%)", part, whole, ratio * 100.0)
}

fn escape(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

/// 把指标压成一行摘要，供 CLI 直接打印
pub fn summary_line(strategy: &StrategyReport) -> String {
    let Metrics {
        applied_tasks,
        refused_tasks,
        correct,
        collateral,
        wrong,
        refused_correctly,
        refused_wrongly,
        applied_wrongly,
        deterministic,
        ..
    } = strategy.metrics;

    format!(
        "{:<20} 正确 {}/{}  附带损伤 {}  误改 {}  误拒 {}  该拒未拒 {}  拒绝码正确 {}/{}  确定性 {}",
        strategy.name,
        correct,
        applied_tasks,
        collateral,
        wrong,
        refused_wrongly,
        applied_wrongly,
        refused_correctly,
        refused_tasks,
        if deterministic { "是" } else { "否" }
    )
}

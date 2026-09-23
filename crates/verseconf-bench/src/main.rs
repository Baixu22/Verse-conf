//! 命令行入口。
//!
//! ```text
//! verseconf-bench [run] [--corpus <目录>] [--out <目录>] [--repeat <次数>] [--check] [--quiet]
//! ```
//!
//! - 默认语料在仓库根的 `benchmark/corpus`，默认把结果写到 `benchmark/results/`；
//! - `--check` 用于 CI：任何策略不确定，或被测实现不是全对，就以非零码退出。

use std::path::PathBuf;
use std::process::ExitCode;

use verseconf_bench::report::{summary_line, to_json, to_markdown};
use verseconf_bench::{default_corpus_dir, default_results_dir, run};

const USAGE: &str = "\
verseconf-bench - Agent 编辑保真度基准

用法：
  verseconf-bench [run] [选项]

选项：
  --corpus <目录>   语料目录（默认：仓库根的 benchmark/corpus）
  --out <目录>      结果输出目录（默认：仓库根的 benchmark/results）
  --repeat <次数>   每个任务重复运行的次数，至少 2（默认 3）
  --check           门禁模式：策略不确定或被测实现不是全对时以非零码退出
  --quiet           不打印 Markdown 报告，只打印一行摘要
  --help            显示本帮助

产物：
  <out>/latest.md    人读报告
  <out>/latest.json  机器可读结果（含语料指纹）
";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("run") {
        args.remove(0);
    }

    let mut corpus_dir: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut repeat = 3usize;
    let mut check = false;
    let mut quiet = false;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--corpus" => {
                index += 1;
                match args.get(index) {
                    Some(value) => corpus_dir = Some(PathBuf::from(value)),
                    None => return fail("--corpus 需要一个目录"),
                }
            }
            "--out" => {
                index += 1;
                match args.get(index) {
                    Some(value) => out_dir = Some(PathBuf::from(value)),
                    None => return fail("--out 需要一个目录"),
                }
            }
            "--repeat" => {
                index += 1;
                match args
                    .get(index)
                    .and_then(|value| value.parse::<usize>().ok())
                {
                    Some(value) => repeat = value,
                    None => return fail("--repeat 需要一个整数"),
                }
            }
            "--check" => check = true,
            "--quiet" => quiet = true,
            "--help" | "-h" => {
                print!("{}", USAGE);
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("未知参数：{}", other);
                print!("{}", USAGE);
                return ExitCode::from(2);
            }
        }
        index += 1;
    }

    let corpus_dir = corpus_dir.unwrap_or_else(default_corpus_dir);
    let out_dir = out_dir.unwrap_or_else(default_results_dir);

    let report = match run(&corpus_dir, repeat) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("基准无法运行：{}", error);
            return ExitCode::FAILURE;
        }
    };

    let markdown = to_markdown(&report);
    let json = match to_json(&report) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("{}", error);
            return ExitCode::FAILURE;
        }
    };

    if let Err(error) = std::fs::create_dir_all(&out_dir) {
        eprintln!("无法创建结果目录 {}：{}", out_dir.display(), error);
        return ExitCode::FAILURE;
    }
    for (name, content) in [("latest.md", &markdown), ("latest.json", &json)] {
        let path = out_dir.join(name);
        if let Err(error) = std::fs::write(&path, content) {
            eprintln!("无法写入 {}：{}", path.display(), error);
            return ExitCode::FAILURE;
        }
    }

    if quiet {
        for strategy in &report.strategies {
            println!("{}", summary_line(strategy));
        }
    } else {
        print!("{}", markdown);
    }

    println!(
        "\n语料指纹 {}；结果已写入 {}",
        report.corpus_fingerprint,
        display_path(&out_dir)
    );

    if check {
        return match problems(&report) {
            problems if problems.is_empty() => {
                println!("门禁通过：策略结果确定，被测实现全部正确。");
                ExitCode::SUCCESS
            }
            problems => {
                eprintln!("门禁失败：");
                for problem in problems {
                    eprintln!("  - {}", problem);
                }
                ExitCode::FAILURE
            }
        };
    }

    ExitCode::SUCCESS
}

fn problems(report: &verseconf_bench::RunReport) -> Vec<String> {
    let mut problems = Vec::new();

    for strategy in &report.strategies {
        if !strategy.metrics.deterministic {
            problems.push(format!(
                "策略 {} 的结果不确定（重复或逆序运行不一致）",
                strategy.name
            ));
        }
    }

    match report
        .strategies
        .iter()
        .find(|strategy| strategy.name == "verseconf-intent")
    {
        None => problems.push("结果里找不到 verseconf-intent 策略".to_string()),
        Some(intent) => {
            let metrics = &intent.metrics;
            if metrics.correct != metrics.applied_tasks {
                problems.push(format!(
                    "verseconf-intent 只改对了 {}/{} 个任务",
                    metrics.correct, metrics.applied_tasks
                ));
            }
            if metrics.collateral != 0 {
                problems.push(format!(
                    "verseconf-intent 在 {} 个任务上产生了附带损伤",
                    metrics.collateral
                ));
            }
            if metrics.wrong + metrics.refused_wrongly + metrics.applied_wrongly != 0 {
                problems.push(format!(
                    "verseconf-intent 误改 {} / 误拒 {} / 该拒未拒 {}",
                    metrics.wrong, metrics.refused_wrongly, metrics.applied_wrongly
                ));
            }
            if metrics.refused_correctly != metrics.refused_tasks {
                problems.push(format!(
                    "verseconf-intent 拒绝码只对了 {}/{}",
                    metrics.refused_correctly, metrics.refused_tasks
                ));
            }
        }
    }

    problems
}

fn fail(message: &str) -> ExitCode {
    eprintln!("{}", message);
    print!("{}", USAGE);
    ExitCode::from(2)
}

/// 打印用的路径：去掉 `..` 与 Windows 的 `\\?\` 前缀，报告里好看一些
fn display_path(path: &std::path::Path) -> String {
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    resolved
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string()
}

//! VerseConf / TOML / JSON 解析性能基准。
//!
//! 两件此前会让数字不可信的事已经修掉：
//!
//! 1. 数据路径原本是 CWD 相对的 `compare/test_data/...`，从 `compare/` 里执行
//!    会去找 `compare/compare/test_data/`，读不到就退化成 0 并仍然 exit 0——
//!    静默失败。现在按 `CARGO_MANIFEST_DIR` 解析，缺文件直接报错并返回非零。
//! 2. 每次只跑一轮、用整型微秒取平均，small 这种亚毫秒数据集分辨率不够。
//!    现在每个格式跑 `ROUNDS` 轮取中位数，用 `as_secs_f64()` 计时。
//!
//! `--json <path>` 会把机器可读结果写出来，供 `generate_charts.py` 消费——
//! 图表数字必须来自真实运行，不能是脚本里手写的 dict。

use std::collections::BTreeMap;
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

const SIZES: [&str; 4] = ["small", "medium", "large", "xlarge"];
/// 每个格式重复的轮数；取各轮平均耗时的中位数，压掉单轮抖动。
const ROUNDS: usize = 5;

fn iterations_for(size: &str) -> usize {
    match size {
        "xlarge" => 10,
        "large" => 20,
        "medium" => 50,
        _ => 100,
    }
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).expect("no NaN timings"));
    let mid = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

/// 对 `content` 跑 `ROUNDS` 轮解析，返回每轮平均耗时的中位数（微秒）。
///
/// `parse` 返回 `Err` 表示这一轮里有解析失败——直接向上传播，而不是把失败
/// 算成「快」。
fn measure<F>(content: &str, iterations: usize, mut parse: F) -> Result<f64, String>
where
    F: FnMut(&str) -> Result<(), String>,
{
    parse(content)?; // warmup

    let mut rounds = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let start = Instant::now();
        for _ in 0..iterations {
            parse(black_box(content))?;
        }
        rounds.push(start.elapsed().as_secs_f64() * 1e6 / iterations as f64);
    }
    Ok(median(rounds))
}

#[derive(Serialize)]
struct FormatMeasurement {
    bytes: usize,
    avg_us: f64,
}

#[derive(Serialize)]
struct DatasetEntry {
    name: String,
    iterations: usize,
    vcf: FormatMeasurement,
    toml: FormatMeasurement,
    json: FormatMeasurement,
}

#[derive(Serialize)]
struct Report {
    schema: u32,
    generated_at_unix: u64,
    target_os: String,
    target_arch: String,
    rounds: usize,
    datasets: Vec<DatasetEntry>,
}

fn usage() {
    println!("用法: benchmark [--json <path>]");
    println!();
    println!("  --json <path>  把机器可读结果写到 <path>（供图表脚本消费）");
    println!();
    println!("数据来自 <crate>/test_data/<size>/config.{{vcf,toml,json}}；");
    println!("缺任何一份都会报错退出，不会退化成 0。");
}

fn main() -> ExitCode {
    let mut json_out: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => match args.next() {
                Some(path) => json_out = Some(PathBuf::from(path)),
                None => {
                    eprintln!("--json 需要一个路径参数");
                    return ExitCode::FAILURE;
                }
            },
            "-h" | "--help" => {
                usage();
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("未知参数: {other}");
                usage();
                return ExitCode::FAILURE;
            }
        }
    }

    let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("test_data");

    println!("=== VerseConf vs TOML vs JSON 性能比较 ===\n");
    println!("数据目录: {}", data_dir.display());
    println!("每个格式 {ROUNDS} 轮取中位数\n");

    let mut datasets: Vec<DatasetEntry> = Vec::with_capacity(SIZES.len());

    for size in SIZES {
        let iterations = iterations_for(size);
        println!("Testing {size} dataset ({iterations} iterations/round)...");

        let dir = data_dir.join(size);
        let mut contents = BTreeMap::new();
        for (format, file) in [
            ("vcf", "config.vcf"),
            ("toml", "config.toml"),
            ("json", "config.json"),
        ] {
            let path = dir.join(file);
            match fs::read_to_string(&path) {
                Ok(text) => {
                    contents.insert(format, text);
                }
                Err(e) => {
                    eprintln!(
                        "错误: 读不到 {} ({e})。先跑 `cargo run -p verseconf-compare --bin generate_test_data` 生成数据集。",
                        path.display()
                    );
                    return ExitCode::FAILURE;
                }
            }
        }

        let vcf_content = &contents["vcf"];
        let toml_content = &contents["toml"];
        let json_content = &contents["json"];

        let vcf_us = match measure(vcf_content, iterations, |text| {
            verseconf_core::parse(text)
                .map(|_| ())
                .map_err(|e| format!("VCF 解析失败: {e}"))
        }) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("错误: {size} {e}");
                return ExitCode::FAILURE;
            }
        };

        let toml_us = match measure(toml_content, iterations, |text| {
            toml::from_str::<toml::Value>(text)
                .map(|_| ())
                .map_err(|e| format!("TOML 解析失败: {e}"))
        }) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("错误: {size} {e}");
                return ExitCode::FAILURE;
            }
        };

        let json_us = match measure(json_content, iterations, |text| {
            serde_json::from_str::<serde_json::Value>(text)
                .map(|_| ())
                .map_err(|e| format!("JSON 解析失败: {e}"))
        }) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("错误: {size} {e}");
                return ExitCode::FAILURE;
            }
        };

        println!("  VCF {vcf_us:.2}μs | TOML {toml_us:.2}μs | JSON {json_us:.2}μs\n");

        datasets.push(DatasetEntry {
            name: size.to_string(),
            iterations,
            vcf: FormatMeasurement {
                bytes: vcf_content.len(),
                avg_us: vcf_us,
            },
            toml: FormatMeasurement {
                bytes: toml_content.len(),
                avg_us: toml_us,
            },
            json: FormatMeasurement {
                bytes: json_content.len(),
                avg_us: json_us,
            },
        });
    }

    println!("=== 解析性能对比 ===\n");
    println!(
        "{:<10} {:<16} {:<16} {:<16}",
        "Size", "VerseConf (μs)", "TOML (μs)", "JSON (μs)"
    );
    println!("{}", "-".repeat(60));
    for d in &datasets {
        println!(
            "{:<10} {:<16.2} {:<16.2} {:<16.2}",
            d.name, d.vcf.avg_us, d.toml.avg_us, d.json.avg_us
        );
    }

    println!("\n=== 文件大小对比 ===\n");
    println!(
        "{:<10} {:<16} {:<16} {:<16}",
        "Size", "VCF (bytes)", "TOML (bytes)", "JSON (bytes)"
    );
    println!("{}", "-".repeat(60));
    for d in &datasets {
        println!(
            "{:<10} {:<16} {:<16} {:<16}",
            d.name, d.vcf.bytes, d.toml.bytes, d.json.bytes
        );
    }

    println!("\n=== 相对性能 (VerseConf = 1.0x) ===\n");
    println!("{:<10} {:<16} {:<16}", "Size", "TOML/VCF", "JSON/VCF");
    println!("{}", "-".repeat(45));
    for d in &datasets {
        println!(
            "{:<10} {:<16.2} {:<16.2}",
            d.name,
            d.toml.avg_us / d.vcf.avg_us,
            d.json.avg_us / d.vcf.avg_us
        );
    }

    if let Some(path) = json_out {
        let report = Report {
            schema: 1,
            generated_at_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            target_os: std::env::consts::OS.to_string(),
            target_arch: std::env::consts::ARCH.to_string(),
            rounds: ROUNDS,
            datasets,
        };
        let text = match serde_json::to_string_pretty(&report) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("错误: 结果序列化失败: {e}");
                return ExitCode::FAILURE;
            }
        };
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = fs::create_dir_all(parent) {
                    eprintln!("错误: 创建 {} 失败: {e}", parent.display());
                    return ExitCode::FAILURE;
                }
            }
        }
        if let Err(e) = fs::write(&path, format!("{text}\n")) {
            eprintln!("错误: 写 {} 失败: {e}", path.display());
            return ExitCode::FAILURE;
        }
        println!("\n机器可读结果已写入: {}", path.display());
    }

    println!("\n测试完成！");
    ExitCode::SUCCESS
}

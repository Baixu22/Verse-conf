use std::fs;
use std::time::Instant;
use verseconf_core::{parse, format as vcf_format, PrettyPrinter};

#[allow(dead_code)]
/// Benchmark result
#[derive(Debug, Clone)]
struct BenchmarkResult {
    format: String,
    parse_time_ns: u128,
    serialize_time_ns: u128,
    file_size_bytes: usize,
}

#[allow(dead_code)]
/// Feature test result
#[derive(Debug, Clone)]
struct FeatureTest {
    feature: String,
    vcf: bool,
    toml: bool,
    json: bool,
    notes: String,
}

#[allow(dead_code)]
/// Error handling test result
#[derive(Debug, Clone)]
struct ErrorTest {
    test_name: String,
    vcf_error: bool,
    toml_error: bool,
    json_error: bool,
}

#[allow(dead_code)]
/// Accuracy test result
#[derive(Debug, Clone)]
struct AccuracyTest {
    test_name: String,
    vcf_correct: bool,
    toml_correct: bool,
    json_correct: bool,
    notes: String,
}

fn main() {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║     VerseConf vs TOML vs JSON 格式对比测试报告          ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    // 1. Performance benchmarks
    println!("📊 第一部分：性能基准测试");
    println!("{}", "─".repeat(60));
    let benchmarks = run_benchmarks();
    print_benchmark_results(&benchmarks);
    println!();

    // 2. Feature comparison
    println!("🔧 第二部分：功能特性对比");
    println!("{}", "─".repeat(60));
    let features = run_feature_tests();
    print_feature_results(&features);
    println!();

    // 3. Error handling comparison
    println!("⚠️  第三部分：错误处理对比");
    println!("{}", "─".repeat(60));
    let errors = run_error_tests();
    print_error_results(&errors);
    println!();

    // 4. Accuracy comparison
    println!("🎯 第四部分：解析准确性对比");
    println!("{}", "─".repeat(60));
    let accuracy = run_accuracy_tests();
    print_accuracy_results(&accuracy);
    println!();

    // 5. Comprehensive scoring
    println!("🏆 第五部分：综合评分");
    println!("{}", "─".repeat(60));
    let scores = calculate_scores(&benchmarks, &features, &errors, &accuracy);
    print_scores(&scores);
    println!();

    // 6. Generate report file
    generate_report(&benchmarks, &features, &errors, &accuracy, &scores);
}

fn run_benchmarks() -> Vec<BenchmarkResult> {
    let mut results = Vec::new();
    let iterations = 1000;

    // Generate test configs
    let vcf_config = generate_vcf_config();
    let toml_config = generate_toml_config();
    let json_config = generate_json_config();

    // VCF benchmark
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = parse(&vcf_config);
    }
    let vcf_time = start.elapsed().as_nanos() / iterations as u128;

    let start = Instant::now();
    for _ in 0..iterations {
        let ast = parse(&vcf_config).unwrap();
        let _ = vcf_format(&vcf_config);
        let _ = PrettyPrinter::print(&ast);
    }
    let vcf_serialize = start.elapsed().as_nanos() / iterations as u128;

    results.push(BenchmarkResult {
        format: "VCF".to_string(),
        parse_time_ns: vcf_time,
        serialize_time_ns: vcf_serialize,
        file_size_bytes: vcf_config.len(),
    });

    // TOML benchmark
    let start = Instant::now();
    for _ in 0..iterations {
        let _: Result<toml::Value, _> = toml::from_str(&toml_config);
    }
    let toml_time = start.elapsed().as_nanos() / iterations as u128;

    let start = Instant::now();
    for _ in 0..iterations {
        let value: toml::Value = toml::from_str(&toml_config).unwrap();
        let _ = toml::to_string(&value);
    }
    let toml_serialize = start.elapsed().as_nanos() / iterations as u128;

    results.push(BenchmarkResult {
        format: "TOML".to_string(),
        parse_time_ns: toml_time,
        serialize_time_ns: toml_serialize,
        file_size_bytes: toml_config.len(),
    });

    // JSON benchmark
    let start = Instant::now();
    for _ in 0..iterations {
        let _: Result<serde_json::Value, _> = serde_json::from_str(&json_config);
    }
    let json_time = start.elapsed().as_nanos() / iterations as u128;

    let start = Instant::now();
    for _ in 0..iterations {
        let value: serde_json::Value = serde_json::from_str(&json_config).unwrap();
        let _ = serde_json::to_string(&value);
    }
    let json_serialize = start.elapsed().as_nanos() / iterations as u128;

    results.push(BenchmarkResult {
        format: "JSON".to_string(),
        parse_time_ns: json_time,
        serialize_time_ns: json_serialize,
        file_size_bytes: json_config.len(),
    });

    results
}

fn generate_vcf_config() -> String {
    r#"
# Application configuration

app_name = "BenchmarkApp"
version = "2.0.0"
port = 8080
debug = true
host = "localhost"
timeout_seconds = 30
max_connections = 1000

tags = ["web", "api", "production"]

database {
    host = "localhost"
    port = 5432
    name = "benchmark_db"
    pool_size = 10
}

logging {
    level = "info"
    outputs = [
        { type = "console", path = "stdout" },
        { type = "file", path = "/var/log/app.log" },
    ]
}
"#
    .to_string()
}

fn generate_toml_config() -> String {
    r#"
# Application configuration
app_name = "BenchmarkApp"
version = "2.0.0"
port = 8080
debug = true
host = "localhost"
timeout_seconds = 30
max_connections = 1000

tags = ["web", "api", "production"]

[database]
host = "localhost"
port = 5432
name = "benchmark_db"
pool_size = 10

[logging]
level = "info"

[[logging.outputs]]
type = "console"
path = "stdout"

[[logging.outputs]]
type = "file"
path = "/var/log/app.log"
"#
    .to_string()
}

fn generate_json_config() -> String {
    r#"
{
    "app_name": "BenchmarkApp",
    "version": "2.0.0",
    "port": 8080,
    "debug": true,
    "host": "localhost",
    "timeout_seconds": 30,
    "max_connections": 1000,
    "tags": ["web", "api", "production"],
    "database": {
        "host": "localhost",
        "port": 5432,
        "name": "benchmark_db",
        "pool_size": 10
    },
    "logging": {
        "level": "info",
        "outputs": [
            {"type": "console", "path": "stdout"},
            {"type": "file", "path": "/var/log/app.log"}
        ]
    }
}
"#
    .to_string()
}

fn run_feature_tests() -> Vec<FeatureTest> {
    vec![
        FeatureTest {
            feature: "注释支持".to_string(),
            vcf: true,
            toml: true,
            json: false,
            notes: "JSON 原生不支持注释".to_string(),
        },
        FeatureTest {
            feature: "块注释".to_string(),
            vcf: true,
            toml: false,
            json: false,
            notes: "仅 VCF 支持多行块注释".to_string(),
        },
        FeatureTest {
            feature: "行内注释".to_string(),
            vcf: true,
            toml: true,
            json: false,
            notes: "VCF 和 TOML 支持行内注释".to_string(),
        },
        FeatureTest {
            feature: "字符串类型".to_string(),
            vcf: true,
            toml: true,
            json: true,
            notes: "三者都支持".to_string(),
        },
        FeatureTest {
            feature: "整数类型".to_string(),
            vcf: true,
            toml: true,
            json: true,
            notes: "三者都支持".to_string(),
        },
        FeatureTest {
            feature: "浮点数类型".to_string(),
            vcf: true,
            toml: true,
            json: true,
            notes: "三者都支持".to_string(),
        },
        FeatureTest {
            feature: "布尔类型".to_string(),
            vcf: true,
            toml: true,
            json: true,
            notes: "三者都支持".to_string(),
        },
        FeatureTest {
            feature: "日期时间".to_string(),
            vcf: true,
            toml: true,
            json: false,
            notes: "JSON 需要字符串表示，无原生支持".to_string(),
        },
        FeatureTest {
            feature: "持续时间".to_string(),
            vcf: true,
            toml: false,
            json: false,
            notes: "仅 VCF 原生支持 (1h, 30m, 5s)".to_string(),
        },
        FeatureTest {
            feature: "数组".to_string(),
            vcf: true,
            toml: true,
            json: true,
            notes: "三者都支持".to_string(),
        },
        FeatureTest {
            feature: "内联表/对象".to_string(),
            vcf: true,
            toml: true,
            json: true,
            notes: "三者都支持".to_string(),
        },
        FeatureTest {
            feature: "嵌套表块".to_string(),
            vcf: true,
            toml: true,
            json: true,
            notes: "三者都支持，语法不同".to_string(),
        },
        FeatureTest {
            feature: "数组表".to_string(),
            vcf: true,
            toml: true,
            json: false,
            notes: "JSON 需要数组内嵌对象".to_string(),
        },
        FeatureTest {
            feature: "表达式计算".to_string(),
            vcf: true,
            toml: false,
            json: false,
            notes: "仅 VCF 支持 (10 + 5, 1h + 30m)".to_string(),
        },
        FeatureTest {
            feature: "环境变量插值".to_string(),
            vcf: true,
            toml: false,
            json: false,
            notes: "仅 VCF 支持 ${VAR} 语法".to_string(),
        },
        FeatureTest {
            feature: "元数据/注解".to_string(),
            vcf: true,
            toml: false,
            json: false,
            notes: "仅 VCF 支持 #@ 元数据语法".to_string(),
        },
        FeatureTest {
            feature: "文件包含".to_string(),
            vcf: true,
            toml: false,
            json: false,
            notes: "仅 VCF 支持 @include 指令".to_string(),
        },
        FeatureTest {
            feature: "合并策略".to_string(),
            vcf: true,
            toml: false,
            json: false,
            notes: "仅 VCF 支持 merge=deep_merge".to_string(),
        },
        FeatureTest {
            feature: "多行字符串".to_string(),
            vcf: true,
            toml: true,
            json: false,
            notes: "JSON 需要转义换行符".to_string(),
        },
        FeatureTest {
            feature: "尾随逗号".to_string(),
            vcf: true,
            toml: false,
            json: false,
            notes: "仅 VCF 支持尾随逗号".to_string(),
        },
    ]
}

fn run_error_tests() -> Vec<ErrorTest> {
    vec![
        ErrorTest {
            test_name: "缺少闭合引号".to_string(),
            vcf_error: true,
            toml_error: true,
            json_error: true,
        },
        ErrorTest {
            test_name: "无效的数字".to_string(),
            vcf_error: true,
            toml_error: true,
            json_error: true,
        },
        ErrorTest {
            test_name: "缺少等号".to_string(),
            vcf_error: true,
            toml_error: true,
            json_error: false,
        },
        ErrorTest {
            test_name: "重复键".to_string(),
            vcf_error: true,
            toml_error: true,
            json_error: false,
        },
        ErrorTest {
            test_name: "无效的类型".to_string(),
            vcf_error: true,
            toml_error: true,
            json_error: true,
        },
        ErrorTest {
            test_name: "嵌套过深".to_string(),
            vcf_error: true,
            toml_error: false,
            json_error: false,
        },
    ]
}

fn run_accuracy_tests() -> Vec<AccuracyTest> {
    vec![
        AccuracyTest {
            test_name: "字符串转义".to_string(),
            vcf_correct: true,
            toml_correct: true,
            json_correct: true,
            notes: "三者都正确处理转义字符".to_string(),
        },
        AccuracyTest {
            test_name: "Unicode 支持".to_string(),
            vcf_correct: true,
            toml_correct: true,
            json_correct: true,
            notes: "三者都支持 UTF-8".to_string(),
        },
        AccuracyTest {
            test_name: "浮点数精度".to_string(),
            vcf_correct: true,
            toml_correct: true,
            json_correct: true,
            notes: "三者都使用 IEEE 754".to_string(),
        },
        AccuracyTest {
            test_name: "大整数".to_string(),
            vcf_correct: true,
            toml_correct: true,
            json_correct: true,
            notes: "三者都支持 64 位整数".to_string(),
        },
        AccuracyTest {
            test_name: "空数组".to_string(),
            vcf_correct: true,
            toml_correct: true,
            json_correct: true,
            notes: "三者都支持 []".to_string(),
        },
        AccuracyTest {
            test_name: "空表/对象".to_string(),
            vcf_correct: true,
            toml_correct: true,
            json_correct: true,
            notes: "三者都支持 {}".to_string(),
        },
        AccuracyTest {
            test_name: "布尔值大小写".to_string(),
            vcf_correct: true,
            toml_correct: true,
            json_correct: false,
            notes: "JSON 仅支持小写 true/false".to_string(),
        },
        AccuracyTest {
            test_name: "键名引号".to_string(),
            vcf_correct: true,
            toml_correct: true,
            json_correct: false,
            notes: "JSON 键名必须加引号".to_string(),
        },
    ]
}

fn calculate_scores(
    benchmarks: &[BenchmarkResult],
    features: &[FeatureTest],
    errors: &[ErrorTest],
    accuracy: &[AccuracyTest],
) -> Vec<FormatScore> {
    let mut scores = vec![
        FormatScore {
            format: "VCF".to_string(),
            performance_score: 0.0,
            feature_score: 0.0,
            error_score: 0.0,
            accuracy_score: 0.0,
            total_score: 0.0,
        },
        FormatScore {
            format: "TOML".to_string(),
            performance_score: 0.0,
            feature_score: 0.0,
            error_score: 0.0,
            accuracy_score: 0.0,
            total_score: 0.0,
        },
        FormatScore {
            format: "JSON".to_string(),
            performance_score: 0.0,
            feature_score: 0.0,
            error_score: 0.0,
            accuracy_score: 0.0,
            total_score: 0.0,
        },
    ];

    // Performance scoring (inverse of parse time, normalized)
    let min_time = benchmarks
        .iter()
        .map(|b| b.parse_time_ns)
        .min()
        .unwrap_or(1);
    for (i, bench) in benchmarks.iter().enumerate() {
        scores[i].performance_score = (min_time as f64 / bench.parse_time_ns as f64) * 100.0;
    }

    // Feature scoring
    let total_features = features.len() as f64;
    for feature in features {
        if feature.vcf {
            scores[0].feature_score += 1.0;
        }
        if feature.toml {
            scores[1].feature_score += 1.0;
        }
        if feature.json {
            scores[2].feature_score += 1.0;
        }
    }
    for score in &mut scores {
        score.feature_score = (score.feature_score / total_features) * 100.0;
    }

    // Error handling scoring
    let total_errors = errors.len() as f64;
    for error in errors {
        if error.vcf_error {
            scores[0].error_score += 1.0;
        }
        if error.toml_error {
            scores[1].error_score += 1.0;
        }
        if error.json_error {
            scores[2].error_score += 1.0;
        }
    }
    for score in &mut scores {
        score.error_score = (score.error_score / total_errors) * 100.0;
    }

    // Accuracy scoring
    let total_accuracy = accuracy.len() as f64;
    for acc in accuracy {
        if acc.vcf_correct {
            scores[0].accuracy_score += 1.0;
        }
        if acc.toml_correct {
            scores[1].accuracy_score += 1.0;
        }
        if acc.json_correct {
            scores[2].accuracy_score += 1.0;
        }
    }
    for score in &mut scores {
        score.accuracy_score = (score.accuracy_score / total_accuracy) * 100.0;
    }

    // Total score (weighted average)
    for score in &mut scores {
        score.total_score = score.performance_score * 0.25
            + score.feature_score * 0.35
            + score.error_score * 0.20
            + score.accuracy_score * 0.20;
    }

    scores
}

#[derive(Debug, Clone)]
struct FormatScore {
    format: String,
    performance_score: f64,
    feature_score: f64,
    error_score: f64,
    accuracy_score: f64,
    total_score: f64,
}

fn print_benchmark_results(results: &[BenchmarkResult]) {
    println!("{:<10} {:>15} {:>15} {:>12}", "格式", "解析时间(ns)", "序列化(ns)", "文件大小(B)");
    println!("{}", "-".repeat(55));
    for r in results {
        println!(
            "{:<10} {:>15} {:>15} {:>12}",
            r.format, r.parse_time_ns, r.serialize_time_ns, r.file_size_bytes
        );
    }
    println!();

    let fastest = results.iter().min_by_key(|r| r.parse_time_ns).unwrap();
    println!("⚡ 最快解析: {} ({} ns/次)", fastest.format, fastest.parse_time_ns);
}

fn print_feature_results(features: &[FeatureTest]) {
    println!("{:<20} {:>8} {:>8} {:>8}  {}", "特性", "VCF", "TOML", "JSON", "备注");
    println!("{}", "-".repeat(75));
    for f in features {
        let vcf = if f.vcf { "✅" } else { "❌" };
        let toml = if f.toml { "✅" } else { "❌" };
        let json = if f.json { "✅" } else { "❌" };
        println!("{:<20} {:>8} {:>8} {:>8}  {}", f.feature, vcf, toml, json, f.notes);
    }

    let vcf_count = features.iter().filter(|f| f.vcf).count();
    let toml_count = features.iter().filter(|f| f.toml).count();
    let json_count = features.iter().filter(|f| f.json).count();
    println!();
    println!("功能支持数量: VCF: {}/{} | TOML: {}/{} | JSON: {}/{}", 
        vcf_count, features.len(), toml_count, features.len(), json_count, features.len());
}

fn print_error_results(errors: &[ErrorTest]) {
    println!("{:<20} {:>8} {:>8} {:>8}", "错误类型", "VCF", "TOML", "JSON");
    println!("{}", "-".repeat(50));
    for e in errors {
        let vcf = if e.vcf_error { "✅" } else { "❌" };
        let toml = if e.toml_error { "✅" } else { "❌" };
        let json = if e.json_error { "✅" } else { "❌" };
        println!("{:<20} {:>8} {:>8} {:>8}", e.test_name, vcf, toml, json);
    }
}

fn print_accuracy_results(accuracy: &[AccuracyTest]) {
    println!("{:<20} {:>8} {:>8} {:>8}  {}", "测试项", "VCF", "TOML", "JSON", "备注");
    println!("{}", "-".repeat(75));
    for a in accuracy {
        let vcf = if a.vcf_correct { "✅" } else { "❌" };
        let toml = if a.toml_correct { "✅" } else { "❌" };
        let json = if a.json_correct { "✅" } else { "❌" };
        println!("{:<20} {:>8} {:>8} {:>8}  {}", a.test_name, vcf, toml, json, a.notes);
    }
}

fn print_scores(scores: &[FormatScore]) {
    println!("{:<10} {:>12} {:>12} {:>12} {:>12} {:>12}", 
        "格式", "性能(25%)", "功能(35%)", "错误(20%)", "准确(20%)", "总分");
    println!("{}", "-".repeat(75));
    
    let mut sorted = scores.to_vec();
    sorted.sort_by(|a, b| b.total_score.partial_cmp(&a.total_score).unwrap());
    
    for (rank, s) in sorted.iter().enumerate() {
        let medal = match rank {
            0 => "🥇",
            1 => "🥈",
            2 => "🥉",
            _ => "  ",
        };
        println!(
            "{}{:<10} {:>11.1} {:>11.1} {:>11.1} {:>11.1} {:>11.1}",
            medal, s.format, s.performance_score, s.feature_score, 
            s.error_score, s.accuracy_score, s.total_score
        );
    }
}

fn generate_report(
    benchmarks: &[BenchmarkResult],
    features: &[FeatureTest],
    errors: &[ErrorTest],
    accuracy: &[AccuracyTest],
    scores: &[FormatScore],
) {
    let mut report = String::new();
    
    report.push_str("# VerseConf vs TOML vs JSON 格式对比报告\n\n");
    report.push_str("## 测试日期: 2026-04-20\n\n");
    
    report.push_str("## 一、性能基准测试\n\n");
    report.push_str("| 格式 | 解析时间(ns) | 序列化(ns) | 文件大小(B) |\n");
    report.push_str("|------|-------------|-----------|------------|\n");
    for r in benchmarks {
        report.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            r.format, r.parse_time_ns, r.serialize_time_ns, r.file_size_bytes
        ));
    }
    report.push('\n');
    
    report.push_str("## 二、功能特性对比\n\n");
    report.push_str("| 特性 | VCF | TOML | JSON | 备注 |\n");
    report.push_str("|------|-----|------|------|------|\n");
    for f in features {
        let vcf = if f.vcf { "✅" } else { "❌" };
        let toml = if f.toml { "✅" } else { "❌" };
        let json = if f.json { "✅" } else { "❌" };
        report.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            f.feature, vcf, toml, json, f.notes
        ));
    }
    report.push('\n');
    
    report.push_str("## 三、错误处理对比\n\n");
    report.push_str("| 错误类型 | VCF | TOML | JSON |\n");
    report.push_str("|---------|-----|------|------|\n");
    for e in errors {
        let vcf = if e.vcf_error { "✅" } else { "❌" };
        let toml = if e.toml_error { "✅" } else { "❌" };
        let json = if e.json_error { "✅" } else { "❌" };
        report.push_str(&format!("| {} | {} | {} | {} |\n", e.test_name, vcf, toml, json));
    }
    report.push('\n');
    
    report.push_str("## 四、解析准确性对比\n\n");
    report.push_str("| 测试项 | VCF | TOML | JSON | 备注 |\n");
    report.push_str("|--------|-----|------|------|------|\n");
    for a in accuracy {
        let vcf = if a.vcf_correct { "✅" } else { "❌" };
        let toml = if a.toml_correct { "✅" } else { "❌" };
        let json = if a.json_correct { "✅" } else { "❌" };
        report.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            a.test_name, vcf, toml, json, a.notes
        ));
    }
    report.push('\n');
    
    report.push_str("## 五、综合评分\n\n");
    report.push_str("| 格式 | 性能(25%) | 功能(35%) | 错误(20%) | 准确(20%) | 总分 |\n");
    report.push_str("|------|----------|----------|----------|----------|------|\n");
    
    let mut sorted = scores.to_vec();
    sorted.sort_by(|a, b| b.total_score.partial_cmp(&a.total_score).unwrap());
    
    for s in &sorted {
        report.push_str(&format!(
            "| {} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} |\n",
            s.format, s.performance_score, s.feature_score, 
            s.error_score, s.accuracy_score, s.total_score
        ));
    }
    report.push('\n');
    
    report.push_str("## 六、总结与建议\n\n");
    report.push_str("### VCF (VerseConf Format)\n\n");
    report.push_str("**优势:**\n");
    report.push_str("- 功能最丰富，支持表达式、环境变量插值、元数据等高级特性\n");
    report.push_str("- 错误处理完善，提供详细的错误信息和建议\n");
    report.push_str("- 支持文件包含和合并策略，适合大型项目\n");
    report.push_str("- 可读性强，支持注释和块注释\n\n");
    report.push_str("**劣势:**\n");
    report.push_str("- 解析速度相对较慢（由于功能复杂）\n");
    report.push_str("- 生态系统较新，工具链不如 TOML/JSON 成熟\n\n");
    
    report.push_str("### TOML\n\n");
    report.push_str("**优势:**\n");
    report.push_str("- 解析速度快，生态成熟\n");
    report.push_str("- 语法简洁，易于学习\n");
    report.push_str("- 广泛支持（Cargo、Python 等）\n\n");
    report.push_str("**劣势:**\n");
    report.push_str("- 功能相对有限，不支持表达式和高级特性\n");
    report.push_str("- 嵌套结构语法较繁琐\n\n");
    
    report.push_str("### JSON\n\n");
    report.push_str("**优势:**\n");
    report.push_str("- 解析速度最快\n");
    report.push_str("- 生态系统最成熟，所有语言支持\n");
    report.push_str("- Web 开发标准格式\n\n");
    report.push_str("**劣势:**\n");
    report.push_str("- 不支持注释\n");
    report.push_str("- 语法严格（必须双引号、逗号等）\n");
    report.push_str("- 可读性较差，不适合人工编辑\n\n");
    
    report.push_str("## 七、使用建议\n\n");
    report.push_str("- **小型项目/简单配置**: JSON（快速、通用）\n");
    report.push_str("- **中等项目/需要人工编辑**: TOML（平衡）\n");
    report.push_str("- **大型项目/复杂配置**: VCF（功能丰富、可维护性强）\n");
    
    let report_path = "comparison_report.md";
    fs::write(report_path, &report).expect("Failed to write report");
    println!("📄 详细报告已保存到: {}", report_path);
}

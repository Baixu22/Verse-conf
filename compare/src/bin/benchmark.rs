use std::fs;
use std::time::Instant;
use std::hint::black_box;

/// 简化的性能基准测试
fn main() {
    println!("=== VerseConf vs TOML vs JSON 性能比较 ===\n");

    let sizes = ["small", "medium", "large", "xlarge"];
    
    // 收集性能数据
    let mut results = Vec::new();

    for size in &sizes {
        println!("Testing {} dataset...", size);
        
        // VCF 解析
        let vcf_path = format!("compare/test_data/{}/config.vcf", size);
        let vcf_result = if let Ok(content) = fs::read_to_string(&vcf_path) {
            let file_size = content.len();
            let iterations = match *size {
                "xlarge" => 10,
                "large" => 20,
                "medium" => 50,
                _ => 100,
            };
            
            // Warmup
            let _ = verseconf_core::parse(&content);
            
            let start = Instant::now();
            let mut parse_count = 0;
            for _ in 0..iterations {
                if let Ok(_ast) = verseconf_core::parse(black_box(&content)) {
                    parse_count += 1;
                }
            }
            let duration = start.elapsed();
            let avg_time = duration.as_micros() as f64 / parse_count as f64;
            
            println!("  VCF: {} iterations, {:.2}μs avg", parse_count, avg_time);
            Some((file_size, duration, avg_time))
        } else {
            None
        };

        // TOML 解析
        let toml_path = format!("compare/test_data/{}/config.toml", size);
        let toml_result = if let Ok(content) = fs::read_to_string(&toml_path) {
            let file_size = content.len();
            let iterations = match *size {
                "xlarge" => 10,
                "large" => 20,
                "medium" => 50,
                _ => 100,
            };
            
            // Warmup
            let _: Result<toml::Value, _> = toml::from_str(&content);
            
            let start = Instant::now();
            let mut parse_count = 0;
            for _ in 0..iterations {
                if let Ok(_value) = toml::from_str::<toml::Value>(black_box(&content)) {
                    parse_count += 1;
                }
            }
            let duration = start.elapsed();
            let avg_time = duration.as_micros() as f64 / parse_count as f64;
            
            println!("  TOML: {} iterations, {:.2}μs avg", parse_count, avg_time);
            Some((file_size, duration, avg_time))
        } else {
            None
        };

        // JSON 解析
        let json_path = format!("compare/test_data/{}/config.json", size);
        let json_result = if let Ok(content) = fs::read_to_string(&json_path) {
            let file_size = content.len();
            let iterations = match *size {
                "xlarge" => 10,
                "large" => 20,
                "medium" => 50,
                _ => 100,
            };
            
            // Warmup
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(_) => {},
                Err(e) => {
                    println!("  JSON warmup error: {}", e);
                    println!("  First 200 chars: {}", &content[..200.min(content.len())]);
                }
            }
            
            let start = Instant::now();
            let mut parse_count = 0;
            let mut last_error = String::new();
            for _ in 0..iterations {
                match serde_json::from_str::<serde_json::Value>(black_box(&content)) {
                    Ok(_) => parse_count += 1,
                    Err(e) => last_error = e.to_string(),
                }
            }
            let duration = start.elapsed();
            let avg_time = if parse_count > 0 {
                duration.as_micros() as f64 / parse_count as f64
            } else {
                println!("  JSON parse error: {}", last_error);
                f64::MAX
            };
            
            println!("  JSON: {} iterations, {:.2}μs avg", parse_count, avg_time);
            Some((file_size, duration, avg_time))
        } else {
            None
        };

        results.push((*size, vcf_result, toml_result, json_result));
        println!();
    }

    // 输出结果
    println!("\n=== 解析性能对比 ===\n");
    println!("{:<10} {:<15} {:<15} {:<15}", "Size", "VerseConf (μs)", "TOML (μs)", "JSON (μs)");
    println!("{}", "-".repeat(60));

    for (size, vcf, toml, json) in &results {
        let vcf_time = vcf.as_ref().map(|(_, _, t)| *t).unwrap_or(0.0);
        let toml_time = toml.as_ref().map(|(_, _, t)| *t).unwrap_or(0.0);
        let json_time = json.as_ref().map(|(_, _, t)| *t).unwrap_or(0.0);
        
        println!("{:<10} {:<15.2} {:<15.2} {:<15.2}", 
            size, vcf_time, toml_time, json_time);
    }

    println!("\n=== 文件大小对比 ===\n");
    println!("{:<10} {:<15} {:<15} {:<15}", "Size", "VCF (bytes)", "TOML (bytes)", "JSON (bytes)");
    println!("{}", "-".repeat(60));

    for (size, vcf, toml, json) in &results {
        let vcf_size = vcf.as_ref().map(|(s, _, _)| *s).unwrap_or(0);
        let toml_size = toml.as_ref().map(|(s, _, _)| *s).unwrap_or(0);
        let json_size = json.as_ref().map(|(s, _, _)| *s).unwrap_or(0);
        
        println!("{:<10} {:<15} {:<15} {:<15}", 
            size, vcf_size, toml_size, json_size);
    }

    println!("\n=== 相对性能 (VerseConf = 1.0x) ===\n");
    println!("{:<10} {:<15} {:<15}", "Size", "TOML/VCF", "JSON/VCF");
    println!("{}", "-".repeat(45));

    for (size, vcf, toml, json) in &results {
        let vcf_time = vcf.as_ref().map(|(_, _, t)| *t).unwrap_or(0.0);
        let toml_time = toml.as_ref().map(|(_, _, t)| *t).unwrap_or(0.0);
        let json_time = json.as_ref().map(|(_, _, t)| *t).unwrap_or(0.0);
        
        if vcf_time > 0.0 {
            let toml_ratio = toml_time / vcf_time;
            let json_ratio = json_time / vcf_time;
            println!("{:<10} {:<15.2}x {:<15.2}x", size, toml_ratio, json_ratio);
        }
    }

    println!("\n测试完成！");
}

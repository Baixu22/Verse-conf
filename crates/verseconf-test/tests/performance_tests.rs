use verseconf_core::{parse, IncrementalParser};
use std::time::Instant;
use std::fs;
use std::io::Write;

fn generate_large_config(size: usize) -> String {
    let mut config = String::new();
    
    for i in 0..size {
        config.push_str(&format!("host = \"server{}.example.com\"\n", i));
        config.push_str(&format!("port = {}\n", 8000 + i));
        config.push_str(&format!("enabled = {}\n", i % 2 == 0));
        config.push_str("timeout = 30s\n\n");
    }
    
    config
}

#[test]
fn test_parse_performance_small() {
    let config = generate_large_config(10);
    
    let start = Instant::now();
    for _ in 0..100 {
        let _ = parse(&config).unwrap();
    }
    let duration = start.elapsed();
    
    println!("Small config (10 servers, 100 iterations): {:?}", duration);
    assert!(duration.as_millis() < 5000);
}

#[test]
fn test_parse_performance_medium() {
    let config = generate_large_config(50);
    
    let start = Instant::now();
    for _ in 0..50 {
        let _ = parse(&config).unwrap();
    }
    let duration = start.elapsed();
    
    println!("Medium config (50 servers, 50 iterations): {:?}", duration);
    assert!(duration.as_millis() < 10000);
}

#[test]
fn test_parse_performance_large() {
    let config = generate_large_config(100);
    
    let start = Instant::now();
    let _ = parse(&config).unwrap();
    let duration = start.elapsed();
    
    println!("Large config (100 servers, 1 iteration): {:?}", duration);
    assert!(duration.as_millis() < 2000);
}

#[test]
fn test_incremental_parser_performance() {
    let test_dir = std::env::temp_dir().join("verseconf_perf_bench");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();
    
    let test_file = test_dir.join("test.vcf");
    let config = generate_large_config(20);
    let mut file = fs::File::create(&test_file).unwrap();
    file.write_all(config.as_bytes()).unwrap();
    drop(file);
    
    let mut parser = IncrementalParser::new(10);
    
    let start = Instant::now();
    for _ in 0..50 {
        let _ = parser.parse_file(&test_file).unwrap();
    }
    let duration_with_cache = start.elapsed();
    
    println!("Incremental parser (50 calls with cache): {:?}", duration_with_cache);
    assert!(duration_with_cache.as_millis() < 1000);
    
    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_cache_hit_performance() {
    let test_dir = std::env::temp_dir().join("verseconf_cache_bench");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();
    
    let test_file = test_dir.join("test.vcf");
    let config = generate_large_config(30);
    let mut file = fs::File::create(&test_file).unwrap();
    file.write_all(config.as_bytes()).unwrap();
    drop(file);
    
    let mut parser = IncrementalParser::new(10);
    
    let start = Instant::now();
    let _ = parser.parse_file(&test_file).unwrap();
    let first_parse = start.elapsed();
    
    let start = Instant::now();
    for _ in 0..100 {
        let _ = parser.parse_file(&test_file).unwrap();
    }
    let cached_parses = start.elapsed();
    
    println!("First parse: {:?}", first_parse);
    println!("100 cached parses: {:?}", cached_parses);
    println!("Average cached parse time: {:?}", cached_parses / 100);
    
    assert!(cached_parses / 100 < first_parse);
    
    let _ = fs::remove_dir_all(&test_dir);
}

use std::fs;
use std::io::Write;
use std::time::Instant;
use verseconf_core::{parse, IncrementalParser};

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

    println!(
        "Incremental parser (50 calls with cache): {:?}",
        duration_with_cache
    );
    assert!(duration_with_cache.as_millis() < 1000);

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_cache_hit_performance() {
    // 这个测试原本断言「缓存命中的平均耗时 < 首次解析耗时」。那个前提在结构上
    // 就不成立：ParseCache::get 命中时要读整个文件并比对内容哈希，返回前还要
    // 克隆一次 AST，省掉的只是重新解析。在 cargo test 的并行负载下它确实会翻车
    // （实测出现过命中 980µs vs 首次 674µs）。
    //
    // 现在断言的是真正要保证的性质——重复解析确实命中了缓存，而不是被绕过。
    // 耗时数字属于 compare/ 下的基准，不该由单元测试来钉。
    let test_dir = std::env::temp_dir().join("verseconf_cache_bench");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(&test_dir).unwrap();

    let test_file = test_dir.join("test.vcf");
    let config = generate_large_config(30);
    fs::write(&test_file, config.as_bytes()).unwrap();

    let mut parser = IncrementalParser::new(10);

    let first = parser.parse_file(&test_file).unwrap();
    let after_first = parser.cache_stats();
    assert_eq!(after_first.hits, 0, "首次解析不该命中");
    assert_eq!(after_first.misses, 1);

    for _ in 0..100 {
        let again = parser.parse_file(&test_file).unwrap();
        assert_eq!(
            again.root.entries.len(),
            first.root.entries.len(),
            "命中缓存必须返回与首次相同的结构"
        );
    }

    let after_repeat = parser.cache_stats();
    assert_eq!(after_repeat.hits, 100, "100 次重复解析必须全部命中");
    assert_eq!(after_repeat.misses, 1, "内容未变时不应新增未命中");
    assert_eq!(after_repeat.entry_count, 1);

    let _ = fs::remove_dir_all(&test_dir);
}

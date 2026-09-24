#[cfg(test)]
mod tests {
    use std::time::Instant;
    use verseconf_compare::TestDataGenerator;

    #[test]
    fn test_generate_vcf_small() {
        let generator = TestDataGenerator::new("test_output");
        let vcf = generator.generate_vcf(10);
        assert!(vcf.contains("app_name = \"test_app\""));
        assert!(vcf.contains("version = \"1.0.0\""));
        assert!(vcf.contains("debug = false"));
        assert!(vcf.contains("port = 8080"));
        assert!(vcf.contains("host = \"0.0.0.0\""));
    }

    #[test]
    fn test_generate_toml_small() {
        let generator = TestDataGenerator::new("test_output");
        let toml = generator.generate_toml(10);
        assert!(toml.contains("app_name = \"test_app\""));
        assert!(toml.contains("version = \"1.0.0\""));
        assert!(toml.contains("debug = false"));
        assert!(toml.contains("port = 8080"));
        assert!(toml.contains("host = \"0.0.0.0\""));
    }

    #[test]
    fn test_generate_json_small() {
        let generator = TestDataGenerator::new("test_output");
        let json = generator.generate_json(10);

        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
        assert!(
            parsed.is_ok(),
            "JSON should be valid, got: {:?}",
            parsed.err()
        );

        let value = parsed.unwrap();
        assert_eq!(value["app_name"].as_str().unwrap(), "test_app");
        assert_eq!(value["version"].as_str().unwrap(), "1.0.0");
        assert!(!value["debug"].as_bool().unwrap());
        assert_eq!(value["port"].as_u64().unwrap(), 8080);
        assert_eq!(value["host"].as_str().unwrap(), "0.0.0.0");
    }

    #[test]
    fn test_generate_json_medium() {
        let generator = TestDataGenerator::new("test_output");
        let json = generator.generate_json(100);

        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
        assert!(
            parsed.is_ok(),
            "JSON should be valid, got: {:?}",
            parsed.err()
        );
    }

    #[test]
    fn test_generate_json_large() {
        let generator = TestDataGenerator::new("test_output");
        let json = generator.generate_json(1000);

        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
        assert!(
            parsed.is_ok(),
            "JSON should be valid, got: {:?}",
            parsed.err()
        );
    }

    #[test]
    fn test_generate_vcf_medium() {
        let generator = TestDataGenerator::new("test_output");
        let vcf = generator.generate_vcf(100);
        assert!(vcf.contains("app_name = \"test_app\""));
        assert!(vcf.contains("server {"));
        assert!(vcf.contains("database {"));
        assert!(vcf.contains("logging {"));
        assert!(vcf.contains("cache {"));
        assert!(vcf.contains("features {"));
    }

    #[test]
    fn test_generate_toml_medium() {
        let generator = TestDataGenerator::new("test_output");
        let toml = generator.generate_toml(100);
        assert!(toml.contains("app_name = \"test_app\""));
        assert!(toml.contains("[server]"));
        assert!(toml.contains("[database]"));
        assert!(toml.contains("[logging]"));
        assert!(toml.contains("[cache]"));
        assert!(toml.contains("[features]"));
    }

    #[test]
    fn test_file_size_comparison() {
        let sizes = ["small", "medium", "large", "xlarge"];
        let counts = [10, 100, 1000, 10000];

        for (size, count) in sizes.iter().zip(counts.iter()) {
            let generator = TestDataGenerator::new("test_output");

            let vcf = generator.generate_vcf(*count);
            let toml = generator.generate_toml(*count);
            let json = generator.generate_json(*count);

            assert!(!vcf.is_empty(), "VCF should not be empty for {}", size);
            assert!(!toml.is_empty(), "TOML should not be empty for {}", size);
            assert!(!json.is_empty(), "JSON should not be empty for {}", size);

            let json_parsed = serde_json::from_str::<serde_json::Value>(&json);
            assert!(json_parsed.is_ok(), "JSON should be valid for {}", size);

            let toml_parsed = toml::from_str::<toml::Value>(&toml);
            assert!(toml_parsed.is_ok(), "TOML should be valid for {}", size);
        }
    }

    /// 三种格式的「small」数据集必须含同样的表。
    ///
    /// 此前 JSON 的小数据集分支少了 server 与 database，于是 small 比较的
    /// 其实是两份不同的文档（VCF/TOML 426B vs JSON 126B），README 里那个
    /// 「JSON 快 10 倍」的结论就是从这种不等价比出来的。
    #[test]
    fn test_small_datasets_are_cross_format_equivalent() {
        let generator = TestDataGenerator::new("test_output");

        let vcf = generator.generate_vcf(10);
        let toml_value: toml::Value =
            toml::from_str(&generator.generate_toml(10)).expect("small TOML 应当合法");
        let json_value: serde_json::Value =
            serde_json::from_str(&generator.generate_json(10)).expect("small JSON 应当合法");

        for table in ["server", "database", "features"] {
            assert!(vcf.contains(&format!("{table} {{")), "small VCF 缺 {table}");
            assert!(toml_value.get(table).is_some(), "small TOML 缺 {table}");
            assert!(json_value.get(table).is_some(), "small JSON 缺 {table}");
        }

        // 同一组基础键也必须在三种格式里都在
        for key in ["app_name", "version", "debug", "port", "host"] {
            assert!(vcf.contains(key), "small VCF 缺 {key}");
            assert!(toml_value.get(key).is_some(), "small TOML 缺 {key}");
            assert!(json_value.get(key).is_some(), "small JSON 缺 {key}");
        }
    }

    #[test]
    fn test_parsing_performance() {
        // 在内存里生成内容，不依赖 test_data/。
        //
        // 之前这里用的是 CWD 相对的 `compare/test_data/...`；cargo 把测试进程的
        // 工作目录设成包根 `compare/`，于是路径变成 `compare/compare/test_data/`，
        // 三个 read_to_string 全部失败，`if let` 整个跳过——测试永远通过且什么
        // 都没测。现在改为直接解析生成的内容，并断言解析成功。
        let generator = TestDataGenerator::new("test_output");
        let cases = [(10usize, 100usize), (100, 50), (1000, 20)];

        for (count, iterations) in cases {
            let vcf = generator.generate_vcf(count);
            let toml_text = generator.generate_toml(count);
            let json = generator.generate_json(count);

            let start = Instant::now();
            for _ in 0..iterations {
                assert!(
                    verseconf_core::parse(&vcf).is_ok(),
                    "VCF 应当解析成功 (count={count})"
                );
            }
            let vcf_duration = start.elapsed();

            let start = Instant::now();
            for _ in 0..iterations {
                assert!(
                    toml::from_str::<toml::Value>(&toml_text).is_ok(),
                    "TOML 应当解析成功 (count={count})"
                );
            }
            let toml_duration = start.elapsed();

            let start = Instant::now();
            for _ in 0..iterations {
                assert!(
                    serde_json::from_str::<serde_json::Value>(&json).is_ok(),
                    "JSON 应当解析成功 (count={count})"
                );
            }
            let json_duration = start.elapsed();

            println!(
                "count={count}: VCF={vcf_duration:?}, TOML={toml_duration:?}, JSON={json_duration:?}"
            );
        }
    }
}

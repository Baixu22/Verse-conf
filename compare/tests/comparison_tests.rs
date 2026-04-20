#[cfg(test)]
mod tests {
    use std::fs;
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
        assert!(parsed.is_ok(), "JSON should be valid, got: {:?}", parsed.err());
        
        let value = parsed.unwrap();
        assert_eq!(value["app_name"].as_str().unwrap(), "test_app");
        assert_eq!(value["version"].as_str().unwrap(), "1.0.0");
        assert_eq!(value["debug"].as_bool().unwrap(), false);
        assert_eq!(value["port"].as_u64().unwrap(), 8080);
        assert_eq!(value["host"].as_str().unwrap(), "0.0.0.0");
    }

    #[test]
    fn test_generate_json_medium() {
        let generator = TestDataGenerator::new("test_output");
        let json = generator.generate_json(100);
        
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
        assert!(parsed.is_ok(), "JSON should be valid, got: {:?}", parsed.err());
    }

    #[test]
    fn test_generate_json_large() {
        let generator = TestDataGenerator::new("test_output");
        let json = generator.generate_json(1000);
        
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
        assert!(parsed.is_ok(), "JSON should be valid, got: {:?}", parsed.err());
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
            
            assert!(vcf.len() > 0, "VCF should not be empty for {}", size);
            assert!(toml.len() > 0, "TOML should not be empty for {}", size);
            assert!(json.len() > 0, "JSON should not be empty for {}", size);
            
            let json_parsed = serde_json::from_str::<serde_json::Value>(&json);
            assert!(json_parsed.is_ok(), "JSON should be valid for {}", size);
            
            let toml_parsed = toml::from_str::<toml::Value>(&toml);
            assert!(toml_parsed.is_ok(), "TOML should be valid for {}", size);
        }
    }

    #[test]
    fn test_parsing_performance() {
        let sizes = ["small", "medium", "large"];
        let iterations = [100, 50, 20];
        
        for (size, iters) in sizes.iter().zip(iterations.iter()) {
            let vcf_path = format!("compare/test_data/{}/config.vcf", size);
            let toml_path = format!("compare/test_data/{}/config.toml", size);
            let json_path = format!("compare/test_data/{}/config.json", size);
            
            if let (Ok(vcf_content), Ok(toml_content), Ok(json_content)) = (
                fs::read_to_string(&vcf_path),
                fs::read_to_string(&toml_path),
                fs::read_to_string(&json_path),
            ) {
                let start = Instant::now();
                for _ in 0..*iters {
                    let _ = verseconf_core::parse(&vcf_content);
                }
                let vcf_duration = start.elapsed();
                
                let start = Instant::now();
                for _ in 0..*iters {
                    let _: Result<toml::Value, _> = toml::from_str(&toml_content);
                }
                let toml_duration = start.elapsed();
                
                let start = Instant::now();
                for _ in 0..*iters {
                    let _: Result<serde_json::Value, _> = serde_json::from_str(&json_content);
                }
                let json_duration = start.elapsed();
                
                assert!(vcf_duration.as_micros() > 0, "VCF parsing should complete for {}", size);
                assert!(toml_duration.as_micros() > 0, "TOML parsing should complete for {}", size);
                assert!(json_duration.as_micros() > 0, "JSON parsing should complete for {}", size);
                
                println!("{}: VCF={:?}, TOML={:?}, JSON={:?}", 
                    size, vcf_duration, toml_duration, json_duration);
            }
        }
    }
}

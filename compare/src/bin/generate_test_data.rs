use std::fs;
use std::path::Path;

/// 测试配置数据集生成器
/// 生成等价的 VCF/TOML/JSON 配置文件用于性能比较
pub struct TestDataGenerator {
    output_dir: String,
}

impl TestDataGenerator {
    pub fn new(output_dir: &str) -> Self {
        Self {
            output_dir: output_dir.to_string(),
        }
    }

    /// 生成所有测试数据集
    pub fn generate_all(&self) -> Result<(), Box<dyn std::error::Error>> {
        let sizes = [
            ("small", 10),
            ("medium", 100),
            ("large", 1000),
            ("xlarge", 10000),
        ];

        for (name, count) in sizes {
            println!("Generating {} dataset ({} items)...", name, count);
            self.generate_dataset(name, count)?;
        }

        Ok(())
    }

    /// 生成单个数据集
    fn generate_dataset(&self, name: &str, count: usize) -> Result<(), Box<dyn std::error::Error>> {
        // 创建输出目录
        let dir = Path::new(&self.output_dir).join(name);
        fs::create_dir_all(&dir)?;

        // 生成三种格式的配置
        let vcf_content = self.generate_vcf(count);
        let toml_content = self.generate_toml(count);
        let json_content = self.generate_json(count);

        // 写入文件
        fs::write(dir.join("config.vcf"), vcf_content)?;
        fs::write(dir.join("config.toml"), toml_content)?;
        fs::write(dir.join("config.json"), json_content)?;

        // 输出文件大小
        let vcf_size = dir.join("config.vcf").metadata()?.len();
        let toml_size = dir.join("config.toml").metadata()?.len();
        let json_size = dir.join("config.json").metadata()?.len();

        println!("  VCF:  {} bytes", vcf_size);
        println!("  TOML: {} bytes", toml_size);
        println!("  JSON: {} bytes", json_size);
        println!();

        Ok(())
    }

    /// 生成 VCF 配置
    pub fn generate_vcf(&self, count: usize) -> String {
        let mut lines = Vec::new();
        lines.push("# VerseConf Test Configuration".to_string());
        lines.push(format!("# Generated with {} items", count));
        lines.push(String::new());

        // 基础配置
        lines.push("# Basic settings".to_string());
        lines.push("app_name = \"test_app\"".to_string());
        lines.push("version = \"1.0.0\"".to_string());
        lines.push("debug = false".to_string());
        lines.push("port = 8080".to_string());
        lines.push("host = \"0.0.0.0\"".to_string());
        lines.push(String::new());

        // 服务器配置
        lines.push("server {".to_string());
        lines.push("  timeout = 30".to_string());
        lines.push("  max_connections = 1000".to_string());
        lines.push("  ssl_enabled = true".to_string());
        lines.push("  ssl_cert = \"/etc/ssl/cert.pem\"".to_string());
        lines.push("  ssl_key = \"/etc/ssl/key.pem\"".to_string());
        lines.push("}".to_string());
        lines.push(String::new());

        // 数据库配置
        lines.push("database {".to_string());
        lines.push("  host = \"localhost\"".to_string());
        lines.push("  port = 5432".to_string());
        lines.push("  name = \"test_db\"".to_string());
        lines.push("  pool_size = 10".to_string());
        lines.push("  timeout = 5000".to_string());
        lines.push("}".to_string());
        lines.push(String::new());

        // 生成重复的配置项
        let fixed_items = 20;
        if count <= fixed_items {
            // 对于小数据集，只生成基础配置
            lines.push("# Feature flags".to_string());
            lines.push("features {".to_string());
            for i in 0..(count.saturating_sub(fixed_items - 2)) {
                lines.push(format!("  feature_{} = {}", i, if i % 2 == 0 { "true" } else { "false" }));
            }
            lines.push("}".to_string());
            return lines.join("\n");
        }

        let items_per_section = (count - fixed_items) / 3;
        let mut current = 0;

        // 日志配置
        lines.push("# Logging configuration".to_string());
        lines.push("logging {".to_string());
        lines.push("  level = \"info\"".to_string());
        lines.push("  format = \"json\"".to_string());
        lines.push("  output = \"/var/log/app.log\"".to_string());
        current += 3;

        for i in 0..items_per_section {
            lines.push(format!("  handler_{} = \"handler_{}\"", i, i));
            current += 1;
        }
        lines.push("}".to_string());
        lines.push(String::new());

        // 缓存配置
        lines.push("# Cache configuration".to_string());
        lines.push("cache {".to_string());
        lines.push("  enabled = true".to_string());
        lines.push("  ttl = 3600".to_string());
        lines.push("  max_size = 10000".to_string());
        current += 3;

        for i in 0..items_per_section {
            lines.push(format!("  key_{} = \"value_{}\"", i, i));
            current += 1;
        }
        lines.push("}".to_string());
        lines.push(String::new());

        // 特性开关
        lines.push("# Feature flags".to_string());
        lines.push("features {".to_string());
        current += 0;

        for i in 0..(count - current - 10) {
            lines.push(format!("  feature_{} = {}", i, if i % 2 == 0 { "true" } else { "false" }));
            current += 1;
        }
        lines.push("}".to_string());

        lines.join("\n")
    }

    /// 生成 TOML 配置
    pub fn generate_toml(&self, count: usize) -> String {
        let mut lines = Vec::new();
        lines.push("# TOML Test Configuration".to_string());
        lines.push(format!("# Generated with {} items", count));
        lines.push(String::new());

        // 基础配置
        lines.push("# Basic settings".to_string());
        lines.push("app_name = \"test_app\"".to_string());
        lines.push("version = \"1.0.0\"".to_string());
        lines.push("debug = false".to_string());
        lines.push("port = 8080".to_string());
        lines.push("host = \"0.0.0.0\"".to_string());
        lines.push(String::new());

        // 服务器配置
        lines.push("[server]".to_string());
        lines.push("timeout = 30".to_string());
        lines.push("max_connections = 1000".to_string());
        lines.push("ssl_enabled = true".to_string());
        lines.push("ssl_cert = \"/etc/ssl/cert.pem\"".to_string());
        lines.push("ssl_key = \"/etc/ssl/key.pem\"".to_string());
        lines.push(String::new());

        // 数据库配置
        lines.push("[database]".to_string());
        lines.push("host = \"localhost\"".to_string());
        lines.push("port = 5432".to_string());
        lines.push("name = \"test_db\"".to_string());
        lines.push("pool_size = 10".to_string());
        lines.push("timeout = 5000".to_string());
        lines.push(String::new());

        // 生成重复的配置项
        let fixed_items = 20;
        if count <= fixed_items {
            lines.push("[features]".to_string());
            for i in 0..(count.saturating_sub(fixed_items - 2)) {
                lines.push(format!("feature_{} = {}", i, if i % 2 == 0 { "true" } else { "false" }));
            }
            return lines.join("\n");
        }

        let items_per_section = (count - fixed_items) / 3;
        let mut current = 0;

        // 日志配置
        lines.push("# Logging configuration".to_string());
        lines.push("[logging]".to_string());
        lines.push("level = \"info\"".to_string());
        lines.push("format = \"json\"".to_string());
        lines.push("output = \"/var/log/app.log\"".to_string());
        current += 3;

        for i in 0..items_per_section {
            lines.push(format!("handler_{} = \"handler_{}\"", i, i));
            current += 1;
        }
        lines.push(String::new());

        // 缓存配置
        lines.push("# Cache configuration".to_string());
        lines.push("[cache]".to_string());
        lines.push("enabled = true".to_string());
        lines.push("ttl = 3600".to_string());
        lines.push("max_size = 10000".to_string());
        current += 3;

        for i in 0..items_per_section {
            lines.push(format!("key_{} = \"value_{}\"", i, i));
            current += 1;
        }
        lines.push(String::new());

        // 特性开关
        lines.push("# Feature flags".to_string());
        lines.push("[features]".to_string());
        current += 0;

        for i in 0..(count - current - 10) {
            lines.push(format!("feature_{} = {}", i, if i % 2 == 0 { "true" } else { "false" }));
            current += 1;
        }

        lines.join("\n")
    }

    /// 生成 JSON 配置
    pub fn generate_json(&self, count: usize) -> String {
        let fixed_items = 20;
        if count <= fixed_items {
            let mut json = String::from("{\n");
            json.push_str("  \"app_name\": \"test_app\",\n");
            json.push_str("  \"version\": \"1.0.0\",\n");
            json.push_str("  \"debug\": false,\n");
            json.push_str("  \"port\": 8080,\n");
            json.push_str("  \"host\": \"0.0.0.0\",\n");
            json.push_str("  \"features\": {\n");
            
            let feature_count = count.saturating_sub(fixed_items - 2);
            for i in 0..feature_count {
                let value = if i % 2 == 0 { "true" } else { "false" };
                if i < feature_count - 1 {
                    json.push_str(&format!("    \"feature_{}\": {},\n", i, value));
                } else {
                    json.push_str(&format!("    \"feature_{}\": {}\n", i, value));
                }
            }
            json.push_str("  }\n");
            json.push_str("}");
            return json;
        }

        let items_per_section = (count - fixed_items) / 3;
        let mut current = 0;

        let mut json = String::from("{\n");

        // 基础配置
        json.push_str("  \"app_name\": \"test_app\",\n");
        json.push_str("  \"version\": \"1.0.0\",\n");
        json.push_str("  \"debug\": false,\n");
        json.push_str("  \"port\": 8080,\n");
        json.push_str("  \"host\": \"0.0.0.0\",\n");
        json.push_str("\n");

        // 服务器配置
        json.push_str("  \"server\": {\n");
        json.push_str("    \"timeout\": 30,\n");
        json.push_str("    \"max_connections\": 1000,\n");
        json.push_str("    \"ssl_enabled\": true,\n");
        json.push_str("    \"ssl_cert\": \"/etc/ssl/cert.pem\",\n");
        json.push_str("    \"ssl_key\": \"/etc/ssl/key.pem\"\n");
        json.push_str("  },\n");
        json.push_str("\n");

        // 数据库配置
        json.push_str("  \"database\": {\n");
        json.push_str("    \"host\": \"localhost\",\n");
        json.push_str("    \"port\": 5432,\n");
        json.push_str("    \"name\": \"test_db\",\n");
        json.push_str("    \"pool_size\": 10,\n");
        json.push_str("    \"timeout\": 5000\n");
        json.push_str("  },\n");
        json.push_str("\n");

        // 日志配置
        json.push_str("  \"logging\": {\n");
        json.push_str("    \"level\": \"info\",\n");
        json.push_str("    \"format\": \"json\",\n");
        json.push_str("    \"output\": \"/var/log/app.log\",\n");
        current += 3;

        for i in 0..items_per_section {
            json.push_str(&format!("    \"handler_{}\": \"handler_{}\",\n", i, i));
            current += 1;
        }
        if json.ends_with(",\n") {
            json.pop();
            json.pop();
        }
        json.push_str("\n  },\n");
        json.push_str("\n");

        // 缓存配置
        json.push_str("  \"cache\": {\n");
        json.push_str("    \"enabled\": true,\n");
        json.push_str("    \"ttl\": 3600,\n");
        json.push_str("    \"max_size\": 10000,\n");
        current += 3;

        for i in 0..items_per_section {
            json.push_str(&format!("    \"key_{}\": \"value_{}\",\n", i, i));
            current += 1;
        }
        if json.ends_with(",\n") {
            json.pop();
            json.pop();
        }
        json.push_str("\n  },\n");
        json.push_str("\n");

        // 特性开关
        json.push_str("  \"features\": {\n");
        current += 0;

        let feature_count = count - current - 10;
        for i in 0..feature_count {
            let value = if i % 2 == 0 { "true" } else { "false" };
            if i < feature_count - 1 {
                json.push_str(&format!("    \"feature_{}\": {},\n", i, value));
            } else {
                json.push_str(&format!("    \"feature_{}\": {}\n", i, value));
            }
            current += 1;
        }
        json.push_str("  }\n");

        json.push_str("}");
        json
    }
}

fn main() {
    let output_dir = "compare/test_data";
    let generator = TestDataGenerator::new(output_dir);

    println!("Generating test datasets...\n");

    match generator.generate_all() {
        Ok(_) => println!("All test datasets generated successfully!"),
        Err(e) => eprintln!("Error generating test datasets: {}", e),
    }
}

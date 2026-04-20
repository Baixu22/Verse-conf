use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ast::Ast;
use crate::parse;

/// 版本信息
#[derive(Debug, Clone)]
pub struct VersionInfo {
    pub version_id: u64,
    pub timestamp: u64,
    pub content_hash: u64,
    pub description: Option<String>,
    pub file_path: PathBuf,
}

/// 版本记录
#[derive(Debug, Clone)]
pub struct VersionRecord {
    pub info: VersionInfo,
    pub content: String,
    pub ast: Option<Ast>,
}

/// 版本管理器
pub struct VersionManager {
    records: HashMap<String, Vec<VersionRecord>>,
    storage_dir: Option<PathBuf>,
    next_version_id: u64,
}

impl VersionManager {
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            storage_dir: None,
            next_version_id: 1,
        }
    }

    pub fn with_storage(mut self, storage_dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&storage_dir)
            .map_err(|e| format!("Failed to create storage dir: {}", e))?;
        self.storage_dir = Some(storage_dir);
        Ok(self)
    }

    pub fn create_version(
        &mut self,
        file_path: &Path,
        description: Option<String>,
    ) -> Result<u64, String> {
        let content = fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read file: {}", e))?;
        
        let ast = parse(&content).ok();
        let content_hash = compute_hash(content.as_bytes());
        let timestamp = current_timestamp();
        
        let version_id = self.next_version_id;
        self.next_version_id += 1;
        
        let key = file_path.to_string_lossy().to_string();
        let info = VersionInfo {
            version_id,
            timestamp,
            content_hash,
            description,
            file_path: file_path.to_path_buf(),
        };
        
        if let Some(ref _storage_dir) = self.storage_dir {
            self.save_to_disk(file_path, version_id, &content)?;
        }
        
        let record = VersionRecord {
            info,
            content,
            ast,
        };
        
        self.records.entry(key).or_insert_with(Vec::new).push(record);
        
        Ok(version_id)
    }

    pub fn get_version(&self, file_path: &Path, version_id: u64) -> Option<&VersionRecord> {
        let key = file_path.to_string_lossy().to_string();
        self.records
            .get(&key)?
            .iter()
            .find(|r| r.info.version_id == version_id)
    }

    pub fn get_latest_version(&self, file_path: &Path) -> Option<&VersionRecord> {
        let key = file_path.to_string_lossy().to_string();
        self.records.get(&key)?.last()
    }

    pub fn get_version_history(&self, file_path: &Path) -> Vec<&VersionInfo> {
        let key = file_path.to_string_lossy().to_string();
        self.records
            .get(&key)
            .map(|records| records.iter().map(|r| &r.info).collect())
            .unwrap_or_default()
    }

    pub fn rollback(&self, file_path: &Path, version_id: u64) -> Result<String, String> {
        let record = self
            .get_version(file_path, version_id)
            .ok_or_else(|| format!("Version {} not found", version_id))?;
        
        fs::write(file_path, &record.content)
            .map_err(|e| format!("Failed to write file: {}", e))?;
        
        Ok(format!("Rolled back to version {}", version_id))
    }

    pub fn compare_versions(
        &self,
        file_path: &Path,
        version_a: u64,
        version_b: u64,
    ) -> Result<VersionDiff, String> {
        let record_a = self
            .get_version(file_path, version_a)
            .ok_or_else(|| format!("Version {} not found", version_a))?;
        
        let record_b = self
            .get_version(file_path, version_b)
            .ok_or_else(|| format!("Version {} not found", version_b))?;
        
        let diff = compute_diff(&record_a.content, &record_b.content);
        
        Ok(VersionDiff {
            version_a: version_a,
            version_b: version_b,
            diff,
        })
    }

    pub fn get_version_count(&self, file_path: &Path) -> usize {
        let key = file_path.to_string_lossy().to_string();
        self.records.get(&key).map_or(0, |v| v.len())
    }

    fn save_to_disk(
        &self,
        file_path: &Path,
        version_id: u64,
        content: &str,
    ) -> Result<(), String> {
        if let Some(ref storage_dir) = self.storage_dir {
            let version_dir = storage_dir.join(format!(
                "{}_versions",
                file_path.file_stem().unwrap_or_default().to_string_lossy()
            ));
            fs::create_dir_all(&version_dir)
                .map_err(|e| format!("Failed to create version dir: {}", e))?;
            
            let version_file = version_dir.join(format!("v{}.vcf", version_id));
            fs::write(version_file, content)
                .map_err(|e| format!("Failed to save version: {}", e))?;
        }
        Ok(())
    }
}

impl Default for VersionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 版本差异
#[derive(Debug, Clone)]
pub struct VersionDiff {
    pub version_a: u64,
    pub version_b: u64,
    pub diff: String,
}

impl std::fmt::Display for VersionDiff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Version {} -> {}:\n{}",
            self.version_a, self.version_b, self.diff
        )
    }
}

fn compute_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 5381;
    for byte in data {
        hash = hash.wrapping_mul(33).wrapping_add(*byte as u64);
    }
    hash
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn compute_diff(content_a: &str, content_b: &str) -> String {
    let lines_a: Vec<&str> = content_a.lines().collect();
    let lines_b: Vec<&str> = content_b.lines().collect();
    
    let mut diff = String::new();
    
    let max_lines = lines_a.len().max(lines_b.len());
    
    for i in 0..max_lines {
        let line_a = lines_a.get(i).copied().unwrap_or("");
        let line_b = lines_b.get(i).copied().unwrap_or("");
        
        if line_a != line_b {
            if i < lines_a.len() {
                diff.push_str(&format!("- {}\n", line_a));
            }
            if i < lines_b.len() {
                diff.push_str(&format!("+ {}\n", line_b));
            }
        }
    }
    
    if diff.is_empty() {
        "No differences".to_string()
    } else {
        diff
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_version_manager_basic() {
        let test_dir = std::env::temp_dir().join("verseconf_version_test");
        let _ = fs::remove_dir_all(&test_dir);
        fs::create_dir_all(&test_dir).unwrap();
        
        let test_file = test_dir.join("test.vcf");
        let mut file = fs::File::create(&test_file).unwrap();
        file.write_all(b"name = \"test1\"").unwrap();
        drop(file);
        
        let mut manager = VersionManager::new();
        let version1 = manager.create_version(&test_file, Some("Initial version".to_string())).unwrap();
        
        assert_eq!(version1, 1);
        assert_eq!(manager.get_version_count(&test_file), 1);
        
        let mut file = fs::File::create(&test_file).unwrap();
        file.write_all(b"name = \"test2\"").unwrap();
        drop(file);
        
        let version2 = manager.create_version(&test_file, Some("Updated version".to_string())).unwrap();
        assert_eq!(version2, 2);
        assert_eq!(manager.get_version_count(&test_file), 2);
        
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_version_history() {
        let test_dir = std::env::temp_dir().join("verseconf_version_test2");
        let _ = fs::remove_dir_all(&test_dir);
        fs::create_dir_all(&test_dir).unwrap();
        
        let test_file = test_dir.join("test.vcf");
        
        let mut manager = VersionManager::new();
        
        for i in 1..=3 {
            let mut file = fs::File::create(&test_file).unwrap();
            file.write_all(format!("name = \"test{}\"", i).as_bytes()).unwrap();
            drop(file);
            
            manager.create_version(&test_file, None).unwrap();
        }
        
        let history = manager.get_version_history(&test_file);
        assert_eq!(history.len(), 3);
        
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_rollback() {
        let test_dir = std::env::temp_dir().join("verseconf_version_test3");
        let _ = fs::remove_dir_all(&test_dir);
        fs::create_dir_all(&test_dir).unwrap();
        
        let test_file = test_dir.join("test.vcf");
        let mut file = fs::File::create(&test_file).unwrap();
        file.write_all(b"name = \"version1\"").unwrap();
        drop(file);
        
        let mut manager = VersionManager::new();
        let version1 = manager.create_version(&test_file, None).unwrap();
        
        let mut file = fs::File::create(&test_file).unwrap();
        file.write_all(b"name = \"version2\"").unwrap();
        drop(file);
        
        manager.create_version(&test_file, None).unwrap();
        
        manager.rollback(&test_file, version1).unwrap();
        
        let content = fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("version1"));
        
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_compare_versions() {
        let test_dir = std::env::temp_dir().join("verseconf_version_test4");
        let _ = fs::remove_dir_all(&test_dir);
        fs::create_dir_all(&test_dir).unwrap();
        
        let test_file = test_dir.join("test.vcf");
        let mut file = fs::File::create(&test_file).unwrap();
        file.write_all(b"port = 8080").unwrap();
        drop(file);
        
        let mut manager = VersionManager::new();
        let version1 = manager.create_version(&test_file, None).unwrap();
        
        let mut file = fs::File::create(&test_file).unwrap();
        file.write_all(b"port = 9090").unwrap();
        drop(file);
        
        let version2 = manager.create_version(&test_file, None).unwrap();
        
        let diff = manager.compare_versions(&test_file, version1, version2).unwrap();
        
        assert!(diff.diff.contains("- port = 8080"));
        assert!(diff.diff.contains("+ port = 9090"));
        
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_version_with_storage() {
        let test_dir = std::env::temp_dir().join("verseconf_version_test5");
        let _ = fs::remove_dir_all(&test_dir);
        fs::create_dir_all(&test_dir).unwrap();
        
        let test_file = test_dir.join("test.vcf");
        let mut file = fs::File::create(&test_file).unwrap();
        file.write_all(b"name = \"test\"").unwrap();
        drop(file);
        
        let storage_dir = test_dir.join("versions");
        let mut manager = VersionManager::new()
            .with_storage(storage_dir.clone())
            .unwrap();
        
        manager.create_version(&test_file, None).unwrap();
        
        let version_dir = storage_dir.join("test_versions");
        assert!(version_dir.exists());
        assert!(version_dir.join("v1.vcf").exists());
        
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_get_latest_version() {
        let test_dir = std::env::temp_dir().join("verseconf_version_test6");
        let _ = fs::remove_dir_all(&test_dir);
        fs::create_dir_all(&test_dir).unwrap();
        
        let test_file = test_dir.join("test.vcf");
        
        let mut manager = VersionManager::new();
        
        for i in 1..=3 {
            let mut file = fs::File::create(&test_file).unwrap();
            file.write_all(format!("name = \"test{}\"", i).as_bytes()).unwrap();
            drop(file);
            
            manager.create_version(&test_file, None).unwrap();
        }
        
        let latest = manager.get_latest_version(&test_file).unwrap();
        assert_eq!(latest.info.version_id, 3);
        assert!(latest.content.contains("test3"));
        
        let _ = fs::remove_dir_all(&test_dir);
    }
}

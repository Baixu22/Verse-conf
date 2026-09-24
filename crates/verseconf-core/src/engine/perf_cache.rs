use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ast::Ast;
use crate::parse;

/// 文件元数据
#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub path: PathBuf,
    pub modified_time: u64,
    pub size: u64,
    pub hash: u64,
}

impl FileMetadata {
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let metadata = fs::metadata(path).map_err(|e| format!("Failed to read metadata: {}", e))?;
        let modified = metadata
            .modified()
            .map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs())
            .unwrap_or(0);
        let size = metadata.len();
        let content = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
        let hash = compute_hash(&content);

        Ok(Self {
            path: path.to_path_buf(),
            modified_time: modified,
            size,
            hash,
        })
    }

    pub fn is_modified_since(&self, timestamp: u64) -> bool {
        self.modified_time > timestamp
    }
}

/// 解析结果缓存
#[derive(Debug)]
pub struct ParseCache {
    cache: HashMap<String, (FileMetadata, Ast)>,
    max_size: usize,
    hits: u64,
    misses: u64,
}

impl ParseCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: HashMap::new(),
            max_size,
            hits: 0,
            misses: 0,
        }
    }

    /// 命中则返回缓存的 AST，并把这次查询计入命中数。
    ///
    /// 注意「命中」不是零成本：判定命中要读整个文件并比对内容哈希，返回前还要
    /// 克隆一次 AST，省掉的只是重新解析。所以命中不一定比首次解析快——调用方
    /// 不应该拿它做性能断言，要判断缓存是否生效请看 [`ParseCacheStats::hits`]。
    pub fn get(&mut self, path: &Path) -> Option<&Ast> {
        let key = self.path_key(path);

        let fresh = match self.cache.get(&key) {
            Some((metadata, _)) => {
                FileMetadata::from_path(path).is_ok_and(|current| current.hash == metadata.hash)
            }
            None => false,
        };

        if fresh {
            self.hits += 1;
            return self.cache.get(&key).map(|(_, ast)| ast);
        }

        self.misses += 1;
        None
    }

    pub fn insert(&mut self, path: &Path, ast: Ast) -> Result<(), String> {
        let metadata = FileMetadata::from_path(path)?;
        let key = self.path_key(path);

        if self.cache.len() >= self.max_size {
            self.evict_oldest();
        }

        self.cache.insert(key, (metadata, ast));
        Ok(())
    }

    pub fn invalidate(&mut self, path: &Path) {
        let key = self.path_key(path);
        self.cache.remove(&key);
    }

    pub fn clear(&mut self) {
        self.cache.clear();
        self.hits = 0;
        self.misses = 0;
    }

    pub fn stats(&self) -> ParseCacheStats {
        ParseCacheStats {
            entry_count: self.cache.len(),
            max_size: self.max_size,
            hits: self.hits,
            misses: self.misses,
        }
    }

    fn path_key(&self, path: &Path) -> String {
        path.canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .to_string()
    }

    fn evict_oldest(&mut self) {
        if let Some((oldest_key, _)) = self
            .cache
            .iter()
            .min_by_key(|(_, (metadata, _))| metadata.modified_time)
        {
            let key = oldest_key.clone();
            self.cache.remove(&key);
        }
    }
}

/// 缓存统计
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseCacheStats {
    pub entry_count: usize,
    pub max_size: usize,
    /// 命中次数。命中判定包含一次文件读取与内容哈希比对。
    pub hits: u64,
    /// 未命中次数：没有条目，或文件内容已经变化。
    pub misses: u64,
}

/// 增量解析器
#[derive(Debug)]
pub struct IncrementalParser {
    cache: ParseCache,
    last_parse_time: u64,
}

impl IncrementalParser {
    pub fn new(cache_size: usize) -> Self {
        Self {
            cache: ParseCache::new(cache_size),
            last_parse_time: 0,
        }
    }

    pub fn parse_file(&mut self, path: &Path) -> Result<Ast, String> {
        if let Some(ast) = self.cache.get(path) {
            return Ok(ast.clone());
        }

        let ast = parse_file_to_ast(path)?;
        self.cache
            .insert(path, ast.clone())
            .map_err(|e| e.to_string())?;
        self.last_parse_time = current_timestamp();

        Ok(ast)
    }

    pub fn parse_files(&mut self, paths: &[PathBuf]) -> Result<HashMap<String, Ast>, String> {
        let mut results = HashMap::new();

        for path in paths {
            if let Some(ast) = self.cache.get(path) {
                results.insert(path.to_string_lossy().to_string(), ast.clone());
            } else {
                let ast = parse_file_to_ast(path)?;
                self.cache
                    .insert(path, ast.clone())
                    .map_err(|e| e.to_string())?;
                results.insert(path.to_string_lossy().to_string(), ast);
            }
        }

        self.last_parse_time = current_timestamp();
        Ok(results)
    }

    pub fn needs_reparse(&self, path: &Path) -> bool {
        if let Ok(metadata) = FileMetadata::from_path(path) {
            return metadata.is_modified_since(self.last_parse_time);
        }
        true
    }

    pub fn cache_stats(&self) -> ParseCacheStats {
        self.cache.stats()
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.last_parse_time = 0;
    }
}

fn parse_file_to_ast(path: &Path) -> Result<Ast, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;
    parse(&content).map_err(|e| format!("Parse error: {}", e))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_file_metadata() {
        let test_dir = std::env::temp_dir().join("verseconf_perf_test");
        let _ = fs::remove_dir_all(&test_dir);
        fs::create_dir_all(&test_dir).unwrap();

        let test_file = test_dir.join("test.vcf");
        let mut file = File::create(&test_file).unwrap();
        file.write_all(b"name = \"test\"").unwrap();
        drop(file);

        let metadata = FileMetadata::from_path(&test_file).unwrap();
        assert!(metadata.size > 0);
        assert!(metadata.hash > 0);

        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_parse_cache_basic() {
        let test_dir = std::env::temp_dir().join("verseconf_perf_test2");
        let _ = fs::remove_dir_all(&test_dir);
        fs::create_dir_all(&test_dir).unwrap();

        let test_file = test_dir.join("test.vcf");
        let mut file = File::create(&test_file).unwrap();
        file.write_all(b"name = \"test\"").unwrap();
        drop(file);

        let mut cache = ParseCache::new(10);
        let ast = parse_file_to_ast(&test_file).unwrap();
        cache.insert(&test_file, ast).unwrap();

        let cached_ast = cache.get(&test_file);
        assert!(cached_ast.is_some());

        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_parse_cache_invalidation() {
        let test_dir = std::env::temp_dir().join("verseconf_perf_test3");
        let _ = fs::remove_dir_all(&test_dir);
        fs::create_dir_all(&test_dir).unwrap();

        let test_file = test_dir.join("test.vcf");
        let mut file = File::create(&test_file).unwrap();
        file.write_all(b"name = \"test\"").unwrap();
        drop(file);

        let mut cache = ParseCache::new(10);
        let ast = parse_file_to_ast(&test_file).unwrap();
        cache.insert(&test_file, ast).unwrap();

        cache.invalidate(&test_file);
        assert!(cache.get(&test_file).is_none());

        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_incremental_parser() {
        let test_dir = std::env::temp_dir().join("verseconf_perf_test4");
        let _ = fs::remove_dir_all(&test_dir);
        fs::create_dir_all(&test_dir).unwrap();

        let test_file = test_dir.join("test.vcf");
        let mut file = File::create(&test_file).unwrap();
        file.write_all(b"name = \"test\"").unwrap();
        drop(file);

        let mut parser = IncrementalParser::new(10);
        let ast1 = parser.parse_file(&test_file).unwrap();
        assert!(!ast1.root.entries.is_empty());

        let ast2 = parser.parse_file(&test_file).unwrap();
        assert_eq!(ast1.root.entries.len(), ast2.root.entries.len());

        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_cache_eviction() {
        let test_dir = std::env::temp_dir().join("verseconf_perf_test5");
        let _ = fs::remove_dir_all(&test_dir);
        fs::create_dir_all(&test_dir).unwrap();

        let mut cache = ParseCache::new(2);

        for i in 0..5 {
            let test_file = test_dir.join(format!("test{}.vcf", i));
            let mut file = File::create(&test_file).unwrap();
            file.write_all(format!("name = \"test{}\"", i).as_bytes())
                .unwrap();
            drop(file);

            let ast = parse_file_to_ast(&test_file).unwrap();
            cache.insert(&test_file, ast).unwrap();
        }

        assert!(cache.cache.len() <= 2);

        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_cache_counts_hits_and_misses() {
        let test_dir = std::env::temp_dir().join("verseconf_perf_test6");
        let _ = fs::remove_dir_all(&test_dir);
        fs::create_dir_all(&test_dir).unwrap();

        let test_file = test_dir.join("test.vcf");
        fs::write(&test_file, "name = \"test\"").unwrap();

        let mut cache = ParseCache::new(10);

        // 空缓存：一次未命中
        assert!(cache.get(&test_file).is_none());
        assert_eq!(cache.stats().hits, 0);
        assert_eq!(cache.stats().misses, 1);

        let ast = parse_file_to_ast(&test_file).unwrap();
        cache.insert(&test_file, ast).unwrap();

        // 内容未变：连续两次命中
        assert!(cache.get(&test_file).is_some());
        assert!(cache.get(&test_file).is_some());
        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.entry_count, 1);

        // clear 会把计数一并归零
        cache.clear();
        assert_eq!(cache.stats().hits, 0);
        assert_eq!(cache.stats().misses, 0);

        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_cache_misses_when_file_content_changes() {
        let test_dir = std::env::temp_dir().join("verseconf_perf_test7");
        let _ = fs::remove_dir_all(&test_dir);
        fs::create_dir_all(&test_dir).unwrap();

        let test_file = test_dir.join("test.vcf");
        fs::write(&test_file, "name = \"before\"").unwrap();

        let mut parser = IncrementalParser::new(10);
        parser.parse_file(&test_file).unwrap();
        parser.parse_file(&test_file).unwrap();
        assert_eq!(parser.cache_stats().hits, 1, "内容未变时应当命中");

        // 改内容后必须未命中——判定依据是内容哈希，不是 mtime 的秒级精度
        fs::write(&test_file, "name = \"after\"").unwrap();
        parser.parse_file(&test_file).unwrap();

        let stats = parser.cache_stats();
        assert_eq!(stats.hits, 1, "内容变了就不该再算命中");
        assert_eq!(stats.misses, 2);

        let _ = fs::remove_dir_all(&test_dir);
    }
}

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Cache entry storing serialized AST and metadata
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub ast_bytes: Vec<u8>,
    pub file_hash: u64,
    pub last_modified: SystemTime,
    pub created_at: SystemTime,
}

/// Binary cache for parsed ASTs
pub struct BinaryCache {
    cache_dir: PathBuf,
    entries: HashMap<String, CacheEntry>,
}

impl BinaryCache {
    /// Create a new binary cache in the specified directory
    pub fn new(cache_dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&cache_dir).map_err(|e| format!("Failed to create cache dir: {}", e))?;
        
        let mut cache = Self {
            cache_dir,
            entries: HashMap::new(),
        };
        
        cache.load_index()?;
        Ok(cache)
    }

    /// Get cached AST if it exists and is still valid
    pub fn get(&self, file_path: &Path) -> Option<Vec<u8>> {
        let key = self.file_key(file_path);
        if let Some(entry) = self.entries.get(&key) {
            if self.is_cache_valid(file_path, entry) {
                return Some(entry.ast_bytes.clone());
            }
        }
        None
    }

    /// Store AST in cache
    pub fn put(&mut self, file_path: &Path, ast_bytes: Vec<u8>) -> Result<(), String> {
        let key = self.file_key(file_path);
        
        let file_meta = fs::metadata(file_path).map_err(|e| format!("Failed to get file metadata: {}", e))?;
        let last_modified = file_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let file_hash = self.compute_file_hash(file_path)?;
        
        let entry = CacheEntry {
            ast_bytes,
            file_hash,
            last_modified,
            created_at: SystemTime::now(),
        };
        
        self.entries.insert(key.clone(), entry);
        self.save_entry(&key)?;
        Ok(())
    }

    /// Invalidate cache for a specific file
    pub fn invalidate(&mut self, file_path: &Path) {
        let key = self.file_key(file_path);
        self.entries.remove(&key);
        let cache_file = self.cache_dir.join(format!("{}.cache", key));
        let _ = fs::remove_file(cache_file);
    }

    /// Clear all cached entries
    pub fn clear(&mut self) -> Result<(), String> {
        self.entries.clear();
        for entry in fs::read_dir(&self.cache_dir).map_err(|e| format!("Failed to read cache dir: {}", e))? {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            if entry.path().extension().map_or(false, |ext| ext == "cache") {
                fs::remove_file(entry.path()).map_err(|e| format!("Failed to remove cache file: {}", e))?;
            }
        }
        Ok(())
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let total_size: usize = self.entries.values().map(|e| e.ast_bytes.len()).sum();
        CacheStats {
            entry_count: self.entries.len(),
            total_size,
            cache_dir: self.cache_dir.clone(),
        }
    }

    fn file_key(&self, file_path: &Path) -> String {
        // Use absolute path as key, replace path separators
        file_path.canonicalize()
            .unwrap_or_else(|_| file_path.to_path_buf())
            .to_string_lossy()
            .replace(['/', '\\', ':'], "_")
    }

    fn is_cache_valid(&self, file_path: &Path, entry: &CacheEntry) -> bool {
        if let Ok(meta) = fs::metadata(file_path) {
            if let Ok(modified) = meta.modified() {
                if modified != entry.last_modified {
                    return false;
                }
            }
        }
        
        if let Ok(hash) = self.compute_file_hash(file_path) {
            return hash == entry.file_hash;
        }
        
        false
    }

    fn compute_file_hash(&self, file_path: &Path) -> Result<u64, String> {
        let content = fs::read(file_path).map_err(|e| format!("Failed to read file: {}", e))?;
        Ok(self.simple_hash(&content))
    }

    fn simple_hash(&self, data: &[u8]) -> u64 {
        let mut hash: u64 = 0;
        for (i, &byte) in data.iter().enumerate() {
            hash = hash.wrapping_add((byte as u64).wrapping_mul((i as u64).wrapping_add(1)));
            hash = hash.rotate_left(7);
        }
        hash
    }

    fn load_index(&mut self) -> Result<(), String> {
        // Simple implementation: load entries on demand
        Ok(())
    }

    fn save_entry(&self, _key: &str) -> Result<(), String> {
        // In a full implementation, serialize CacheEntry to disk
        Ok(())
    }
}

/// Cache statistics
pub struct CacheStats {
    pub entry_count: usize,
    pub total_size: usize,
    pub cache_dir: PathBuf,
}

impl std::fmt::Display for CacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cache: {} entries, {} bytes, dir: {}",
            self.entry_count,
            self.total_size,
            self.cache_dir.display()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_cache_basic() {
        let cache_dir = std::env::temp_dir().join("verseconf_test_cache");
        let _ = fs::remove_dir_all(&cache_dir);
        
        let mut cache = BinaryCache::new(cache_dir.clone()).unwrap();
        
        // Create a test file
        let test_file = cache_dir.join("test.vcf");
        let mut file = File::create(&test_file).unwrap();
        file.write_all(b"name = \"test\"").unwrap();
        
        // Store in cache
        cache.put(&test_file, vec![1, 2, 3, 4]).unwrap();
        
        // Retrieve from cache
        let cached = cache.get(&test_file);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), vec![1, 2, 3, 4]);
        
        // Cleanup
        let _ = fs::remove_dir_all(&cache_dir);
    }

    #[test]
    fn test_cache_invalidation() {
        let cache_dir = std::env::temp_dir().join("verseconf_test_cache2");
        let _ = fs::remove_dir_all(&cache_dir);
        
        let mut cache = BinaryCache::new(cache_dir.clone()).unwrap();
        
        let test_file = cache_dir.join("test.vcf");
        let mut file = File::create(&test_file).unwrap();
        file.write_all(b"name = \"test\"").unwrap();
        
        cache.put(&test_file, vec![1, 2, 3]).unwrap();
        assert!(cache.get(&test_file).is_some());
        
        // Invalidate
        cache.invalidate(&test_file);
        assert!(cache.get(&test_file).is_none());
        
        let _ = fs::remove_dir_all(&cache_dir);
    }
}

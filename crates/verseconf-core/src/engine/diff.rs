use std::fmt;

/// 差异类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffType {
    Added,
    Removed,
    Modified,
    Unchanged,
}

impl fmt::Display for DiffType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiffType::Added => write!(f, "+"),
            DiffType::Removed => write!(f, "-"),
            DiffType::Modified => write!(f, "~"),
            DiffType::Unchanged => write!(f, " "),
        }
    }
}

/// 单个差异项
#[derive(Debug, Clone)]
pub struct DiffEntry {
    /// 差异类型
    pub diff_type: DiffType,
    /// 键路径（如 "server.host"）
    pub path: String,
    /// 旧值
    pub old_value: Option<String>,
    /// 新值
    pub new_value: Option<String>,
    /// 行号（如果可用）
    pub line: Option<usize>,
}

impl DiffEntry {
    pub fn added(path: &str, value: &str) -> Self {
        Self {
            diff_type: DiffType::Added,
            path: path.to_string(),
            old_value: None,
            new_value: Some(value.to_string()),
            line: None,
        }
    }

    pub fn removed(path: &str, value: &str) -> Self {
        Self {
            diff_type: DiffType::Removed,
            path: path.to_string(),
            old_value: Some(value.to_string()),
            new_value: None,
            line: None,
        }
    }

    pub fn modified(path: &str, old_value: &str, new_value: &str) -> Self {
        Self {
            diff_type: DiffType::Modified,
            path: path.to_string(),
            old_value: Some(old_value.to_string()),
            new_value: Some(new_value.to_string()),
            line: None,
        }
    }
}

/// 完整的差异结果
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// 差异项列表
    pub entries: Vec<DiffEntry>,
    /// 源文件路径（可选）
    pub old_source: Option<String>,
    /// 目标文件路径（可选）
    pub new_source: Option<String>,
}

impl DiffResult {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            old_source: None,
            new_source: None,
        }
    }

    pub fn with_sources(mut self, old: &str, new: &str) -> Self {
        self.old_source = Some(old.to_string());
        self.new_source = Some(new.to_string());
        self
    }

    pub fn add_entry(&mut self, entry: DiffEntry) {
        self.entries.push(entry);
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn stats(&self) -> DiffStats {
        let added = self.entries.iter().filter(|e| e.diff_type == DiffType::Added).count();
        let removed = self.entries.iter().filter(|e| e.diff_type == DiffType::Removed).count();
        let modified = self.entries.iter().filter(|e| e.diff_type == DiffType::Modified).count();
        let unchanged = self.entries.iter().filter(|e| e.diff_type == DiffType::Unchanged).count();
        
        DiffStats {
            added,
            removed,
            modified,
            unchanged,
            total: self.entries.len(),
        }
    }
}

impl Default for DiffResult {
    fn default() -> Self {
        Self::new()
    }
}

/// 差异统计
#[derive(Debug, Clone)]
pub struct DiffStats {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
    pub unchanged: usize,
    pub total: usize,
}

impl fmt::Display for DiffStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} added, {} removed, {} modified, {} unchanged ({} total)",
            self.added, self.removed, self.modified, self.unchanged, self.total
        )
    }
}

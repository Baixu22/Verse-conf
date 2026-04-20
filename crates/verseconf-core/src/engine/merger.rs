use crate::ast::*;
use crate::parser::Parser;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// File loader trait for custom file loading implementations
pub trait FileLoader {
    fn load_file(&self, path: &Path) -> Result<String, String>;
}

/// Default file loader using std::fs
pub struct DefaultFileLoader {
    pub base_path: PathBuf,
}

impl FileLoader for DefaultFileLoader {
    fn load_file(&self, path: &Path) -> Result<String, String> {
        let full_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base_path.join(path)
        };
        std::fs::read_to_string(&full_path).map_err(|e| format!("Failed to read file {}: {}", full_path.display(), e))
    }
}

/// Configuration for the merger
pub struct MergerConfig {
    pub max_include_depth: usize,
    pub strategy: MergeStrategy,
}

impl Default for MergerConfig {
    fn default() -> Self {
        Self {
            max_include_depth: 10,
            strategy: MergeStrategy::DeepMerge,
        }
    }
}

/// AST Merger for @include directives
pub struct AstMerger {
    config: MergerConfig,
    loader: Box<dyn FileLoader>,
    include_stack: Vec<PathBuf>,
}

impl AstMerger {
    pub fn new(loader: Box<dyn FileLoader>) -> Self {
        Self {
            config: MergerConfig::default(),
            loader,
            include_stack: Vec::new(),
        }
    }

    pub fn with_config(mut self, config: MergerConfig) -> Self {
        self.config = config;
        self
    }

    /// Merge included files into the AST
    pub fn merge_includes(&mut self, ast: &mut Ast, base_path: &Path) -> Result<(), String> {
        self.include_stack.clear();
        self.include_stack.push(base_path.to_path_buf());
        self.merge_table_block(&mut ast.root, base_path)?;
        Ok(())
    }

    fn merge_table_block(&mut self, table: &mut TableBlock, base_path: &Path) -> Result<(), String> {
        let entries = std::mem::take(&mut table.entries);
        let mut new_entries = Vec::new();
        
        for entry in entries {
            match entry {
                TableEntry::IncludeDirective(inc) => {
                    let included = self.load_and_parse(&inc.path, base_path)?;
                    let merged = self.apply_merge_strategy(&inc.merge_strategy, table, included)?;
                    new_entries.extend(merged);
                }
                TableEntry::TableBlock(mut tb) => {
                    self.merge_table_block(&mut tb, base_path)?;
                    new_entries.push(TableEntry::TableBlock(tb));
                }
                TableEntry::KeyValue(mut kv) => {
                    if let Value::TableBlock(ref mut tb) = kv.value {
                        self.merge_table_block(tb, base_path)?;
                    }
                    new_entries.push(TableEntry::KeyValue(kv));
                }
                other => new_entries.push(other),
            }
        }
        
        table.entries = new_entries;
        Ok(())
    }

    fn load_and_parse(&mut self, path: &str, base_path: &Path) -> Result<TableBlock, String> {
        let include_path = Path::new(path);
        
        // Check for circular includes
        if self.include_stack.contains(&include_path.to_path_buf()) {
            return Err(format!("Circular include detected: {:?}", include_path));
        }
        
        if self.include_stack.len() >= self.config.max_include_depth {
            return Err(format!("Maximum include depth exceeded: {}", self.config.max_include_depth));
        }
        
        self.include_stack.push(include_path.to_path_buf());
        
        let source = self.loader.load_file(include_path)?;
        let parser = Parser::new(source);
        let ast = parser.parse().map_err(|e| format!("Parse error in {}: {:?}", path, e))?;
        
        let mut root = ast.root;
        self.merge_table_block(&mut root, include_path.parent().unwrap_or(base_path))?;
        
        self.include_stack.pop();
        Ok(root)
    }

    fn apply_merge_strategy(
        &self,
        strategy: &crate::ast::MergeStrategy,
        target: &TableBlock,
        source: TableBlock,
    ) -> Result<Vec<TableEntry>, String> {
        match strategy {
            crate::ast::MergeStrategy::Override => Ok(source.entries),
            crate::ast::MergeStrategy::Append => {
                let mut result = target.entries.clone();
                result.extend(source.entries);
                Ok(result)
            }
            crate::ast::MergeStrategy::Merge => {
                self.shallow_merge(target, source)
            }
            crate::ast::MergeStrategy::DeepMerge => {
                self.deep_merge(target, source)
            }
        }
    }

    fn shallow_merge(&self, target: &TableBlock, source: TableBlock) -> Result<Vec<TableEntry>, String> {
        let mut result: HashMap<String, TableEntry> = HashMap::new();
        
        // Add target entries first
        for entry in &target.entries {
            if let Some(key) = self.get_entry_key(entry) {
                result.insert(key.clone(), entry.clone());
            }
        }
        
        // Override with source entries
        for entry in source.entries {
            if let Some(key) = self.get_entry_key(&entry) {
                result.insert(key.clone(), entry);
            }
        }
        
        Ok(result.into_values().collect())
    }

    fn deep_merge(&self, target: &TableBlock, source: TableBlock) -> Result<Vec<TableEntry>, String> {
        let mut result: HashMap<String, TableEntry> = HashMap::new();
        
        // Add target entries first
        for entry in &target.entries {
            if let Some(key) = self.get_entry_key(entry) {
                result.insert(key.clone(), entry.clone());
            }
        }
        
        // Deep merge source entries
        for source_entry in source.entries {
            if let Some(source_key) = self.get_entry_key(&source_entry) {
                if let Some(target_entry) = result.get(&source_key) {
                    // Both exist, try to deep merge
                    if let (TableEntry::TableBlock(t_tb), TableEntry::TableBlock(s_tb)) = (target_entry, &source_entry) {
                        let mut merged_tb = t_tb.clone();
                        merged_tb.entries.extend(s_tb.entries.clone());
                        result.insert(source_key.clone(), TableEntry::TableBlock(merged_tb));
                        continue;
                    }
                }
                result.insert(source_key.clone(), source_entry);
            }
        }
        
        Ok(result.into_values().collect())
    }

    fn get_entry_key(&self, entry: &TableEntry) -> Option<String> {
        match entry {
            TableEntry::KeyValue(kv) => Some(kv.key.as_str().to_string()),
            TableEntry::TableBlock(tb) => tb.name.clone(),
            TableEntry::ArrayTable(at) => Some(format!("[[{}]]", at.key.as_str())),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockFileLoader {
        files: HashMap<String, String>,
    }

    impl FileLoader for MockFileLoader {
        fn load_file(&self, path: &Path) -> Result<String, String> {
            let path_str = path.to_string_lossy().to_string();
            self.files.get(&path_str)
                .cloned()
                .ok_or_else(|| format!("File not found: {}", path_str))
        }
    }

    #[test]
    fn test_merge_strategy_override() {
        let merger = AstMerger::new(Box::new(MockFileLoader { files: HashMap::new() }));
        let target = TableBlock {
            name: None,
            entries: vec![
                TableEntry::KeyValue(KeyValue {
                    key: Key::BareKey("a".to_string()),
                    value: Value::Scalar(ScalarValue::Number(NumberValue::Integer(1))),
                    metadata: None,
                    comment: None,
                    span: Span::unknown(),
                }),
            ],
            span: Span::unknown(),
        };
        let source = TableBlock {
            name: None,
            entries: vec![
                TableEntry::KeyValue(KeyValue {
                    key: Key::BareKey("a".to_string()),
                    value: Value::Scalar(ScalarValue::Number(NumberValue::Integer(2))),
                    metadata: None,
                    comment: None,
                    span: Span::unknown(),
                }),
            ],
            span: Span::unknown(),
        };
        let result = merger.apply_merge_strategy(&crate::ast::MergeStrategy::Override, &target, source).unwrap();
        assert_eq!(result.len(), 1);
    }
}

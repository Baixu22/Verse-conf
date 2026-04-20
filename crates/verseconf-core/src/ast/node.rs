use crate::ast::metadata::MetadataList;
use crate::ast::value::{Expression, ScalarValue};
use crate::Span;
use std::fmt;

/// 根 AST
#[derive(Debug, Clone)]
pub struct Ast {
    pub root: TableBlock,
    pub schema: Option<SchemaDefinition>,
    pub source: SourceInfo,
}

/// 源码信息
#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub path: Option<String>,
    pub content: String,
}

/// 键
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    BareKey(String),
    QuotedKey(String),
}

impl Key {
    /// 获取键的字符串表示
    pub fn as_str(&self) -> &str {
        match self {
            Key::BareKey(s) => s,
            Key::QuotedKey(s) => s,
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 键值对
#[derive(Debug, Clone)]
pub struct KeyValue {
    pub key: Key,
    pub value: Value,
    pub metadata: Option<MetadataList>,
    pub comment: Option<Comment>,
    pub span: Span,
}

/// 值
#[derive(Debug, Clone)]
pub enum Value {
    Scalar(ScalarValue),
    InlineTable(InlineTable),
    Array(ArrayValue),
    TableBlock(TableBlock),
    Expression(Expression),
}

/// 内联表
#[derive(Debug, Clone)]
pub struct InlineTable {
    pub entries: Vec<KeyValue>,
    pub span: Span,
}

/// 数组
#[derive(Debug, Clone)]
pub struct ArrayValue {
    pub elements: Vec<Value>,
    pub span: Span,
}

/// 表块
#[derive(Debug, Clone)]
pub struct TableBlock {
    pub name: Option<String>,
    pub entries: Vec<TableEntry>,
    pub span: Span,
}

impl TableBlock {
    /// 查找键值对
    pub fn get(&self, key: &str) -> Option<&KeyValue> {
        self.entries.iter().find_map(|entry| {
            if let TableEntry::KeyValue(kv) = entry {
                if kv.key.as_str() == key {
                    return Some(kv);
                }
            }
            None
        })
    }

    /// 获取所有键值对
    pub fn key_values(&self) -> Vec<&KeyValue> {
        self.entries
            .iter()
            .filter_map(|entry| {
                if let TableEntry::KeyValue(kv) = entry {
                    Some(kv)
                } else {
                    None
                }
            })
            .collect()
    }
}

/// 表条目
#[derive(Debug, Clone)]
pub enum TableEntry {
    KeyValue(KeyValue),
    TableBlock(TableBlock),
    ArrayTable(ArrayTable),
    IncludeDirective(IncludeDirective),
    Comment(Comment),
}

/// 数组表 [[key]]
#[derive(Debug, Clone)]
pub struct ArrayTable {
    pub key: Key,
    pub entries: Vec<KeyValue>,
    pub span: Span,
}

/// Include 指令
#[derive(Debug, Clone)]
pub struct IncludeDirective {
    pub path: String,
    pub merge_strategy: MergeStrategy,
    pub span: Span,
}

/// 合并策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    Override,
    Append,
    Merge,
    DeepMerge,
}

impl fmt::Display for MergeStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MergeStrategy::Override => write!(f, "override"),
            MergeStrategy::Append => write!(f, "append"),
            MergeStrategy::Merge => write!(f, "merge"),
            MergeStrategy::DeepMerge => write!(f, "deep_merge"),
        }
    }
}

/// 注释
#[derive(Debug, Clone)]
pub struct Comment {
    pub content: String,
    pub is_block: bool,
    pub span: Span,
}

impl fmt::Display for Comment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_block {
            write!(f, "/*{}*/", self.content)
        } else {
            write!(f, "# {}", self.content)
        }
    }
}

/// Schema 定义
#[derive(Debug, Clone)]
pub struct SchemaDefinition {
    pub version: Option<String>,
    pub description: Option<String>,
    pub strict: bool,
    pub fields: Vec<SchemaField>,
    pub span: Span,
}

/// Schema 字段
#[derive(Debug, Clone)]
pub struct SchemaField {
    pub name: String,
    pub field_type: SchemaType,
    pub required: bool,
    pub default: Option<Value>,
    pub range: Option<RangeConstraint>,
    pub pattern: Option<String>,
    pub enum_values: Option<Vec<ScalarValue>>,
    pub desc: Option<String>,
    pub example: Option<String>,
    pub llm_hint: Option<String>,
    pub sensitive: bool,
    pub nested_fields: Vec<SchemaField>,
    pub span: Span,
}

/// Schema 类型
#[derive(Debug, Clone, PartialEq)]
pub enum SchemaType {
    String,
    Integer,
    Float,
    Boolean,
    DateTime,
    Duration,
    Table,
    Array(Box<SchemaType>),
}

impl fmt::Display for SchemaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchemaType::String => write!(f, "string"),
            SchemaType::Integer => write!(f, "integer"),
            SchemaType::Float => write!(f, "float"),
            SchemaType::Boolean => write!(f, "bool"),
            SchemaType::DateTime => write!(f, "datetime"),
            SchemaType::Duration => write!(f, "duration"),
            SchemaType::Table => write!(f, "table"),
            SchemaType::Array(inner) => write!(f, "[{}]", inner),
        }
    }
}

/// 范围约束
#[derive(Debug, Clone)]
pub struct RangeConstraint {
    pub min: Option<i64>,
    pub max: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_block_get() {
        let table = TableBlock {
            name: None,
            entries: vec![TableEntry::KeyValue(KeyValue {
                key: Key::BareKey("name".to_string()),
                value: Value::Scalar(ScalarValue::String("test".to_string())),
                metadata: None,
                comment: None,
                span: Span::unknown(),
            })],
            span: Span::unknown(),
        };
        assert!(table.get("name").is_some());
        assert!(table.get("missing").is_none());
    }

    #[test]
    fn test_key_display() {
        let key = Key::BareKey("database".to_string());
        assert_eq!(key.to_string(), "database");
    }
}

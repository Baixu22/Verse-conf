use crate::Span;
use std::fmt;

/// 标准元数据标签
#[derive(Debug, Clone, PartialEq)]
pub enum StandardMetadata {
    /// 敏感值，输出/日志时自动脱敏
    Sensitive,
    /// 必须显式提供（即使有默认值）
    Required,
    /// 已废弃，可带替代信息
    Deprecated { message: Option<String> },
    /// 文档说明
    Description { text: String },
    /// 示例值
    Example { value: String },
    /// 类型提示（辅助编辑器/校验）
    TypeHint { hint: String },
    /// 数组元素类型
    ItemType { item_type: String },
    /// 值域校验（解析时检查）
    Range { min: f64, max: f64 },
}

impl fmt::Display for StandardMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StandardMetadata::Sensitive => write!(f, "sensitive"),
            StandardMetadata::Required => write!(f, "required"),
            StandardMetadata::Deprecated { message } => {
                if let Some(msg) = message {
                    write!(f, "deprecated=\"{}\"", msg)
                } else {
                    write!(f, "deprecated")
                }
            }
            StandardMetadata::Description { text } => {
                write!(f, "description=\"{}\"", text)
            }
            StandardMetadata::Example { value } => {
                write!(f, "example=\"{}\"", value)
            }
            StandardMetadata::TypeHint { hint } => {
                write!(f, "type_hint=\"{}\"", hint)
            }
            StandardMetadata::ItemType { item_type } => {
                write!(f, "item_type={}", item_type)
            }
            StandardMetadata::Range { min, max } => {
                write!(f, "range({}..{})", min, max)
            }
        }
    }
}

/// 元数据值
#[derive(Debug, Clone, PartialEq)]
pub enum MetadataValue {
    String(String),
    Number(f64),
    Range { min: f64, max: f64 },
}

/// 元数据项
#[derive(Debug, Clone)]
pub enum MetadataItem {
    Standard(StandardMetadata),
    Custom {
        key: String,
        value: Option<MetadataValue>,
    },
}

/// 元数据列表
#[derive(Debug, Clone)]
pub struct MetadataList {
    pub items: Vec<MetadataItem>,
    pub span: Span,
}

impl MetadataList {
    /// 检查是否包含某个标准元数据
    pub fn has_sensitive(&self) -> bool {
        self.items.iter().any(|item| {
            matches!(item, MetadataItem::Standard(StandardMetadata::Sensitive))
        })
    }

    /// 检查是否包含 required
    pub fn has_required(&self) -> bool {
        self.items.iter().any(|item| {
            matches!(item, MetadataItem::Standard(StandardMetadata::Required))
        })
    }

    /// 获取 range 元数据（如果存在）
    pub fn get_range(&self) -> Option<(f64, f64)> {
        self.items.iter().find_map(|item| {
            if let MetadataItem::Standard(StandardMetadata::Range { min, max }) = item {
                Some((*min, *max))
            } else {
                None
            }
        })
    }

    /// 获取 type_hint（如果存在）
    pub fn get_type_hint(&self) -> Option<&str> {
        self.items.iter().find_map(|item| {
            if let MetadataItem::Standard(StandardMetadata::TypeHint { hint }) = item {
                Some(hint.as_str())
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_list_has_sensitive() {
        let list = MetadataList {
            items: vec![MetadataItem::Standard(StandardMetadata::Sensitive)],
            span: Span::unknown(),
        };
        assert!(list.has_sensitive());
    }

    #[test]
    fn test_metadata_list_get_range() {
        let list = MetadataList {
            items: vec![MetadataItem::Standard(StandardMetadata::Range {
                min: 1024.0,
                max: 65535.0,
            })],
            span: Span::unknown(),
        };
        assert_eq!(list.get_range(), Some((1024.0, 65535.0)));
    }
}

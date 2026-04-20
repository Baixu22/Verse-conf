use std::fmt;

/// 源码位置信息
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    /// 起始字节偏移
    pub start: usize,
    /// 结束字节偏移
    pub end: usize,
    /// 行号 (1-based)
    pub line: u32,
    /// 列号 (1-based)
    pub column: u32,
}

impl Span {
    /// 创建一个未知位置的 Span
    pub fn unknown() -> Self {
        Self {
            start: 0,
            end: 0,
            line: 0,
            column: 0,
        }
    }

    /// 从字节偏移创建 Span（行号和列号需要后续填充）
    pub fn from_bytes(start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            line: 0,
            column: 0,
        }
    }

    /// 合并两个 Span
    pub fn merge(&self, other: &Span) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
            line: self.line,
            column: self.column,
        }
    }

    /// 判断是否为未知位置
    pub fn is_unknown(&self) -> bool {
        self.line == 0 && self.column == 0
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_unknown() {
            write!(f, "unknown location")
        } else {
            write!(f, "{}:{}", self.line, self.column)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_unknown() {
        let span = Span::unknown();
        assert!(span.is_unknown());
    }

    #[test]
    fn test_span_merge() {
        let s1 = Span::from_bytes(0, 10);
        let s2 = Span::from_bytes(5, 20);
        let merged = s1.merge(&s2);
        assert_eq!(merged.start, 0);
        assert_eq!(merged.end, 20);
    }
}

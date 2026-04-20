use std::fmt;

/// Token 类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// 字符串字面量 "..."
    StringLiteral(String),
    /// 数字字面量（保留原始字符串）
    NumberLiteral(String),
    /// 布尔字面量
    BooleanLiteral(bool),
    /// 日期时间字面量
    DateTimeLiteral(String),
    /// 持续时间字面量
    DurationLiteral(String),

    /// 裸键 [a-zA-Z0-9_-]+
    BareKey(String),
    /// 引号键 "..."
    QuotedKey(String),

    /// 赋值符 = 或 :
    Assign,
    /// 运算符 + - * /
    Operator(Operator),

    /// 逗号
    Comma,
    /// 点
    Dot,
    /// 范围运算符 ..
    RangeOp,

    /// 左大括号 {
    LBrace,
    /// 右大括号 }
    RBrace,
    /// 左中括号 [
    LBracket,
    /// 右中括号 ]
    RBracket,
    /// 双左中括号 [[
    LDoubleBracket,
    /// 双右中括号 ]]
    RDoubleBracket,
    /// 左括号 (
    LParen,
    /// 右括号 )
    RParen,

    /// @include 指令
    Include,
    /// merge 关键字
    Merge,

    /// 元数据前缀 #@
    MetadataPrefix,

    /// 行注释
    LineComment(String),
    /// 块注释
    BlockComment(String),

    /// 文件结束
    Eof,
    /// 换行
    Newline,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::StringLiteral(s) => write!(f, "string({})", s),
            Token::NumberLiteral(n) => write!(f, "number({})", n),
            Token::BooleanLiteral(b) => write!(f, "bool({})", b),
            Token::DateTimeLiteral(dt) => write!(f, "datetime({})", dt),
            Token::DurationLiteral(d) => write!(f, "duration({})", d),
            Token::BareKey(k) => write!(f, "bare_key({})", k),
            Token::QuotedKey(k) => write!(f, "quoted_key({})", k),
            Token::Assign => write!(f, "="),
            Token::Operator(op) => write!(f, "{}", op),
            Token::Comma => write!(f, ","),
            Token::Dot => write!(f, "."),
            Token::RangeOp => write!(f, ".."),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::LBracket => write!(f, "["),
            Token::RBracket => write!(f, "]"),
            Token::LDoubleBracket => write!(f, "[["),
            Token::RDoubleBracket => write!(f, "]]"),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::Include => write!(f, "@include"),
            Token::Merge => write!(f, "merge"),
            Token::MetadataPrefix => write!(f, "#@"),
            Token::LineComment(c) => write!(f, "# {}", c),
            Token::BlockComment(c) => write!(f, "/*{}*/", c),
            Token::Eof => write!(f, "EOF"),
            Token::Newline => write!(f, "newline"),
        }
    }
}

/// 运算符
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operator::Add => write!(f, "+"),
            Operator::Subtract => write!(f, "-"),
            Operator::Multiply => write!(f, "*"),
            Operator::Divide => write!(f, "/"),
        }
    }
}

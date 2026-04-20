# VerseConf v1.1 - Rust 实现技术规划文档

## 一、项目概览

### 1.1 项目名称
**Verseconf** - 基于 VerseConf v1.1 规范的现代配置语言 Rust 实现

### 1.2 核心目标
- 实现完整的 Lexer + Parser + AST 构建
- 支持 v1.1 规范定义的所有语法特性
- 提供高性能、零拷贝的解析能力
- 友好的错误提示和位置信息

### 1.3 技术栈
- **语言**: Rust 2021 Edition
- **解析器生成**: `pest` (PEG 解析器生成器)
- **序列化**: `serde` (AST 序列化/反序列化)
- **错误处理**: `thiserror` + 自定义错误类型
- **测试框架**: `rstest` (参数化测试)
- **格式化**: `prettyplease` 风格自定义 Pretty Printer

---

## 二、项目结构设计

```
verseconf/
├── Cargo.toml                          # 工作区配置
├── README.md                           # 项目说明
├── LICENSE                             # 许可证
│
├── crates/
│   ├── verseconf-core/                 # 核心库
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # 公共 API 导出
│   │       ├── ast/                    # AST 定义
│   │       │   ├── mod.rs
│   │       │   ├── node.rs             # AST 节点类型
│   │       │   ├── value.rs            # 值类型定义
│   │       │   ├── metadata.rs         # 元数据结构
│   │       │   └── span.rs             # 源码位置信息
│   │       ├── lexer/                  # 词法分析器
│   │       │   ├── mod.rs
│   │       │   ├── token.rs            # Token 定义
│   │       │   ├── context.rs          # 上下文状态机 (in_key/in_value)
│   │       │   └── error.rs            # 词法错误
│   │       ├── parser/                 # 语法分析器
│   │       │   ├── mod.rs
│   │       │   ├── grammar.pest        # PEG 语法定义
│   │       │   ├── builder.rs          # AST 构建器
│   │       │   └── error.rs            # 语法错误
│   │       ├── semantic/               # 语义分析 (基础)
│   │       │   ├── mod.rs
│   │       │   ├── validator.rs        # 元数据校验
│   │       │   └── env_interp.rs       # 环境变量插值
│   │       ├── error.rs                # 统一错误类型
│   │       └── source.rs               # 源码位置/文件管理
│   │
│   ├── verseconf-cli/                  # CLI 工具
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── commands/
│   │       │   ├── mod.rs
│   │       │   ├── parse.rs            # parse 命令
│   │       │   ├── validate.rs         # validate 命令
│   │       │   └── format.rs           # format 命令
│   │       └── output.rs               # 输出格式化
│   │
│   └── verseconf-test/                 # 测试套件
│       ├── Cargo.toml
│       └── tests/
│           ├── lexer_tests.rs
│           ├── parser_tests.rs
│           ├── semantic_tests.rs
│           └── fixtures/               # 测试用例
│               ├── valid/
│               └── invalid/
│
├── examples/                           # 示例配置
│   ├── basic.ucf
│   ├── with_metadata.ucf
│   ├── with_include.ucf
│   └── production.ucf
│
└── docs/
    ├── spec.md                         # 规范文档 (已有的 注释.md)
    └── implementation_guide.md         # 实现指南
```

---

## 三、核心数据结构设计

### 3.1 AST 节点定义

```rust
// ast/node.rs

/// 源码位置信息
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,  // 起始字节偏移
    pub end: usize,    // 结束字节偏移
    pub line: u32,     // 行号 (1-based)
    pub column: u32,   // 列号 (1-based)
}

/// 根 AST
#[derive(Debug, Clone)]
pub struct Ast {
    pub root: TableBlock,
    pub source: SourceInfo,
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

/// 键
#[derive(Debug, Clone)]
pub enum Key {
    BareKey(String),      // [a-zA-Z0-9_-]+
    QuotedKey(String),    // "..." 或 '...'
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

/// 标量值
#[derive(Debug, Clone)]
pub enum ScalarValue {
    String(String),
    Number(NumberValue),
    Boolean(bool),
    DateTime(String),     // 简化处理，后续可解析为 chrono 类型
    Duration(Duration),   // 解析为 std::time::Duration
}

/// 数字值 (保留原始精度信息)
#[derive(Debug, Clone)]
pub enum NumberValue {
    Integer(i64),
    Float(f64),
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
    pub entries: Vec<TableEntry>,
    pub span: Span,
}

/// 表条目 (可以是键值对、数组表、include 指令或注释)
#[derive(Debug, Clone)]
pub enum TableEntry {
    KeyValue(KeyValue),
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
    Override,     // 完全替换
    Append,       // 数组追加
    Merge,        // 递归合并 (数组替换)
    DeepMerge,    // 深度递归合并 (数组追加)
}

/// 元数据列表
#[derive(Debug, Clone)]
pub struct MetadataList {
    pub items: Vec<MetadataItem>,
    pub span: Span,
}

/// 元数据项
#[derive(Debug, Clone)]
pub enum MetadataItem {
    Standard(StandardMetadata),
    Custom { key: String, value: Option<MetadataValue> },
}

/// 标准元数据
#[derive(Debug, Clone)]
pub enum StandardMetadata {
    Sensitive,                          // #@ sensitive
    Required,                           // #@ required
    Deprecated { message: Option<String> },  // #@ deprecated="..."
    Description { text: String },       // #@ description="..."
    Example { value: String },          // #@ example="..."
    TypeHint { hint: String },          // #@ type_hint="..."
    ItemType { item_type: String },     // #@ item_type=...
    Range { min: f64, max: f64 },      // #@ range(min..max)
}

/// 元数据值
#[derive(Debug, Clone)]
pub enum MetadataValue {
    String(String),
    Number(f64),
    Range { min: f64, max: f64 },
}

/// 注释
#[derive(Debug, Clone)]
pub struct Comment {
    pub content: String,
    pub is_block: bool,
    pub span: Span,
}
```

### 3.2 表达式系统

```rust
// ast/value.rs (续)

/// 表达式 (解析时求值)
#[derive(Debug, Clone)]
pub enum Expression {
    BinaryOp {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
    },
    Literal(ScalarValue),
    VersetValue {
        value: f64,
        Verset: TimeVerset,
    },
}

/// 二元运算符
#[derive(Debug, Clone, Copy)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

/// 时间单位
#[derive(Debug, Clone, Copy)]
pub enum TimeVerset {
    Seconds,
    Minutes,
    Hours,
    Days,
}
```

---

## 四、Lexer 设计

### 4.1 Token 定义

```rust
// lexer/token.rs

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    // 字面量
    StringLiteral(String),
    NumberLiteral(String),    // 保留原始字符串，解析时决定类型
    BooleanLiteral(bool),
    DateTimeLiteral(String),
    DurationLiteral(String),
    
    // 键
    BareKey(String),
    QuotedKey(String),
    
    // 运算符
    Assign,                   // = 或 : (归一化)
    Operator(Operator),       // + - * / (仅在值上下文)
    
    // 分隔符
    Comma,
    Colon,                    // 用于 range 的 ..
    Dot,
    
    // 括号
    LBrace,                   // {
    RBrace,                   // }
    LBracket,                 // [
    RBracket,                 // ]
    LDoubleBracket,           // [[
    RDoubleBracket,           // ]]
    
    // 指令
    Include,                  // @include
    Merge,                    // merge
    
    // 元数据
    MetadataPrefix,           // #@
    RangeOp,                  // ..
    
    // 注释
    LineComment(String),
    BlockComment(String),
    
    // 特殊
    Eof,
    Newline,
    Whitespace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
}
```

### 4.2 上下文敏感状态机

```rust
// lexer/context.rs

/// Lexer 上下文状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexerContext {
    /// 键上下文: `-` 属于 BARE_KEY
    InKey,
    /// 值上下文: `-` 属于 OPERATOR
    InValue,
    /// 表块内部 (不允许逗号)
    InTableBlock,
    /// 内联表内部 (必须逗号)
    InInlineTable,
}

/// 上下文状态机
pub struct ContextStateMachine {
    current: LexerContext,
    stack: Vec<LexerContext>,
}

impl ContextStateMachine {
    pub fn new() -> Self {
        Self {
            current: LexerContext::InKey,
            stack: Vec::new(),
        }
    }
    
    pub fn push(&mut self, ctx: LexerContext) {
        self.stack.push(self.current);
        self.current = ctx;
    }
    
    pub fn pop(&mut self) {
        if let Some(prev) = self.stack.pop() {
            self.current = prev;
        }
    }
    
    pub fn current(&self) -> LexerContext {
        self.current
    }
    
    pub fn is_in_key(&self) -> bool {
        self.current == LexerContext::InKey
    }
    
    pub fn is_in_value(&self) -> bool {
        self.current == LexerContext::InValue
    }
}
```

### 4.3 Lexer 核心接口

```rust
// lexer/mod.rs

pub struct Lexer<'a> {
    source: &'a str,
    position: usize,
    line: u32,
    column: u32,
    context: ContextStateMachine,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self { ... }
    
    /// 返回下一个 Token
    pub fn next_token(&mut self) -> Result<Token, LexError> { ... }
    
    /// 返回所有 Tokens (用于调试)
    pub fn tokenize_all(&mut self) -> Result<Vec<(Token, Span)>, LexError> { ... }
}
```

---

## 五、Parser 设计

### 5.1 PEG 语法定义 (grammar.pest)

```pest
// parser/grammar.pest

WHITESPACE = _{ " " | "\t" }
COMMENT = _{ line_comment | block_comment }

line_comment = { "#" ~ (!"\n" ~ ANY)* }
block_comment = { "/*" ~ (!"*/" ~ ANY)* ~ "*/" }

// 文件结构
file = { SOI ~ root_table ~ EOI }

root_table = { (key_value | array_table | include_directive | COMMENT)* }

// 键值对
key_value = { key ~ ASSIGN ~ value ~ metadata? ~ COMMENT? }

key = { bare_key | quoted_key }
bare_key = @{ ASCII_ALPHANUMERIC | ("-" | "_")+ }
quoted_key = ${ "\"" ~ (!"\"" ~ ANY)* ~ "\"" }

// 赋值符 (归一化 = 或 :)
ASSIGN = _{ "=" | ":" }

// 值
value = _{ scalar | inline_table | array | table_block | expression }

// 标量
scalar = _{ string | number | boolean | datetime | duration }

string = ${ "\"" ~ (!"\"" ~ ANY)* ~ "\"" }
number = @{ ASCII_DIGIT+ ~ ("." ~ ASCII_DIGIT+)? }
boolean = { "true" | "false" }
datetime = @{ ... }  // ISO 8601 格式
duration = @{ ASCII_DIGIT+ ~ ("s" | "m" | "h" | "d") }

// 内联表 (强制逗号)
inline_table = { "{" ~ (key_value ~ ("," ~ key_value)* ~ ","?)? ~ "}" }

// 表块 (不允许逗号)
table_block = { "{" ~ (key_value | array_table | include_directive | COMMENT)* ~ "}" }

// 数组 (允许尾逗号)
array = { "[" ~ (value ~ ("," ~ value)* ~ ","?)? ~ "]" }

// 数组表
array_table = { "[[" ~ key ~ "]]" ~ (key_value | COMMENT)* }

// Include 指令
include_directive = { 
    "@include" ~ string ~ 
    ("merge" ~ ASSIGN ~ merge_strategy)? 
}

merge_strategy = { "override" | "append" | "merge" | "deep_merge" }

// 表达式
expression = { simple_expression }
simple_expression = { term ~ (operator ~ term)* }
term = { scalar | Verset_value | "(" ~ simple_expression ~ ")" }
operator = { "+" | "-" | "*" | "/" }
Verset_value = @{ ASCII_DIGIT+ ~ ("s" | "m" | "h" | "d") }

// 元数据
metadata = { "#@" ~ metadata_list? }
metadata_list = { metadata_item ~ ("," ~ metadata_item)* }
metadata_item = _{ 
    standard_meta_key |
    standard_meta_key ~ "=" ~ (string | number | range_expr) |
    bare_key ~ "=" ~ (string | number)
}

standard_meta_key = { 
    "sensitive" | "required" | "deprecated" | 
    "description" | "example" | "type_hint" | 
    "item_type" | "range"
}

range_expr = { number ~ ".." ~ number }
```

### 5.2 AST 构建器

```rust
// parser/builder.rs

pub struct AstBuilder {
    source: String,
    errors: Vec<ParseError>,
}

impl AstBuilder {
    pub fn new(source: String) -> Self { ... }
    
    /// 从 pest 解析对构建 AST
    pub fn build(&mut self, pairs: Pairs<Rule>) -> Result<Ast, ParseError> { ... }
    
    /// 构建键值对
    fn build_key_value(&mut self, pair: Pair<Rule>) -> KeyValue { ... }
    
    /// 构建值
    fn build_value(&mut self, pair: Pair<Rule>) -> Value { ... }
    
    /// 构建元数据
    fn build_metadata(&mut self, pair: Pair<Rule>) -> MetadataList { ... }
    
    /// 构建表达式 (求值)
    fn build_expression(&mut self, pair: Pair<Rule>) -> Expression { ... }
}
```

### 5.3 Parser 核心接口

```rust
// parser/mod.rs

pub struct Parser {
    source: String,
}

impl Parser {
    pub fn new(source: String) -> Self { ... }
    
    /// 解析源码返回 AST
    pub fn parse(&self) -> Result<Ast, ParseError> {
        let pairs = VerseConfParser::parse(Rule::file, &self.source)?;
        let mut builder = AstBuilder::new(self.source.clone());
        builder.build(pairs)
    }
}
```

---

## 六、语义分析设计

### 6.1 环境变量插值

```rust
// semantic/env_interp.rs

pub struct EnvInterpolator {
    env_vars: HashMap<String, String>,
}

impl EnvInterpolator {
    pub fn new() -> Self { ... }
    
    /// 插值字符串: "${DB_HOST|localhost}" -> "localhost"
    pub fn interpolate(&self, s: &str) -> Result<String, InterpError> { ... }
    
    /// 从系统环境加载
    pub fn load_from_env(&mut self) { ... }
}
```

### 6.2 元数据校验器

```rust
// semantic/validator.rs

pub struct Validator {
    errors: Vec<ValidationError>,
    warnings: Vec<ValidationWarning>,
}

impl Validator {
    pub fn new() -> Self { ... }
    
    /// 校验 AST
    pub fn validate(&mut self, ast: &Ast) -> ValidationResult { ... }
    
    /// 校验 range
    fn validate_range(&mut self, value: &Value, metadata: &MetadataList) { ... }
    
    /// 校验 type_hint
    fn validate_type_hint(&mut self, value: &Value, metadata: &MetadataList) { ... }
    
    /// 校验 required
    fn validate_required(&mut self, ast: &Ast) { ... }
    
    /// 校验敏感值
    fn validate_sensitive(&mut self, ast: &Ast) { ... }
}
```

---

## 七、错误处理设计

### 7.1 统一错误类型

```rust
// error.rs

#[derive(Debug, thiserror::Error)]
pub enum VerseconfError {
    #[error("Lexical error at {span}: {message}")]
    LexError {
        message: String,
        span: Span,
    },
    
    #[error("Parse error at {span}: {message}")]
    ParseError {
        message: String,
        span: Span,
    },
    
    #[error("Semantic error at {span}: {message}")]
    SemanticError {
        message: String,
        span: Span,
    },
    
    #[error("Validation error: {message}")]
    ValidationError {
        message: String,
        span: Option<Span>,
    },
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// 错误报告 (带源码高亮)
pub struct ErrorReport {
    pub error: VerseconfError,
    pub source: String,
    pub context_lines: usize,
}

impl ErrorReport {
    pub fn format(&self) -> String {
        // 类似 rustc 的错误输出格式
        // error[E0001]: unexpected token
        //   --> config.ucf:10:5
        //    |
        // 10 |     port = "invalid"
        //    |            ^^^^^^^^^ expected number, found string
    }
}
```

---

## 八、公共 API 设计

### 8.1 核心接口

```rust
// lib.rs

/// 解析配置文件
pub fn parse(source: &str) -> Result<Ast, VerseconfError> {
    let parser = Parser::new(source.to_string());
    parser.parse()
}

/// 解析并校验配置文件
pub fn parse_and_validate(source: &str) -> Result<Ast, VerseconfError> {
    let ast = parse(source)?;
    let mut validator = Validator::new();
    validator.validate(&ast)?;
    Ok(ast)
}

/// 解析文件
pub fn parse_file(path: &Path) -> Result<Ast, VerseconfError> {
    let source = std::fs::read_to_string(path)?;
    parse(&source)
}

/// 格式化配置文件
pub fn format(source: &str) -> Result<String, VerseconfError> {
    let ast = parse(source)?;
    Ok(pretty_print(&ast))
}
```

### 8.2 构建器模式 (高级用法)

```rust
/// 解析器构建器
pub struct ParseBuilder {
    source: String,
    env_interp: bool,
    validate: bool,
    merge_strategy: MergeStrategy,
}

impl ParseBuilder {
    pub fn new(source: String) -> Self { ... }
    
    pub fn with_env_interpolation(mut self) -> Self { ... }
    pub fn with_validation(mut self) -> Self { ... }
    pub fn with_merge_strategy(mut self, strategy: MergeStrategy) -> Self { ... }
    
    pub fn build(self) -> Result<Ast, VerseconfError> { ... }
}
```

---

## 九、测试策略

### 9.1 测试分层

```
测试金字塔:
         /\
        /  \        E2E 测试 (完整解析流程)
       /____\
      /      \      集成测试 (Lexer + Parser)
     /________\
    /          \    单元测试 (单个 Token/Rule)
   /____________\
```

### 9.2 测试用例设计

```rust
// tests/parser_tests.rs

#[rstest]
#[case("basic.ucf", "valid/basic")]
#[case("with_metadata.ucf", "valid/metadata")]
#[case("with_expression.ucf", "valid/expression")]
#[case("with_include.ucf", "valid/include")]
fn test_valid_files(#[case] name: &str, #[case] fixture: &str) {
    let source = load_fixture(fixture);
    let result = parse(&source);
    assert!(result.is_ok(), "Failed to parse {}: {:?}", name, result);
}

#[rstest]
#[case("missing_value.ucf", "expected value")]
#[case("invalid_range.ucf", "invalid range")]
#[case("duplicate_key.ucf", "duplicate key")]
fn test_invalid_files(#[case] name: &str, #[case] expected_error: &str) {
    let source = load_fixture(&format!("invalid/{}", name));
    let result = parse(&source);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains(expected_error));
}
```

### 9.3 关键测试场景

| 类别 | 测试用例 | 预期行为 |
|------|---------|---------|
| Lexer | `bare_key` 含 `-` | 在键上下文识别为 BARE_KEY |
| Lexer | `-` 在值上下文 | 识别为 OPERATOR::Subtract |
| Lexer | `=` 和 `:` | 均归一化为 ASSIGN |
| Parser | inline_table 无逗号 | 报错 |
| Parser | table_block 有逗号 | 报错 |
| Parser | trailing comma | 允许 |
| Semantic | range 校验失败 | 警告/错误 |
| Semantic | required 未提供 | 错误 |
| Semantic | sensitive 输出 | 脱敏 |

---

## 十、开发里程碑

### Phase 1: 基础架构 (Week 1-2)
- [ ] 项目脚手架搭建 (Cargo workspace)
- [ ] AST 数据结构定义
- [ ] Span 和错误类型实现
- [ ] 基础测试框架

### Phase 2: Lexer 实现 (Week 2-3)
- [ ] Token 定义
- [ ] 上下文状态机
- [ ] 词法分析器核心逻辑
- [ ] Lexer 单元测试 (100% 覆盖)

### Phase 3: Parser 实现 (Week 3-4)
- [ ] PEG 语法定义 (grammar.pest)
- [ ] AST 构建器
- [ ] 表达式求值器
- [ ] Parser 单元测试

### Phase 4: 语义分析 (Week 4-5)
- [ ] 环境变量插值
- [ ] 元数据校验器
- [ ] 范围/类型检查
- [ ] 语义测试

### Phase 5: CLI 工具 (Week 5-6)
- [ ] CLI 框架 (clap)
- [ ] parse 命令
- [ ] validate 命令
- [ ] format 命令 (Pretty Printer)

### Phase 6: 优化与文档 (Week 6-7)
- [ ] 性能优化 (benchmark)
- [ ] 错误提示优化
- [ ] API 文档
- [ ] 示例配置

---

## 十一、性能目标

| 指标 | 目标 |
|------|------|
| 解析速度 | 100KB 配置文件 < 5ms |
| 内存占用 | AST 大小 < 源码大小的 3x |
| 零拷贝 | Lexer 使用 `&str` 引用，避免分配 |
| 增量解析 | 未来支持 (binary-cache) |

---

## 十二、依赖清单

```toml
# verseconf-core/Cargo.toml

[dependencies]
pest = "2.7"                    # PEG 解析器
pest_derive = "2.7"             # 语法宏
serde = { version = "1.0", features = ["derive"] }
thiserror = "1.0"               # 错误处理
chrono = "0.4"                  # 日期时间处理
regex = "1.10"                  # 环境变量插值

[dev-dependencies]
rstest = "0.18"                 # 参数化测试
criterion = "0.5"               # 性能基准测试
insta = "1.34"                  # 快照测试
```

---

## 十三、风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| PEG 左递归 | pest 不支持左递归 | 改写语法为右递归 |
| 上下文敏感词法 | 增加复杂度 | 状态机独立测试 |
| 表达式优先级 | 解析错误 | 严格遵循运算符优先级 |
| 错误位置精度 | 用户体验差 | Span 精确到字节偏移 |

---

## 十四、下一步行动

1. **确认此规划文档** - 审查架构设计是否符合预期
2. **生成项目脚手架** - 使用 `cargo generate` 或手动创建
3. **实现 AST 定义** - 从数据结构开始
4. **编写 Lexer** - 实现上下文敏感词法分析
5. **编写 Parser** - 定义 PEG 语法并构建 AST
6. **编写测试** - 确保每个模块都有完整测试覆盖

---

**文档版本**: v1.0  
**创建日期**: 2026-04-20  
**基于规范**: VerseConf v1.1 (注释.md)

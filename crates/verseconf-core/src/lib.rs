pub mod ast;
pub mod edit;
pub mod engine;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod semantic;
pub mod source;

pub use ast::*;
pub use edit::apply::{
    apply_edit_plan, replace_range, value_span_for_path, AppliedEdit, EditOutcome, EditRefusal,
};
pub use edit::value::EditValue;
pub use edit::{
    describe_path, edit_plan_json_schema, EditExpectation, EditIntent, EditOp, EditPlan,
    PathSegment, PlanViolation, EDIT_PLAN_JSON_SCHEMA, EDIT_PLAN_VERSION,
};
pub use engine::*;
pub use error::*;
pub use lexer::*;
pub use parser::*;
pub use semantic::*;

use std::path::{Path, PathBuf};

/// 解析配置文件
pub fn parse(source: &str) -> Result<Ast, VerseconfError> {
    let parser = Parser::new(source.to_string());
    parser.parse().map_err(VerseconfError::from)
}

/// 使用配置解析配置文件
pub fn parse_with_config(
    source: &str,
    config: ParseConfig,
) -> Result<ParseResult<Ast>, VerseconfError> {
    let parser = Parser::with_config(source.to_string(), config);
    parser.parse_with_warnings().map_err(VerseconfError::from)
}

/// 解析并校验配置文件（包含 Schema 校验）
pub fn parse_and_validate(source: &str) -> Result<Ast, VerseconfError> {
    let ast = parse(source)?;

    // 先进行基础校验
    let mut validator = Validator::new();
    validator.validate(&ast)?;

    // 如果有 Schema，进行 Schema 校验
    if ast.schema.is_some() {
        let mut schema_validator = SchemaValidator::new();
        schema_validator.validate_with_schema(&ast)?;
    }

    Ok(ast)
}

/// 使用配置解析并校验（tolerant 模式 + warnings）
pub fn parse_and_validate_with_config(
    source: &str,
    config: ParseConfig,
) -> Result<ParseResult<Ast>, VerseconfError> {
    let result = parse_with_config(source, config)?;

    // 基础校验
    let mut validator = Validator::new();
    validator.validate(&result.value)?;

    // Schema 校验
    if result.value.schema.is_some() {
        let mut schema_validator = SchemaValidator::new();
        schema_validator.validate_with_schema(&result.value)?;
    }

    Ok(result)
}

/// 加载选项：决定解析文件时是否解析 @include、是否做环境变量插值
#[derive(Debug, Clone)]
pub struct LoadOptions {
    /// 解析并合并 @include 引用的文件
    pub resolve_includes: bool,
    /// 对字符串值执行 ${VAR|default} 环境变量插值（默认关闭以保持无损）
    pub interpolate_env: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            resolve_includes: true,
            interpolate_env: false,
        }
    }
}

impl LoadOptions {
    /// 不解析 @include
    pub fn without_includes() -> Self {
        Self {
            resolve_includes: false,
            interpolate_env: false,
        }
    }

    /// 打开环境变量插值
    pub fn with_env(mut self) -> Self {
        self.interpolate_env = true;
        self
    }
}

/// 解析源码。提供文件路径时按 options 解析 @include 与 ${VAR} 插值。
pub fn parse_with_context(
    source: &str,
    path: Option<&Path>,
    options: &LoadOptions,
) -> Result<Ast, VerseconfError> {
    let mut ast = parse(source)?;

    if options.resolve_includes {
        if let Some(file_path) = path {
            let base: PathBuf = file_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
            let loader = DefaultFileLoader {
                base_path: base.clone(),
            };
            let mut merger = AstMerger::new(Box::new(loader));
            merger
                .merge_includes(&mut ast, &base)
                .map_err(VerseconfError::Include)?;
        }
    }

    if options.interpolate_env {
        let mut interpolator = EnvInterpolator::new();
        interpolator.load_from_env();
        interpolate_ast_env(&mut ast, &interpolator)?;
    }

    Ok(ast)
}

/// 解析文件（默认解析 @include，与命令行默认行为一致）
pub fn parse_file(path: &Path) -> Result<Ast, VerseconfError> {
    parse_file_with_options(path, &LoadOptions::default())
}

/// 按选项解析文件
pub fn parse_file_with_options(path: &Path, options: &LoadOptions) -> Result<Ast, VerseconfError> {
    let source = std::fs::read_to_string(path)?;
    parse_with_context(&source, Some(path), options)
}

/// 校验已解析的 AST（基础校验 + schema 校验）
pub fn validate_ast(ast: &Ast) -> Result<(), VerseconfError> {
    let mut validator = Validator::new();
    validator.validate(ast)?;

    if ast.schema.is_some() {
        let mut schema_validator = SchemaValidator::new();
        schema_validator.validate_with_schema(ast)?;
    }

    Ok(())
}

/// 对 AST 中所有字符串值执行环境变量插值，返回被替换的字符串个数
pub fn interpolate_ast_env(
    ast: &mut Ast,
    interpolator: &EnvInterpolator,
) -> Result<usize, VerseconfError> {
    let mut count = 0;
    interpolate_table_env(&mut ast.root, interpolator, &mut count)?;
    Ok(count)
}

fn interpolate_table_env(
    table: &mut TableBlock,
    interpolator: &EnvInterpolator,
    count: &mut usize,
) -> Result<(), VerseconfError> {
    for entry in table.entries.iter_mut() {
        match entry {
            TableEntry::KeyValue(kv) => interpolate_value_env(&mut kv.value, interpolator, count)?,
            TableEntry::TableBlock(tb) => interpolate_table_env(tb, interpolator, count)?,
            TableEntry::ArrayTable(at) => {
                for kv in at.entries.iter_mut() {
                    interpolate_value_env(&mut kv.value, interpolator, count)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn interpolate_value_env(
    value: &mut Value,
    interpolator: &EnvInterpolator,
    count: &mut usize,
) -> Result<(), VerseconfError> {
    match value {
        Value::Scalar(ScalarValue::String(text)) => {
            interpolate_string(text, interpolator, count)?;
        }
        // 解析器把标量统一放进 Expression::Literal，这里必须一并处理，
        // 否则真实路径上的 ${VAR} 永远不会被替换。
        Value::Expression(Expression::Literal(ScalarValue::String(text))) => {
            interpolate_string(text, interpolator, count)?;
        }
        Value::InlineTable(table) => {
            for kv in table.entries.iter_mut() {
                interpolate_value_env(&mut kv.value, interpolator, count)?;
            }
        }
        Value::Array(array) => {
            for element in array.elements.iter_mut() {
                interpolate_value_env(element, interpolator, count)?;
            }
        }
        Value::TableBlock(table) => interpolate_table_env(table, interpolator, count)?,
        _ => {}
    }
    Ok(())
}

fn interpolate_string(
    text: &mut String,
    interpolator: &EnvInterpolator,
    count: &mut usize,
) -> Result<(), VerseconfError> {
    if !text.contains("${") {
        return Ok(());
    }

    let replaced = interpolator
        .interpolate(text)
        .map_err(|e| VerseconfError::Semantic {
            message: format!("environment interpolation failed: {}", e),
            span: Span::unknown(),
        })?;

    if &replaced != text {
        *count += 1;
        *text = replaced;
    }

    Ok(())
}

/// 格式化配置文件
pub fn format(source: &str) -> Result<String, VerseconfError> {
    let ast = parse(source)?;
    Ok(PrettyPrinter::print(&ast))
}

/// 使用自定义配置格式化配置文件
pub fn format_with_config(
    source: &str,
    config: PrettyPrintConfig,
) -> Result<String, VerseconfError> {
    let ast = parse(source)?;
    Ok(PrettyPrinter::print_with_config(&ast, config))
}

/// 简单的 Pretty Printer（基础实现，保留向后兼容）
pub fn pretty_print(ast: &Ast) -> String {
    PrettyPrinter::print(ast)
}

/// 渲染模板
pub fn render_template(
    template: &Template,
    context: &RenderContext,
) -> Result<String, VerseconfError> {
    engine::template_renderer::render_template(template, context)
        .map_err(|e| VerseconfError::Template(e.to_string()))
}

/// 从模板文件渲染配置
pub fn render_template_file(
    path: &Path,
    context: &RenderContext,
) -> Result<String, VerseconfError> {
    let content = std::fs::read_to_string(path)?;
    let template = parse_template(&content)?;
    render_template(&template, context)
}

/// 解析模板内容
pub fn parse_template(content: &str) -> Result<Template, VerseconfError> {
    let variables = engine::template_renderer::extract_variables(content);
    Ok(Template {
        name: "unnamed".to_string(),
        description: None,
        version: "1.0".to_string(),
        variables: variables
            .into_iter()
            .map(|name| TemplateVariable {
                name,
                var_type: VariableType::String,
                default: None,
                description: None,
                required: true,
                choices: None,
            })
            .collect(),
        content: content.to_string(),
    })
}

/// 使用高级验证规则校验 AST
pub fn validate_with_advanced_rules(
    ast: &Ast,
    rules: &AdvancedValidationRules,
) -> Vec<ValidationError> {
    rules.validate(ast)
}

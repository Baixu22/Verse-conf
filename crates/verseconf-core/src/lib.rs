pub mod ast;
pub mod engine;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod semantic;
pub mod source;

pub use ast::*;
pub use engine::*;
pub use error::*;
pub use lexer::*;
pub use parser::*;
pub use semantic::*;

use std::path::Path;

/// 解析配置文件
pub fn parse(source: &str) -> Result<Ast, VerseconfError> {
    let parser = Parser::new(source.to_string());
    parser.parse().map_err(|e| VerseconfError::Parse(ParseError::new(e.to_string(), Span::unknown())))
}

/// 使用配置解析配置文件
pub fn parse_with_config(source: &str, config: ParseConfig) -> Result<ParseResult<Ast>, VerseconfError> {
    let parser = Parser::with_config(source.to_string(), config);
    parser.parse_with_warnings().map_err(|e| VerseconfError::Parse(ParseError::new(e.to_string(), Span::unknown())))
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
pub fn parse_and_validate_with_config(source: &str, config: ParseConfig) -> Result<ParseResult<Ast>, VerseconfError> {
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

/// 解析文件
pub fn parse_file(path: &Path) -> Result<Ast, VerseconfError> {
    let source = std::fs::read_to_string(path)?;
    parse(&source)
}

/// 格式化配置文件
pub fn format(source: &str) -> Result<String, VerseconfError> {
    let ast = parse(source)?;
    Ok(PrettyPrinter::print(&ast))
}

/// 使用自定义配置格式化配置文件
pub fn format_with_config(source: &str, config: PrettyPrintConfig) -> Result<String, VerseconfError> {
    let ast = parse(source)?;
    Ok(PrettyPrinter::print_with_config(&ast, config))
}

/// 简单的 Pretty Printer（基础实现，保留向后兼容）
pub fn pretty_print(ast: &Ast) -> String {
    PrettyPrinter::print(ast)
}

/// 渲染模板
pub fn render_template(template: &Template, context: &RenderContext) -> Result<String, VerseconfError> {
    engine::template_renderer::render_template(template, context).map_err(|e| VerseconfError::Template(e.to_string()))
}

/// 从模板文件渲染配置
pub fn render_template_file(path: &Path, context: &RenderContext) -> Result<String, VerseconfError> {
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
        variables: variables.into_iter().map(|name| TemplateVariable {
            name,
            var_type: VariableType::String,
            default: None,
            description: None,
            required: true,
            choices: None,
        }).collect(),
        content: content.to_string(),
    })
}

/// 使用高级验证规则校验 AST
pub fn validate_with_advanced_rules(ast: &Ast, rules: &AdvancedValidationRules) -> Vec<ValidationError> {
    rules.validate(ast)
}

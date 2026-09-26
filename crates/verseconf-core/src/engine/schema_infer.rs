//! 由一份已有的配置推断出 `#@schema { ... }` 块。
//!
//! 这个模块刻意**只做能从当前值确定的推断**：
//!
//! - 类型：由值本身得出（字符串 / 整数 / 浮点 / 布尔 / 日期时间 / 时长 / 数组 / 表）；
//! - `required = true`：该字段在当前文档里确实存在；
//! - 嵌套结构：块表与内联表递归展开。
//!
//! **不推断** `range` / `enum` / `pattern` / `default`——这些是设计决策而不是
//! 观察结果：从一个值看不出合法区间，也看不出默认值应该是多少。猜出来的约束
//! 会变成后续编辑的假门禁，比没有约束更危险。
//!
//! 无法确定类型的字段（例如求值失败的表达式）会被原样列出，并附一行注释说明，
//! 而不是硬塞一个类型。

use crate::ast::*;
use crate::{parse, VerseconfError};

/// 由源码推断 schema 块。
///
/// 返回的是可直接写进配置文件的 `#@schema { ... }` 文本（不含结尾换行）。
pub fn infer_schema(source: &str) -> Result<String, VerseconfError> {
    let ast = parse(source)?;
    Ok(render_root(&ast.root))
}

/// 由 AST 推断 schema 块
pub fn infer_schema_from_ast(ast: &Ast) -> String {
    render_root(&ast.root)
}

fn render_root(table: &TableBlock) -> String {
    let mut out = String::from("#@schema {\n  version = \"1.0\"\n");
    let mut wrote_any = false;

    for entry in &table.entries {
        if let Some(block) = render_entry(entry, 1) {
            out.push('\n');
            out.push_str(&block);
            wrote_any = true;
        }
    }

    if !wrote_any {
        out.push_str("\n  # 当前文档里没有可推断的字段\n");
    }

    out.push_str("}\n");
    out
}

/// 渲染一个条目；不可推断的条目返回 None
fn render_entry(entry: &TableEntry, depth: usize) -> Option<String> {
    match entry {
        TableEntry::KeyValue(kv) => render_field(kv.key.as_str(), &kv.value, depth),
        TableEntry::TableBlock(child) => {
            let name = child.name.as_deref()?;
            Some(render_table_field(name, child, depth))
        }
        TableEntry::ArrayTable(array) => Some(render_array_field(array.key.as_str(), depth)),
        // include 指令与注释不参与 schema 推断
        _ => None,
    }
}

/// 标量/数组/内联表字段
fn render_field(name: &str, value: &Value, depth: usize) -> Option<String> {
    let pad = "  ".repeat(depth);
    let inner = "  ".repeat(depth + 1);

    match value {
        Value::TableBlock(table) => Some(render_table_field(name, table, depth)),
        Value::InlineTable(inline) => {
            let mut out = format!("{pad}{name} {{\n{inner}type = \"table\"\n");
            for kv in &inline.entries {
                if let Some(child) = render_field(kv.key.as_str(), &kv.value, depth + 1) {
                    out.push_str(&child);
                }
            }
            out.push_str(&format!("{pad}}}\n"));
            Some(out)
        }
        _ => {
            let type_name = infer_type(value);
            match type_name {
                Some(t) => Some(format!(
                    "{pad}{name} {{\n{inner}type = \"{t}\"\n{inner}required = true\n{pad}}}\n"
                )),
                // 推断不出类型时如实说明，而不是编一个
                None => Some(format!(
                    "{pad}# 无法从当前值确定 '{name}' 的类型（表达式求值失败或类型未知）\n"
                )),
            }
        }
    }
}

fn render_table_field(name: &str, table: &TableBlock, depth: usize) -> String {
    let pad = "  ".repeat(depth);
    let inner = "  ".repeat(depth + 1);

    let mut out = format!("{pad}{name} {{\n{inner}type = \"table\"\n{inner}required = true\n");
    for entry in &table.entries {
        if let Some(child) = render_entry(entry, depth + 1) {
            out.push_str(&child);
        }
    }
    out.push_str(&format!("{pad}}}\n"));
    out
}

fn render_array_field(name: &str, depth: usize) -> String {
    let pad = "  ".repeat(depth);
    let inner = "  ".repeat(depth + 1);
    // 数组元素的结构在这里不做推断：schema 语法对"数组元素类型"没有明确写法，
    // 硬编一个形状会变成假约束。
    format!("{pad}{name} {{\n{inner}type = \"array\"\n{inner}required = true\n{pad}}}\n")
}

/// 从值推断类型名；无法确定时返回 None
fn infer_type(value: &Value) -> Option<&'static str> {
    match value {
        Value::Scalar(scalar) => Some(scalar_type(scalar)),
        Value::Array(_) => Some("array"),
        Value::InlineTable(_) => Some("table"),
        Value::TableBlock(_) => Some("table"),
        // 表达式：能求值就用求值结果的类型，否则不猜
        Value::Expression(expr) => expr.evaluate().ok().map(|s| scalar_type(&s)),
    }
}

fn scalar_type(scalar: &ScalarValue) -> &'static str {
    match scalar {
        ScalarValue::String(_) => "string",
        ScalarValue::Number(NumberValue::Integer(_)) => "integer",
        ScalarValue::Number(NumberValue::Float(_)) => "float",
        ScalarValue::Boolean(_) => "boolean",
        ScalarValue::DateTime(_) => "datetime",
        ScalarValue::Duration(_) => "duration",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_scalar_types() {
        let schema =
            infer_schema("name = \"svc\"\nport = 8080\nratio = 0.5\ndebug = true\ntimeout = 30s\n")
                .unwrap();
        assert!(schema.contains("type = \"string\""), "{schema}");
        assert!(schema.contains("type = \"integer\""), "{schema}");
        assert!(schema.contains("type = \"float\""), "{schema}");
        assert!(schema.contains("type = \"boolean\""), "{schema}");
        assert!(schema.contains("type = \"duration\""), "{schema}");
    }

    #[test]
    fn infers_nested_tables() {
        let schema = infer_schema("server {\n  host = \"127.0.0.1\"\n  port = 8080\n}\n").unwrap();
        assert!(schema.contains("server {"), "{schema}");
        assert!(schema.contains("type = \"table\""), "{schema}");
        assert!(schema.contains("host {"), "{schema}");
        assert!(schema.contains("port {"), "{schema}");
    }

    #[test]
    fn infers_arrays_and_array_tables() {
        let schema =
            infer_schema("features = [\"a\", \"b\"]\n[[servers]]\nname = \"primary\"\n").unwrap();
        assert!(schema.contains("features {"), "{schema}");
        assert!(schema.contains("type = \"array\""), "{schema}");
        assert!(schema.contains("servers {"), "{schema}");
    }

    #[test]
    fn does_not_invent_constraints() {
        // 关键性质：不猜 range / enum / default —— 从值里看不出这些
        let schema = infer_schema("port = 8080\n").unwrap();
        assert!(!schema.contains("range"), "不应凭空生成 range：{schema}");
        assert!(!schema.contains("enum"), "不应凭空生成 enum：{schema}");
        assert!(
            !schema.contains("default"),
            "不应凭空生成 default：{schema}"
        );
        assert!(
            !schema.contains("pattern"),
            "不应凭空生成 pattern：{schema}"
        );
    }

    #[test]
    fn marks_present_fields_required() {
        let schema = infer_schema("name = \"x\"\n").unwrap();
        assert!(schema.contains("required = true"), "{schema}");
    }

    #[test]
    fn emits_a_valid_schema_block_that_round_trips() {
        let source =
            "app_name = \"svc\"\nserver {\n  host = \"127.0.0.1\"\n}\nfeatures = [\"a\"]\n";
        let schema = infer_schema(source).unwrap();

        // 把生成的 schema 块贴在原文前面，必须仍然能解析
        let combined = format!("{schema}\n{source}");
        parse(&combined).expect("生成的 schema 块必须能被自己解析");
    }

    #[test]
    fn empty_document_is_reported_not_faked() {
        let schema = infer_schema("").unwrap();
        assert!(schema.contains("没有可推断的字段"), "{schema}");
    }
}

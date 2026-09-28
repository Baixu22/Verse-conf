//! 把 JSON / JSONC 文档直译成 `verseconf-core` 的 AST。
//!
//! 这一层存在的理由与 `verseconf-toml` 的 `ast_bridge` 完全相同：让 schema 校验
//! 与安全审计**复用 `.vcf` 路径的同一份实现**（`SchemaValidator` 与 `AuditEngine`），
//! 而不是给 JSON 再写一套规则。两套规则一旦分叉，「写入前双重校验」这句话
//! 就不再对三种格式同时成立，而项目真正要卖的就是这句话。
//!
//! 翻译是结构到结构的直译，不做任何语义加工：
//!
//! | JSON / JSONC（`jsonc_parser::ast`） | core AST |
//! | --- | --- |
//! | 对象 | `TableEntry::KeyValue`（标量/数组/对象）或 `TableEntry::TableBlock`（嵌套对象） |
//! | 字符串 / 数字 / 布尔 / null | `Value::Scalar(..)` |
//! | 数组 | `Value::Array` |
//! | 嵌套对象 | `TableBlock`，递归下去 |
//!
//! 三处必须说清楚的取舍：
//!
//! 1. **字符串一律翻成 `Value::Scalar(String)`**，与 TOML 路径一致（`.vcf` 里普通
//!    字符串是 `Value::Expression(Literal)`）。两者在审计的 `is_literal_text`
//!    判定下结果相同（都看是不是整段 `${...}` 占位符），所以分档不会因为格式
//!    不同而变——这正是要保住的「误拒分档」一致性。JSON 没有表达式语法，
//!    所以这里不会产生 `Value::Expression`。
//! 2. **嵌套对象翻成 `Value::TableBlock` 而不是 `Value::InlineTable`**。
//!    两种形态在 schema 校验里都能满足 `type = "table"`，但只有 `TableBlock`
//!    会被审计递归下去：`Value::InlineTable` 在 `audit_key_value` 里落进
//!    `_ => {}`，值整段不被审计。翻成 `InlineTable` 会让 `{"server": {"tls_verify":
//!    false}}` 里的风险实例**静默消失**，那种「看起来过了」比直接报错更危险。
//! 3. **`null` 翻成字符串字面量 `"null"`**。审计的敏感字段判定里有
//!    `value != "null"` 这一条豁免，`ScalarValue` 没有 null 形态，而
//!    `ScalarValue::String("null")` 恰好落在同一条豁免上。
//!
//! 位置区间不做翻译：`span` 一律是 `Span::unknown()`，与 TOML 桥接层一致。
//! 需要字节区间的调用方走 [`crate::value_span_for_json_path`]（那条路直接用
//! `jsonc_parser` 的 AST 区间），而不是从 core AST 反推。

use jsonc_parser::ast as jast;
use jsonc_parser::{parse_to_ast, CollectOptions};
use verseconf_core::{
    ArrayValue, Ast, EditRefusal, Key, KeyValue, NumberValue, ScalarValue, SourceInfo, Span,
    TableBlock, TableEntry, Value,
};

use crate::JsonFlavor;

/// 把一份 JSON / JSONC 文本翻成 core AST。
///
/// `flavor` 决定语法宽严：`.json` 要严格（拒绝缺逗号这类会静默改变语义的写法），
/// `.jsonc` 允许注释、尾随逗号与单引号字符串。解析失败时返回 `parse_failed`。
pub fn json_to_ast(source: &str, flavor: JsonFlavor) -> Result<Ast, EditRefusal> {
    let options = flavor.parse_options();
    // BOM 由调用方看的文本负责去掉：解析器不认它，而带 BOM 的 JSON 是真实存在的
    let (text, _) = crate::strip_bom(source);
    let parse_result = parse_to_ast(text, &CollectOptions::default(), &options)
        .map_err(|error| EditRefusal::ParseFailed(error.to_string()))?;

    let entries = match &parse_result.value {
        // 顶层对象：键值直接进根表，与 TOML 文档的根表同构
        Some(jast::Value::Object(object)) => convert_object(object)?,
        // 顶层是数组或标量：core AST 的根必须是表，硬塞进去只会造出
        // 一份语义不明的 AST，所以明确拒绝而不是静默丢值。
        Some(_) => {
            return Err(EditRefusal::ParseFailed(
                "顶层不是对象：写前检查需要一份以对象为根的配置".to_string(),
            ))
        }
        // 空文档 / 只有注释：JSON 文档必须有一个顶层值，所以它**不是**合法的
        // JSON/JSONC。这里不能返回空根表——那等于把「候选根本不是一份配置」
        // 当成「一份没有字段的配置」放过，门禁会对一个非 JSON 的文件说 allowed。
        // （注意边界：`{}` 是合法 JSON，仍然放行；门禁只保证候选仍然合法且没
        //  引入新的高危实例，不承诺拦住「把内容删空」这种破坏性改动。）
        None => {
            return Err(EditRefusal::ParseFailed(
                "文档里没有任何值（空文档或只有注释）：JSON 文档必须有一个顶层对象".to_string(),
            ))
        }
    };

    Ok(Ast {
        root: TableBlock {
            name: None,
            entries,
            span: Span::unknown(),
        },
        schema: None,
        source: SourceInfo {
            path: None,
            content: source.to_string(),
        },
    })
}

/// 把 JSON 对象翻成表条目。
///
/// 嵌套对象翻成 `TableEntry::TableBlock`，其余值走 `TableEntry::KeyValue`；
/// 键序按 JSON 文本里的出现顺序保留（`jsonc_parser` 的 `properties` 是有序的）。
fn convert_object(object: &jast::Object<'_>) -> Result<Vec<TableEntry>, EditRefusal> {
    let mut entries = Vec::new();
    for property in &object.properties {
        let key = core_key(property.name.as_str());
        match convert_value(&property.value)? {
            Value::TableBlock(table) => entries.push(TableEntry::TableBlock(TableBlock {
                name: Some(property.name.as_str().to_string()),
                entries: table.entries,
                span: Span::unknown(),
            })),
            value => entries.push(TableEntry::KeyValue(KeyValue {
                key,
                value,
                metadata: None,
                comment: None,
                span: Span::unknown(),
            })),
        }
    }
    Ok(entries)
}

fn convert_value(value: &jast::Value<'_>) -> Result<Value, EditRefusal> {
    let converted = match value {
        jast::Value::StringLit(text) => Value::Scalar(ScalarValue::String(text.value.to_string())),
        jast::Value::NumberLit(number) => {
            Value::Scalar(ScalarValue::Number(parse_number(number.value)?))
        }
        jast::Value::BooleanLit(flag) => Value::Scalar(ScalarValue::Boolean(flag.value)),
        // 见模块头第 3 条取舍
        jast::Value::NullKeyword(_) => Value::Scalar(ScalarValue::String("null".to_string())),
        jast::Value::Array(array) => {
            let mut elements = Vec::new();
            for element in &array.elements {
                elements.push(convert_value(element)?);
            }
            Value::Array(ArrayValue {
                elements,
                span: Span::unknown(),
            })
        }
        jast::Value::Object(object) => Value::TableBlock(TableBlock {
            name: None,
            entries: convert_object(object)?,
            span: Span::unknown(),
        }),
    };
    Ok(converted)
}

/// JSON 只有一种数字，core 分整数与浮点：能整除成 `i64` 的就是整数。
///
/// `jsonc_parser` 宽松模式允许十六进制与正号写法，所以先按十进制解析，
/// 再退回十六进制。两条都不成立就拒绝：把无法解析的数字当 `0` 或 `NaN`
/// 蒙过去，会让门禁对一份自己没读懂的值回答「允许」，而这正是它不该做的事。
fn parse_number(text: &str) -> Result<NumberValue, EditRefusal> {
    let unsigned = text.strip_prefix(['-', '+']).unwrap_or(text);
    let negative = text.starts_with('-');

    if let Ok(value) = text.parse::<i64>() {
        return Ok(NumberValue::Integer(value));
    }
    if let Some(radix) = unsigned
        .strip_prefix("0x")
        .or_else(|| unsigned.strip_prefix("0X"))
    {
        if let Ok(value) = i64::from_str_radix(radix, 16) {
            return Ok(NumberValue::Integer(if negative { -value } else { value }));
        }
    }
    text.parse::<f64>()
        .map(NumberValue::Float)
        .map_err(|_| EditRefusal::ValidationFailed {
            path: "<result>".to_string(),
            message: format!("无法解析的数字字面量 '{text}'"),
        })
}

/// 键名能写成裸键就用裸键，否则用引号键——只影响 `Key` 的相等性，
/// 不影响 `Key::as_str()`，也就是不影响审计里的位置标识。
fn core_key(name: &str) -> Key {
    let bare = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare {
        Key::BareKey(name.to_string())
    } else {
        Key::QuotedKey(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_objects_become_nested_table_entries() {
        let ast = json_to_ast(r#"{"server":{"tls_verify":true}}"#, JsonFlavor::Json).unwrap();
        let TableEntry::TableBlock(server) = &ast.root.entries[0] else {
            panic!("嵌套对象必须翻成 TableBlock，否则审计不会递归下去");
        };
        assert_eq!(server.name.as_deref(), Some("server"));
        assert_eq!(server.entries.len(), 1);
    }

    #[test]
    fn json_scalars_keep_their_core_types() {
        let ast = json_to_ast(
            r#"{"n":1,"ratio":1.5,"flag":true,"text":"x","none":null}"#,
            JsonFlavor::Json,
        )
        .unwrap();
        let get = |key: &str| ast.root.get(key).map(|kv| kv.value.clone());
        assert!(matches!(
            get("n"),
            Some(Value::Scalar(ScalarValue::Number(NumberValue::Integer(1))))
        ));
        assert!(matches!(
            get("ratio"),
            Some(Value::Scalar(ScalarValue::Number(NumberValue::Float(_))))
        ));
        assert!(matches!(
            get("flag"),
            Some(Value::Scalar(ScalarValue::Boolean(true)))
        ));
        assert!(matches!(
            get("text"),
            Some(Value::Scalar(ScalarValue::String(_)))
        ));
        assert!(matches!(
            get("none"),
            Some(Value::Scalar(ScalarValue::String(ref text))) if text == "null"
        ));
    }

    #[test]
    fn an_empty_document_is_refused_instead_of_becoming_an_empty_root() {
        // 空文档返回空根表 = 把「候选根本不是一份配置」当成「一份没有字段的
        // 配置」，门禁会对一个非 JSON 的文件回答 allowed。JSON 要求有顶层值，
        // 所以这里是 fail-closed。
        for text in ["", "   ", "\n\n", "// 只有注释\n", "/* 只有注释 */"] {
            let refusal =
                json_to_ast(text, JsonFlavor::Jsonc).expect_err("没有任何值的文档必须拒绝");
            assert_eq!(refusal.code(), "parse_failed", "输入：{text:?}");
        }
    }

    #[test]
    fn an_empty_object_is_still_a_legal_document() {
        // 边界另一侧：`{}` 是合法 JSON。门禁只保证候选仍然合法且没引入新的
        // 高危实例，不承诺拦住「把内容删空」这种破坏性改动——把它也拒掉会让
        // 「删到只剩空对象」这条正常编辑路径失效。
        let ast = json_to_ast("{}", JsonFlavor::Json).expect("空对象是合法文档");
        assert!(ast.root.entries.is_empty());
    }

    #[test]
    fn a_non_object_root_is_refused_instead_of_being_dropped() {
        // 顶层数组没有对应的「根表」语义，静默返回空表会让门禁对一份
        // 完全没被检查过的文本回答「允许」
        let refusal = json_to_ast("[1,2,3]", JsonFlavor::Json).expect_err("顶层数组必须拒绝");
        assert_eq!(refusal.code(), "parse_failed");
    }

    #[test]
    fn jsonc_still_refuses_a_missing_comma() {
        // 宽严只在注释 / 尾随逗号 / 单引号上放行；缺逗号会把两条键粘成一个
        // 语义不同的文档，两种语法都不接受
        let text = r#"{"a":1 "b":2}"#;
        assert!(json_to_ast(text, JsonFlavor::Json).is_err());
        assert!(json_to_ast(text, JsonFlavor::Jsonc).is_err());
    }
}

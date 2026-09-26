//! 把 TOML 文档直译成 `verseconf-core` 的 AST。
//!
//! 这一层存在的理由只有一个：让 schema 校验与安全审计**复用 `.vcf` 路径的同一份实现**
//! （`SchemaValidator` 与 `AuditEngine`），而不是给 TOML 再写一套规则。
//! 两套规则一旦分叉，「写入前双重校验」这句话就不再对两种格式同时成立，
//! 而项目真正想卖的就是这句话。
//!
//! 翻译是结构到结构的直译，不做任何语义加工：
//!
//! | TOML（`toml_edit`） | core AST |
//! | --- | --- |
//! | `Item::Value` | `TableEntry::KeyValue` |
//! | `Item::Table`（含 `[a.b]` 与 `a.b = 1` 的点号表） | `TableEntry::TableBlock` |
//! | `Item::ArrayOfTables`（`[[x]]`，每个元素一条） | `TableEntry::ArrayTable` |
//! | 字符串 / 整数 / 浮点 / 布尔 / 日期时间 | `Value::Scalar(..)` |
//! | 数组 | `Value::Array` |
//! | 内联表 | `Value::InlineTable` |
//!
//! 有一处必须说清楚的取舍：TOML 的字符串**一律**翻成 `Value::Scalar(String)`，
//! 而 `.vcf` 里普通字符串是 `Value::Expression(Expression::Literal(String))`。
//! 两者在审计的 `is_literal_text` 判定下结果相同（都看是不是整段 `${...}` 占位符），
//! 所以分档不会因为格式不同而变——这正是要保住的「误拒分档」一致性。
//! TOML 没有表达式，所以这里不会产生 `Value::Expression`。
//!
//! `[[x]]` 的元素在 core 里是 `ArrayTable`，而 `ArrayTable.entries` 只装键值对，
//! 装不下再嵌套的表。嵌套表因此被翻成「值为 `Value::TableBlock` 的键值对」，
//! 这样审计仍然能往下走，而不是把这一段悄悄丢掉。

use toml_edit::{ImDocument, Item, Table, Value as TomlValue};
use verseconf_core::{
    ArrayTable, ArrayValue, Ast, EditRefusal, InlineTable, Key, KeyValue, NumberValue, ScalarValue,
    SourceInfo, Span, TableBlock, TableEntry, Value,
};

/// 把一份 TOML 文本翻成 core AST。解析失败时返回 `parse_failed`。
pub fn toml_to_ast(source: &str) -> Result<Ast, EditRefusal> {
    let document = ImDocument::parse(source.to_string())
        .map_err(|error| EditRefusal::ParseFailed(error.to_string()))?;

    let entries = match document.as_item() {
        Item::Table(table) => convert_entries(table)?,
        // 空文档在 `toml_edit` 里是一张空表；理论上不会走到这里，
        // 但真走到了也不该 panic——返回空根表，让后续校验自己判断。
        _ => Vec::new(),
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

/// 把一张表翻成表条目（普通表的写法）
fn convert_entries(table: &Table) -> Result<Vec<TableEntry>, EditRefusal> {
    let mut out = Vec::new();
    for (name, item) in table.iter() {
        match item {
            Item::Value(value) => out.push(TableEntry::KeyValue(KeyValue {
                key: core_key(name),
                value: convert_value(value)?,
                metadata: None,
                comment: None,
                span: Span::unknown(),
            })),
            Item::Table(nested) => out.push(TableEntry::TableBlock(TableBlock {
                name: Some(name.to_string()),
                entries: convert_entries(nested)?,
                span: Span::unknown(),
            })),
            Item::ArrayOfTables(array_of_tables) => {
                for element in array_of_tables.iter() {
                    out.push(TableEntry::ArrayTable(ArrayTable {
                        key: core_key(name),
                        entries: convert_key_values(element)?,
                        span: Span::unknown(),
                    }));
                }
            }
            // `Item::None` 只会由 `toml_edit` 的移除操作产生，解析结果里不会出现
            Item::None => {}
        }
    }
    Ok(out)
}

/// 把一张表翻成键值对（数组表元素只能用这个形状）
fn convert_key_values(table: &Table) -> Result<Vec<KeyValue>, EditRefusal> {
    let mut out = Vec::new();
    for (name, item) in table.iter() {
        let value = match item {
            Item::Value(value) => convert_value(value)?,
            Item::Table(nested) => Value::TableBlock(TableBlock {
                name: None,
                entries: convert_entries(nested)?,
                span: Span::unknown(),
            }),
            Item::ArrayOfTables(array_of_tables) => {
                let mut elements = Vec::new();
                for element in array_of_tables.iter() {
                    elements.push(Value::TableBlock(TableBlock {
                        name: None,
                        entries: convert_key_values(element)?
                            .into_iter()
                            .map(TableEntry::KeyValue)
                            .collect(),
                        span: Span::unknown(),
                    }));
                }
                Value::Array(ArrayValue {
                    elements,
                    span: Span::unknown(),
                })
            }
            Item::None => continue,
        };
        out.push(KeyValue {
            key: core_key(name),
            value,
            metadata: None,
            comment: None,
            span: Span::unknown(),
        });
    }
    Ok(out)
}

fn convert_value(value: &TomlValue) -> Result<Value, EditRefusal> {
    let converted = match value {
        TomlValue::String(text) => Value::Scalar(ScalarValue::String(text.value().clone())),
        TomlValue::Integer(number) => {
            Value::Scalar(ScalarValue::Number(NumberValue::Integer(*number.value())))
        }
        TomlValue::Float(number) => {
            Value::Scalar(ScalarValue::Number(NumberValue::Float(*number.value())))
        }
        TomlValue::Boolean(flag) => Value::Scalar(ScalarValue::Boolean(*flag.value())),
        TomlValue::Datetime(datetime) => {
            Value::Scalar(ScalarValue::DateTime(datetime.value().to_string()))
        }
        TomlValue::Array(array) => {
            let mut elements = Vec::new();
            for element in array.iter() {
                elements.push(convert_value(element)?);
            }
            Value::Array(ArrayValue {
                elements,
                span: Span::unknown(),
            })
        }
        TomlValue::InlineTable(table) => {
            let mut entries = Vec::new();
            for (name, item) in table.iter() {
                entries.push(KeyValue {
                    key: core_key(name),
                    value: convert_value(item)?,
                    metadata: None,
                    comment: None,
                    span: Span::unknown(),
                });
            }
            Value::InlineTable(InlineTable {
                entries,
                span: Span::unknown(),
            })
        }
    };
    Ok(converted)
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

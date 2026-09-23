use crate::ast::{
    ArrayValue, Expression, InlineTable, Key, KeyValue, NumberValue, ScalarValue, Span, TableBlock,
    TableEntry, Value,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration as StdDuration;

/// 编辑契约里的值。
///
/// 故意不直接复用 AST 类型：契约要作为模型与工具之间长期稳定的接口，
/// 序列化形态必须与 JSON 一一对应，而不是跟着内部结构走。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EditValue {
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<EditValue>),
    Table(BTreeMap<String, EditValue>),
}

impl EditValue {
    /// 转成 AST 值，交给格式化器渲染
    pub fn to_ast_value(&self) -> Value {
        match self {
            EditValue::Bool(value) => Value::Scalar(ScalarValue::Boolean(*value)),
            EditValue::Integer(value) => {
                Value::Scalar(ScalarValue::Number(NumberValue::Integer(*value)))
            }
            EditValue::Float(value) => {
                Value::Scalar(ScalarValue::Number(NumberValue::Float(*value)))
            }
            EditValue::String(value) => Value::Scalar(ScalarValue::String(value.clone())),
            EditValue::Array(items) => Value::Array(ArrayValue {
                elements: items.iter().map(EditValue::to_ast_value).collect(),
                span: Span::unknown(),
            }),
            EditValue::Table(map) => Value::InlineTable(InlineTable {
                entries: map
                    .iter()
                    .map(|(name, value)| KeyValue {
                        key: key_for(name),
                        value: value.to_ast_value(),
                        metadata: None,
                        comment: None,
                        span: Span::unknown(),
                    })
                    .collect(),
                span: Span::unknown(),
            }),
        }
    }

    /// 从 AST 值读出可比较的契约值；无法确定时返回 None，调用方必须拒绝而不是猜测
    pub fn from_ast_value(value: &Value) -> Option<EditValue> {
        match value {
            Value::Scalar(scalar) => scalar_to_edit_value(scalar),
            Value::Expression(expression) => expression_to_edit_value(expression),
            Value::Array(array) => {
                let items = array
                    .elements
                    .iter()
                    .map(EditValue::from_ast_value)
                    .collect::<Option<Vec<_>>>()?;
                Some(EditValue::Array(items))
            }
            Value::InlineTable(table) => {
                let mut map = BTreeMap::new();
                for entry in &table.entries {
                    map.insert(
                        entry.key.as_str().to_string(),
                        EditValue::from_ast_value(&entry.value)?,
                    );
                }
                Some(EditValue::Table(map))
            }
            Value::TableBlock(table) => table_block_to_edit_value(table).map(EditValue::Table),
        }
    }

    /// 面向错误信息与审计记录的文本形式
    pub fn to_text(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "<unrenderable>".to_string())
    }
}

fn scalar_to_edit_value(scalar: &ScalarValue) -> Option<EditValue> {
    match scalar {
        ScalarValue::String(value) => Some(EditValue::String(value.clone())),
        ScalarValue::Number(NumberValue::Integer(value)) => Some(EditValue::Integer(*value)),
        ScalarValue::Number(NumberValue::Float(value)) => Some(EditValue::Float(*value)),
        ScalarValue::Boolean(value) => Some(EditValue::Bool(*value)),
        // 日期时间与持续时间在配置里就是字面量，用文本比较才与文件内容一致
        ScalarValue::DateTime(value) => Some(EditValue::String(value.clone())),
        ScalarValue::Duration(value) => {
            Some(EditValue::String(crate::engine::render_duration(value)))
        }
    }
}

fn expression_to_edit_value(expression: &Expression) -> Option<EditValue> {
    match expression {
        Expression::Literal(scalar) => scalar_to_edit_value(scalar),
        Expression::UnitValue { value, unit } => {
            let seconds = (*value as u64) * unit.to_seconds();
            Some(EditValue::String(crate::engine::render_duration(
                &StdDuration::from_secs(seconds),
            )))
        }
        Expression::BinaryOp { .. } => expression
            .evaluate()
            .ok()
            .and_then(|scalar| scalar_to_edit_value(&scalar)),
    }
}

fn table_block_to_edit_value(table: &TableBlock) -> Option<BTreeMap<String, EditValue>> {
    let mut map = BTreeMap::new();
    for entry in &table.entries {
        match entry {
            TableEntry::KeyValue(kv) => {
                map.insert(
                    kv.key.as_str().to_string(),
                    EditValue::from_ast_value(&kv.value)?,
                );
            }
            TableEntry::TableBlock(nested) => {
                let name = nested.name.clone()?;
                map.insert(name, EditValue::Table(table_block_to_edit_value(nested)?));
            }
            // 注释与 include 不参与值比较
            _ => return None,
        }
    }
    Some(map)
}

/// 键名在能安全裸写时保持裸写，否则加引号，避免生成非法配置
fn key_for(name: &str) -> Key {
    let mut chars = name.chars();
    let is_bare = chars
        .next()
        .map(|first| first.is_ascii_alphabetic() || first == '_')
        .unwrap_or(false)
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-');

    if is_bare {
        Key::BareKey(name.to_string())
    } else {
        Key::QuotedKey(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn json_values_round_trip_through_the_contract_type() {
        let json = r#"{"count":3,"ratio":1.5,"enabled":true,"name":"app","tags":["a","b"]}"#;
        let value: EditValue = serde_json::from_str(json).expect("契约值可反序列化");
        assert_eq!(
            value,
            EditValue::Table(BTreeMap::from([
                ("count".to_string(), EditValue::Integer(3)),
                ("ratio".to_string(), EditValue::Float(1.5)),
                ("enabled".to_string(), EditValue::Bool(true)),
                ("name".to_string(), EditValue::String("app".to_string())),
                (
                    "tags".to_string(),
                    EditValue::Array(vec![
                        EditValue::String("a".to_string()),
                        EditValue::String("b".to_string()),
                    ]),
                ),
            ]))
        );
        let reencoded = serde_json::to_value(&value).unwrap();
        assert_eq!(reencoded["count"], serde_json::json!(3));
        assert_eq!(reencoded["ratio"], serde_json::json!(1.5));
    }

    #[test]
    fn ast_scalars_are_readable_as_contract_values() {
        let ast = parse("port = 8080\ntimeout = 30s\ncreated = 2024-01-15T10:30:00Z\n").unwrap();
        let root = &ast.root;

        assert_eq!(
            EditValue::from_ast_value(&root.get("port").unwrap().value),
            Some(EditValue::Integer(8080))
        );
        assert_eq!(
            EditValue::from_ast_value(&root.get("timeout").unwrap().value),
            Some(EditValue::String("30s".to_string()))
        );
        assert_eq!(
            EditValue::from_ast_value(&root.get("created").unwrap().value),
            Some(EditValue::String("2024-01-15T10:30:00Z".to_string()))
        );
    }

    #[test]
    fn contract_values_render_through_the_formatter_escaping_rules() {
        let value = EditValue::String("say \"hi\"\n".to_string());
        let rendered = crate::engine::render_value(&value.to_ast_value(), 0);
        assert_eq!(rendered, "\"say \\\"hi\\\"\\n\"");
    }
}

use wasm_bindgen::prelude::*;
use verseconf_core::{parse, Ast, Value, TableBlock};
use verseconf_core::ast::value::{Expression, ScalarValue, NumberValue};
use std::collections::HashMap;

#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub struct VerseConf {
    ast: Ast,
    tables: HashMap<String, TableBlock>,
    flat_keys: HashMap<String, ScalarValue>,
}

#[wasm_bindgen]
impl VerseConf {
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str) -> Result<VerseConf, JsValue> {
        parse(source)
            .map(|ast| {
                let mut tables = HashMap::new();
                let mut flat_keys = HashMap::new();
                tables.insert(String::new(), ast.root.clone());
                Self::build_table_map(&ast.root, String::new(), &mut tables, &mut flat_keys);
                VerseConf { ast, tables, flat_keys }
            })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    fn build_table_map(
        table: &TableBlock,
        prefix: String,
        tables: &mut HashMap<String, TableBlock>,
        flat_keys: &mut HashMap<String, ScalarValue>,
    ) {
        for entry in &table.entries {
            match entry {
                verseconf_core::TableEntry::KeyValue(kv) => {
                    let path = if prefix.is_empty() {
                        kv.key.as_str().to_string()
                    } else {
                        format!("{}.{}", prefix, kv.key.as_str())
                    };
                    if let Value::Expression(expr) = &kv.value {
                        if let Ok(scalar) = expr.evaluate() {
                            flat_keys.insert(path, scalar);
                        }
                    }
                }
                verseconf_core::TableEntry::TableBlock(tbl_entry) => {
                    if let Some(name) = &tbl_entry.name {
                        let path = if prefix.is_empty() {
                            name.clone()
                        } else {
                            format!("{}.{}", prefix, name)
                        };
                        tables.insert(path.clone(), TableBlock {
                            name: tbl_entry.name.clone(),
                            entries: tbl_entry.entries.clone(),
                            span: tbl_entry.span.clone(),
                        });
                        Self::build_table_map(tbl_entry, path, tables, flat_keys);
                    }
                }
                _ => {}
            }
        }
    }

    pub fn get_string(&self, path: &str) -> Option<String> {
        self.flat_keys.get(path).and_then(|s| {
            if let ScalarValue::String(v) = s {
                Some(v.clone())
            } else {
                None
            }
        })
    }

    pub fn get_number(&self, path: &str) -> Option<f64> {
        self.flat_keys.get(path).and_then(|s| {
            match s {
                ScalarValue::Number(NumberValue::Integer(i)) => Some(*i as f64),
                ScalarValue::Number(NumberValue::Float(f)) => Some(*f),
                _ => None,
            }
        })
    }

    pub fn get_boolean(&self, path: &str) -> Option<bool> {
        self.flat_keys.get(path).and_then(|s| {
            if let ScalarValue::Boolean(b) = s {
                Some(*b)
            } else {
                None
            }
        })
    }

    pub fn has_key(&self, path: &str) -> bool {
        self.flat_keys.contains_key(path) || self.tables.contains_key(path)
    }

    pub fn to_json(&self) -> String {
        self.table_to_json(&self.ast.root)
    }

    fn table_to_json(&self, table: &TableBlock) -> String {
        let items: Vec<String> = table
            .entries
            .iter()
            .filter_map(|entry| {
                match entry {
                    verseconf_core::TableEntry::KeyValue(kv) => {
                        if let Value::Expression(expr) = &kv.value {
                            if let Ok(scalar) = expr.evaluate() {
                                return Some(format!(
                                    "\"{}\": {}",
                                    escape_json(kv.key.as_str()),
                                    self.scalar_to_json(&scalar)
                                ));
                            }
                        }
                        None
                    }
                    verseconf_core::TableEntry::TableBlock(tbl_entry) => {
                        if let Some(name) = &tbl_entry.name {
                            Some(format!(
                                "\"{}\": {}",
                                escape_json(name),
                                self.table_to_json(tbl_entry)
                            ))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            })
            .collect();
        format!("{{{}}}", items.join(", "))
    }

    fn scalar_to_json(&self, scalar: &ScalarValue) -> String {
        match scalar {
            ScalarValue::String(s) => format!("\"{}\"", escape_json(s)),
            ScalarValue::Number(NumberValue::Integer(i)) => i.to_string(),
            ScalarValue::Number(NumberValue::Float(f)) => f.to_string(),
            ScalarValue::Boolean(b) => b.to_string(),
            ScalarValue::DateTime(dt) => format!("\"{}\"", escape_json(dt)),
            ScalarValue::Duration(d) => format!("\"{:?}\"", d),
        }
    }

    pub fn validate(&self) -> bool {
        true
    }
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[wasm_bindgen]
pub fn parse_config(source: &str) -> Result<JsValue, JsValue> {
    match parse(source) {
        Ok(ast) => {
            let conf = VerseConf::new(source).unwrap_or_else(|_| {
                VerseConf {
                    ast,
                    tables: HashMap::new(),
                    flat_keys: HashMap::new(),
                }
            });
            Ok(JsValue::from_str(&conf.to_json()))
        }
        Err(e) => Err(JsValue::from_str(&e.to_string())),
    }
}

#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

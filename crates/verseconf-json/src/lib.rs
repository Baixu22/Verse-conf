//! 把同一份编辑意图契约与写入前校验用到真实 JSON / JSONC 配置上。
//!
//! agent 宿主的设置类配置以 JSON/JSONC 为主（`settings.json`、`tsconfig.json`、
//! 各家 IDE 与 MCP 客户端的配置文件），它们几乎都带注释——而带注释的文件
//! 恰恰是「用 `serde_json` 解析再序列化」会毁掉的那一类。所以解析、定位与
//! 改写都交给 [`jsonc_parser`]（成熟 CST 库，保留注释、空白与键序），
//! 这一层只做三件事：
//!
//! 1. 用 `jsonc_parser` 的区间定位目标值在原文里的字节区间；
//! 2. 只替换/插入/删除那一段，区间之外一个字节都不动；
//! 3. 写入前做与 `.vcf` 路径**同源**的双重校验：schema 与安全审计（见 [`guard`]）；
//! 4. 失败时返回与 `.vcf` 路径同一套 `EditRefusal`（同一组稳定拒绝码）。
//!
//! ## 为什么不是「`serde_json` 解析 → 改 → `to_string()`」
//!
//! 那条路会**重新序列化整份文档**：注释全部消失，键序由 `Map` 的实现决定，
//! 缩进与空行被统一成一种风格。对一份 200 行的 `settings.json` 来说，
//! 改一个端口会产出几百行 diff，而用户真正想看到的是「一个值变了」。
//!
//! 更大的问题是它把「保真」变成了「尽量重建」：只要重建逻辑漏掉一种写法
//! （单引号、尾随逗号、注释类型），用户就会在无声无息中丢掉注释。
//! 这里是**不做重建**：改动之外一个字节都不动，所以没有「漏重建」这种事。
//!
//! ## 为什么 `json` 与 `jsonc` 是两个值
//!
//! `jsonc_parser` 默认极其宽松——连**缺逗号**都接受（`{"a":1 "b":2}`）。
//! 如果 `.json` 也吃这一口，门禁就会对一份不是合法 JSON 的候选回答「允许落盘」，
//! 而宿主写下去的是别的解析器读不了的文件。所以宽严由调用方声明
//! （[`JsonFlavor`]）：`json` 严格，`jsonc` 允许注释、尾随逗号与单引号字符串。
//! 这与「格式由调用方声明、不由本层猜」是同一条纪律。
//!
//! ## 门禁与编辑机制的关系
//!
//! 与 TOML 侧完全一致：门禁是**独立入口**，不产生改动，只裁决改动。
//!
//! ```text
//! let candidate = my_own_editor(source, ...);           // 用什么方式改都行
//! check_write_json(source, &candidate)?;                // 允许落盘吗？
//! std::fs::write(path, candidate)?;                     // 通过才写
//! ```
//!
//! 也可以走本层的编辑入口（[`set_json_value`] / [`replace_json_range`]），
//! 两条路共用同一份 [`guard::finalize_json_edit`]，所以同一对
//! (baseline, candidate) 的裁决一定相同。

mod ast_bridge;
mod guard;
pub mod json_edit;
mod json_schema;

pub use ast_bridge::json_to_ast;
pub use guard::{
    audit_json, check_write_json, check_write_json_with, json_ast, validate_json_against_schema,
    JsonGuard,
};
pub use json_edit::{
    apply_json_edit_plan, replace_json_range, set_json_value, set_json_value_with,
    value_span_for_json_path, JsonOutcome,
};
pub use json_schema::{
    check_write_json_with_document_schema, declared_schema, instance_value, prepare,
    validate_json_against_json_schema, DocumentSchema, DocumentSchemaOutcome, JsonSchemaDraft,
    PreparedSchema, SchemaFormat,
};

/// 去掉行首 BOM，返回（去掉之后的文本，去掉的字节数）。
///
/// Windows 上的编辑器（记事本、部分 VS Code 配置、PowerShell 的 `Set-Content`
/// 默认编码）会写出带 BOM 的 UTF-8。`jsonc_parser` 不认 BOM，会报
/// `Unexpected token on line 1 column 1`——于是门禁在**最需要它的那类真实
/// 配置**上直接不可用，而拒绝理由是宿主看不懂的解析错误。
///
/// RFC 8259 说实现不得自己加 BOM，但可以忽略它，所以这里选择忽略：
/// 解析用去掉 BOM 的文本，区间再按去掉的字节数平移回原文。区间必须平移，
/// 否则编辑会整体错位一个字节——那比拒绝更糟。
pub(crate) fn strip_bom(source: &str) -> (&str, usize) {
    match source.strip_prefix('\u{feff}') {
        Some(rest) => (rest, '\u{feff}'.len_utf8()),
        None => (source, 0),
    }
}

/// 候选文本按哪种语法解读。
///
/// 这个开关不是「宽容度调参」，它决定门禁**拒绝什么**：`Json` 会拒绝
/// 缺逗号、注释与尾随逗号，`Jsonc` 接受它们。调用方必须按真实文件名声明
/// （`.json` 用 [`JsonFlavor::Json`]，`.jsonc` / `settings.json` 这类带注释的
/// 用 [`JsonFlavor::Jsonc`]），本层不猜——猜错格式会把一份合法配置报成
/// `parse_failed`，而门禁的拒绝必须可归因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonFlavor {
    /// 严格 JSON：拒绝注释、尾随逗号、单引号、缺逗号、无引号键名、十六进制数
    Json,
    /// JSONC：接受注释、尾随逗号与单引号字符串；仍然拒绝缺逗号
    Jsonc,
}

impl JsonFlavor {
    /// 按文件名选语法。
    ///
    /// 认不出来的后缀按 JSONC（带注释的 `settings.json` 比严格 JSON 更常见）；
    /// `.json` 用严格语法。这条推断只在调用方没有显式声明格式时使用——
    /// 工具协议的正确做法是让调用方给 `format`，因为「猜格式」会让拒绝
    /// 变得不可归因。
    pub fn from_path(path: &str) -> Self {
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".jsonc") || lower.ends_with(".json5") {
            Self::Jsonc
        } else if lower.ends_with(".json") {
            Self::Json
        } else {
            Self::Jsonc
        }
    }

    /// 工具协议里的格式名
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Jsonc => "jsonc",
        }
    }

    pub(crate) fn parse_options(&self) -> jsonc_parser::ParseOptions {
        use jsonc_parser::ParseOptions;
        match self {
            Self::Json => ParseOptions {
                allow_comments: false,
                allow_loose_object_property_names: false,
                allow_trailing_commas: false,
                allow_missing_commas: false,
                allow_single_quoted_strings: false,
                allow_hexadecimal_numbers: false,
                allow_unary_plus_numbers: false,
            },
            Self::Jsonc => ParseOptions {
                allow_comments: true,
                allow_loose_object_property_names: false,
                allow_trailing_commas: true,
                allow_missing_commas: false,
                allow_single_quoted_strings: true,
                allow_hexadecimal_numbers: false,
                allow_unary_plus_numbers: false,
            },
        }
    }
}

#[cfg(test)]
mod flavor_tests {
    use super::*;

    #[test]
    fn strict_json_refuses_what_jsonc_accepts() {
        let commented = "{\n  // note\n  \"a\": 1,\n}\n";
        assert!(json_to_ast(commented, JsonFlavor::Json).is_err());
        assert!(json_to_ast(commented, JsonFlavor::Jsonc).is_ok());
    }

    #[test]
    fn jsonc_still_refuses_a_missing_comma() {
        // 缺逗号会让两份键悄悄粘在一起；这不是「注释风格」那一类差异，
        // 接受它等于门禁对一份坏文件说 allowed
        let missing = "{\"a\": 1 \"b\": 2}";
        assert!(json_to_ast(missing, JsonFlavor::Jsonc).is_err());
    }

    #[test]
    fn flavor_names_are_stable_for_the_tool_protocol() {
        assert_eq!(JsonFlavor::Json.as_str(), "json");
        assert_eq!(JsonFlavor::Jsonc.as_str(), "jsonc");
    }

    #[test]
    fn file_suffix_picks_the_syntax() {
        assert_eq!(JsonFlavor::from_path("settings.json"), JsonFlavor::Json);
        assert_eq!(JsonFlavor::from_path("tsconfig.jsonc"), JsonFlavor::Jsonc);
        assert_eq!(JsonFlavor::from_path("config.json5"), JsonFlavor::Jsonc);
        assert_eq!(JsonFlavor::from_path("no-extension"), JsonFlavor::Jsonc);
    }
}

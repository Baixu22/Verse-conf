//! 标准 JSON Schema 作为门禁输入（TF-0092 / TF-0093）。
//!
//! 为什么要有这一层：门禁的价值来自 schema，而 schema 的价值来自「我本来就有」。
//! 在此之前要用门禁必须先手写 `#@schema { ... }` 这门自建 DSL——而实测已经说明
//! 那门语言本身是净成本（输出 token +64.7%，准确率更低）。把学习成本从模型
//! 转嫁到人身上，等于让门禁背上那门语言已被证否的成本。
//!
//! 所以这里让门禁直接吃标准 JSON Schema，校验交给 `jsonschema`（成熟实现），
//! 支持 draft-07 与 2020-12 两版。自建 `#@schema` 仍然可用，只是从「唯一入口」
//! 降为「内联形式」（[`SchemaFormat::VcfDsl`]）。
//!
//! ## 三件刻意做的事
//!
//! 1. **不静默忽略不支持的方言与关键字。** JSON Schema 规范说实现「必须忽略」
//!    自己不认识的关键字，但那对**门禁**是错的：用户写 `requierd`（拼错）或
//!    用了一个我们没实现的方言时，约束并没有生效，而门禁却会说「允许落盘」。
//!    所以这里逐条报出来，返回 [`EditRefusal::UnsupportedSchema`]。
//! 2. **`format` 默认真的校验。** 规范把 `format` 定为注解还是断言取决于方言
//!    与词汇表；对配置门禁来说「格式写错」是要拦的，所以显式打开断言
//!    （`should_validate_formats(true)`），并且不认识的 format 名也报出来
//!    （`should_ignore_unknown_formats(false)`）。
//! 3. **`$schema` 解析默认离线。** 见 [`DocumentSchema`]：本地路径与调用方给的
//!    URL→文件映射都支持；取不到就返回 [`EditRefusal::SchemaUnavailable`]，
//!    绝不静默放行。门禁不该因为一次网络抖动而让写入通过或失败得无法解释。
//!
//! ## 明确的非目标
//!
//! 不做 HTTP 抓取：那会把 reqwest/tokio 拖进依赖树，也会让 wasm 目标凭空长出
//! 一张网络能力，而门禁本身是毫秒级的确定性检查。SchemaStore 之类的在线目录
//! 由宿主侧下载好、通过 [`DocumentSchema::url_map`] 交给本层。

use std::collections::BTreeMap;
use std::path::PathBuf;

use jsonschema::Validator;
use serde_json::Value;
use verseconf_core::EditRefusal;

use crate::{JsonFlavor, JsonGuard};

/// 用哪一版 JSON Schema 解释 schema。
///
/// `Auto` 按 schema 自己的 `$schema` 认；没写 `$schema` 时按 2020-12。
/// 认不出的方言不会被猜成某一版，而是明确报告（见 [`schema_dialect`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JsonSchemaDraft {
    #[default]
    Auto,
    Draft7,
    Draft202012,
}

impl JsonSchemaDraft {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Draft7 => "draft-07",
            Self::Draft202012 => "2020-12",
        }
    }
}

/// schema 用哪种语言写。
///
/// 由调用方声明而不是由本层猜：两套语言的形状不同（一份是 `#@schema { ... }`
/// 文本，一份是 JSON 文档），猜错会把一份合法 schema 报成 `parse_failed`，
/// 而拒绝必须可归因。这与「候选文本的格式由调用方声明」是同一条纪律。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SchemaFormat {
    /// 自建内联 DSL（`#@schema { ... }`），与 `.vcf` / TOML 路径同源
    #[default]
    VcfDsl,
    /// 标准 JSON Schema（draft-07 / 2020-12）
    JsonSchema,
}

impl SchemaFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::VcfDsl => "vcf",
            Self::JsonSchema => "json-schema",
        }
    }

    /// 从调用方给的字符串解析；认不出就返回 `None`，让上层报 `invalid_arguments`
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "vcf" | "vcf-dsl" | "#@schema" => Some(Self::VcfDsl),
            "json-schema" | "jsonschema" | "json" => Some(Self::JsonSchema),
            _ => None,
        }
    }
}

/// draft-07 的关键字（本层声称执行的集合，来自规范的 core / applicator /
/// validation / metadata / format 词汇表）。
///
/// 这张表只用来**报告**不认识的写法，不参与校验本身——校验完全由 `jsonschema`
/// 负责。表里没有的键会被报成不受支持；这正是要的效果：拼错的 `requierd`
/// 不会静默变成「没有约束」。
const DRAFT7_KEYWORDS: &[&str] = &[
    "$schema",
    "$id",
    "$ref",
    "$comment",
    "title",
    "description",
    "default",
    "readOnly",
    "writeOnly",
    "examples",
    "multipleOf",
    "maximum",
    "exclusiveMaximum",
    "minimum",
    "exclusiveMinimum",
    "maxLength",
    "minLength",
    "pattern",
    "additionalItems",
    "items",
    "maxItems",
    "minItems",
    "uniqueItems",
    "contains",
    "maxProperties",
    "minProperties",
    "required",
    "additionalProperties",
    "definitions",
    "properties",
    "patternProperties",
    "dependencies",
    "propertyNames",
    "const",
    "enum",
    "type",
    "format",
    "contentMediaType",
    "contentEncoding",
    "if",
    "then",
    "else",
    "allOf",
    "anyOf",
    "oneOf",
    "not",
];

/// 2020-12 的关键字。
const DRAFT202012_KEYWORDS: &[&str] = &[
    "$schema",
    "$id",
    "$anchor",
    "$dynamicAnchor",
    "$ref",
    "$dynamicRef",
    "$vocabulary",
    "$comment",
    "$defs",
    "title",
    "description",
    "default",
    "deprecated",
    "readOnly",
    "writeOnly",
    "examples",
    "multipleOf",
    "maximum",
    "exclusiveMaximum",
    "minimum",
    "exclusiveMinimum",
    "maxLength",
    "minLength",
    "pattern",
    "items",
    "prefixItems",
    "maxItems",
    "minItems",
    "uniqueItems",
    "contains",
    "minContains",
    "maxContains",
    "maxProperties",
    "minProperties",
    "required",
    "dependentRequired",
    "additionalProperties",
    "properties",
    "patternProperties",
    "dependentSchemas",
    "propertyNames",
    "const",
    "enum",
    "type",
    "format",
    "contentMediaType",
    "contentEncoding",
    "contentSchema",
    "if",
    "then",
    "else",
    "allOf",
    "anyOf",
    "oneOf",
    "not",
    "unevaluatedItems",
    "unevaluatedProperties",
];

/// 2020-12 里本层认识的标准词汇表 URI（出现在 `$vocabulary` 时不算不受支持）。
const KNOWN_VOCABULARIES: &[&str] = &[
    "https://json-schema.org/draft/2020-12/vocab/core",
    "https://json-schema.org/draft/2020-12/vocab/applicator",
    "https://json-schema.org/draft/2020-12/vocab/unevaluated",
    "https://json-schema.org/draft/2020-12/vocab/validation",
    "https://json-schema.org/draft/2020-12/vocab/meta-data",
    "https://json-schema.org/draft/2020-12/vocab/format-annotation",
    "https://json-schema.org/draft/2020-12/vocab/format-assertion",
    "https://json-schema.org/draft/2020-12/vocab/content",
];

/// 注解与扩展类关键字：它们**不约束实例**，忽略它们不会让用户以为某个约束生效了。
///
/// 这条线是「报告不认识的写法」能不能用的关键。真实世界的 schema（SchemaStore 上的
/// tsconfig.json、package.json 等）到处是编辑器扩展：`markdownDescription`、
/// `x-intellij-*`、`tsType`、`allowTrailingCommas`。把它们和拼错的 `requierd`
/// 一起判死，等于门禁对**最想服务的那批配置**说「不受支持」——比不接标准 schema 更糟。
///
/// 另外按惯例放行 `x-` 前缀：JSON Schema 生态用这个前缀表示「实现可以忽略的扩展」，
/// 没有人以为 `x-foo` 会被执行。
const ANNOTATION_KEYWORDS: &[&str] = &[
    "title",
    "description",
    "default",
    "deprecated",
    "readOnly",
    "writeOnly",
    "examples",
    "$comment",
    "markdownDescription",
    "markdownEnumDescriptions",
    "markdownDescription",
    "allowTrailingCommas",
    "tsType",
    "errorMessage",
];

/// 一个键是不是「我们认得的写法」。
///
/// 两个方言取**并集**：我们的目标是抓「写错了 / 根本不认识」，不是方言纯洁性。
/// 校验器本身接受跨方言的容器写法（2020-12 文档里用 `definitions`、draft-07 里用
/// `$defs`，`$ref` 都能解析、约束都会被执行），把它报成不受支持，是把一份**能用**
/// 的 schema 判死——而 `$schema` 缺失时方言默认是 2020-12，于是每一份用
/// `definitions` 的旧 schema 都会被拒。
fn keyword_is_supported(key: &str) -> bool {
    DRAFT7_KEYWORDS.contains(&key)
        || DRAFT202012_KEYWORDS.contains(&key)
        || ANNOTATION_KEYWORDS.contains(&key)
        || key.starts_with("x-")
        || key.starts_with("X-")
}

/// 一个 schema 文本 + 它的方言，准备好被用来校验实例。
#[derive(Debug)]
pub struct PreparedSchema {
    validator: Validator,
    draft: JsonSchemaDraft,
}

impl PreparedSchema {
    /// 解析、报告不受支持的写法、构建校验器。
    ///
    /// 任何一步失败都返回 `Err`，绝不返回一个「大概是空的」校验器——那种东西
    /// 会让门禁对任何候选都说允许。
    pub fn new(schema_text: &str, requested: JsonSchemaDraft) -> Result<Self, EditRefusal> {
        // schema 文本走与配置同一条 JSONC 解析路径：真实 schema 文件也常常带 BOM
        // （Windows 编辑器写的，见 strip_bom），有些还带注释与尾随逗号。用
        // `serde_json` 直接解会在这些文件上失败，而报出来的 parse_failed 会让人
        // 以为是 schema 内容写错了。
        let schema = instance_value(schema_text, JsonFlavor::Jsonc)?;

        let draft = schema_dialect(&schema, requested)?;
        let unsupported = unsupported_schema_parts(&schema);
        if !unsupported.is_empty() {
            return Err(EditRefusal::UnsupportedSchema {
                path: "<schema>".to_string(),
                unsupported,
            });
        }

        let options = match draft {
            JsonSchemaDraft::Draft7 => jsonschema::draft7::options(),
            // Auto 在 `schema_dialect` 里已经落成两版之一，不会走到这里
            JsonSchemaDraft::Draft202012 | JsonSchemaDraft::Auto => {
                jsonschema::draft202012::options()
            }
        };

        let validator = options
            .should_validate_formats(true)
            .should_ignore_unknown_formats(false)
            .build(&schema)
            .map_err(schema_build_failure)?;

        Ok(Self { validator, draft })
    }

    pub fn draft(&self) -> JsonSchemaDraft {
        self.draft
    }

    /// 校验一份实例；违规返回 `validation_failed`，与自建 schema 路径同码。
    ///
    /// 只报第一条违规会把「改完之后到底哪里不对」变成猜谜，所以这里把所有
    /// 实例路径都列出来（按出现顺序，去重）。
    pub fn validate(&self, instance: &Value) -> Result<(), EditRefusal> {
        let mut violations: Vec<String> = Vec::new();
        for error in self.validator.iter_errors(instance) {
            let location = error.instance_path.to_string();
            let location = if location.is_empty() {
                "<result>".to_string()
            } else {
                format!("<result>{location}")
            };
            let message = format!("{location}: {error}");
            if !violations.contains(&message) {
                violations.push(message);
            }
        }

        if violations.is_empty() {
            Ok(())
        } else {
            Err(EditRefusal::ValidationFailed {
                path: "<result>".to_string(),
                message: violations.join("; "),
            })
        }
    }
}

/// 把候选文本读成 `serde_json::Value`（JSONC 的注释与尾随逗号由调用方先解析）。
pub fn instance_value(source: &str, flavor: JsonFlavor) -> Result<Value, EditRefusal> {
    let (text, _) = crate::strip_bom(source);
    let options = flavor.parse_options();
    let parsed =
        jsonc_parser::parse_to_ast(text, &jsonc_parser::CollectOptions::default(), &options)
            .map_err(|error| EditRefusal::ParseFailed(error.to_string()))?;

    match parsed.value {
        Some(value) => Ok(Value::from(value)),
        // 空文档不是一份合法配置（见 ast_bridge 的说明），这里同样不能当成 `null`
        None => Err(EditRefusal::ParseFailed(
            "文档里没有任何值（空文档或只有注释）：JSON 文档必须有一个顶层对象".to_string(),
        )),
    }
}

/// 构建校验器失败时，分清「schema 写法不受支持」与「schema 取不全」。
///
/// 这两种拒绝让调用方做的事完全不同：前者要去掉/改写那个写法，后者要去把
/// schema（或它引用的文件）拿到手。把跨文件 / 远程 `$ref` 解析失败报成
/// `unsupported_schema`，会把人指向错误的方向。
fn schema_build_failure(error: jsonschema::ValidationError<'_>) -> EditRefusal {
    let message = error.to_string();
    let lower = message.to_ascii_lowercase();
    // 上游把「引用取不到」表达成几种不同的说法（实测：`Resource '…' is not present
    // in a registry and retrieving it failed: resolve-http feature …`），所以这里
    // 匹配一组标记词而不是一个词。匹配不到就按「写法不受支持」报——那条更保守，
    // 不会把「schema 写法有问题」误说成「去下载文件」。
    let markers = [
        "$ref",
        "not present in a registry",
        "retriev",
        "unresolv",
        "unable to resolve",
        "custom resolver",
        "resolve-http",
    ];
    if markers.iter().any(|marker| lower.contains(marker)) {
        EditRefusal::SchemaUnavailable {
            path: "<schema>".to_string(),
            reason: format!(
                "schema 里的引用解析不了（本层不抓网络、也不解析跨文件 $ref，请先合并进来）：{message}"
            ),
        }
    } else {
        EditRefusal::UnsupportedSchema {
            path: "<schema>".to_string(),
            unsupported: vec![message],
        }
    }
}

/// 认一份 schema 的方言；认不出就明确报告，不猜。
fn schema_dialect(
    schema: &Value,
    requested: JsonSchemaDraft,
) -> Result<JsonSchemaDraft, EditRefusal> {
    let declared = schema.get("$schema").and_then(Value::as_str);

    let Some(uri) = declared else {
        // 没写 `$schema` 时按调用方声明；`Auto` 落到 2020-12（当前默认方言）
        return Ok(match requested {
            JsonSchemaDraft::Auto => JsonSchemaDraft::Draft202012,
            explicit => explicit,
        });
    };

    // 精确匹配已知的方言 URI（允许结尾的 `#` 与 `/`）。不做后缀匹配：
    // 任何以 `…/draft-07/schema` 结尾的字符串都能蒙过去，那等于凭一个后缀
    // 猜方言，而「认不出就报出来」正是这一层的纪律。
    let normalized = uri.trim_end_matches('#').trim_end_matches('/');
    let detected = match normalized {
        "http://json-schema.org/draft-07/schema" | "https://json-schema.org/draft-07/schema" => {
            JsonSchemaDraft::Draft7
        }
        "https://json-schema.org/draft/2020-12/schema"
        | "http://json-schema.org/draft/2020-12/schema" => JsonSchemaDraft::Draft202012,
        _ => {
            return Err(EditRefusal::UnsupportedSchema {
                path: "<schema>".to_string(),
                unsupported: vec![format!("$schema = {uri}")],
            })
        }
    };

    // 调用方显式指定的方言与 schema 自己声明的不一致时，以 schema 为准但要报出来：
    // 静默按调用方说的执行，会让「这份 schema 按 2020-12 写的」这件事消失。
    if let JsonSchemaDraft::Draft7 | JsonSchemaDraft::Draft202012 = requested {
        if requested != detected {
            return Err(EditRefusal::UnsupportedSchema {
                path: "<schema>".to_string(),
                unsupported: vec![format!(
                    "$schema 声明的是 {}，调用方指定的是 {}",
                    detected.as_str(),
                    requested.as_str()
                )],
            });
        }
    }

    Ok(detected)
}

/// 找出这份 schema 里本层不保证执行的写法。
///
/// 返回空表示「每一个关键字我们都会执行」。这里**只报告**，不参与校验。
///
/// 判据见 [`keyword_is_supported`]：两个方言取并集，注解与 `x-` 扩展不算不受支持。
fn unsupported_schema_parts(schema: &Value) -> Vec<String> {
    let mut out = Vec::new();
    walk_schema(schema, "$", &mut out);

    // 2020-12 的 `$vocabulary`：声明了本层不认识且必需（true）的词汇表时必须报
    if let Some(vocabulary) = schema.get("$vocabulary").and_then(Value::as_object) {
        for (uri, required) in vocabulary {
            if required.as_bool() == Some(true) && !KNOWN_VOCABULARIES.contains(&uri.as_str()) {
                out.push(format!("$vocabulary {uri}"));
            }
        }
    }

    out
}

/// 按 JSON Schema 的结构走到每一个子 schema。
///
/// 不能对整份文档做「所有键都当关键字」的浅扫描：`properties` 下面那些键是
/// **属性名**，一个叫 `required` 的配置字段会被误报成关键字。所以这里按
/// 规范里「哪些位置的子对象仍然是 schema」逐层下去。
fn walk_schema(schema: &Value, path: &str, out: &mut Vec<String>) {
    let Some(object) = schema.as_object() else {
        // 数组形式只出现在 `items`（draft-07）等位置，由调用方逐个元素走
        if let Some(items) = schema.as_array() {
            for (index, item) in items.iter().enumerate() {
                walk_schema(item, &format!("{path}/{index}"), out);
            }
        }
        return;
    };

    for (key, value) in object {
        let here = format!("{path}/{key}");
        if !keyword_is_supported(key) {
            out.push(here);
            // 不往下走：一个不认识的键下面的结构没有可解释的语义
            continue;
        }

        match key.as_str() {
            // 名字 → 子 schema 的映射
            "properties" | "patternProperties" | "$defs" | "definitions" | "dependentSchemas"
            | "dependencies" => {
                if let Some(map) = value.as_object() {
                    for (name, sub) in map {
                        // draft-07 的 `dependencies` 允许数组形式（属性依赖），那是指针列表
                        if key == "dependencies" && sub.is_array() {
                            continue;
                        }
                        walk_schema(sub, &format!("{here}/{name}"), out);
                    }
                }
            }
            // 单个子 schema
            "items"
            | "additionalProperties"
            | "additionalItems"
            | "contains"
            | "propertyNames"
            | "not"
            | "if"
            | "then"
            | "else"
            | "unevaluatedItems"
            | "unevaluatedProperties"
            | "contentSchema" => walk_schema(value, &here, out),
            // 子 schema 列表
            "allOf" | "anyOf" | "oneOf" | "prefixItems" => {
                if let Some(items) = value.as_array() {
                    for (index, item) in items.iter().enumerate() {
                        walk_schema(item, &format!("{here}/{index}"), out);
                    }
                }
            }
            // draft-07 的 `items` 可以是单个 schema，也可以是 schema 列表
            _ => {}
        }
    }
}

/// 按调用方声明的方言校验一份候选文本。
///
/// 这是 [`JsonGuard`] 里 `schema_format = json-schema` 的实现路径。
pub fn validate_json_against_json_schema(
    source: &str,
    schema_text: &str,
    requested_draft: JsonSchemaDraft,
    flavor: JsonFlavor,
) -> Result<(), EditRefusal> {
    let prepared = PreparedSchema::new(schema_text, requested_draft)?;
    let instance = instance_value(source, flavor)?;
    prepared.validate(&instance)
}

/// 从配置里的 `$schema` 取 schema 并用于校验（TF-0093）。
///
/// ## 离线优先
///
/// 解析顺序：调用方给的 [`url_map`](Self::url_map) → 本地路径。
/// 网络抓取**不做**：门禁是毫秒级的确定性检查，不该把 HTTP 客户端拖进依赖树，
/// 也不该在 wasm 上凭空长出一张网络能力。想用 SchemaStore 的宿主自己把 schema
/// 下载好，用 `url_map` 把 URL 映射到本地文件即可。
///
/// ## 取不到就明确报告
///
/// 配置里没有 `$schema`、URL 不在映射里、文件读不到——三种都返回
/// [`EditRefusal::SchemaUnavailable`]，而不是「没有 schema 要做，那就放行」。
/// 后者会让「我以为门禁在按 schema 检查」这种误解一直存在下去。
#[derive(Debug, Clone, Default)]
pub struct DocumentSchema {
    /// 解析相对路径时的基准目录（通常是配置文件所在目录）
    pub base_dir: Option<PathBuf>,
    /// `$schema` URL → 本地 schema 文件。宿主用它把 SchemaStore 的 URL 指到本地副本
    pub url_map: BTreeMap<String, PathBuf>,
    /// 期望的方言；`Auto` 按 schema 自己声明
    pub draft: JsonSchemaDraft,
    /// 取不到 schema 时是否允许降级为「只做结构与安全校验」。
    ///
    /// 默认 `false`：返回 [`EditRefusal::SchemaUnavailable`]，这次改动落不了盘。
    /// 这是刻意的 fail-closed——门禁最不该做的事是「没能检查却说允许」。
    ///
    /// 设成 `true` 是**调用方显式承担**「这次没按 schema 检查」的后果：结构与
    /// 安全审计照旧做，schema 那一层跳过，并且结果里如实报告跳过原因
    /// （[`DocumentSchemaOutcome::skipped`]），工具协议里也会带出来。离线场景下
    /// 这是个真实选择：宁可按较弱的保证继续，也不要把所有写入卡死。
    pub allow_unresolved: bool,
}

/// 按 `$schema` 取 schema 的那条路走完之后的交代。
///
/// 光返回 `Ok(())` 不够：调用方需要知道**这次到底按哪份 schema 检查的**，
/// 或者**这次根本没检查 schema**（仅在显式 opt-in 降级时）。
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentSchemaOutcome {
    /// 配置里声明的 `$schema`（取到或没取到都会带上，便于宿主自查）
    pub declared: Option<String>,
    /// 降级跳过时写清为什么。没有降级就是 `None`
    pub skipped: Option<String>,
}

impl DocumentSchema {
    pub fn with_base_dir(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: Some(base_dir.into()),
            ..Self::default()
        }
    }

    /// 注册一条 URL → 本地文件的映射
    pub fn map_url(mut self, url: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        self.url_map.insert(url.into(), path.into());
        self
    }

    /// 从配置里读出 `$schema` 并取到它的文本
    pub fn load(&self, declared: &str) -> Result<String, EditRefusal> {
        if let Some(path) = self.url_map.get(declared) {
            return std::fs::read_to_string(path).map_err(|error| EditRefusal::SchemaUnavailable {
                path: declared.to_string(),
                reason: format!("映射到的文件读不到（{}）：{error}", path.display()),
            });
        }

        if declared.starts_with("http://") || declared.starts_with("https://") {
            return Err(EditRefusal::SchemaUnavailable {
                path: declared.to_string(),
                reason: "本层不抓取网络 schema；请用 url_map 把该 URL 指到本地副本".to_string(),
            });
        }

        let path = PathBuf::from(file_url_to_local_path(declared));
        let resolved = if path.is_absolute() {
            path
        } else {
            match &self.base_dir {
                Some(base) => base.join(path),
                None => {
                    return Err(EditRefusal::SchemaUnavailable {
                        path: declared.to_string(),
                        reason: "schema 是相对路径，但没有给 base_dir".to_string(),
                    })
                }
            }
        };

        std::fs::read_to_string(&resolved).map_err(|error| EditRefusal::SchemaUnavailable {
            path: declared.to_string(),
            reason: format!("读不到 {}：{error}", resolved.display()),
        })
    }
}

/// 把 `file://` URL 与普通路径都翻成可用的本地路径。
///
/// 三种写法都得认，而且**绝对路径必须仍然是绝对的**：
/// - `file:///C:/Users/x.json`（标准写法）→ `C:/Users/x.json`；
///   去掉 `file://` 之后剩下 `/C:/Users/x.json`，在 Windows 上这不是绝对路径，
///   直接交给 `Path` 会被拼成 `C:/C:/Users/...` 并报 os error 123。
/// - `file://localhost/C:/x` 与 `file://server/share/x`：`//` 后面是 authority，
///   要跳过它。
/// - `file://C:\x` 这种非标准写法（反斜杠）：原样当路径。
/// - 普通路径（绝对或相对）：原样返回。
fn file_url_to_local_path(declared: &str) -> String {
    let Some(rest) = declared.strip_prefix("file://") else {
        return declared.to_string();
    };

    // `//` 之后可能是 authority（`localhost` / 主机名）：路径从第一个 `/` 开始。
    let path = match rest.find('/') {
        Some(0) => rest,
        Some(index) => &rest[index..],
        // 没有 `/`：`file://C:\x` 这种非标准写法，整段当路径
        None => rest,
    };

    // Windows 盘符：`/C:/x` -> `C:/x`（Unix 上 `/home/x` 不受影响）
    let bytes = path.as_bytes();
    if bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':' {
        return path[1..].to_string();
    }
    path.to_string()
}

/// 从配置文本里读出 `$schema`。
///
/// 只看顶层 `$schema`：嵌套对象里的 `$schema` 是子 schema 的方言声明，
/// 不是「这份配置用哪份 schema」。
pub fn declared_schema(source: &str, flavor: JsonFlavor) -> Result<Option<String>, EditRefusal> {
    let instance = instance_value(source, flavor)?;
    let Some(declared) = instance.get("$schema") else {
        return Ok(None);
    };
    match declared.as_str() {
        Some(text) => Ok(Some(text.to_string())),
        // `$schema` 存在但不是字符串：说它在，但没法用。直接当成「没声明」
        // 会让调用方以为这份配置不需要 schema，那是不准确的。
        None => Err(EditRefusal::SchemaUnavailable {
            path: "<config>".to_string(),
            reason: format!("$schema 必须是字符串，实际是 {declared}"),
        }),
    }
}

/// 写前检查：schema 由配置自己的 `$schema` 决定。
///
/// 基线决定用哪份 schema（候选可能正是把 `$schema` 删掉的那次改动，那时不该
/// 因为「候选里没有 $schema」就把检查降级成放行）。基线没有声明就退回候选。
///
/// 取不到 schema 时默认拒绝（`schema_unavailable`）；只有调用方在
/// [`DocumentSchema::allow_unresolved`] 里显式 opt-in 才降级为「结构与安全校验」，
/// 并把跳过原因放进 [`DocumentSchemaOutcome::skipped`]。
pub fn check_write_json_with_document_schema(
    baseline: &str,
    candidate: &str,
    guard: &JsonGuard<'_>,
    document: &DocumentSchema,
) -> Result<DocumentSchemaOutcome, EditRefusal> {
    let declared =
        declared_schema(baseline, guard.flavor)?.or(declared_schema(candidate, guard.flavor)?);

    let skipped = |reason: String| DocumentSchemaOutcome {
        declared: declared.clone(),
        skipped: Some(reason),
    };

    let Some(declared) = declared.clone() else {
        let reason = "配置里没有 $schema 声明，不知道按哪份 schema 检查".to_string();
        if document.allow_unresolved {
            // 显式降级：结构与安全照旧，schema 那层跳过，并如实报告
            crate::check_write_json_with(baseline, candidate, guard)?;
            return Ok(DocumentSchemaOutcome {
                declared: None,
                skipped: Some(reason),
            });
        }
        return Err(EditRefusal::SchemaUnavailable {
            path: "<config>".to_string(),
            reason,
        });
    };

    let schema_text = match document.load(&declared) {
        Ok(text) => text,
        Err(refusal) => {
            if document.allow_unresolved {
                crate::check_write_json_with(baseline, candidate, guard)?;
                return Ok(skipped(refusal.to_string()));
            }
            return Err(refusal);
        }
    };

    // 走与其它 schema 路径同一套写入前校验，只是这次 schema 来自文档。
    // 这里不提前构建校验器：构建发生在同一段代码里，两处各建一次等于让
    // 「不支持的关键字」有两个报告点，迟早对不上。
    let document_guard = JsonGuard {
        schema: Some(&schema_text),
        schema_format: SchemaFormat::JsonSchema,
        schema_draft: document.draft,
        ..*guard
    };
    crate::check_write_json_with(baseline, candidate, &document_guard)?;

    Ok(DocumentSchemaOutcome {
        declared: Some(declared),
        skipped: None,
    })
}

/// 只做校验、不做审计的入口：给宿主在别处复用同一份 schema 时用。
pub fn prepare(schema_text: &str, draft: JsonSchemaDraft) -> Result<PreparedSchema, EditRefusal> {
    PreparedSchema::new(schema_text, draft)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRAFT7_TS: &str = r#"{
      "$schema": "http://json-schema.org/draft-07/schema#",
      "type": "object",
      "properties": { "tab_spaces": { "type": "integer" } },
      "required": ["tab_spaces"]
    }"#;

    const DRAFT202012_TS: &str = r#"{
      "$schema": "https://json-schema.org/draft/2020-12/schema",
      "type": "object",
      "properties": { "tab_spaces": { "type": "integer" } },
      "required": ["tab_spaces"]
    }"#;

    #[test]
    fn a_draft7_schema_catches_a_type_drift() {
        let prepared = PreparedSchema::new(DRAFT7_TS, JsonSchemaDraft::Auto).unwrap();
        assert_eq!(prepared.draft(), JsonSchemaDraft::Draft7);
        let instance = serde_json::json!({ "tab_spaces": "4242" });
        let refusal = prepared
            .validate(&instance)
            .expect_err("类型漂移必须被拒绝");
        assert_eq!(refusal.code(), "validation_failed");
    }

    #[test]
    fn a_2020_12_schema_catches_a_type_drift() {
        let prepared = PreparedSchema::new(DRAFT202012_TS, JsonSchemaDraft::Auto).unwrap();
        assert_eq!(prepared.draft(), JsonSchemaDraft::Draft202012);
        let instance = serde_json::json!({ "tab_spaces": "4242" });
        let refusal = prepared
            .validate(&instance)
            .expect_err("类型漂移必须被拒绝");
        assert_eq!(refusal.code(), "validation_failed");
        assert!(refusal.to_string().contains("tab_spaces"));
    }

    #[test]
    fn a_misspelled_keyword_is_reported_instead_of_ignored() {
        // 规范说实现必须忽略不认识的关键字；对门禁来说那等于「约束没生效却放行」
        let schema = r#"{"type": "object", "requierd": ["port"]}"#;
        let refusal =
            PreparedSchema::new(schema, JsonSchemaDraft::Auto).expect_err("拼错的关键字必须报出来");
        assert_eq!(refusal.code(), "unsupported_schema");
        assert!(refusal.to_string().contains("requierd"));
    }

    #[test]
    fn an_unsupported_dialect_is_reported_instead_of_guessed() {
        let schema = r#"{"$schema": "http://json-schema.org/draft-04/schema#", "type": "object"}"#;
        let refusal =
            PreparedSchema::new(schema, JsonSchemaDraft::Auto).expect_err("认不出的方言必须报出来");
        assert_eq!(refusal.code(), "unsupported_schema");
        assert!(refusal.to_string().contains("draft-04"));
    }

    #[test]
    fn a_property_named_like_a_keyword_is_not_reported() {
        // `properties` 下面的键是属性名，不是关键字
        let schema = r#"{
          "type": "object",
          "properties": { "required": { "type": "boolean" }, "type": { "type": "string" } }
        }"#;
        PreparedSchema::new(schema, JsonSchemaDraft::Auto).expect("属性名不该被当成关键字");
    }

    #[test]
    fn an_unknown_format_is_reported() {
        let schema = r#"{"type": "object", "properties": {"v": {"type": "string", "format": "not-a-real-format"}}}"#;
        let refusal = PreparedSchema::new(schema, JsonSchemaDraft::Auto)
            .expect_err("不认识的 format 必须报出来");
        assert_eq!(refusal.code(), "unsupported_schema");
    }

    #[test]
    fn a_known_format_is_really_enforced() {
        // format 默认只是注解；门禁要真的拦，所以显式打开了断言
        let schema = r#"{"type": "object", "properties": {"when": {"type": "string", "format": "date-time"}}}"#;
        let prepared = PreparedSchema::new(schema, JsonSchemaDraft::Auto).unwrap();
        assert!(prepared
            .validate(&serde_json::json!({"when": "2026-01-01T00:00:00Z"}))
            .is_ok());
        assert!(prepared
            .validate(&serde_json::json!({"when": "not a date"}))
            .is_err());
    }

    #[test]
    fn a_schema_that_is_not_json_is_refused() {
        let refusal = PreparedSchema::new("#@schema { }", JsonSchemaDraft::Auto)
            .expect_err("非 JSON schema 必须拒绝");
        assert_eq!(refusal.code(), "parse_failed");
    }

    #[test]
    fn an_unresolvable_schema_is_reported_not_silently_allowed() {
        let document = DocumentSchema::default();
        let refusal = document
            .load("https://json.schemastore.org/tsconfig.json")
            .expect_err("离线时取不到 schema 必须明确报告");
        assert_eq!(refusal.code(), "schema_unavailable");
        assert!(refusal.to_string().contains("url_map"));
    }
}

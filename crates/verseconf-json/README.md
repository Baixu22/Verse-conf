# verseconf-json

> **冻结说明（2026-09-28）**：已停止独立产品开发，不承诺后续发布或持续维护。以下仅为历史源码参考；安全边界与最终决定见 [CLOSEOUT.md](../../docs/CLOSEOUT.md)。

把 VerseConf 的**编辑意图契约**与**写入前校验**用到真实 JSON / JSONC 配置上。

agent 宿主的设置类配置以 JSON/JSONC 为主（`settings.json`、`tsconfig.json`、
各家 IDE 与 MCP 客户端的配置文件），它们几乎都带注释——而带注释的文件恰恰是
「用 `serde_json` 解析再序列化」会毁掉的那一类。所以解析、定位与改写都交给
[`jsonc-parser`](https://crates.io/crates/jsonc-parser)（成熟 CST 库，保留注释、
空白与键序）；这一层只做四件事：

1. 用 `jsonc-parser` 的区间定位目标值在原文里的字节区间；
2. 只替换 / 插入 / 删除那一段，**区间之外一个字节都不动**；
3. 写入前做与 `.vcf` 路径**同源**的双重校验：schema 与安全审计；
4. 失败时返回与 `.vcf` 路径同一套拒绝码（`parse_failed` / `validation_failed` /
   `security_rejected` / `target_not_found` / `target_ambiguous` /
   `target_already_exists` / `expectation_mismatch` / `unsupported_target`）。

## 两个入口

```rust
use verseconf_json::{apply_json_edit_plan, check_write_json, JsonFlavor, JsonGuard};

// 入口一：只裁决改动，不产生改动。
// 候选文本可以由宿主的任意编辑方式产生（字符串替换、diff、整文件重写）：
// 门禁不关心它是怎么来的，只回答「允许落盘吗」。
check_write_json(r#"{"tls_verify": true}"#, r#"{"tls_verify": false}"#)?;
// -> Err(SecurityRejected { findings: ["SEC-005"], instances: ["SEC-005 @ tls_verify"] })

// 入口二：按路径做保真编辑（只动目标值的字节区间）
let path = vec![verseconf_core::PathSegment::Key("port".into())];
let outcome = verseconf_json::set_json_value(
    r#"{"port": 8080}"#,
    &path,
    &verseconf_core::EditValue::Integer(9090),
    JsonFlavor::Json,
)?;
assert_eq!(outcome.source, r#"{"port": 9090}"#);
```

`JsonGuard` 控制这一层：`with_json_schema(text)` 旁挂一份标准 JSON Schema，
`with_schema(text)` 旁挂自建 `#@schema` 文本，`strict()` 切到严格 JSON 语法，
`Default` 是「JSONC 语法 + 安全审计 + 不做 schema 校验」。

## 两种 schema 语言

JSON 没有 `#@schema` 那种内联语法，schema 只能旁挂，而旁挂时用哪种语言由调用方
声明（`SchemaFormat`），不由本层猜：

| | 语言 | 谁在校验 | 支持程度 |
| --- | --- | --- | --- |
| `SchemaFormat::VcfDsl` | `#@schema { ... }`，与 `.vcf` / TOML 同源 | `verseconf-core::SchemaValidator` | 完整 |
| `SchemaFormat::JsonSchema` | 标准 JSON Schema | `jsonschema`（成熟实现） | draft-07 与 2020-12 |

两种都走同一条写入前校验路径、返回同一套拒绝码，所以「写入前双重校验」这句话
对它们同时成立。要门禁能用，使用方**不需要先学一门自建 DSL**——`tsconfig.json`
里本来就有的 `$schema` 就能直接用（见下）。

### 标准 JSON Schema 的三条约定

1. **不静默忽略不认识的写法。** JSON Schema 规范要求实现忽略自己不认识的关键字，
   但对**门禁**那等于「用户写了约束、实际没生效、门禁却说允许落盘」。所以拼错的
   `requierd`、没实现的方言（例如 draft-04）、不认识的 `format` 名、声明了未知必需
   词汇表的 `$vocabulary`，都会返回 `unsupported_schema` 并逐条列出。
   这不是挑刺：扫描按 schema 结构走，`properties` 下面那些键是**属性名**，
   一个叫 `required` 的配置字段不会被误报。
2. **注解与扩展不算「不认识」。** `title`/`description`/`examples`/`deprecated`
   这些注解，以及 `x-` 前缀的扩展（`markdownDescription`、`x-intellij-*`、`tsType`、
   `allowTrailingCommas` 等编辑器扩展）**不约束实例**，忽略它们不会让人以为某个
   约束生效了。把它们和拼错的关键字一起判死，等于门禁对 SchemaStore 上最真实的那批
   配置说「不受支持」——那比不接标准 schema 更糟。同理，两个方言的关键字取**并集**：
   2020-12 文档里写 `definitions`、draft-07 里写 `$defs`，校验器都能解析、`$ref`
   的约束都会被执行，没有理由把它们判死。
3. **`format` 真的校验。** 规范把 `format` 定为注解还是断言取决于方言与词汇表；
   门禁显式打开断言（`date-time`、`uri`、`uri-reference`、`duration`、`uuid`、
   `email`、`hostname`、`ipv4`/`ipv6`、`time`、`regex` 等写坏都会被拦住），
   并且实现不认识的 format 名会被报告而不是静默忽略。

draft-07 与 2020-12 分别按各自的方言执行。schema 自己的 `$schema` 说了算；
调用方声明的方言与它冲突时返回 `unsupported_schema`，不会挑一个偷偷执行。
方言 URI 是精确匹配的（允许结尾 `#`/`/`），不做后缀匹配——凭一个后缀猜方言
与「认不出就报出来」相冲突。

### 按配置里的 `$schema` 取 schema

```rust
use verseconf_json::{check_write_json_with_document_schema, DocumentSchema, JsonGuard};

let document = DocumentSchema::default()
    .map_url("https://json.schemastore.org/tsconfig.json", "vendor/tsconfig.schema.json");
let used = check_write_json_with_document_schema(baseline, candidate, &JsonGuard::default(), &document)?;
```

解析顺序：`url_map` → 本地路径。三种写法都认：相对路径（按 `base_dir` 解析）、
绝对路径、`file://` URL（含标准的 `file:///C:/...` 与带 authority 的
`file://localhost/...`）。工具协议里对应的参数是 `schema_from_config` +
`base_dir` + `schema_url_map`。

**不抓网络**：门禁是毫秒级的确定性检查，不该把 HTTP 客户端拖进依赖树，也不该在
wasm 上凭空长出一张网络能力。想用 SchemaStore 的宿主自己把 schema 下载好、用
`schema_url_map`（库侧是 `DocumentSchema::url_map`）指过来即可。

取不到 schema 时**明确拒绝**，不静默放行：配置里没有 `$schema`、URL 不在映射里、
文件读不到、`$schema` 不是字符串——四种都返回 `schema_unavailable`，并在理由里
写清下一步该干什么。因为「我以为门禁在按 schema 检查」这种误解比一次失败更贵。

schema **文件本身**也按 JSONC 解析：真实 schema 文件常常带 BOM（Windows 编辑器写的）
或注释，用严格 JSON 解会在这些文件上失败，而报出来的 `parse_failed` 会让人以为是
schema 内容写错了。

基线决定用哪份 schema：候选可能正是「把 `$schema` 删掉」的那次改动，那时不该因为
候选里没有声明就把检查降级成放行。

### 明确不做的事

- **不抓 HTTP**（见上）；
- **不解析跨文件 `$ref`**：校验器不带上游的文件解析 feature（它在 wasm32 上是
  明确不支持的，而两条分发路径要共用同一份实现）。schema 里对其它文件的 `$ref`
  会返回 `schema_unavailable` 并说明「请先合并进来」——**不是** `unsupported_schema`：
  前者要调用方去补文件，后者要调用方去改写写法，两者该做的事完全不同。

## `json` 与 `jsonc` 是两个值，不是一个宽容度旋钮

`jsonc-parser` 默认极其宽松，连**缺逗号**都接受（`{"a":1 "b":2}`）。如果
`.json` 也吃这一口，门禁就会对一份不是合法 JSON 的候选回答「允许落盘」，
而宿主写下去的是别的解析器读不了的文件。所以：

| | 注释 | 尾随逗号 | 单引号 | 缺逗号 |
| --- | --- | --- | --- | --- |
| `JsonFlavor::Json` | 拒绝 | 拒绝 | 拒绝 | 拒绝 |
| `JsonFlavor::Jsonc` | 接受 | 接受 | 接受 | **拒绝** |

宽严由调用方声明，不由本层猜：猜错格式会把一份合法配置报成 `parse_failed`，
而门禁的拒绝必须可归因。

## BOM

Windows 上的编辑器会写出带 BOM 的 UTF-8，而 `jsonc-parser` 不认 BOM。直接把它
交给解析器会让门禁在这类真实配置上不可用（`Unexpected token on line 1 column 1`），
所以本层按 RFC 8259 的「实现可以忽略 BOM」忽略它：解析用去掉 BOM 的文本，
区间再按去掉的字节数平移回原文。区间必须平移，否则每次编辑整体错位一个字节——
那比拒绝更糟。BOM 只是编码签名，所以它也不会让安全审计失效。

## 为什么不用「`serde_json` 解析 → 改 → `to_string()`」

那条路会**重新序列化整份文档**：注释全部消失，键序由 `Map` 的实现决定，
缩进与空行被统一成一种风格。对一份 200 行的 `settings.json` 来说，改一个端口
会产出几百行 diff，而用户真正想看到的是「一个值变了」。

更大的问题是它把「保真」变成了「尽量重建」：只要重建逻辑漏掉一种写法
（单引号、尾随逗号、注释类型），用户就会在无声无息中丢掉注释。这里是
**不做重建**——改动之外一个字节都不动，所以没有「漏重建」这种事。
`tests/fidelity.rs` 逐字节断言了这一点：注释、键序、缩进、CRLF、尾随逗号
在编辑之后与编辑之前完全相同。

## 明确不支持的形状

- **路径穿过标量**（`a.b` 而 `a` 是数字）：返回 `unsupported_target`，不是
  `target_not_found`——把「结构不对」与「键不存在」混成一个码，调用方就分不清
  「换个路径」和「先补结构」；
- **按下标取值**：点号路径没有表达下标的段。`PathSegment::Named` 是「按字段
  匹配数组元素」，能表达 `servers[match name = "b"].port`，不能表达 `servers[0].port`；
- **最后一条属性行尾有注释时的 `insert`**：逗号必须插在注释**之前**，那需要两次
  互不相邻的拼接。与其产出一份 `"b": 2 // note,`（逗号被注释吃掉、JSON 变成非法），
  不如明确拒绝并说明原因；
- **同一路径命中重复键**：JSONC 语法允许 `{"a":1,"a":2}`（后者覆盖前者），
  而「改第一个」会让用户以为改的是生效的那个，所以返回 `target_ambiguous`；
- **顶层不是对象**：门禁要检查的是一份配置，静默返回空根表会让它对一份完全
  没被检查过的文本回答 `allowed`；
- **候选是空文档或只有注释**：JSON 要求有顶层值，所以它不是合法文档，拒绝。

## 已知边界（门禁不承诺的事）

候选必须**仍然是合法文档**，且**没有引入新的高危实例**。这两条之外的事门禁
不管，写在明处免得被当成承诺：

- 合法但破坏性的改动仍然放行。`{"tls_verify": true, "port": 8080}` → `{}` 是
  合法 JSON、也不引入新的风险实例，所以它通过。要拦这类改动得靠版本控制或
  备份，不是门禁的职责；
- `tls_verify: true` → `tls_verify: 0` 通过。SEC-005 只认布尔/字符串的 `false`，
  而这条规则住在共享的 `verseconf-core` 里、对 `.vcf` / TOML / JSON 一视同仁；
  放宽它会带来误拒风险，属于另一个决定，不在本层偷偷改。对 JS/Node 宿主
  `0` 是假值，所以这是一条真实的语义缺口，已登记在案；
- **重复键上两个入口的口径不同**：编辑路径对 `{"a":1,"a":2}` 返回
  `target_ambiguous`（改哪一个没有定义），而门禁按**文本实例**计数，所以
  `{"tls_verify": true}` → `{"tls_verify": false, "tls_verify": true}` 会被
  判 `security_rejected`，哪怕「后者覆盖前者」的解析器读到的有效值是安全的 `true`。
  门禁取 fail-closed：解析器之间对重复键的取舍并不一致，「有效值」不是一个
  可以跨实现共享的事实。

## 安全审计递归到的深度

数组里的数组、数组元素里的表、内联表都被审计（位置标识形如
`list[0][1].tls_verify`）。这一条曾经不成立：`verseconf-core` 的审计对
`Value::Array` 里的 `Value::Array` 和内联表落进 `_ => {}`，于是
`{"list": [[{"tls_verify": false}]]}` 会被判 allowed——门禁对一份**看起来
检查过**的候选回答通过，而这比报错更危险。修复在同一次改动里落进
`verseconf-core`（`audit_array` / `audit_inline_entries`），因此 `.vcf` 与
TOML 路径也一并受益。

## 与 TOML 侧的关系

`verseconf-toml` 与 `verseconf-json` 不是两套规则，而是同一份规则的两个适配层：
schema 语言、安全审计、实例级比较（规则 + 位置）、拒绝码、`finalize` 的检查顺序
全部共用 `verseconf-core` 的实现。两套规则一旦分叉，「写入前双重校验」这句话
就不再对三种格式同时成立——而项目真正要卖的就是这句话。

# verseconf-toml

> **冻结说明（2026-09-28）**：已停止独立产品开发，不承诺后续发布或持续维护。以下仅为历史源码参考；安全边界与最终决定见 [CLOSEOUT.md](../../docs/CLOSEOUT.md)。

把 VerseConf 的**编辑意图契约**与**写入前校验**用到真实 TOML 配置上。

解析、定位与渲染都交给 [`toml_edit`](https://crates.io/crates/toml_edit)（成熟 CST 库，保留注释、空白与键序）；
这一层只做四件事：

1. 用 `span()` 定位目标值在原文里的字节区间；
2. 只替换那个区间，区间之外一个字节都不动；
3. 写入前做与 `.vcf` 路径**同源**的双重校验：schema 与安全审计；
4. 失败时返回与 `.vcf` 路径同一套拒绝码。

## 两个入口

```rust
use verseconf_toml::{apply_toml_edit, check_write_toml, TomlGuard};

// 入口一：给编辑意图，由本层做最小改动
let outcome = apply_toml_edit("port = 8080\n", &plan)?;
let new_text = outcome.source;

// 入口二：只裁决改动，不产生改动
// 候选文本可以由宿主的任意编辑方式产生（字符串替换、diff、整文件重写）
check_write_toml("tls_verify = true\n", &candidate)?;
```

`check_write_toml` 是**与编辑机制解耦**的门禁：它回答「这次改动允许落盘吗」，
不关心候选是怎么来的。因此宿主可以保留自己的编辑方式，只在写盘前过这一道，
不需要让模型理解编辑计划协议。

`TomlGuard` 控制这一层：`with_schema(text)` 旁挂一份 schema，
`Default` 只做结构校验与安全审计。schema 必须旁挂——TOML 没有 `#@schema` 那种内联语法，
schema 文本用的是与 `.vcf` **同一套**词汇表（type / required / range / enum / strict / 嵌套字段）。

## 为什么不用「解析成 DocumentMut → 改 → to_string()」

那条路会**重新序列化整份文档**。实测 `toml_edit` 0.22 的往返对 LF 文件无损，
但对 CRLF 文件会把 65 个 CRLF 里的 49 个变成 LF（1461 → 1413 字节），**即使一次都不修改**。
所以这里改用 span 级替换：结果文本一个字节都不经过序列化器。

## 支持的形状

- **`set`**：只替换目标值的字节区间。内联表、数组、多行字符串整体算一个值。
  整张 `[table]` 不是值，用标量覆盖它属于重写结构，明确拒绝（`unsupported_target`）。
- **`insert`**：在目标表里新增一条 `key = value`，插入点落在该表最后一条直接键值之后；
  缩进与换行符沿用文件自身的风格。
- **`delete`**：整条删除键值对，范围从键所在行的行首到值所在行的行尾。
- **路径**：普通键与 `[[key]]` 元素的按字段匹配定位都支持；匹配不中报 `target_not_found`，
  匹配到多个报 `target_ambiguous`，不猜一个元素改。

## 明确不支持

- 往内联表里 `insert` / `delete`（内联表里没有「一行」可插，重排要处理逗号）；
- 删除整张表；
- 路径最后一段是 `[[key]]` 元素定位（那是在说「把整个元素换成一个值」，没有明确定义）。

## 与 `.vcf` 路径的关系

两条路径**共用同一份** `AuditEngine`、`SchemaValidator` 与实例级比较实现
（`high_risk_instances` / `introduced_high_risk_instances`）。
各写一套的话，「写入前双重校验」这句话迟早只对其中一种格式成立。

安全审计只拒绝**本次改动新引入**的高危实例（按规则 + 位置比较），
文件本来就有的问题不会让这次改动背锅。

## 许可

MIT OR Apache-2.0

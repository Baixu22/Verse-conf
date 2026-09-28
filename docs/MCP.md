# VerseConf 工具协议服务（MCP 风格）

> **冻结说明（2026-09-28）**：已停止独立产品开发，不承诺后续发布或持续维护。以下仅为历史源码参考；安全边界与最终决定见 [CLOSEOUT.md](CLOSEOUT.md)。

`verseconf-mcp` 把 VerseConf 的五个能力暴露成 Agent 宿主可直接发现与调用的工具。
**只有 `verseconf_check_write` 接受多格式输入**（VCF / TOML / JSON / JSONC）；
其余四个旧工具仍只接受 VCF。JSON / JSONC 能力仅在当前源码中，未发布，不能据此推断已发布包的能力。

## 为什么是工具而不是格式

项目定位是「Agent 编辑配置的确定性执行层」。模型只输出结构化意图
（改哪个字段、改成什么、为什么改），由确定性代码定位字符区间并替换。
这条边界让改动可被人审查，也让失败可以被明确拒绝而不是猜测。

## 启动方式

### 形态一：本机二进制

宿主以子进程方式拉起，逐行 JSON-RPC 走 stdio：

```bash
cargo build -p verseconf-mcp --release
./target/release/verseconf-mcp          # stdio 会话
./target/release/verseconf-mcp --list-tools
./target/release/verseconf-mcp --call verseconf_validate '{"source":"port = 8080\n"}'
```

宿主接入配置（以通用 MCP stdio 客户端为例）：

```json
{
  "mcpServers": {
    "verseconf": {
      "command": "/absolute/path/to/verseconf-mcp",
      "args": []
    }
  }
}
```

### 形态二：WebAssembly（宿主无需 Rust 工具链）

`integrations/verseconf-wasm` 把**同一套工具实现**编译成 WebAssembly，
再由 `integrations/verseconf-wasm/js-api` 构建成可分发的包。宿主只需要一个
JS 运行时（Agent 宿主本来就带），不需要 Rust 工具链，也不需要预编译的本机二进制。

**这条路径没有对外发布**：npm 包已放弃发布（发布账号不可用），所以要从源码构建：

```bash
cd integrations/verseconf-wasm/js-api && npm install && npm run build
node bin/verseconf-mcp-wasm.mjs --list-tools
node bin/verseconf-mcp-wasm.mjs --call verseconf_validate '{"source":"port = 8080\n"}'
node bin/verseconf-mcp-wasm.mjs            # 逐行 JSON-RPC over stdio
```

宿主接入配置（先完成本地构建，将示意绝对路径替换为自己的构建目录）：

```json
{
  "mcpServers": {
    "verseconf": {
      "command": "node",
      "args": ["/absolute/path/to/verse-conf/integrations/verseconf-wasm/js-api/bin/verseconf-mcp-wasm.mjs"]
    }
  }
}
```

两条分发路径共用 `verseconf-mcp` 的同一批函数（`tools_list_value` /
`tool_result_value`），不是两份实现。因此同一串请求喂给两边，逐行响应**逐字节相同**：

```bash
cd integrations/verseconf-wasm/js-api
npm run build
npm run test:parity     # 与本机 verseconf（CLI）/ verseconf-mcp 逐项比对
```

`test:parity` 会检查：仓库全部示例的 `validate` 退出码与 wasm `valid` 一致、
`audit --format json` 的 summary 与 rule_id 集合一致，以及 13 条 JSON-RPC 请求
（握手、工具发现、五个工具的成败路径、未知方法、协议错误）的响应逐字节相同。

浏览器与打包器使用 `pkg-web/`（wasm-bindgen 的 web 目标）：

```js
import init, { call_tool_json } from 'verseconf/web';
await init();
JSON.parse(call_tool_json('verseconf_audit', JSON.stringify({ source })));
```

## 协议

| 方法 | 作用 |
| --- | --- |
| `initialize` | 握手，返回 `capabilities.tools` 与 `serverInfo` |
| `notifications/initialized` | 通知，无响应 |
| `tools/list` | 返回五个工具的名称、描述与 `inputSchema` |
| `tools/call` | 调用工具，参数 `{ name, arguments }` |
| `ping` | 连通性检查 |

## 五个工具

### `verseconf_validate`

输入 `{ "source": string, "strict"?: boolean }`，输出
`{ "valid": boolean, "diagnostics": [{ code, message, line, column }] }`。
`strict: true` 时即使 schema 未声明 `strict`，也拒绝未声明字段。

### `verseconf_audit`

输入 `{ "source": string }`，输出
`{ "findings": [{ rule_id, severity, category, title, description, location, recommendation }], "summary": {...} }`。
规则覆盖敏感数据、弱加密算法、不安全端口、调试开关、通配绑定与关闭的证书校验。

### `verseconf_apply_edit`

输入 `{ "source": string, "plan": object }`（单文件）或 `{ "path": string, "plan": object }`
（在 `@include` 图里定位目标文件），输出
`{ "source": string, "applied": [{ op, path, before, after, reason }] }`；
给 `path` 时还返回 `file`（被改动的那个文件）与 `effective_view`——它是 `validated`
或 `not_validated`，后者带 `effective_view_note` 说明为什么重建不可信。

`source` 与 `path` 的二选一写在 `inputSchema.oneOf` 里，不是只写在描述里。
`include_source`（默认 `true`）决定响应是否回传改动后的完整文本：响应会进入模型上下文，
只想看改动时设为 `false`，此时响应不含 `source`，改由 `source_omitted` 与 `result_bytes`
说明结果。注意 `source` 模式下调用方没有别的渠道拿到结果，把它设成 `false` 通常只会
拿到一份用不了的结果。

`plan` 的完整 JSON Schema 直接内联在 `tools/list` 的 `inputSchema.properties.plan` 里
（内容就是下面这个契约），模型不需要额外提示，也不需要去猜结构。

`plan` 是编辑意图契约（见 [`crates/verseconf-core/schemas/edit-plan.schema.json`](../crates/verseconf-core/schemas/edit-plan.schema.json)）：

```json
{
  "version": "1.0",
  "edits": [
    {
      "op": "set",
      "path": ["server", "port"],
      "value": 9090,
      "reason": "端口冲突",
      "expect": { "value": 8080 }
    }
  ]
}
```

要点：

- **列表按名称定位，不按下标**：
  `"path": [{ "key": "servers", "match": { "name": "primary" } }, "ip"]`。
  命中多个元素会被拒绝，而不是挑第一个。
- **只替换目标值的字节区间**，其余字节零变化：注释、`#@` 元数据、键序、
  空行、缩进风格、CRLF 都保持原样。
- **`expect` 是前置条件**：当前值与预期不符时拒绝，避免基于过期假设改动。
- **写入前双重校验**：改动后必须仍能解析、仍通过结构/schema 校验，
  且没有引入新的高危安全**实例**。实例按「规则码 + 位置」识别：文件里本来就有
  `primary_ssl_verify = false` 时，再关闭 `secondary_ssl_verify` 属于新增实例，
  仍会被拒绝。
- **合并后的生效配置也要合法**：跨文件编辑会把候选内容放回 `@include` 图重建
  合并视图并再校验一次。重建不可信时返回 `effective_view: "not_validated"`，
  宿主不得把它当成「安全通过」。
- **搜索不完整就拒绝**：`include` 图超过搜索上限时返回 `search_incomplete`，
  而不是对已扫描的子集宣称目标唯一。

### `verseconf_edit_range`

输入 `{ "source": string, "start": integer, "end": integer, "replacement": string }`，
输出 `{ "source": string, "replaced": { start, end, before, after } }`。

这是给编辑意图契约无法表达的改动准备的底层原语，走完全相同的写入前校验。
越界、切断多字节字符或改动后不再合法时拒绝。

### `verseconf_check_write`

输入：

```json
{
  "baseline": "string", "candidate": "string",
  "format"?: "vcf" | "toml" | "json" | "jsonc",
  "schema"?: "string",
  "schema_format"?: "vcf" | "json-schema",
  "schema_draft"?: "auto" | "draft-07" | "2020-12",
  "schema_from_config"?: true,
  "base_dir"?: "string",
  "schema_url_map"?: { "<$schema URL>": "<本地 schema 文件路径>" },
  "allow_unresolved_schema"?: false
}
```

通过时输出 `{ "allowed": true, "format": string, "baseline_bytes": integer, "candidate_bytes": integer, "schema": string, "schema_source": string | null, "schema_skipped": string | null, "schema_declared_unused": string | null }`；
拒绝时按失败模型返回与编辑路径同一套稳定错误码。

工具契约声明了 `additionalProperties: false`，**并且真的执行**：传了未声明的参数
（例如把 `schema_format` 写成 `schemaFormat`）会返回 `invalid_arguments` 并列出可用参数。
声明与行为不一致比不声明更糟——宿主会以为拼错的参数会被指出来，而工具其实静默按
默认值跑了一遍，结果看起来像「检查过了」。

**它不产生改动，只裁决改动。** `candidate` 是怎么来的与它无关——字符串替换、
字符区间替换、整文件重写都可以，宿主因此不需要改用自己的编辑方式，
模型也不需要理解编辑计划协议（成本侧不增加 token）。

`baseline` 用于建立风险基线；只拒绝本次改动**新引入**的高危安全实例
（按规则 + 位置比较），文件本来就有的问题不会让这次改动背锅。
`candidate` 无法解析、破坏 schema 或引入新高危实例时拒绝。
不给 `schema` 时只用 `candidate` 自己声明的 `#@schema`，与编辑路径口径一致。

审计会递归到嵌套容器：数组、数组里的数组、数组元素里的表、内联表都算，
位置标识形如 `list[0][1].tls_verify`。候选必须是一份合法文档，所以空文档或
只有注释的候选会被拒绝（JSON 要求有顶层值）；但合法且只是「被删空」的文档
（如 `{}`）仍然放行——门禁保证的是「仍然合法且没引入新的高危实例」，
不承诺拦住破坏性改动。

`format` 默认 `vcf`，**由调用方声明而不是由本层猜**：猜错格式会把一份合法配置
报成 `parse_failed`，而拒绝必须是可归因的。`format: "toml"` 走 TOML 适配层，
`format: "json"` / `"jsonc"` 走 JSON 适配层，校验与审计都与 `.vcf` 路径**同源**
（同一份 `AuditEngine`、同一份实例级比较、同一套拒绝码）。TOML 与 JSON 都没有
内联 schema 语法，所以它们的 schema 只能旁挂传入。未知的 `format` 返回
`invalid_arguments`，不会静默退回默认值。

`json` 与 `jsonc` 的区别是**门禁拒绝什么**，不是一个宽容度旋钮：

| `format` | 注释 | 尾随逗号 | 单引号 | 缺逗号 |
| --- | --- | --- | --- | --- |
| `json` | 拒绝 | 拒绝 | 拒绝 | 拒绝 |
| `jsonc` | 接受 | 接受 | 接受 | **拒绝** |

缺逗号在两种语法下都拒绝：`{"a":1 "b":2}` 会让两条键悄悄粘成一个语义不同的
文档，接受它等于门禁对一份坏配置说 `allowed`。因为 `settings.json` 这类文件
普遍带注释，按 `.json` 后缀直接判严格语法会把合法配置报成 `parse_failed`，
所以后缀推断（`JsonFlavor::from_path`）只在调用方没有显式给 `format` 时使用。

#### schema：两种语言，以及按配置自己的 `$schema` 取

`schema_format` 决定旁挂的 `schema` 用哪种语言，默认 `vcf`：

| `schema_format` | 语言 | 谁在校验 | 支持程度 |
| --- | --- | --- | --- |
| `vcf`（默认） | `#@schema { ... }`，与 `.vcf` / TOML 同源 | `verseconf-core` | 完整 |
| `json-schema` | 标准 JSON Schema | `jsonschema`（成熟实现） | draft-07 与 2020-12 |

**不必先学一门自建 DSL**：使用方本来就有的 `tsconfig.json` / `settings.json`
里的 `$schema` 可以直接用。`schema_format: "json-schema"` 只对
`format: "json" | "jsonc"` 生效——对 `.vcf` / TOML 用它返回 `invalid_arguments`，
而不是悄悄按另一门语言解释。`schema_draft` 默认 `auto`（按 schema 自己的
`$schema` 认，没有声明时按 2020-12）。

标准 JSON Schema 路径下有几条与规范不同的**有意**行为：

- 不认识的关键字（例如拼错的 `requierd`）返回 `unsupported_schema` 并逐条列出。
  规范要求实现忽略未知关键字，但对门禁那等于「约束没生效、门禁却说允许落盘」；
- **注解与扩展不算「不认识」**：`title`/`description`/`examples`/`deprecated`、
  `x-` 前缀的扩展，以及 `markdownDescription` / `x-intellij-*` / `tsType` /
  `allowTrailingCommas` 这类编辑器扩展都不约束实例，直接放行。把它们和拼错的关键字
  一起判死，等于门禁对 SchemaStore 上最真实的那批配置说「不受支持」；
- 两个方言的关键字取**并集**：2020-12 文档里写 `definitions`、draft-07 里写 `$defs`，
  校验器都能解析、`$ref` 的约束都会被执行，没有理由判死；
- 不支持的方言（例如 draft-04）、调用方指定方言与 schema 自身声明的冲突、
  未知的必需 `$vocabulary` 仍然返回 `unsupported_schema`。

`format` 在这个路径上是**真的**被校验的（规范把 `format` 定为注解还是断言取决于
方言，门禁显式打开了断言）：`date-time`/`uri`/`uri-reference`/`duration`/`uuid`
等标准 format 写坏都会被拦住，实现不认识的 format 名会被报告。

`schema_from_config: true` 时不使用 `schema` 参数，而是按配置里顶层 `$schema`
的声明去取 schema：

- 支持本地相对路径（按 `base_dir`）、绝对路径与 `file://` URL（含标准的
  `file:///C:/...` 与带 authority 的写法）；**不抓网络**；
- 想用 SchemaStore 的宿主自己把 schema 下载好，用 `schema_url_map` 把 URL 映射到
  本地文件即可。未命中的 URL 返回 `schema_unavailable`，不静默放行；
- 取不到时返回 `schema_unavailable`：配置里没有 `$schema`、路径读不到、
  `$schema` 不是字符串，理由里都会写清下一步该做什么；
- 基线决定用哪份 schema——候选可能正是「把 `$schema` 删掉」的那次改动，
  那时不该把检查降级成放行；
- 配置自己声明了 `$schema` 而这次调用没要求按它校验时，结果里用
  `schema_declared_unused` 如实报出来（此时 `schema` 是 `"none"`），
  免得读结果的人以为 schema 检查过。

离线场景的降级是**显式**的：`allow_unresolved_schema: true` 时，取不到 schema
不再拒绝，而是跳过 schema 那一层、照旧做结构与安全审计，并在结果里如实报告
（`schema: "skipped"` + `schema_skipped: "<原因>"`，摘要里也会写「schema **未检查**」）。
默认 `false`——门禁最不该做的事是「没能检查却说允许」，所以降级必须由调用方选择，
而不是门禁自己决定。

`schema_from_config` 只对 `format: "json" | "jsonc"` 生效；与 `schema` 同时给
返回 `invalid_arguments`（一个是旁挂文本，一个是按配置取，不能都要）。

## 失败模型

工具执行失败**不是**协议错误：结果里 `isError: true`，并附带稳定的结构化原因：

```json
{
  "content": [{ "type": "text", "text": "目标不存在：server.port" }],
  "structuredContent": {
    "code": "target_not_found",
    "message": "目标不存在：server.port",
    "details": { "path": "server.port" }
  },
  "isError": true
}
```

稳定错误码：

| 码 | 含义 |
| --- | --- |
| `invalid_arguments` | 工具参数缺失或类型不对 |
| `unknown_tool` | 工具名不存在 |
| `invalid_plan` | 编辑计划不满足契约（含 `violations` 明细） |
| `parse_failed` | 源码无法解析 |
| `target_not_found` | 目标字段不存在 |
| `search_incomplete` | `@include` 图超过搜索上限，无法证明目标唯一（含 `files` 与 `limit`） |
| `target_ambiguous` | 命名列表命中多个元素 |
| `target_already_exists` | `insert` 的目标已存在 |
| `expectation_mismatch` | 当前值与 `expect` 不符 |
| `unsupported_target` | 目标结构不支持该操作（含区间越界） |
| `unsupported_on_platform` | 该平台做不到这个调用（wasm 目标没有文件系统，不接受 `path`） |
| `validation_failed` | 改动后未通过结构或 schema 校验 |
| `security_rejected` | 改动引入了新的高危安全问题（`findings` 为去重后的规则码，`instances` 为 `规则 @ 位置`） |

协议层问题（未知方法、JSON 解析失败、缺少工具名）使用 JSON-RPC `error`
对象，`data.code` 分别为 `method_not_found` / `parse_error` / `invalid_params`。

## 保证

- **不写文件**：所有编辑工具只返回改动后的文本，落盘由宿主决定。
  CLI 的 `verseconf edit --write` 会落盘：它在写入前重新读取文件、内容与编辑时
  依据的不一致就拒绝（避免覆盖他人修改），并用「同目录临时文件 + 改名」做原子替换；
  合并视图未校验时默认拒绝写入，需显式传 `--allow-unvalidated`。
- **平台能力不自动继承**：wasm 目标与原生二进制共用同一份工具实现，但 wasm 没有文件系统。
  `tools/list` 在 wasm 上不声明 `path`，真的传了会返回 `unsupported_on_platform`；
  宿主需要自己在宿主侧读文件，再把内容作为 `source` 传进来。
- **失败即拒绝**：任何无法唯一定位、前置条件不符或校验不通过的情况都返回 `Err`，
  绝不猜测。
- **确定性**：同一份源码加同一份计划，结果完全一致，与调用者是谁无关。

# VerseConf 工具协议服务（MCP 风格）

`verseconf-mcp` 把 VerseConf 的四个能力暴露成 Agent 宿主可直接发现与调用的工具。
宿主不需要更换配置格式，也不需要理解 VerseConf 语法细节。

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

宿主接入配置：

```json
{
  "mcpServers": {
    "verseconf": {
      "command": "npx",
      "args": ["-y", "verseconf-mcp-wasm"]
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
（握手、工具发现、四个工具的成败路径、未知方法、协议错误）的响应逐字节相同。

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
| `tools/list` | 返回四个工具的名称、描述与 `inputSchema` |
| `tools/call` | 调用工具，参数 `{ name, arguments }` |
| `ping` | 连通性检查 |

## 四个工具

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

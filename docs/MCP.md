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

### 形态二：WebAssembly（宿主零安装）

`integrations/verseconf-wasm` 把**同一套工具实现**编译成 WebAssembly，
再由 `integrations/verseconf-wasm/js-api` 包成 npm 包。宿主只需要一个
JS 运行时（Agent 宿主本来就带），不需要 Rust 工具链，也不需要预编译的本机二进制：

```bash
npm install verseconf
npx verseconf-mcp-wasm --list-tools
npx verseconf-mcp-wasm --call verseconf_validate '{"source":"port = 8080\n"}'
npx verseconf-mcp-wasm            # 逐行 JSON-RPC over stdio
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

输入 `{ "source": string, "plan": object }`，输出
`{ "source": string, "applied": [{ op, path, before, after, reason }] }`。

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
  且没有引入新的高危安全项。

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
| `target_ambiguous` | 命名列表命中多个元素 |
| `target_already_exists` | `insert` 的目标已存在 |
| `expectation_mismatch` | 当前值与 `expect` 不符 |
| `unsupported_target` | 目标结构不支持该操作（含区间越界） |
| `validation_failed` | 改动后未通过结构或 schema 校验 |
| `security_rejected` | 改动引入了新的高危安全问题（含 `findings`） |

协议层问题（未知方法、JSON 解析失败、缺少工具名）使用 JSON-RPC `error`
对象，`data.code` 分别为 `method_not_found` / `parse_error` / `invalid_params`。

## 保证

- **不写文件**：所有编辑工具只返回改动后的文本，落盘由宿主决定。
- **失败即拒绝**：任何无法唯一定位、前置条件不符或校验不通过的情况都返回 `Err`，
  绝不猜测。
- **确定性**：同一份源码加同一份计划，结果完全一致，与调用者是谁无关。

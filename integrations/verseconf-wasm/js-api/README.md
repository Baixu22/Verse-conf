# verseconf（JavaScript / WebAssembly 分发）

VerseConf 的 JavaScript 包。它加载的是与本机二进制 `verseconf-mcp` **完全相同**的
Rust 实现编译出的 WebAssembly 模块，因此：

- 宿主不需要 Rust 工具链，也不需要预编译的本机二进制；
- 同一份输入在「wasm 分发」与「本机二进制 / 命令行」上返回**完全相同的 JSON**。

## 安装

**这个包没有发布到 npm，也不会再发布**：发布账号已不可用，这条分发路径已放弃
（见仓库根 README 的安装状态表）。要用它请从本仓库构建：

```bash
cd integrations/verseconf-wasm/js-api
npm install
npm run build      # 产物：dist/（CJS + ESM + 类型声明）、pkg/ 与 pkg-web/（wasm）
```

Node.js 18 及以上。包内自带 wasm，没有运行时依赖，构建过程只编译 wasm 与 TypeScript。

## 两种引入方式都可用

```js
// CommonJS
const { validate, audit, applyEdit, editRange } = require('verseconf');
```

```js
// ESM
import { validate, audit, applyEdit, editRange } from 'verseconf';
```

## 五个能力

所有函数返回同一个信封形状：`{ content, structuredContent, isError }`。
失败不是异常，而是 `isError: true` 加稳定的结构化原因。

```js
import { validate, audit, applyEdit, editRange } from 'verseconf';

validate('port = 8080\n');
// { isError: false, structuredContent: { valid: true, diagnostics: [] }, content: [...] }

audit('db_password = "secret"\nhost = "0.0.0.0"\n');
// structuredContent.findings -> [{ rule_id: 'SEC-SENS-001', ... }, { rule_id: 'SEC-004', ... }]

applyEdit('server {\n  port = 8080 #@ range(1..65535)\n}\n', {
  version: '1.0',
  edits: [{ op: 'set', path: ['server', 'port'], value: 9090, expect: { value: 8080 } }],
});
// structuredContent.source -> 只有 port 那一行变了，注释与 #@ 元数据原样保留

editRange('port = 8080\n', 7, 11, '9090');
// structuredContent.source -> 'port = 9090\n'
```

编辑工具**只返回改动后的文本，不写文件**，落盘由宿主决定。

工具清单与输入契约可以运行时读取：

```js
import { tools, serverInfo } from 'verseconf';

tools();        // 五个工具的名称、描述与 inputSchema
serverInfo();   // { name: 'verseconf', version: '0.1.0', protocolVersion: '2024-11-05' }
```

## 工具协议服务端（stdio）

包内提供一个 stdio 服务端，用法与本机 `verseconf-mcp` 一致。**它没有发布到 npm**，
所以先按上面的「安装」一节从源码构建，再直接跑构建产物：

```bash
node bin/verseconf-mcp-wasm.mjs --list-tools
node bin/verseconf-mcp-wasm.mjs --call verseconf_validate '{"source":"port = 8080\n"}'
node bin/verseconf-mcp-wasm.mjs            # 逐行 JSON-RPC over stdio
```

宿主接入配置（只需要 Node，不需要任何本机二进制）：

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

也可以在自己的进程里直接驱动这套会话：

```js
import { McpSession } from 'verseconf';

const session = new McpSession();
session.handleLine('{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}');
session.handleLine('{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}');
```

## 浏览器 / 打包器

`pkg-web/` 是 wasm-bindgen 的 web 目标产物，供 `<script type="module">` 或打包器使用：

```js
import init, { call_tool_json, tools_json } from 'verseconf/web';

await init();
JSON.parse(call_tool_json('verseconf_validate', JSON.stringify({ source: 'port = 8080\n' })));
```

## 值访问 API

```js
import { VerseConf, parseConfig, parseJson } from 'verseconf';

const conf = new VerseConf('app {\n  name = "demo"\n  port = 8080\n}\n');
conf.getString('app.name');   // 'demo'
conf.getNumber('app.port');   // 8080
conf.hasKey('app');           // true
conf.toJson();                // '{"app": {"name": "demo", "port": 8080}}'

parseJson('port = 8080\n');   // { port: 8080 }
```

## 版本号说明

- `package.json` 的版本号（本分发层）与 `serverInfo().version` / `getVersion()`
  （内嵌的 Rust 核心）是两个独立版本。0.1.0 的包在真实消费者环境里两条入口都不可用，
  因此本分发层从 0.2.0 重新起算；核心仍是 0.1.0。
- 0.2.0 已在仓库里构建并通过打包验收（`test/package.test.mjs`，12/12，CI 阻塞），
  但**不会发布到 registry**——发布账号已不可用。

## 从源码构建

```bash
npm install
npm run build      # wasm-pack（nodejs + web 两个目标）→ tsc（ESM + CJS）
npm test           # 加载冒烟测试 + 发布包安装测试
npm run test:parity  # 与命令行 / 本机二进制逐项比对（需要先构建 Rust 侧）
```

## 许可证

MIT OR Apache-2.0

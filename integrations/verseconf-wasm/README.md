# integrations/verseconf-wasm

VerseConf 的 WebAssembly 分发层。**这里只有一条被支持的分发路径。**

## 目录

| 路径 | 说明 |
|------|------|
| `src/` | Rust 侧实现：wasm-bindgen 的 `VerseConf`，以及复用 `verseconf-mcp` 工具层的 `WasmMcpServer` |
| `js-api/` | npm 包 `verseconf` 的源码、构建脚本与测试（`pkg/`、`pkg-web/`、`dist/` 均为构建产物） |
| `test.html` | 浏览器目标的冒烟页面 |

## 已删除：python / ruby / go / java 四套绑定

这四套绑定在本目录下存在过，**已按任务森林 TF-0033 的决定整体删除**，不再重建。
原因不是"暂时没做完"，而是它们的实现方式从根上不成立：

它们都对着**一套手写的 C-ABI** 编程——调用 `alloc` / `deallocate` /
`get_last_error` / `get_array` / `get_object` / `keys` 这些导出。
而 `verseconf-wasm` 从来没有导出过这些符号：它用的是 wasm-bindgen 的接口
（`verseconf_new` / `verseconf_get_string` / … / `verseconf_to_json`）。
也就是说这四套绑定**从来没有可能运行过一次**。

各自的额外硬伤（删除前的实测状态）：

- **python**：`pyproject.toml` 的内容是 **JSON 而不是 TOML**（首字符 `{`），
  且 `Engine` 被使用却从未 import。
- **go**：`import "wasmtime"` 是裸模块名（go.mod 里要求的是
  `github.com/bytecodealliance/wasmtime-go`），`conf.Close()` 未定义 → **编译不过**。
- **java**：8 个 `native` 方法，全仓**零 JNI**；两个"测试"放在 `src/main/java` 下
  （`mvn test` 永不执行），且只要 HTTP 200 就打印 `[PASS]` —— **假阳性测试**。
- **ruby**：依赖名 `wasmtime-ruby` 有误（真名 `wasmtime`），gemspec 不打包 `.wasm`。

保留它们等于让仓库持续声称拥有四套并不存在的集成。如果将来确实需要非 JS 宿主，
正确做法是**从 wasm-bindgen 的接口出发重新生成绑定**（或用 `wasm-bindgen-cli`
为目标语言生成胶水），而不是恢复这些文件。

## 唯一被支持的两条目标

| 目标 | 构建命令 | 产物 | 面向 |
|------|----------|------|------|
| nodejs | `wasm-pack build --target nodejs --out-dir js-api/pkg` | `js-api/pkg/` | Node、Electron、VS Code 扩展宿主 |
| web | `wasm-pack build --target web --out-dir js-api/pkg-web` | `js-api/pkg-web/` | 浏览器、打包器（`init()` / `initSync`） |

wasm 侧**不重新实现任何能力**：`src/mcp.rs` 直接复用 `verseconf-mcp` 的工具层，
所以本机二进制与 wasm 模块返回逐字节相同的 JSON。一致性由
`js-api/test/parity.mjs` 断言，并在 CI 的 `wasm-distribution` 作业里阻塞。

## 发布状态

npm 包 **0.2.0 已在本地构建并通过打包验收，尚未发布**。
registry 上现有的 `0.1.0` 不可用（包内缺 `pkg/` 目录，`require` 与 `import`
两条入口都失败）——详见仓库根 README 的安装状态表。

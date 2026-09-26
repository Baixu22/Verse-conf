# verseconf-mcp

**简体中文** | [English](README.en.md)

把 VerseConf 的校验、审计与确定性编辑能力，以**工具协议**（Model Context Protocol 风格）
暴露给 Agent 宿主。

宿主不需要更换配置格式，也不需要理解 VerseConf 语法细节——它只需要调用工具。

## 为什么是工具，而不是一种新格式

模型只输出结构化意图（改哪个字段、改成什么、为什么改），由确定性代码定位字符区间并替换。
这条边界让改动可被人审查，也让失败可以被明确拒绝而不是猜测。

## 安装

```bash
cargo install verseconf-mcp
```

## 五个工具

| 工具 | 作用 |
|---|---|
| `verseconf_validate` | 解析 + schema 校验，返回带行列号的错误 |
| `verseconf_audit` | 安全审计（通配绑定、明文口令等），返回稳定错误码 |
| `verseconf_apply_edit` | 按编辑计划做字符区间最小改动，写入前双重校验 |
| `verseconf_edit_range` | 直接替换指定字符区间（更底层的入口） |
| `verseconf_check_write` | 写前检查：给定原文与候选文本，判断这次改动是否允许落盘；不关心候选怎么产生 |

失败时返回**结构化的拒绝原因**，而不是一段自然语言。

## 用法

```bash
verseconf-mcp --list-tools                              # 离线查看工具契约
verseconf-mcp                                           # stdio 会话（逐行 JSON-RPC）
verseconf-mcp --call verseconf_validate '{"source":"port = 8080\n"}'
```

## 宿主接入

```json
{
  "mcpServers": {
    "verseconf": {
      "command": "verseconf-mcp",
      "args": []
    }
  }
}
```

如果宿主不想依赖本机二进制，同一套工具实现也已经编译成 WebAssembly，可由
JS 运行时直接加载，不需要 Rust 工具链。

> **但这条分发路径没有对外发布**：npm 包已放弃发布（发布账号不可用）。
> 已发布的 `verseconf@0.1.0` 缺 `pkg/` 目录、两条入口都不可用；修复版 0.2.0
> 在仓库里构建并通过打包验收，但不会再发布。请使用上面的本机二进制形态，
> 或从源码构建 wasm 产物（见 `integrations/verseconf-wasm/js-api`）。

## 相关

- [完整工具契约与协议细节](https://github.com/Baixu22/Verse-conf/blob/main/docs/MCP.md)
- [核心库 verseconf-core](https://crates.io/crates/verseconf-core)
- [主仓库](https://github.com/Baixu22/Verse-conf)

## 许可证

MIT OR Apache-2.0

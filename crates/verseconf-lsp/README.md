# verseconf-lsp

**简体中文** | [English](README.en.md)

VerseConf 的语言服务器（Language Server Protocol），为编辑器提供 `.vcf` 文件的
实时诊断、补全与导航。

## 安装

```bash
cargo install verseconf-lsp
```

编辑器扩展会在 `server/bin/<platform>-<arch>/` 下查找这个可执行文件；
如果你手动接入，直接以 stdio 方式拉起 `verseconf-lsp` 即可。

## 能力

| 能力 | 说明 |
|---|---|
| 诊断 | 解析错误带行列号发布为 `Diagnostic`，编辑器内实时可见 |
| 增量同步 | 正确处理 `did_change` 的 range 增量，不会损坏文档内容 |
| 补全 | 触发字符 `.` 与 `=` |
| 悬停 | 字段信息；无信息时返回 `null` 而不是样板卡 |
| 跳转定义 / 查找引用 | 跨 `@include` 的文件结构 |
| 语义高亮 | 语义 token |

## 编辑器集成

VS Code 扩展随主仓库发布（`.vsix` 由 CI 按平台构建语言服务器后打包），
它同时处理了按平台解析服务端路径、非 Windows 平台的执行位等问题。

## 相关

- [主仓库](https://github.com/Baixu22/Verse-conf)
- [核心库 verseconf-core](https://crates.io/crates/verseconf-core)
- [工具协议服务 verseconf-mcp](https://crates.io/crates/verseconf-mcp)

## 许可证

MIT OR Apache-2.0

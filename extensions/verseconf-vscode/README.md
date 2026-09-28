# VerseConf VSCode Extension

> **冻结说明（2026-09-28）**：已停止独立产品开发，不承诺后续发布或持续维护。以下仅为历史源码参考；安全边界与最终决定见 [CLOSEOUT.md](../../docs/CLOSEOUT.md)。

VerseConf 语言的历史编辑器集成：语法高亮、实时语法诊断、格式化，以及通过语言服务器协议
接入的部分语言能力。

## Features

- **Syntax Highlighting**：`.vcf` 文件语法高亮
- **Validation**：实时语法诊断（语言服务器提供，不承诺实时 schema 诊断）
- **Formatting**：文档格式化（保注释、保 `#@` 元数据）
- **Hover / Completion**：键与类型的悬停说明与补全
- **LSP Integration**：完整的语言服务器协议支持

## Requirements

- VSCode 1.75.0 或更高版本
- 语言服务器二进制。本地构建需自行准备，并放入对应目录或通过 `verseconf.lsp.serverPath` 指定。

## 安装

### 从扩展包安装

分发已停止。历史流水线（`.github/workflows/extension.yml`）曾用于产出扩展包，
旧 artifact 不保证仍可下载。若已持有 `verseconf-<版本>.vsix`，可按以下方式安装：

```bash
code --install-extension verseconf-0.1.0.vsix
```

或在 VSCode 中 `Extensions: Install from VSIX...`。

### 语言服务器在包内的位置

历史打包设计按 `<platform>-<arch>` 放置语言服务器，以下仅为目录布局参考，
不代表持续支持四个平台或保证包内已有这些二进制：

```
extensions/verseconf-vscode/
└── server/bin/
    ├── win32-x64/verseconf-lsp.exe
    ├── linux-x64/verseconf-lsp
    ├── darwin-x64/verseconf-lsp
    └── darwin-arm64/verseconf-lsp
```

扩展启动时按 `process.platform` 与 `process.arch` 解析到对应目录；找不到时给出
可操作的提示（而不是静默失败）。也可以用设置 `verseconf.lsp.serverPath` 指向
自己构建的二进制。

## 本地开发

```bash
cd extensions/verseconf-vscode
npm install
npm run compile        # tsc -> out/
npm test               # 语言服务器路径解析的单元测试
```

在本机跑语言服务器需要先构建并把二进制放到对应平台目录：

```bash
cd ../..               # 仓库根目录
cargo build -p verseconf-lsp --release
mkdir -p extensions/verseconf-vscode/server/bin/linux-x64
cp target/release/verseconf-lsp extensions/verseconf-vscode/server/bin/linux-x64/
```

（Windows 上对应 `server/bin/win32-x64/verseconf-lsp.exe`。）

按 `F5` 启动扩展开发宿主。

## 打包与验收

```bash
npm run verify                    # vsce ls 清单 + 真正打出 .vsix 并核对内容
npm run verify -- --require-server  # 额外要求包内带当前平台的语言服务器
npm run package                   # 只打包，产出 verseconf-<版本>.vsix
```

`verify` 会检查 `.vsix` 里包含 `out/extension.js`、语言配置、TextMate 语法、
`README`/`CHANGELOG`/`LICENSE`、`vscode-languageclient` 运行时依赖，以及每个
`server/bin/<平台>/` 下的语言服务器二进制；同时确认 sourcemap 已被排除。

## Configuration

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `verseconf.format.aiCanonical` | boolean | false | AI-friendly formatting |
| `verseconf.lsp.enabled` | boolean | true | Enable LSP features |
| `verseconf.lsp.serverPath` | string | `""` | 自定义语言服务器路径（留空则用包内二进制） |
| `verseconf.validation.strict` | boolean | false | Strict validation mode |

## Commands

| Command | Description |
|---------|-------------|
| `verseconf.format` | Format the current document |
| `verseconf.validate` | Validate the current document |
| `verseconf.schema.generate` | Generate schema from document |

## License

MIT OR Apache-2.0

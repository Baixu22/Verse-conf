# VerseConf VSCode Extension

VerseConf 语言的编辑器支持：语法高亮、实时诊断、格式化，以及通过语言服务器协议
接入的完整语言能力。

## Features

- **Syntax Highlighting**：`.vcf` 文件语法高亮
- **Validation**：实时语法与 schema 诊断（语言服务器提供）
- **Formatting**：文档格式化（保注释、保 `#@` 元数据）
- **Hover / Completion**：键与类型的悬停说明与补全
- **LSP Integration**：完整的语言服务器协议支持

## Requirements

- VSCode 1.75.0 或更高版本
- 语言服务器二进制。扩展包按平台自带，无需自行构建（见下）

## 安装

### 从扩展包安装

扩展包由流水线产出（`.github/workflows/extension.yml`），下载 `verseconf-<版本>.vsix` 后：

```bash
code --install-extension verseconf-0.1.0.vsix
```

或在 VSCode 中 `Extensions: Install from VSIX...`。

### 语言服务器在包内的位置

扩展包按 `<platform>-<arch>` 同时携带多个平台的语言服务器：

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

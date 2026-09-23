# Changelog

## 0.1.0

首个可安装版本。

- 语法高亮、语言配置与 `.vcf` 文件关联
- 通过 `vscode-languageclient` 接入 VerseConf 语言服务器（stdio）
- 命令：格式化文档、校验文档、从文档生成 schema
- 设置：`verseconf.format.aiCanonical`、`verseconf.lsp.enabled`、
  `verseconf.lsp.serverPath`、`verseconf.validation.strict`
- 语言服务器按 `<platform>-<arch>` 解析：扩展包内同时携带
  `server/bin/win32-x64/`、`server/bin/linux-x64/`、`server/bin/darwin-x64/`、
  `server/bin/darwin-arm64/` 四份二进制，找不到时给出可操作提示而不是静默失败

# Changelog

## 未发布

三个命令此前都是占位实现，现在都接上了真实能力：

- **格式化**：语言服务器新增 `documentFormattingProvider`，复用核心库的保注释格式化器。
  注释与 `#@` 元数据保留、结果幂等；文档无法解析时不做任何修改，而不是给出猜出来的结果。
  缩进跟随编辑器的 `tabSize` / `insertSpaces`。
- **校验**：读取语言服务器发布的实时诊断（与编辑器里看到的是同一份），报出首个问题的
  行列号并可跳到问题面板。服务端未运行时如实告知，不再假报"通过"。
- **生成 schema**：新增自定义请求 `verseconf/generateSchema`，由当前文档推断
  `#@schema` 块并插入文件开头。只写能从值确定的类型与 `required`，
  不猜 `range` / `enum` / `default` / `pattern`；文档已有 `#@schema` 时拒绝执行。

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

# 收尾验收记录

日期：2026-09-28。本记录在收尾快照生成后补充，不属于该快照的文件集合，也不修改它。

## 结果

| 检查 | 结果 |
| --- | --- |
| `cargo test --workspace --offline --locked` | 576 passed，0 failed，0 ignored；包含 6 条新增已知边界测试 |
| `cargo fmt --all -- --check` | 退出 0 |
| `cargo clippy --workspace --all-targets --offline --locked -- -D warnings` | 退出 0 |
| 统计分析与历史分类器 Node 测试 | 20 passed，0 failed |
| `node benchmark/verify-closeout.mjs` | 3539 个文件 SHA-256 / 长度匹配；确认性与门禁五轮原冻结全部通过 |
| 核验器负向测试 | 临时副本中改动文件 / 删除文件均退出 1，未触碰真实证据 |
| 原有文件保留核对 | 核对收尾前 3529 个文件；24 个文档 / 配置入口按范围修改，既有实现与历史实验内容未改变或删除 |
| 原始 README / workflow 快照 | 4/4 与收尾开始时工作树逐字节一致 |
| 本地文档链接 | 88 个目标存在 |
| workflow YAML | 两份均可解析，仅 `workflow_dispatch`，只读 contents 权限，无打包发布或 artifact 上传步骤 |
| npm 防误发布 | `private: true` |
| Git 保存范围 | 收尾快照列出的文件无一被 `.gitignore` 排除 |
| `git diff --check` | 退出 0 |

Rust 在 Windows MSVC 环境中执行。第一次启动因 Git Bash 到 cmd 的路径转义失败，没有开始测试；纠正路径后完整通过。测试构建时出现 wasm DLL 的 MSVC 库创建提示（linker warning），不影响测试；最终 Clippy 的 `-D warnings` 检查通过。

## 保存与发布边界

- 原先被忽略的 confirmation 逐运行目录有 3225 个文件，约 12.84 MB，包含 1422 份 TOML、计划与读回 JSON；已取消该目录的忽略规则，文件原文未改。
- 快照绑定 3539 个文件、16,830,075 字节的内容；证明收尾时完整性，不证明历史实验二进制身份。
- 原有未提交改动仍在工作树。没有 stage、commit、push、远程 archived 设置、发布或撤包。
- 没有修改任何宿主或用户设置。没有调用模型 API，没有启动新实验，没有修复启发式规则。
- 未执行远程 CI、wasm 分发构建、VSIX 打包或 registry 复验；不把本地检查说成远程 CI 通过。
- 本地 workflow 改动尚未推送，因此未宣称远程 Actions 已停止。

最终维护决定见 [CLOSEOUT.md](CLOSEOUT.md)。

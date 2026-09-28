# VerseConf

**简体中文** | [English](README.en.md)

> **已冻结 · 2026-09-28**
>
> VerseConf 停止作为独立产品线继续开发。本仓库保留配置编辑、写前检查的实现与实验记录，
> 不再扩展 `.vcf`、意图协议、LSP、VSCode、WebAssembly 或分发渠道，不承诺后续版本与维护时效。
> 未证明真实宿主中的净收益，不建议把现有门禁作为默认安全边界。

[![Status](https://img.shields.io/badge/status-frozen-lightgrey)](docs/CLOSEOUT.md)
[![License](https://img.shields.io/badge/license-MIT%20or%20Apache--2.0-green.svg)](LICENSE)

## 收尾决定

- **停止产品扩张和新一轮模型实验**。没有第六轮实验或新的格式适配计划。
- **保留源码、测试、原始结果和负结论**。冻结不是删除，也不是把历史失败改写为成功。
- **停止自动 CI、打包和分发产物生产**。只保留手动的离线证据核验与 Rust 回归入口；不构建 VSIX / wasm 分发矩阵。
- **不把宿主并入作为收尾前提**。没有修改任何宿主；未来的消费者可以参考差分裁决机制，不必引入整个项目。
- **没有执行远程归档、提交、推送、发布或撤包**。这里的“冻结”是当前源码树的维护决定，不代表 GitHub 的 archived 属性已设置。

最终裁决、证据边界与保留范围见 [docs/CLOSEOUT.md](docs/CLOSEOUT.md)。
重新开工的约束见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 已实现，不等于已证明值得采用

当前源码包含：

- VCF 的路径定位、`expect` 前置条件、局部源码编辑和结构化拒绝码。
- 基于 `toml_edit` / `jsonc-parser` 的 TOML、JSON / JSONC 适配。
- 与编辑协议解绑的 `check_write(baseline, candidate)`，以及 JSON 的可选标准 JSON Schema 校验。
- CLI、MCP、LSP、VSCode 与 wasm 封装。它们证明接口可调用，不证明已有业务消费者。

“最小改动”指支持的编辑中非目标源码区间保持不变，不是最少磁盘写入、历史回滚或多文件事务。
`check_write` 只是对调用方已有候选的判定；是否阻止落盘由宿主负责，独立 MCP 调用不可充当不可绕过的写屏障。

## 实验结论

确认性实验测的是 **TOML 标量 set 的编辑接口**，不是 `.vcf` 语言、完整 Agent 循环或生产部署。
1422 次运行来自 79 个任务、16 个来源，每臂 474 次，不是 1422 个独立任务。

| 编辑方式 | 语义正确 | 严格字节保真 |
| --- | ---: | ---: |
| 字符串替换 | 462/474（97.5%） | 459/474（96.8%） |
| 成熟库 span 编辑 | 430/474（90.7%） | 430/474（90.7%） |
| VerseConf intent | 421/474（88.8%） | 421/474（88.8%） |

intent 的模型 token / 严格正确结果约比 span 高 8.9%，未达到原定净收益门槛。
这支持停止推广该协议，不支持“字符串替换在所有任务上普遍优越”的外推。
见 [确认性裁决](benchmark/confirmation/VERDICT.md) 与 [来源聚类报告](benchmark/confirmation/results/cluster-report.md)。

后续五轮测的是既有 TOML 上的安全门禁，重复使用相同 8 份文档、12 个任务，不是五次独立验证。
第五轮有 72 次生成机会、60 次成功生成、12 次失败；按冻结结果谓词，风险计数从 9 降到 4，拦截 5 次。
这是局部拦截证据，**没有证明拒绝后仍完成任务、人工修复减少或总成本降低**。

第五轮历史裁决的“有增量”不能作为产品验收通过：

- 预登记要求“显著低于”，运行器仅判断 `B < A`，未执行相应显著性检验。
- 原冻结没有绑定运行器、分类器和被测二进制；当前共享运行器不能保证复现所有历史汇总口径。
- “实现假阳性 0/68”不是 68 次独立模型评审；即使按零事件近似，上界也约 4.4%，不足证明误拒率 ≤2%。
- 政策拒绝仍有用户成本；分类器确认文本形态存在，不等于确认业务语义正确。

[第五轮历史裁决](benchmark/gate-increment-round5/VERDICT.md) 原样保留；
[最终收尾裁决](docs/CLOSEOUT.md) 补充限制，不追溯修改原始数字。

## 必须知道的安全边界

| 情形 | 当前行为与限制 |
| --- | --- |
| 新增 `tls_verify=false` | 命中 SEC-005 并拒绝 |
| `verify_email=true` 改成 `false` | 也会命中 SEC-005；规则按路径中的 `ssl` / `verify` 子串判断，不理解该字段是否控制 TLS |
| 新增 `NODE_TLS_REJECT_UNAUTHORIZED="0"` | 当前规则未覆盖；可能放行 |
| 无 schema 时把整数改成字符串 | 可以放行；不保证类型保持 |
| 无 schema 时把配置改成 `{}` | 可以放行；不保证原任务或应用仍可用 |
| schema 存在 | 只校验实际提供或显式启用的 schema；不是宿主真实加载验证 |
| 已有风险改成另一项同位置、同规则风险 | 实例差分不一定认作新增风险；不等于完整的“风险没有变坏”证明 |
| `${ENV_VAR}` 等引用文本 | 被规则接受不等于目标程序支持这种插值或安全替代方案已生效 |
| YAML | 当前门禁不支持 |

可执行的已知边界记录见 [known_boundaries.rs](crates/verseconf-json/tests/known_boundaries.rs)。
这些测试描述冻结版本的局限，不把误拦或漏拦定义为正确安全政策。

CLI 的落盘前重读与 rename 之间也存在并发窗口；不能声称有严格 CAS、多文件事务、持久历史回滚或完整安全保证。

## 历史发布状态

本次没有联网复核 registry，也没有修改已发布版本。

| 渠道 | 收尾状态 |
| --- | --- |
| crates.io | 历史记录为 core / cli / lsp / mcp / toml 0.3.0 已发布；不再计划新版本。0.1.0 早于关键修复，不建议使用 |
| JSON / JSONC | 源码树实现，未发布；registry 上的 MCP 0.3.0 不含它。停止“下一次发布”的计划 |
| npm | 放弃发布；历史 0.1.0 包缺失 wasm 产物，不可作为可用入口 |
| VSCode / wasm / CDN | 停止持续构建分发；保留历史源码与本地构建说明，不保证旧 Actions 产物仍可下载 |
| Docker | 仓库 Dockerfile 是运行 `fortune` 的占位文件，不是 VerseConf 镜像；不补建、不分发 |

## 离线核验与复现

在仓库根目录运行：

```bash
node benchmark/verify-closeout.mjs
node --test benchmark/analysis/cluster_report.test.mjs benchmark/gate-increment/classify.test.mjs
cargo test --workspace --offline --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
```

Cargo 的离线命令需要本机已有依赖和工具链（Windows 需要 MSVC 构建环境）。
手动 GitHub 验证可以下载构建依赖，但不调用模型 API、不发布包、不上传分发产物。
证据快照是 **2026-09-28 收尾时** 的 SHA-256 清单，不是补造的开跑前预登记或历史二进制证明。

## 资料导航

- [最终收尾裁决](docs/CLOSEOUT.md) · [贡献与重新开工约束](CONTRIBUTING.md)
- [原始 README 与流水线快照](docs/archive/2026-09-28/README.md)（原文按字节保存为 `.txt`，仅作历史资料）
- [语言规范](docs/SPECIFICATION.md) · [历史教程](docs/TUTORIAL.md) · [MCP 契约](docs/MCP.md)
- [编辑保真度基准](benchmark/README.md) · [确认性实验](benchmark/confirmation/VERDICT.md)
- [核心库](crates/verseconf-core/README.md) · [TOML](crates/verseconf-toml/README.md) · [JSON](crates/verseconf-json/README.md)
- [VSCode 源码](extensions/verseconf-vscode/README.md) · [wasm 源码](integrations/verseconf-wasm/README.md)

许可证保持 [MIT OR Apache-2.0](LICENSE)。

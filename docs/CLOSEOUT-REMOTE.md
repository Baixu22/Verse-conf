# 远端合并与验证补充记录

日期：2026-09-28。本文件补充本地收尾完成后，用户明确要求提交与合并的后续事件，不改写原验收记录。

## 合并

收尾提交 `979f85708602673e6f551e542569e97b9596e9d0` 通过 [PR #1](https://github.com/Baixu22/Verse-conf/pull/1) 合并，主分支合并提交为 `c9e5e76fb11409bac690f89fd744185fbe284284`。
根 README 和原验收记录中的“未提交 / 未推送”描述本地收尾当时的状态，不代表后续禁止用户授权提交。

## 首次远端验证与修正

[首次手动运行](https://github.com/Baixu22/Verse-conf/actions/runs/36417285214)：

- Rust workspace、格式检查和 Clippy 全部通过。
- 3539 文件 SHA-256 和全部历史冻结检查通过。
- Node 测试 19 通过、1 失败：回放断言 `回放到的真实拒绝太少：0`。

原因：`classify.test.mjs` 调用 `run.mjs` 的 `checkWrite`，它启动 `target/debug/verseconf-mcp`。新的证据作业只安装 Node，没有构建原生门禁。两个作业互相隔离，Rust 作业的输出不会自动出现在证据作业。本地已有构建产物，因此本地 20/20 无法覆盖这个干净环境前置条件。

修正仅在证据作业运行 Node 测试前增加：

1. Rust 工具链与缓存；
2. `cargo build -p verseconf-mcp --locked`；
3. `verseconf-mcp --list-tools`，显式确认可执行文件可用。

没有改分类规则、测试阈值、原始候选、预登记或历史裁决；没有跳过失败测试，没有新增模型调用或分发动作。

## 不覆盖冻结历史

- 原 `benchmark/closeout-snapshot.json` 字节不变。
- 原 workflow 和核验器保存在 `docs/archive/2026-09-28-ci-prerequisite/`，与旧快照哈希一致。
- `benchmark/closeout-amendment-2026-09-28-ci.json` 绑定旧快照哈希、两份旧文件的保留位置、两份修正文件的新哈希及本记录。
- 核验器明确只允许此次 workflow / 核验器两文件修正，同时检查旧文件保留副本、新文件及其他全部冻结文件。

原快照仍是收尾时记录；这份修正只是验证前置条件维护，不是历史实验重新测量。

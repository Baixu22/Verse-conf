# confirmation：确认性复验的预登记（TF-0071）

口径与裁决规则见 **[PREREGISTRATION.md](PREREGISTRATION.md)**。这里只说怎么用。

## 冻结物

| 文件 | 是什么 | 谁生成 |
| --- | --- | --- |
| `tasks.json` | 79 个预登记的标量 `set` 任务 | `cargo run -p verseconf-toml --example generate_tasks -- --out benchmark/confirmation` |
| `weights.json` | 预登记的加权使用分布（LF 0.9 / CRLF 0.1） | 手写，开跑前定死 |
| `meta.json` | 三件冻结物的 SHA-256 与统计 | `preregistration.mjs --freeze` |

## 核验

```bash
# 结构级 + 指纹：任务集还是不是当初冻结的那一份，生成器改没改
node benchmark/confirmation/preregistration.mjs --verify

# 逐条核对：每条任务是不是真的命中了它声称覆盖的特征
cargo test -p verseconf-toml --test preregistration
```

两条都在 CI 里是阻塞的。第一条抓「有人手改了冻结物」或「生成器改了但没重跑」，
第二条抓「特征声称与事实不符」。

## 重新生成任务集

```bash
cargo run -p verseconf-toml --example generate_tasks -- --report   # 只报数，不写文件
cargo run -p verseconf-toml --example generate_tasks -- --out benchmark/confirmation
node benchmark/confirmation/preregistration.mjs --freeze
```

**只有在还没开跑的时候才能做这件事。** 第一次运行之后重新生成 = 换了一套任务，
必须作废本轮数据、重新预登记。

## 结果分析

任务集是 `benchmark/analysis/cluster_report.mjs` 的输入格式的来源之一：TF-0074 的
harness 每跑完一次就写一行 JSONL，然后：

```bash
node benchmark/analysis/cluster_report.mjs --runs <runs.jsonl> \
  --weights benchmark/confirmation/weights.json --out <目录>
```

输入格式与统计口径见 `benchmark/analysis/README.md`。

## 门槛来自哪里

`PREREGISTRATION.md` 第七节的门槛**不是**在这里新立的，直接取自全局目标 TF-0001 的
`success_metrics`（硬门禁 / 主指标 / 替代指标）。理由写在 7.1：实验设计者另立一套门槛，
就等于在同一次实验里同时决定「测什么」和「多少算过」。要改门槛，先改 TF-0001。

两条必须在开跑前知道的事：

- **主指标的对比基准是 `toml-edit-span`**（最薄的成熟保真库封装），不是 `toml-edit-thin`
  ——后者走序列化写入，在 CRLF 上丢字节，不构成「保真」封装。
- **替代指标本轮无法评估**：它需要一组能触发安全门禁的任务，而这里冻结的 79 个任务是
  刻意门禁中立的（两臂输出实测逐字节相同）。要评估校验层，得另立一组任务与另一份预登记。

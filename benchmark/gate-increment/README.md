# gate-increment：门禁在真实高风险任务上有没有增量（TF-0098 / TF-0099 / TF-0100）

这个目录冻结一件事的**口径与样本**：门禁在真实使用中到底有没有增量。

| 文件 | 是什么 | 谁生成 |
| --- | --- | --- |
| `tasks.json` | 12 条任务：8 份真实文档 × 只写意图的指令，每条配一个安全写法 | 手写，开跑前冻结 |
| `PREREGISTRATION.md` | 主指标、误拒预算、样本量、裁决规则、无法测量的部分 | 手写，开跑前冻结 |
| `meta.json` | 任务集 / 判定口径 / 语料指纹与统计 | `verify.mjs --freeze` |

## 为什么不是又一轮「手写任务」

消融实验的 8 条任务（`benchmark/ablation/`）证明了机制能工作；确认性复验的 79 条
刻意中立任务证明了它不乱拦。两者都不回答「真实使用中有没有增量」。

这一轮的两个改变：

1. **文档是真的**：8 份来自真实上游项目（来源、上游链接、许可证见
   `benchmark/holdout/sources.json`），不是内联字符串；
2. **指令只写意图、不写做法**：模型自己决定怎么写，实验测的正是它会自然写成哪一版。

改动本身是**按审计规则类别派生的**，不是从真实提交历史里挖出来的——这条偏差写在
`PREREGISTRATION.md` 第一节，不藏。

## 核验（两条都在 CI 里是阻塞的）

```bash
# 结构级 + 指纹：冻结物有没有被人手改过
node benchmark/gate-increment/verify.mjs --verify

# 语义级：每条任务是不是真的测到了它声称的东西
cargo test -p verseconf-toml --test gate_increment
```

第二条不是走过场——它逐条把「高风险写法」与「安全写法」真的应用到真实文档上，
然后问门禁要答案：

- `blocking` 组的 9 条必须真的被拒，且拒绝理由要**指名它声称的规则**；
- `advisory` 组的 3 条必须真的放行（Medium/Low 按设计只告警，拦下来就是过度拦截）；
- 12 条的安全写法必须全部放行（否则这条任务是误拒源，不是收益源）；
- 锚点在真实文档里必须**恰好出现一次**（手写任务集最容易犯的错是「什么都没测到」
  或「改的是另一处」）。

## 重新冻结

```bash
node benchmark/gate-increment/verify.mjs --freeze
```

**只有在还没开跑的时候才能做。** 第一次运行之后重新冻结 = 换了一套样本，
必须作废本轮数据、重新预登记。开跑之后 `--verify` 会直接失败，就是为了不让这件事
悄悄发生。

## 结果与裁决

运行器是本目录的 `run.mjs`（不是确认性复验那个：那一轮的 harness 是围绕编辑计划信封写的，
这一轮的任务是「给一份真实文件 + 一句意图」）。它与确认性复验共用同一套模型调用信封
（`POST /responses`：`model` / `instructions` / `input` / `reasoning`），并要求至少两个档位。

两条与口径有关的实现细节：

- **两臂用同一次生成**（配对设计）：门禁是确定性代码，对同一候选的裁决确定，
  为另一臂再生成一遍只会引入模型方差。口径写在 `PREREGISTRATION.md` 第二节。
- **门禁通过 stdio JSON-RPC 调用，不走 `--call <json>`**：真实文档有几十 KB，
  而命令行参数在 Windows 上有约 32K 的上限，超了会被截断成坏 JSON——第一次跑控制集时
  15 条出现了 8 条「拒绝」，全部是截断造成的假象。stdio 没有这个上限，而且它本来就是
  宿主真正用的那条路径。

```bash
node benchmark/gate-increment/verify.mjs --verify          # 冻结物没漂移
node benchmark/gate-increment/run.mjs --dry-run --tiers a,b # 控制集自检，不调模型
node benchmark/gate-increment/run.mjs \
  --tiers <生成档位1>,<生成档位2> --judge-tier <评审档位> \
  --repeat 3 --concurrency 4
```

跑完写 `results/runs.jsonl`（每行一次观测）、`results/latest.json` 与 `results/latest.md`
（按第五节逐条给出裁决）。裁决本身写在 **[VERDICT.md](VERDICT.md)**——它只逐条对照
预登记的规则，不改规则；裁决节点 TF-0102 读这些数字，不再重算。

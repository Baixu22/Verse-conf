# 确认性复验的裁决（TF-0068 / TF-0074）

**运行**：2026-09-26，1422 次运行（79 任务 × 3 方案 × 2 档位 × 3 次），
1 次模型调用失败已计入分母。语料是冻结的 16 份 holdout 文档（8 LF / 8 CRLF）。
档位固定为 `glm-5.3-flash` 与 `glm-5.3`，reasoning 固定 `minimal`。

原料：[`results/runs.jsonl`](results/runs.jsonl)（1422 行）、
[`results/cluster-report.md`](results/cluster-report.md)（按来源聚类的区间）。

---

## 一、裁决

按预登记 [§7.6](PREREGISTRATION.md) 的裁决表：

| 情形 | 裁决 |
| --- | --- |
| **硬门禁不过（H1 或 H2）** | **收尾为「小型可靠配置编辑库」**，停止扩大平台叙事 |

**本轮 H1 与 H2 都不通过。因此按预登记规则收尾。**

主指标 M1 也不通过，与 [§7.7](PREREGISTRATION.md) 开跑前就写下的不利先验一致
（当时记录：探索性数据上 `verseconf-intent` 比 `toml-edit-span` 贵约 7.9%，正确率相同）。

---

## 二、逐条对照

### 硬门禁

| 口径 | verseconf-intent | host-string-replace | toml-edit-span | 判定 |
| --- | --- | --- | --- | --- |
| 语义正确 | 88.8% | 97.5% | 90.7% | **低于两者 → H1 不过** |
| 字节保真 | 88.8% | 96.8% | 90.7% | **低于两者 → H1 不过** |
| 静默误改 | 10 | 9 | 0 | **高于两者 → H2 不过** |

按来源聚类的 95% 区间（§7.3 的诚实条款要求同时给出）：

| 方案 | 语义正确 [区间] | 字节保真 [区间] |
| --- | --- | --- |
| host-string-replace | 97.5% [94.2%, 100.0%] | 96.8% [92.5%, 100.0%] |
| toml-edit-span | 90.7% [76.9%, 99.8%] | 90.7% [76.9%, 99.8%] |
| verseconf-intent | 88.8% [75.5%, 97.0%] | 88.8% [75.5%, 97.0%] |

区间很宽（16 个来源），三者的区间互相重叠。**点估计低于对照即为不过**，
所以 H1 的判定不依赖区间；但区间重叠这件事必须一起说：它与对照之间的差距
**不是**一个能被这批语料稳定区分出来的差距。

### 主指标

| 方案 | 每个最终正确任务的模型侧 token |
| --- | --- |
| toml-edit-span（预登记指定的基准） | 3,461 |
| verseconf-intent | 3,768 |
| **改善** | **−8.9%（更贵）→ M1 不过** |

口径限制（§7.4）：只能测到模型输出与调用次数，**人工修复与审查无法测量**，
所以这个数字只能叫「模型侧成本」，不能叫总成本。

### 按档位

| 档位 | 语义正确 | 字节保真 |
| --- | --- | --- |
| glm-5.3 | 92.7% | 92.5% |
| glm-5.3-flash | 92.0% | 91.7% |

两个档位差别很小（0.7 个百分点），结论不依赖单一档位。

---

## 三、必须一起读的混淆项（否则就是过度解读）

**`verseconf-intent` 的提示词没有告诉模型「命名字段定位」怎么写。**

harness 给该臂的形状示例是
`{"version":"1.0","edits":[{"op":"set","path":["<字段路径>"],"value":"<新值>"}]}`
——只示范了**字符串段**。而真实工具契约（`tools/list` 内联的
`$defs.segment.oneOf`，TF-0061）同时支持 `[[key]]` 数组表的
`{"key": ..., "match": {...}}` **对象段**。

后果是可验证的：

| 任务子集 | host-string-replace | toml-edit-span | verseconf-intent |
| --- | --- | --- | --- |
| 普通路径任务（72 条，432 次） | 98.6% | 99.5% | **97.5%** |
| 命名字段定位任务（7 条，42 次） | 78.6% | **0.0%** | **0.0%** |

`verseconf-intent` 在命名字段任务上 0/42，失败信息全部是
`unsupported_target: ...（`test` 是数组表，要用按字段匹配的元素定位）`
——即模型退化成写了字符串路径。而**唯一有能力表达这种路径的就是它**。
`host-string-replace` 不需要路径，所以不受影响，拿到 33/42。

把命名字段任务剔除后，`verseconf-intent` 是 97.5%，仍低于两个对照（98.6% / 99.5%）。
所以：

- **H1 无论是否修正这个提示词缺口都不通过**，但**差距的量级主要由这个缺口造成**：
  整体 88.8% → 剔除后 97.5%，缺口贡献了约 8.7 个百分点里的 7.6 个。
- **H2 与 M1 不受这个缺口影响**：`verseconf-intent` 的 10 次静默误改**全部发生在普通路径任务上**
  （`opendal-package-build`、`pandas-tool-cibuildwheel-build-verbosity`、
  `terminal-bench-metadata-expert_time_estimate_min`、`tinyvec-tab_spaces`、`which-licenses-*`），
  而且逐条查过不是判定器误判——模型把布尔/整数/浮点写成了**字符串**
  （`false → "true"`、`3 → "4242"`、`15.0 → "42.5"`），意图契约照单写入、没有报错。
  这正是「静默误改」要抓的东西：调用方拿不到任何信号。

**结论的稳健性**：即使把提示词缺口补上重跑，按 §7.6 的规则，**只要 H2 仍不通过，裁决不变**。
`toml-edit-span` 在这一轮是 0 次静默误改，`verseconf-intent` 是 10 次，且都不在受缺口影响的任务上。

---

## 四、开跑前发现并修正的缺陷（都已记录）

1. **判定器 §三 的 check 2 / check 3 是子串判定**：值本身含键名（`matplotlib = "pandas:plotting._matplotlib"`）
   或含 `=`（`requires-python = ">=3.11"`）时会把完全字节保真的结果判成不保真。
   79 条任务里 5 条（6.3%）因此对任何方案都不可通过。**开跑前**改为按位置判定并重新冻结——
   若等跑完再改，整轮 1422 次运行全部作废。
2. **1 条任务自指**：`regex-test--invalid-regex-no-crash-100--name` 改的 `name` 正是它自己的匹配字段，
   改完再也无法被同一选择器定位，口径 B 对它必然失败。只影响 1/79，对三方案同等，
   已写入 [§八 边界](PREREGISTRATION.md)，本轮不重新生成任务集（会改动冻结指纹）。
3. **`toml-edit-*` 两臂的点号路径表达不了命名字段定位**：这是协议能力差异，
   与上面第 3 节的提示词缺口是两回事——前者是**对照臂**的能力边界，后者是**被测臂**的表述缺口。

---

## 五、边界（不要在它之外引用）

- 只覆盖标量 `set`；`insert` / `delete` 与复杂值不在本轮。
- 16 份文档来自 16 个上游项目，但不是随机抽样自真实世界的 TOML 分布；7 份是包清单。
- 加权汇总（LF 0.9 / CRLF 0.1）是**按假设分布重加权**的结果，语料实际是 8/8，
  与预登记分布差 39.4%，**不能**说成生产环境平均值。
- 替代指标 A1（安全门禁）**不在本轮**：本轮任务刻意门禁中立。
  它的实测在 [`../ablation/`](../ablation/)（TF-0079）：5 条会引入 Critical/High 实例的改动，
  对照臂全部静默写入，加上校验层后全部被拒绝且每次都挡住真实引入，79 条中立任务 0 误拒。
- 成本只覆盖模型侧（token 与调用次数），人工修复与审查未测。

---

## 六、复现

```bash
# 前置：构建 harness 用到的示例
cargo build -p verseconf-toml --examples

# 确认性运行（需要网关凭据；1422 次调用，并发 8 约 30 分钟）
PILOT_BASE=<接口> PILOT_API_KEY=<密钥> \
  node benchmark/confirmation/run.mjs --runner model \
    --tier glm-5.3-flash --tier glm-5.3 --concurrency 8 \
    --out benchmark/confirmation/results

# 按来源聚类的不确定性报告
node benchmark/analysis/cluster_report.mjs \
  --runs benchmark/confirmation/results/runs.jsonl \
  --weights benchmark/confirmation/weights.json \
  --out benchmark/confirmation/results
```

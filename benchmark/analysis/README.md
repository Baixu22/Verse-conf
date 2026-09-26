# 按来源聚类的不确定性分析（TF-0073）

确认性复验（TF-0069 / TF-0074）的结果分析工具。它只做一件事：**把重复运行按
「运行 → 任务 → 文档 → 来源」的嵌套结构算清楚**，并给区间而不是点估计。

## 为什么需要它

上一轮的 195 次运行**不是** 195 个独立样本。C1 的 12 次失败其实是 4 个任务各重复 3 次，
根因集中在唯一那份 CRLF 文件上；补上 C2 基线后结论翻转过两次。把 N 次运行当成
N 个独立样本，就会把**一份文件的特性**说成普遍规律，而且区间看起来还很窄。

拿真实数据跑一遍就能看见这件事（`scratch/pilot-5arm/report.json`，探索性数据）：

```
| 层级 | 数量 |
| 运行 | 195 |
| 任务 | 13 |
| 文档 | 3 |
| 来源 | 3 |          ← 独立单元只有 3 个

字节保真：84.6% [50.0%, 100.0%]
```

区间宽到 `[50%, 100%]` 才是这批数据的真实信息量：**3 个来源分不清「机制有效」
和「只在 3 份文件里的 2 份上有效」**。点估计 `84.6%` 单独出现会假装它分得清。

## 用法

```bash
# 确认性结果（TF-0074 的 harness 直接写出的输入格式）
node benchmark/analysis/cluster_report.mjs --runs <runs.jsonl> --out <目录> \
  --weights benchmark/analysis/weights.example.json

# 把探索性试点的 report.json 转过来复算（只用于验证分析口径）
node benchmark/analysis/cluster_report.mjs --from-pilot <report.json> \
  --document-endings <endings.json> --out <目录>
```

产物是 `cluster-report.md` 与 `cluster-report.json`。不加 `--out` 时打到 stdout。

| 选项 | 说明 |
| --- | --- |
| `--seed` | 重抽种子，默认 `20260926`。同一种子必须得到逐字节相同的输出 |
| `--iterations` | 重抽次数，默认 2000 |
| `--weights` | 预登记的使用分布（JSON，键是 `LF` / `CRLF`，和为 1） |
| `--document-endings` | 只给 `--from-pilot` 用：试点报告里没有行尾类型 |

`weights.example.json` 只是**格式示例**，不是预登记值。真实权重由 TF-0071 冻结，
在开跑之前写定；拿示例文件跑出来的加权数字没有任何意义。

## 输入格式

JSONL，一行一次运行：

```json
{"cluster":"tokio","document":"tokio-Cargo.toml","ending":"LF","arm":"verseconf-intent","model":"glm-5.3-flash","task":"tokio-root-string-model","repeat":1,"semantic_ok":true,"byte_ok":true,"applied":true}
```

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| `cluster` | ✅ | **来源**。独立单元就是它，不是运行、也不是文档 |
| `document` | ✅ | 文档标识 |
| `ending` | ✅ | `LF` 或 `CRLF`；其它值直接报错，不做归类 |
| `arm` | ✅ | 方案（`verseconf-intent` / `toml-edit-span` / …） |
| `model` | ✅ | 模型档位 |
| `task` | ✅ | 任务标识（只需在文档内唯一） |
| `repeat` | ✅ | 第几次重复 |
| `semantic_ok` | ✅ | 口径 B：语义正确 |
| `byte_ok` | ✅ | 口径 A：严格字节保真 |
| `applied` | ⬜ | 是否确实改动了文件。缺省时按「至少一个口径过了」推断，并在报告里注明推断了几次 |

任何一行缺字段、类型不对、行尾类型不认识，脚本都会**直接失败**而不是跳过——
一份被悄悄丢掉的运行会让分母变小，而分母变小不会自己报出来。

## 两个口径分开报

语义正确（口径 B）与字节保真（口径 A）**各自**给通过率与区间，不合成一个数。
两者差值就是「值改对了但格式被动了」的比例。

## 区间怎么算

**cluster bootstrap**：重抽**来源簇**，簇里的文档、文档里的任务、任务里的重复运行
整体跟着走。独立单元是来源，不是运行。

- 只有 1 个来源时**不给区间**，直接写「区间不可用：只有 1 个来源」。
  按文档分组的那张表因此天然没有区间——每份文档就是它自己那一簇。
  这是要指出来的事实，不是要绕过去的麻烦。
- 随机数用固定种子，同一份输入重复跑得到逐字节相同的输出。

## 加权汇总的规矩

`--weights` 给的是**预登记的使用分布**，不是从语料里量出来的比例。规矩有两条：

1. **不给权重就不出加权汇总**。拿语料自己的比例冒充使用分布，正是这一节要避免的事。
2. **加权汇总永远不叫「生产环境平均值」**。权重是一个假设，不是一个测量结果。
   当语料分布与目标分布差超过 10%（`OVERSAMPLING_TOLERANCE`）时，报告会打出
   过采样警告，明确写出「这是按假设分布重加权的结果，不能说成生产环境平均值」。

这两条都有回归测试盯着（`cluster_report.test.mjs`）。

## 测试

```bash
node --test benchmark/analysis/cluster_report.test.mjs
```

其中最重要的一条是
`concentrated_evidence_gets_a_much_wider_interval_than_spread_evidence`：
同样的通过率、同样的运行次数，只把失败集中到一个来源里，区间就必须显著变宽。
这一条不成立的话，整个 TF-0073 就没有意义。

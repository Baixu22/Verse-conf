# gate-increment-round2：修正后的门禁再测一次（TF-0098 第二轮）

方法与冻结纪律与第一轮完全相同，见 [`../gate-increment/README.md`](../gate-increment/README.md)。
这里只说**差异**与**怎么读**。

## 这一轮是什么，不是什么

- **是**：同一语料（8 份真实文档 / 12 条任务，锚点逐字节相同）上的**再测量**。
  第一轮之后修掉了两条实现缺陷，这一轮量它们有没有生效。
- **不是**：独立复现。换一批样本才算独立复现，那要另立一轮。
  引用结论时必须写成「同一语料上的再测量」。

第一轮的数据与裁决**保留不动**（`../gate-increment/`）：它记录的是修正前的实现。

## 与第一轮的两处实现差异

| | 第一轮 | 第二轮 |
| --- | --- | --- |
| URL 内嵌凭据（`https://user:pass@host/`） | 看不见（占观测到全部风险 45%） | 新增阻断规则 `SEC-SENS-002` |
| 值是 `SCREAMING_SNAKE_CASE`（指向环境变量） | 判成硬编码凭据（误拒） | 按**引用**处理，仍告警不阻断 |

## 两条确定性对照（不依赖模型）

跑之前 `--dry-run` 就会检查：

1. `url-credential-is-now-blocked` —— 把凭据写进镜像地址的 URL 值里，**必须被拒**；
2. `env-var-name-is-not-blocked` —— `token-env = "PANDAS_INTERNAL_UPLOAD_TOKEN"`，**必须放行**。

任一条不满足即为硬失败、本轮结论作废：它说明修复没生效，或者引入了回归。

## 冻结与运行

```bash
node benchmark/gate-increment/verify.mjs --freeze  --dir benchmark/gate-increment-round2
node benchmark/gate-increment/verify.mjs --verify  --dir benchmark/gate-increment-round2
node benchmark/gate-increment/run.mjs --dry-run    --dir benchmark/gate-increment-round2 --tiers a,b
node benchmark/gate-increment/run.mjs --dir benchmark/gate-increment-round2 \
  --tiers glm-5.3-flash,deepseek-v4.1-flash --judge-tier sn-kimi-k3 --repeat 3 --concurrency 5
```

运行器与核验脚本都是第一轮那一份（`run.mjs` / `verify.mjs` 支持 `--dir`），
所以两轮的模型调用信封、配对设计、评审口径与主指标定义**逐字相同**——
唯一变化的变量是被测的门禁实现。

## 开跑前登记的三条可证伪预期

见 [`PREREGISTRATION.md`](PREREGISTRATION.md) 第五节：E1（B 臂 P2 残留应为 0）、
E2（`token-env` 那类误拒不再出现）、E3（A 臂发生率与第一轮同量级，否则两轮不可比）。
它们不是裁决规则，而是「修复是否生效」的检查，任何一条不成立都要写明。

## 结果

跑完写 `results/`，裁决写在 [`VERDICT.md`](VERDICT.md)。

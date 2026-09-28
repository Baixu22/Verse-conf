# gate-increment-round4：把「误拒」分成两类之后重新判定

方法、冻结纪律与运行器与前三轮相同，见 [`../gate-increment/README.md`](../gate-increment/README.md)。
这里只说**这一轮唯一的改动**与**怎么读**。

## 唯一的改动是测量口径，不是门禁实现

前三轮共 7 次"误拒"里，**6 次是同一条刻意未改的判定**（SEC-005 对"用环境变量关证书校验"），
只剩 1 次是已修的实现缺陷；而且同一形态在不同 repetition 里被旧问法判成过不同答案。
也就是说旧问法把两件事混在了一起：

- **规则说错了**（候选里其实没有那个形态）→ 实现假阳性，**2% 预算该管的就是它**；
- **规则说对了，只是用户不同意被拦** → 政策拒绝，属于产品决定，不该占用实现质量预算。

第四轮因此在旧问题之外**多问一个事实核对问题**（只允许回答是/否、只看候选文本）：

```bash
node benchmark/gate-increment/run.mjs --dir benchmark/gate-increment-round4 --grounding ...
```

- 回答**否** → `implementation_false_positives`（计入 2% 预算，67 × 2% = 1.34 → 允许 ≤1，≥2 即超）；
- 回答**是** → `policy_refusals`（逐个列出，不计入预算）。

旧答案（`misrejects`）仍然记录，用于与前几轮对比。

## 这是收紧，不是放宽

预算现在只管"规则说错了"这一类，政策拒绝被逐个列出来接受审视。
`PREREGISTRATION.md` 第三节的 **E2** 明确登记：本轮实现假阳性应当 ≤1 次；
**若 ≥2 次，裁决仍按规则 3 判「过度拦截」**——这条预期是可以被证伪的。

## 四轮回合的关系

| 轮次 | 被测实现 / 口径 | A 臂 | B 臂 | 成本 |
| --- | --- | --- | --- | --- |
| [第一轮](../gate-increment/) | 修正前 | 11 | 5 | 旧口径误拒 2/67 |
| [第二轮](../gate-increment-round2/) | +URL 内嵌凭据、+环境变量名 | 9 | 0 | 2/67 |
| [第三轮](../gate-increment-round3/) | +凭据机制名 | 8 | 0 | 3/67 |
| **第四轮（本目录）** | 三条修复都在位 + **两分类口径** | 见 [VERDICT.md](VERDICT.md) | | |

四轮用的是**同一批文档与同一批任务**。所以第四轮是同一语料上的第三次再测量；
**前三轮的裁决按旧口径仍然有效**，引用第四轮结论时必须同时说明口径。

## 冻结与运行

```bash
node benchmark/gate-increment/verify.mjs --freeze --dir benchmark/gate-increment-round4
node benchmark/gate-increment/verify.mjs --verify --dir benchmark/gate-increment-round4
node benchmark/gate-increment/run.mjs --dry-run --dir benchmark/gate-increment-round4 --tiers a,b
node benchmark/gate-increment/run.mjs --dir benchmark/gate-increment-round4 --grounding \
  --tiers glm-5.3-flash,deepseek-v4.1-flash --judge-tier sn-kimi-k3 --repeat 3 --concurrency 5
```

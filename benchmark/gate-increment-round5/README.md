# gate-increment-round5：用确定性分类器测误拒率

方法、冻结纪律与运行器与第一至四轮相同，见 [`../gate-increment/README.md`](../gate-increment/README.md)。
这里只说**这一轮的测量口径**与**怎么读**。

## 与前四轮的唯一差别：成本怎么量

前四轮的成本数据全部不可引用，原因都在测量工具：

| 轮次 | 成本怎么量的 | 为什么不能用 |
| --- | --- | --- |
| 一至三轮 | LLM 问「这是误拒吗」 | 把「用户不同意被拦」与「规则说错了」混在一起；同一形态答案不稳定 |
| 第四轮 | LLM 问「形态真的存在吗」 | 自相矛盾：9 次拒绝判 7 次假阳性，人工核对确认至少 2 次误标 |
| **第五轮** | **确定性形态谓词**（`--classify`） | 见 [VERDICT.md](VERDICT.md) |

```bash
node benchmark/gate-increment/run.mjs --dir benchmark/gate-increment-round5 --classify ...
```

分类器（[`../gate-increment/classify.mjs`](../gate-increment/classify.mjs)）逐规则、逐实例地从候选里
按门禁给出的位置取出那一处的值，再判「该规则针对的形态在不在」：

- `shape_absent` → **实现假阳性**（规则说错了），计入 2% 预算；
- `shape_present` → **政策拒绝**（形态确凿存在，只是用户可能不同意被拦），单独报告；
- `undetermined` → 单独报告，不当作任一侧。

**本轮不调 LLM 评审**，判定只用分类器。分类器的占位词表（裸值 / URL 角色词两套）在开跑前
冻结在 `classify.mjs` 与 `classify.test.mjs` 里，由 CI 跑自检——对词表有异议属于测量定义之争，
必须在开跑前提出。

## 五轮回合的关系

| 轮次 | 被测实现 / 测量口径 | A 臂 | B 臂 | 成本 |
| --- | --- | --- | --- | --- |
| [第一轮](../gate-increment/) | 修正前 / LLM 旧问法 | 11 | 5 | 2/67 |
| [第二轮](../gate-increment-round2/) | +URL 内嵌凭据、+环境变量名 / LLM 旧问法 | 9 | 0 | 2/67 |
| [第三轮](../gate-increment-round3/) | +凭据机制名 / LLM 旧问法 | 8 | 0 | 3/67 |
| [第四轮](../gate-increment-round4/) | 同上 / LLM 事实核对 | 9 | 0 | 7 假阳性（不可用） |
| **第五轮（本目录）** | +占位文案豁免 / **确定性分类器** | 见 VERDICT | | |

五轮用的是同一批文档与同一批任务：第五轮是**同一语料上的第四次再测量**，不是独立复现；
前四轮的裁决按当时口径仍然有效。

## 冻结与运行

```bash
node benchmark/gate-increment/verify.mjs --freeze --dir benchmark/gate-increment-round5
node benchmark/gate-increment/verify.mjs --verify --dir benchmark/gate-increment-round5
node benchmark/gate-increment/run.mjs --dry-run --dir benchmark/gate-increment-round5 --tiers a,b
node benchmark/gate-increment/run.mjs --dir benchmark/gate-increment-round5 --classify \
  --tiers glm-5.3-flash,deepseek-v4.1-flash --repeat 3 --concurrency 5
```

> 注意：脚本是通过 shell 调 `target/debug/verseconf-mcp.exe` 问门禁的，
> 所以改完 core 要先 `cargo build -p verseconf-mcp` 再跑，否则量到的是旧实现。

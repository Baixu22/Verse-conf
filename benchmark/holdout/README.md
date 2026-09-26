# holdout 语料（TF-0070）

确认性复验（TF-0069）的入口。**这批文件在实验开跑之前冻结**，之后只读。

## 为什么需要它

阶段四的五方对照（`benchmark/pilot/`）是**探索性**结果：只有 3 份文档、全部来自
同一台机器，而且 13 个任务里有 4 个落在**唯一那份 CRLF 文件**上——VerseConf 的
全部优势都来自它。样本在第一次空结果之后扩大过，补上 C2 基线后结论翻转过两次。

在这样的样本上，任何跨文件的结论都只是对 3 个文件与一条基线的拟合。要判定
预登记的门槛，必须换一批**没看过的、来源足够分散的**文件重跑。

## 冻结事实

| 项 | 值 |
| --- | --- |
| 文档数 | **16** |
| 来源簇数 | **16**（每个来源只贡献一份文档） |
| 行尾 | **LF 8 份 / CRLF 8 份** |
| 总字节 / 总行数 / 总键值对 | 140,669 B / 5,468 行 / 2,357 |
| **语料指纹** | `69b8d6df86be4eba4b89e40798333d6a9fe058df8d8a71f4e941f1552f7f6ac3` |
| 已排除 | `codex` / `starship` / `rtk`（已用于探索性实验） |

语料指纹 = 对 `cluster:document:sha256` 逐行排序后取 SHA-256。它只由「来源、文件名、
内容」决定，与来源路径无关——语料换一台机器仍然能核验是同一份。

### 16 份文档

| 簇（来源） | 文件 | 行尾 | 字节 | 行 | 键值对 |
| --- | --- | --- | --- | --- | --- |
| clap | `clap-Cargo.toml` | LF | 15,155 | 647 | 410 |
| tokio | `tokio-Cargo.toml` | LF | 17,637 | 995 | 454 |
| regex | `regex-regression.toml` | LF | 28,124 | 985 | 487 |
| pandas | `pandas-pyproject.toml` | LF | 27,432 | 835 | 288 |
| opendal | `opendal-Cargo.toml` | LF | 12,386 | 701 | 301 |
| reqwest | `reqwest-Cargo.toml` | LF | 13,129 | 505 | 186 |
| terminal-bench | `terminal-bench-task.toml` | LF | 987 | 41 | 24 |
| verseshell | `verseshell-Cargo.toml` | LF | 2,836 | 128 | 71 |
| which | `which-deny.toml` | CRLF | 11,279 | 234 | 26 |
| half | `half-Makefile.toml` | CRLF | 2,280 | 78 | 30 |
| node-gyp | `node-gyp-pyproject.toml` | CRLF | 3,002 | 114 | 26 |
| vscode-python | `vscode-python-pyproject.toml` | CRLF | 2,620 | 83 | 9 |
| versepixpin | `versepixpin-pyproject.toml` | CRLF | 2,121 | 65 | 16 |
| browser-harness | `browser-harness-pyproject.toml` | CRLF | 625 | 27 | 11 |
| typeshed | `typeshed-tensorflow-METADATA.toml` | CRLF | 773 | 16 | 8 |
| tinyvec | `tinyvec-rustfmt.toml` | CRLF | 283 | 14 | 10 |

来源路径、上游链接、文件内声明的许可证与来源说明记在 `sources.json`；
逐文件的字节数、行数、行尾计数、SHA-256 与能力画像记在 `manifest.json`。

## 能力画像（冻结时固定，供 TF-0071 挑任务）

| 特征 | 覆盖到的文档数 | 备注 |
| --- | --- | --- |
| 字符串 | 16 | |
| 数组 | 15 | |
| 布尔 | 13 | |
| 整数 | 11 | |
| 数组表 `[[x]]` | 7 | tokio 172 处、regex 105 处、clap 67 处 |
| 内联表 `{ ... }` | 6 | |
| 非 ASCII 字形 | 4 | opendal / regex / versepixpin / verseshell |
| 带引号的键 | 3 | node-gyp / pandas / versepixpin |
| 多行字符串 `"""` | 4 | clap / pandas / regex / tokio |
| 浮点 | 2 | terminal-bench / which |
| **行尾注释** | **1** | 只有 pandas，9 行 |

**行尾注释是这批语料最薄的一格，而且必须写在这里而不是等跑完再解释**：
本机上唯一一份行尾注释密集的 CRLF 文档就是 `starship.toml`（16 行），
而它已用于探索性实验、必须排除；CRLF 侧其余候选最多只有 1–3 行。
也就是说 **CRLF 侧没有行尾注释任务可用**。TF-0071 要么把这条特征只放在
pandas（LF）上并如实报告，要么在**预登记之前**再补一份文档并重新冻结
（补文档会改变语料指纹，必须在开跑前完成）。

另有一条已经核验的性质：**16 份文档的审计基线都是干净的**——没有任何一份
含既有的高危实例（硬编码凭据、弱算法、关闭 SSL 校验等）。所以安全门禁
（TF-0077 / TF-0079）挡住的东西只会是**本次改动新引入**的，不会和文件本来就有的
问题混在一起。这条性质由 `cargo test -p verseconf-toml --test holdout_corpus`
里的断言守住，补新文档时破坏了它会直接失败。

## 怎么核验

```bash
node benchmark/holdout/verify.mjs          # 字节级：SHA-256、字节数、行尾、指纹、验收形状
cargo test -p verseconf-toml --test holdout_corpus   # 解析级：每份都能被适配层的解析器解析
```

`freeze.mjs` 从 `sources.json` 重新生成 `documents/` 与 `manifest.json`；
它会**硬失败**而不是尽量成功：来源读不到、行尾不是纯 LF/CRLF、簇重复都会中止——
一份混行尾的文档会让后面的结论无法解释。

## 边界（不要在它之外引用）

- **16 份文档全部来自同一台机器。** 来源是 16 个不同的上游项目，但都是这台
  机器上碰巧装着的版本；它不是「随机抽样自真实世界的 TOML 分布」。
- **7 份是包清单**（Cargo.toml / pyproject.toml），不是应用配置。真实宿主的
  配置形态（`~/.codex/config.toml` 那类）在这批里没有对应样本——那类文件
  正是探索性实验用掉的三份，已按规则排除。
- **LF 与 CRLF 各自绑定了不同的来源**，所以「行尾类型」与「具体是哪个项目」
  在这批语料里无法完全分开。这正是行尾类型必须按文档聚类报告、不能当成
  独立样本的原因。
- **语料只决定结论有没有可能成立**，本身不产生任何结论。

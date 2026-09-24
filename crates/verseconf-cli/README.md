# verseconf-cli

VerseConf 配置语言的命令行工具。

> **包名与命令名不同**：包叫 `verseconf-cli`，装出来的可执行文件叫 `verseconf`。
> 这跟 `ripgrep` 包装出 `rg` 命令是一回事。

## 安装

```bash
cargo install verseconf-cli
```

## 命令

| 命令 | 用途 |
|---|---|
| `verseconf parse <文件>` | 解析并打印结构（`--no-include` 可关闭 `@include` 展开） |
| `verseconf validate <文件>` | 校验（含 schema）；`--fix --write` 对合法文件逐字节零改动 |
| `verseconf format <文件>` | 格式化，保留注释与 `#@` 元数据，幂等 |
| `verseconf audit <文件>` | 安全审计（通配绑定、明文口令等） |
| `verseconf doc <文件>` | 由 schema 生成文档 |
| `verseconf diff <a> <b>` | 比较两份配置 |
| `verseconf env <子命令>` | 环境管理 |
| `verseconf template <子命令>` | 模板相关 |
| `verseconf version <子命令>` | 版本管理 |

## 例子

```bash
# 解析
verseconf parse config.vcf

# 校验；对已经合法的文件 --fix 不会改动任何字节
verseconf validate config.vcf
verseconf validate config.vcf --fix --write

# 格式化（两次输出一致）
verseconf format config.vcf -o formatted.vcf

# 安全审计
verseconf audit config.vcf

# 环境变量插值：${DB_HOST|localhost}
verseconf format config.vcf --env
```

## 相关

- [主仓库与语言说明](https://github.com/Baixu22/Verse-conf)
- [核心库 verseconf-core](https://crates.io/crates/verseconf-core)

## 许可证

MIT OR Apache-2.0

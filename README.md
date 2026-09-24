# VerseConf

> **Agent 编辑配置的确定性执行层。**
> 模型只给出「改哪个字段、改成什么」，由确定性代码完成字符区间级最小改动，
> 写入前做 schema 与安全双重校验；**无法确定时拒绝，而不是猜测**。

[![crates.io](https://img.shields.io/crates/v/verseconf-core.svg)](https://crates.io/crates/verseconf-core)
[![CI](https://github.com/Baixu22/Verse-conf/actions/workflows/ci.yml/badge.svg)](https://github.com/Baixu22/Verse-conf/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20or%20Apache--2.0-green.svg)](LICENSE)

---

## 目录

- [一、问题](#一问题)
- [二、做法](#二做法)
- [三、实测效果](#三实测效果)
- [四、安装](#四安装)
- [五、快速开始](#五快速开始)
- [六、配置语言](#六配置语言)
- [七、命令行参考](#七命令行参考)
- [八、给 Agent 宿主接入](#八给-agent-宿主接入)
- [九、编辑器支持](#九编辑器支持)
- [十、项目结构](#十项目结构)
- [十一、开发](#十一开发)
- [十二、文档索引](#十二文档索引)
- [十三、已知限制与路线](#十三已知限制与路线)

---

## 一、问题

让模型改配置，今天的通行做法是把**整个文件**交给它重写。代价可以量化：

| 代价 | 具体表现 |
|------|----------|
| 改动之外的字节也被改 | 只想换一个端口号，整个文件被重新排版 |
| 注释与元数据丢失 | 行尾注释、`#@` 标注、键序被抹掉 |
| 该拒绝的照单执行 | 目标有歧义、改动破坏 schema、引入安全风险时不会停下 |
| 无法审计 | 只留下一份新文件，说不清改了什么、为什么改 |

## 二、做法

1. **只接受语义意图** —— 改哪个字段、改成什么、为什么改、前置条件是什么。
2. **由确定性代码定位** 目标值的字符区间，只替换那一段字节。
3. **写入前双重校验** —— schema 校验 + 安全审计，任一失败即拒绝。
4. **失败即拒绝** —— 目标不存在、有歧义、前置条件不符时返回稳定错误码，
   调用方不应写入任何文件。

### 不做什么

- 不追求成为通用配置语言，不在表达力上与 Pkl / CUE / KCL 竞争
- 不为「生态丰富」而堆砌语言无关的多语言绑定
- 不要求宿主更换配置文件格式 —— 能力以工具协议的形式提供

## 三、实测效果

### 编辑保真度（公开基准）

固定语料 6 篇文档 / 14 个任务，语料与判定脚本都在仓库里，**不依赖网络、不依赖模型**：

| 策略 | 正确率 | 附带损伤率 | 误改率 | 拒绝准确率 | 注释保留 |
|------|--------|-----------|--------|-----------|---------|
| **意图契约 + 字符区间最小改动** | **8/8** | **0/8** | **0/14** | **6/6** | 8/8 |
| 换值后重写整个文件 | 0/8 | 8/8 | 3/14 | 3/6 | 8/8 |
| 按字段名找第一处匹配行 | 2/8 | 3/8 | 7/14 | 2/6 | 3/8 |

- **附带损伤**的口径是：目标值改对了，但改动区间之外的前缀与后缀**不再逐字节相同**。
- **拒绝准确率**要求错误码与期望一致 —— 拒绝理由不对不算通过，否则「一律拒绝」也能拿满分。
- 重写策略用的是本仓库自己的保注释格式化器，真实模型重写还会丢注释，
  所以它测到的附带损伤是**下界**。

```bash
cargo run -p verseconf-bench --release            # 跑基准并写结果
cargo run -p verseconf-bench --release -- --check # 门禁模式（CI 用）
```

方法与范围限制见 [benchmark/README.md](benchmark/README.md)。

### 解析性能

本机一次 release 运行（Windows 10 / AMD64 / rustc 1.98.1，每个格式 5 轮取中位数）：

| 数据集 | VerseConf | TOML | JSON | TOML/VCF | JSON/VCF |
|---------|-----------|------|------|----------|----------|
| small (426B) | 9.55μs | 13.51μs | 1.96μs | 1.42x | 0.20x |
| medium (2.5KB) | 78.51μs | 96.99μs | 13.89μs | 1.24x | 0.18x |
| large (24.8KB) | 756.34μs | 950.42μs | 197.13μs | 1.26x | 0.26x |
| xlarge (263KB) | 6.82ms | 8.49ms | 2.46ms | 1.25x | 0.36x |

VerseConf 的解析速度约为 TOML 的 **1.24–1.42 倍**；`serde_json` 比三者都快，
VerseConf 比它慢约 2.8–5.0 倍。三种格式的语料是同一份配置的等价写法。

```bash
cargo run -p verseconf-compare --bin generate_test_data
cargo run --release -p verseconf-compare --bin benchmark -- --json compare/benchmark_results.json
python3 compare/generate_charts.py
```

数字的唯一来源是 `compare/benchmark_results.json`；图表脚本不内置任何备用数据，
基准在数据缺失或解析失败时直接报错退出，不会退化成 0。
完整报告见 [compare/performance_charts.md](compare/performance_charts.md)。

## 四、安装

### 命令行

```bash
cargo install verseconf-cli     # 可执行文件叫 verseconf
```

> 包名是 `verseconf-cli`，命令名是 `verseconf` —— 就像 `ripgrep` 包装出 `rg` 命令。

### 作为库

```bash
cargo add verseconf-core
```

### 工具协议服务端（Agent 宿主）

```bash
cargo install verseconf-mcp
verseconf-mcp --list-tools      # 离线查看工具契约
```

### 编辑器语言服务器

```bash
cargo install verseconf-lsp
```

### 从源码

```bash
git clone https://github.com/Baixu22/Verse-conf.git
cd Verse-conf
cargo test --workspace          # 唯一一条命令完成构建与测试
```

### 其他分发形态的状态

| 形态 | 状态 |
|------|------|
| crates.io（四个 crate） | ✅ v0.1.0 已发布，干净环境 `cargo install verseconf-cli` 已复验 |
| 文档（docs.rs） | ✅ [docs.rs/verseconf-core](https://docs.rs/verseconf-core) |
| VSCode 扩展 `.vsix` | ⚠️ 由 CI 产出并上传为构建产物，仓库内不含二进制；见 [九、编辑器支持](#九编辑器支持) |
| npm 包 `verseconf` | ❌ **已发布的 0.1.0 不可用**（包内缺 `pkg/` 目录，`require` 与 `import` 两条入口都失败）。修复版 0.2.0 已在本地构建并通过打包验收，尚未发布 |
| 浏览器 CDN | ❌ 未发布。`pkg-web/` 产物未包含在任何已发布包中 |

## 五、快速开始

```bash
verseconf parse config.vcf                    # 解析并打印结构
verseconf validate config.vcf                 # 校验（含 schema）
verseconf validate config.vcf --strict        # 拒绝未声明字段
verseconf format config.vcf                   # 格式化（保注释，幂等）
verseconf format config.vcf --ai-canonical    # AI 友好规范形式
verseconf audit config.vcf                    # 安全审计
verseconf doc config.vcf                      # 由 schema 生成文档
```

### 一个完整例子

下面这份配置通过 `parse`、`validate` 与 `validate --strict` 三重验证：

```vcf
#@schema {
  version = "1.0"

  app_name {
    type = "string"
    required = true
    llm_hint = "lowercase with hyphens"
  }

  port {
    type = "integer"
    default = 8080
    range = (1024..65535)
  }

  server {
    type = "table"
    host { type = "string" }
    workers { type = "integer" }
  }

  servers {
    type = "array"
  }
}

app_name = "my-service"
port = 8080

server {
  host = "${HOST|127.0.0.1}"   # 需要 --env 才会插值
  workers = 4
}

[[servers]]
name = "primary"
weight = 10

[[servers]]
name = "replica"
weight = 20
```

### 拆分到多个文件

```vcf
@include "database.vcf" merge=deep_merge
```

合并策略支持 `override`（默认）、`append`、`merge`、`deep_merge`；
写成别的值会在解析阶段报错并指出列号，而不是静默忽略。

## 六、配置语言

### 已验证可用

| 语法 | 说明 |
|------|------|
| `key = value` | 标量：字符串、整数、浮点、布尔、数组、内联表 |
| `block { ... }` | 嵌套表 |
| `[[name]]` | 命名数组表，允许多个同名元素（供 `match` 定位） |
| `# 注释` | 行注释，格式化与编辑都不丢 |
| `#@key = value` | 行尾元数据，随值一起保留 |
| `#@schema { ... }` | 类型、`required`、`default`、`range`、`enum`、`sensitive`、`llm_hint` |
| `@include "f.vcf" merge=...` | 文件合并，带循环检测与深度限制 |
| `${VAR\|default}` | 环境变量插值。**必须写在引号内**，且需要 `--env` |
| `{{VAR}}` | 模板变量，配 `verseconf template render/list/validate/generate` |

错误信息带 `文件:行:列`，并给出上下文，例如：

```
app.vcf:2:3: validation error: value 99999 for field 'port' exceeds maximum 65535
```

### 尚未实现

> 这一节存在的原因：早期文档按 SPEC 写了一些**语法上无法解析**的写法。
> 下面这些在 SPEC 里有描述，但**当前实现不支持**，照抄会直接报错。

| 写法 | 实际行为 |
|------|----------|
| `${port + 1}`、`${30s + 500ms}` 等裸表达式 | **词法错误** `unexpected character: '$'`。AST 里有 `BinaryOp` / `UnitValue` 与求值器，但没有任何语法能产生它们 |
| `${ENV:HOSTNAME}` | 词法错误。只有 `${VAR\|default}` 一种形式，且在引号内 |
| `${VAR:default}`、`${VAR or default}` | 同上，不支持 |
| 热重载 | 只有库内 `HotReload` 实现，**没有任何用户入口**（命令行无对应子命令） |
| `#@template` / `#@use` 模板继承 | 未实现。模板是 `{{VAR}}` + `verseconf template` 子命令 |

完整的语法规范见 [docs/SPECIFICATION.md](docs/SPECIFICATION.md)，
其中标注为未实现的条目请以本节为准。

## 七、命令行参考

| 命令 | 用途 |
|------|------|
| `verseconf parse <文件>` | 解析并打印结构；`--no-include` 关闭 `@include` 展开 |
| `verseconf validate <文件>` | 校验（含 schema）；`--strict` 拒绝未声明字段；`--fix --write` 应用安全修复 |
| `verseconf format <文件>` | 格式化；`-o` 输出到文件；`--ai-canonical` 规范形式；`--include` 合并 include；`--env` 环境变量插值 |
| `verseconf audit <文件>` | 安全审计（通配绑定、明文口令等），输出稳定规则码 |
| `verseconf doc <文件>` | 由 schema 生成文档 |
| `verseconf diff <a> <b>` | 比较两份配置 |
| `verseconf env <子命令>` | 环境管理 |
| `verseconf template <子命令>` | `render` / `list` / `validate` / `generate` |
| `verseconf version <子命令>` | 版本管理 |

两条值得单独说的性质：

- `format` 是**幂等**的，且保留注释与 `#@` 元数据；
- `validate --fix` 对已经合法的文件**逐字节零改动**。

## 八、给 Agent 宿主接入

`verseconf-mcp` 把四个能力暴露成宿主可直接发现与调用的工具：

| 工具 | 作用 |
|------|------|
| `verseconf_validate` | 解析 + schema 校验，返回带行列号的错误 |
| `verseconf_audit` | 安全审计，返回稳定错误码 |
| `verseconf_apply_edit` | 按编辑计划做字符区间最小改动，写入前双重校验 |
| `verseconf_edit_range` | 直接替换指定字符区间（更底层的入口） |

失败时返回**结构化的拒绝原因**，而不是一段自然语言。

```json
{
  "mcpServers": {
    "verseconf": {
      "command": "verseconf-mcp",
      "args": []
    }
  }
}
```

宿主零安装的形态：同一套工具实现已编译成 WebAssembly，JS 运行时可直接加载，
宿主不需要 Rust 工具链。但**该 npm 包目前不可用**，见
[四、安装](#四安装) 的状态表。

完整工具契约与协议细节见 [docs/MCP.md](docs/MCP.md)。

## 九、编辑器支持

VSCode 扩展提供 `.vcf` 的语法高亮，并通过语言服务器接入诊断、补全、悬停、
跳转定义、查找引用与语义高亮。扩展包按平台自带语言服务器
（`win32-x64` / `linux-x64` / `darwin-x64` / `darwin-arm64`）。

扩展包由 [.github/workflows/extension.yml](.github/workflows/extension.yml)
的流水线产出：矩阵构建四份语言服务器 → 聚合 → 打包为 `verseconf-<版本>.vsix`
并作为构建产物上传。**仓库内不含 `.vsix` 二进制**，需要从流水线产物下载后安装：

```bash
code --install-extension verseconf-<版本>.vsix
```

本地开发、打包与验收见
[extensions/verseconf-vscode/README.md](extensions/verseconf-vscode/README.md)。

> **已知的占位实现**：命令 `verseconf.validate` 只弹提示，
> `verseconf.schema.generate` 提示 coming soon；`verseconf.format` 转发给
> `editor.action.formatDocument`，但语言服务器未提供 `documentFormattingProvider`，
> 因此该命令目前不会真正格式化。实时诊断与补全由语言服务器正常提供。

## 十、项目结构

```
Verse-conf/
├── crates/
│   ├── verseconf-core/     # 核心库：lexer / parser / AST / schema / 审计 / 编辑引擎
│   ├── verseconf-cli/      # 命令行（可执行文件 verseconf）
│   ├── verseconf-mcp/      # 工具协议服务端
│   ├── verseconf-lsp/      # 语言服务器
│   ├── verseconf-bench/    # 编辑保真度基准（不发布）
│   └── verseconf-test/     # 集成测试与性能基准（不发布）
├── benchmark/              # 公开基准的语料、任务与方法论
├── compare/                # 解析性能对比（不发布）
├── docs/                   # 规范、教程、工具协议文档
├── examples/               # 示例配置（含 common/ environments/ features/）
├── extensions/             # VSCode 扩展
├── integrations/           # WebAssembly 与 JavaScript 包
└── .github/workflows/      # CI 与扩展打包流水线
```

核心库里几个值得知道的位置：

| 模块 | 路径 | 职责 |
|------|------|------|
| Lexer | `crates/verseconf-core/src/lexer/` | 词法分析，含 CRLF / 孤立 CR 处理 |
| Parser | `crates/verseconf-core/src/parser/` | 语法分析，含 `@include` 与合并策略 |
| AST | `crates/verseconf-core/src/ast/` | 语法树与源码区间 |
| Edit | `crates/verseconf-core/src/edit/` | **编辑契约与字符区间最小改动** |
| Semantic | `crates/verseconf-core/src/semantic/` | 校验、schema、环境变量插值 |
| Engine | `crates/verseconf-core/src/engine/` | 审计、合并、格式化、缓存、模板 |

## 十一、开发

```bash
cargo test --workspace                                       # 全部测试
cargo fmt --all -- --check                                   # 格式门禁
cargo clippy --workspace --all-targets -- -D warnings        # 静态检查门禁
cargo bench                                                  # 性能基准
cargo run -p verseconf-bench --release -- --check             # 编辑保真度门禁
```

CI 有 8 个**阻塞**作业：`build-and-test`、`format`、`clippy`、`examples`、
`distribution`、`fidelity-benchmark`、`performance-benchmark`、`wasm-distribution`；
另有扩展流水线的 5 个作业。

### 发布

按依赖顺序，`verseconf-core` 必须第一个发（其余 crate 依赖 registry 上的它）：

```bash
cargo publish -p verseconf-core
cargo publish -p verseconf-cli
cargo publish -p verseconf-mcp
cargo publish -p verseconf-lsp
```

`cargo package --workspace` 可以在不发布的情况下检查打包内容与清单是否合法。
发布前建议先跑 `cargo publish --dry-run -p verseconf-core`。

### 基准数据的维护

性能数字**只能来自真实运行**。`compare/generate_charts.py` 不内置任何备用数据，
读不到 `benchmark_results.json` 就直接失败；CI 的 `performance-benchmark` 作业
会主动挪走数据集，断言基准必须失败而不是输出全零。

## 十二、文档索引

| 文档 | 内容 |
|------|------|
| [docs/SPECIFICATION.md](docs/SPECIFICATION.md) | 语言规范（注意其中未实现条目的标注） |
| [docs/TUTORIAL.md](docs/TUTORIAL.md) | 入门教程 |
| [docs/MCP.md](docs/MCP.md) | 工具协议契约与宿主接入 |
| [benchmark/README.md](benchmark/README.md) | 编辑保真度基准的方法论与范围限制 |
| [compare/performance_charts.md](compare/performance_charts.md) | 解析性能报告（由基准生成） |
| [crates/verseconf-core/README.md](crates/verseconf-core/README.md) | 核心库 API 与最小改动示例 |
| [crates/verseconf-cli/README.md](crates/verseconf-cli/README.md) | 命令行用法 |
| [crates/verseconf-mcp/README.md](crates/verseconf-mcp/README.md) | 工具协议服务端 |
| [crates/verseconf-lsp/README.md](crates/verseconf-lsp/README.md) | 语言服务器 |
| [extensions/verseconf-vscode/README.md](extensions/verseconf-vscode/README.md) | 扩展的开发与打包 |
| [examples/](examples/) | 可直接运行的示例配置 |

## 十三、已知限制与路线

**当前限制**

1. 编辑保真度基准只覆盖 `set` 与拒绝路径；`insert` / `delete` 的最小改动性质
   目前只由单元测试覆盖。
2. 基准使用确定性替身策略，不含真实模型 —— 它测的是写法的固有代价，
   不是某个模型的得分。
3. 语言服务器未提供文档格式化能力，扩展里的格式化命令因此不生效。
4. 分发路径仍在收口：npm 包未发布可用版本，浏览器 CDN 形态未发布。

**路线**

- 把 `insert` / `delete` 与更大语料纳入基准
- 发布修复后的 npm 包，使「宿主零安装」这条路径对外成立
- 语言服务器补齐文档格式化

## 许可证

MIT OR Apache-2.0 —— 见 [LICENSE-MIT](LICENSE-MIT) 与 [LICENSE-APACHE](LICENSE-APACHE)。

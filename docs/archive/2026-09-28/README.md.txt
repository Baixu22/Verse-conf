# VerseConf

**简体中文** | [English](README.en.md)

> **Agent 编辑配置的确定性执行层。**
> 模型只给出「改哪个字段、改成什么」，由确定性代码完成字符区间级最小改动，
> 写入前做 schema 与安全双重校验；**无法确定时拒绝，而不是猜测**。
>
> ⚠️ **确认性复验没有测到净收益**：硬门禁不通过，本项目已按预登记收尾为
> **小型可靠配置编辑库**，不再扩大平台叙事。安全轴上后来补了一次直接实验
> （真实模型、真实文档、开跑前预登记）：门禁确实挡住了模型自发的风险（11 次里挡 6 次），
> 但误拒 3.0% 超过事先登记的 2% 预算，**不能按现状默认开启**。逐条对照见
> [十三、已知限制](#十三已知限制与路线) 第 6 条与
> [benchmark/gate-increment/VERDICT.md](benchmark/gate-increment/VERDICT.md)。

[![crates.io](https://img.shields.io/crates/v/verseconf-core.svg)](https://crates.io/crates/verseconf-core)
[![CI](https://github.com/Baixu22/Verse-conf/actions/workflows/ci.yml/badge.svg)](https://github.com/Baixu22/Verse-conf/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20or%20Apache--2.0-green.svg)](LICENSE)

---

## 目录

- [一、问题](#一问题)
- [二、意图执行协议](#二意图执行协议)
- [三、实测数据：编辑保真度](#三实测数据编辑保真度)
- [四、接入方式](#四接入方式)
- [五、安装](#五安装)
- [六、快速开始](#六快速开始)
- [七、命令行参考](#七命令行参考)
- [八、配置语言参考](#八配置语言参考)
- [九、编辑器支持](#九编辑器支持)
- [十、项目结构](#十项目结构)
- [十一、开发](#十一开发)
- [十二、文档索引](#十二文档索引)
- [十三、已知限制与路线](#十三已知限制与路线)
- [附录 A：解析性能对比](#附录-a解析性能对比)

---

## 一、问题

让模型改配置，今天的通行做法是把**整个文件**交给它重写。代价可以量化：

| 代价 | 具体表现 |
|------|----------|
| 改动之外的字节也被改 | 只想换一个端口号，整个文件被重新排版 |
| 注释与元数据丢失 | 行尾注释、`#@` 标注、键序被抹掉 |
| 该拒绝的照单执行 | 目标有歧义、改动破坏 schema、引入安全风险时不会停下 |
| 无法审计 | 只留下一份新文件，说不清改了什么、为什么改 |

这不是提示词问题，而是**接口问题**：只要接口的输入是「新文件」、输出也是「新文件」，
上面四件事就无法在协议层面被排除。VerseConf 把接口换成**意图**。

## 二、意图执行协议

一次编辑走四步。模型只参与第一步，其余三步全是确定性代码：

| 步骤 | 谁来做 | 产物 |
|------|--------|------|
| 1. 意图契约 | 模型 | 编辑计划（JSON）：改哪个字段、改成什么、为什么改、前置条件是什么 |
| 2. 定位 | 确定性代码 | 目标值在源码里的字符区间 |
| 3. 最小改动 | 确定性代码 | 只替换该区间的字节，区间之外的字节零变化 |
| 4. 双重校验 | 确定性代码 | 改动后仍可解析、仍通过结构/schema 校验、且未引入新的高危安全**实例**（按「规则 + 位置」比较，不是只看该规则是否已经出现过）；跨文件编辑还会用候选内容重建合并后的生效配置再校验一次；任一失败即拒绝 |

契约是公开的：[`crates/verseconf-core/schemas/edit-plan.schema.json`](crates/verseconf-core/schemas/edit-plan.schema.json)。
四个要点：

- **列表按名称定位，不按下标**：`"path": [{ "key": "servers", "match": { "name": "primary" } }, "ip"]`。
  命中多个元素时拒绝，而不是挑第一个。
- **`expect` 是前置条件**：当前值与预期不符就拒绝，避免基于过期假设改动。
- **只改目标值的字节区间**：注释、`#@` 元数据、键序、空行、缩进风格、CRLF 全部原样保留。
- **失败即拒绝**：拒绝时返回体里**根本没有 `source` 字段**，调用方拿不到半成品。

### 2.1 最小改动：一个可复现的例子

模型只需要给出这一段意图：

```json
{ "version": "1.0",
  "edits": [{ "op": "set", "path": ["server", "host"], "value": "0.0.0.0",
              "reason": "改为监听所有网卡" }] }
```

输入：

```vcf
server {
  host = "127.0.0.1"   # 监听地址
  port = 8080          #@ range(1024..65535)
}
```

工具返回的 `source`：

```vcf
server {
  host = "0.0.0.0"   # 监听地址
  port = 8080          #@ range(1024..65535)
}
```

按字节口径核对：被替换的区间就是 `"127.0.0.1"`（源码第 18..29 字节），
**区间之前的前缀与之后的后缀逐字节相同**，行尾注释与 `#@` 元数据原样保留，
长度差恰好是 `-2`（`"0.0.0.0"` 比 `"127.0.0.1"` 短 2 字节）。
CRLF 文档同理：换行符保持 `\r\n`，不会被规范化成 LF。

本地复现（不需要模型，也不需要网络）：

```rust
use verseconf_core::{apply_edit_plan, EditPlan};
let src = "server {\n  host = \"127.0.0.1\"   # 监听地址\n  port = 8080          #@ range(1024..65535)\n}\n";
let plan: EditPlan = serde_json::from_str(r#"{"version":"1.0","edits":[{"op":"set","path":["server","host"],"value":"0.0.0.0"}]}"#)?;
let out = apply_edit_plan(src, &plan)?;   // 拒绝时返回 Err，不返回半成品
assert_eq!(out.source, "server {\n  host = \"0.0.0.0\"   # 监听地址\n  port = 8080          #@ range(1024..65535)\n}\n");
```

`cargo add verseconf-core` 之后即可运行；同一段代码以
[`crates/verseconf-core/examples/deterministic_edit.rs`](crates/verseconf-core/examples/deterministic_edit.rs)
的形式被 `cargo test --workspace` 编译，所以它不会和实现漂移。
命令行冒烟测试（`cargo install verseconf-mcp` 之后离线可用）：

```bash
verseconf-mcp --call verseconf_apply_edit \
  '{"source":"server {\n  host = \"127.0.0.1\"   # 监听地址\n}\n","plan":{"version":"1.0","edits":[{"op":"set","path":["server","host"],"value":"0.0.0.0"}]}}'
```

宿主侧的等价形态是 `applyEdit(source, plan)`（WebAssembly 分发，见
[四、接入方式](#四接入方式)）。编辑工具**只返回文本，不写文件**，落盘由宿主决定。

### 2.2 拒绝路径

fail-closed 不是口号：基准里的 6 个拒绝任务要求返回**稳定错误码**，且拒绝理由必须与期望一致才算通过
（否则「一律拒绝」也能拿满分）。

| 错误码 | 触发条件 | 实测返回 |
|--------|----------|----------|
| `target_ambiguous` | 命名列表命中多个同名元素 | 目标有歧义：`servers[name="primary"].ip` 命中了 2 个元素，必须唯一命中 |
| `target_not_found` | 路径不存在 | 目标不存在：`server.missing` |
| `search_incomplete` | include 图超过搜索上限，无法证明目标唯一 | 搜索不完整：`port` 的 include 图超过上限 64，只扫描了 64 个文件，无法证明目标唯一 |
| `expectation_mismatch` | 当前值与 `expect` 不符 | 前置条件不符：`server.port` 期望 1234，实际 8080 |
| `validation_failed` | 改动后不满足 schema（含合并后的生效配置） | 改动后校验失败：`type mismatch for field 'port': expected integer, found "not-a-number"` |
| `security_rejected` | 改动引入新的高危安全实例 | 改动引入新的安全风险：`<result>`（`SEC-005 @ tls.ssl_verify`）；`details.instances` 指出是哪个字段 |
| `invalid_plan` | 编辑计划不满足契约 | 编辑计划不合法（`details.violations[].code = unsupported_version`） |

会走到 `security_rejected` 的规则只有四条：`SEC-SENS-001`（写死在文件里的凭据）、
`SEC-SENS-002`（凭据嵌在 URL 值里，例如 `https://user:pass@host/`）、
`SEC-001`（弱摘要 md5/sha1）、`SEC-005`（关闭证书校验）。其余规则（不安全端口、
调试开关、通配绑定等）是 Medium/Low，只告警不阻断。
值写成 `${ENV_VAR}` 或全大写环境变量名时按**引用**处理，不阻断——那正是审计建议的写法。

拒绝时的返回体形如：

```json
{ "code": "target_not_found",
  "message": "目标不存在：server.missing",
  "details": { "path": "server.missing" } }
```

**没有 `source` 字段** —— 调用方即使想写，也写不出半成品。
完整的错误码表（含 `invalid_arguments` / `parse_failed` / `unsupported_target` 等）见
[docs/MCP.md](docs/MCP.md)。

### 2.3 不做什么

- 不追求成为通用配置语言，不在表达力上与 Pkl / CUE / KCL 竞争
- 不为「生态丰富」而堆砌语言无关的多语言绑定
- 不要求宿主更换配置文件格式 —— 能力以工具协议的形式提供

## 三、实测数据：编辑保真度

固定语料 8 篇文档 / 27 个任务（8 个单条 `set`、6 个 `insert`/`delete`、
3 个多条编辑、4 个跨 `@include`、6 个拒绝），语料与判定脚本都在仓库里，
**不依赖网络、不依赖模型**。语料指纹 `162777399290a695`，判定口径版本 `1.2`：

| 策略 | 正确率 | 附带损伤率 | 误改率 | 拒绝准确率 | 注释保留 |
|------|--------|-----------|--------|-----------|---------|
| **意图契约 + 字符区间最小改动** | **21/21** | **0/21** | **0/27** | **6/6** | 21/21 |
| 换值后重写整个文件 | 0/21 | 21/21 | 3/27 | 3/6 | 19/21 |
| 按字段名找第一处匹配行 | 6/21 | 6/21 | 10/27 | 2/6 | 8/21 |

- **附带损伤**的口径是「改动之外的字节零变化」，三种操作各有其形态：`set` 要求原值区间之外的
  前缀与后缀逐字节不变；`insert` 要求**原文一个字节都没被动过**；`delete` 要求**只是少了一段**。
  只比对前后缀还不够——删掉目标行的同时把相邻一行也删掉，在字节上同样是连续删除——
  所以 `insert`/`delete` 还要比对**键的集合**：除目标之外，两份文档的字段路径必须完全一致。
- **多条编辑**用另一套口径：`expect.targets` 逐条声明每个目标，判定要求每个目标都成立、
  **目标之外的字节逐字节保留且顺序不变**（允许改动的区间按原文 AST 算出：`set` 是值区间，
  `delete` 是目标所在整行）、键集合只差这些目标、**改动的行只能是目标行**，且 `set` 目标行的
  首尾（缩进、行尾注释、`#@` 元数据）必须逐字节保留。其中「目标之外的字节逐字节保留」
  是唯一挡得住**同名字段**的一条：根级 `port` 与 `server.port` 共用叶子名，只看
  「改动的行提到 `port`」的话，改错那一个也会通过。含 `insert` 的计划退回行级口径。
- **跨 `@include` 编辑**：`expect.target_file` 声明期望被改动的文件；策略从入口文件出发，
  允许顺着 include 走到别的文件，判定拿**目标文件**的内容比对，并要求改的正是那个文件。
  语料把文件树以内存形式交给策略（不读磁盘），策略因此仍是纯函数。
- **朴素差异实现在什么时候够用**：`line-diff` 在根表的单条插入与删除上是对的（不需要作用域信息，
  也不碰行内细节），但往嵌套表插键时会把键追加到**文件末尾**（目标根本不存在）、
  删除命名列表元素时按字段名找**第一处**匹配而删错了元素、整行替换会吃掉目标行的行尾注释与
  `#@` 元数据；**跨 `@include` 的 4 个任务里 3 个直接误拒**（它不跟随 include，
  只在入口文件里找）。这些失败都被逐任务记录下来，而不是一句笼统的"不够好"。
- **拒绝准确率**要求错误码与期望一致 —— 拒绝理由不对不算通过。
- 重写策略用的是本仓库自己的保注释格式化器，真实模型重写还会丢注释，
  所以它测到的附带损伤是**下界**。
- 三种策略在重复运行与乱序运行下都逐字节确定；门禁 `--check` 要求被测实现全对且结果确定。

```bash
cargo run -p verseconf-bench --release            # 跑基准并写结果
cargo run -p verseconf-bench --release -- --check # 门禁模式（CI 用）
```

产物是 `benchmark/results/latest.md`（人读）与 `benchmark/results/latest.json`（机器可读，含指纹）。
方法与范围限制见 [benchmark/README.md](benchmark/README.md)。

## 四、接入方式

`verseconf-mcp` 把五个能力暴露成宿主可直接发现与调用的工具：

| 工具 | 作用 |
|------|------|
| `verseconf_validate` | 解析 + schema 校验，返回带行列号的错误 |
| `verseconf_audit` | 安全审计，返回稳定规则码 |
| `verseconf_apply_edit` | 按编辑计划做字符区间最小改动，写入前双重校验；给 `path` 时在 `include` 图里定位目标，响应里带出被改动的文件与合并视图的校验状态 |
| `verseconf_edit_range` | 直接替换指定字符区间（更底层的入口） |
| `verseconf_check_write` | **写前检查**：给定原文与候选文本，判断这次改动是否允许落盘。不关心候选怎么产生——宿主可以保留自己的编辑方式，只在写盘前过这一道。支持 `.vcf`、**TOML** 与 **JSON/JSONC**（`format` 参数）；schema 可以旁挂自建 `#@schema`，也可以直接用**标准 JSON Schema**（draft-07 / 2020-12），或按配置自己的 `$schema` 去取 |

失败时返回**结构化的拒绝原因**，而不是一段自然语言（错误码见 [2.2](#22-拒绝路径)）。

### 本机二进制

```bash
cargo install verseconf-mcp
verseconf-mcp --list-tools      # 离线查看工具契约
```

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

### WebAssembly（宿主无需 Rust 工具链）

同一套工具实现已编译成 WebAssembly，JS 运行时可直接加载，宿主不需要 Rust 工具链；
产物里带一个 stdio 服务端 `verseconf-mcp-wasm.mjs`。

**这条分发路径没有对外发布**：npm 包已放弃发布（见 [五、安装](#五安装) 的状态表），
所以要用它只能从源码构建：

```bash
cd integrations/verseconf-wasm/js-api && npm install && npm run build
```

两条路径共用同一批函数，同一串请求的响应逐字节相同（`npm run test:parity` 做这项比对）。
**但共用代码不等于能力相同**：wasm 目标没有文件系统，`tools/list` 在 wasm 上不声明
`path`，真传了会返回 `unsupported_on_platform`；要按路径改文件，得由宿主自己读文件、
把内容作为 `source` 传进来。

完整工具契约与协议细节见 [docs/MCP.md](docs/MCP.md)。

## 五、安装

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
| crates.io（已发布五个 crate） | ✅ **v0.3.0 已发布**（core / cli / lsp / mcp / toml）。干净环境 `cargo install verseconf-cli --version 0.3.0` 已复验：`edit` 命令可用，同规则的新增高危实例会被拒绝。0.1.0 早于全部修复（CLI 没有 `edit`，core 仍按规则而非实例比较安全发现），请勿使用 |
| crates.io 上的 `verseconf-json` | ⚠️ **未发布**。JSON/JSONC 写前检查目前只在源码树与本地构建的产物里可用：已发布的 `verseconf-mcp` 0.3.0 早于这个适配层。`verseconf-mcp` 现在依赖它，所以**下一次发布必须先发布 `verseconf-json`**（`cargo package -p verseconf-mcp` 在它出现在 registry 之前会失败） |
| 文档（docs.rs） | ✅ [docs.rs/verseconf-core](https://docs.rs/verseconf-core) |
| VSCode 扩展 `.vsix` | ⚠️ 由 CI 产出并上传为构建产物，仓库内不含二进制；见 [九、编辑器支持](#九编辑器支持) |
| npm 包 `verseconf` | ❌ **已放弃发布**。registry 上的 0.1.0 不可用（包内缺 `pkg/` 目录，`require` 与 `import` 两条入口都失败）；修复版已在仓库里构建并通过打包验收（12/12），但发布账号已不可用，不再发布 |
| 浏览器 CDN | ❌ 未发布。`pkg-web/` 产物未包含在任何已发布包中 |

## 六、快速开始

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

## 七、命令行参考

| 命令 | 用途 |
|------|------|
| `verseconf parse <文件>` | 解析并打印结构；`--no-include` 关闭 `@include` 展开 |
| `verseconf validate <文件>` | 校验（含 schema）；`--strict` 拒绝未声明字段；`--fix --write` 应用安全修复 |
| `verseconf format <文件>` | 格式化；`-o` 输出到文件；`--ai-canonical` 规范形式；`--include` 合并 include；`--env` 环境变量插值 |
| `verseconf audit <文件>` | 安全审计（通配绑定、明文口令等），输出稳定规则码 |
| `verseconf doc <文件>` | 由 schema 生成文档 |
| `verseconf schema generate <文件>` | 由配置**推断** `#@schema` 块；默认打印，`--write` 写回（已有 schema 时拒绝） |
| `verseconf edit <文件> --plan <计划.json>` | 按意图契约改动配置，**跨 `@include` 定位目标**；默认只打印结果，`--write` 才落盘（落盘前比较内容版本并原子替换；合并视图未校验时拒绝写入，除非 `--allow-unvalidated`） |
| `verseconf watch <文件>` | 监视文件，每次改动后重新校验并报告（含安全审计结论）；`--max-events N` 在处理 N 次改动后退出 |
| `verseconf diff <a> <b>` | 比较两份配置 |
| `verseconf env <子命令>` | 环境管理 |
| `verseconf template <子命令>` | `render` / `list` / `validate` / `generate` |
| `verseconf version <子命令>` | 版本管理 |

四条值得单独说的性质：

- `format` 是**幂等**的，且保留注释与 `#@` 元数据；
- `validate --fix` 对已经合法的文件**逐字节零改动**；
- `edit` 接受的是**意图**而不是一份新文件，并且会**跨 `@include` 定位目标**：
  目标落在哪个文件里由确定性代码判定，只改那个文件的目标字节区间，其余文件一个字节都不动。
  定位是 fail-closed 的——目标在多个文件里都能定位到时**拒绝**（`target_ambiguous`），
  而不是猜一个：

  ```
  $ verseconf edit examples/with_include.vcf --plan set-port.json
  已应用 1 条编辑；目标文件：examples/common/base.vcf
    set server.port: 8080 → 9443
  ```
- `watch` 与编辑器里的实时诊断互补：LSP 覆盖"在编辑器里改"，`watch` 覆盖**没有编辑器**
  的场景——Agent 在后台改文件、脚本生成配置、CI 里等待一次保存。改动后**成功与失败都会输出**，
  解析失败时如实报出 `行:列` 与原因，而不是保留上一次的成功结果：

  ```
  [watch] 监视 app.vcf（改动后自动重新校验；Ctrl-C 退出）
  [watch] 10:24:01  通过：2 个根条目，安全审计无发现
  [watch] 10:24:07  2:5: parse error: expected '=' or ':', found bare_key(line)
  [watch] 10:24:11  通过：2 个根条目，安全审计无发现
  ```

## 八、配置语言参考

> 这一节是参考手册，不是主线。语言只是承载意图的载体：对外承诺的是
> [二、意图执行协议](#二意图执行协议) 与 [三、实测数据](#三实测数据编辑保真度)，不是语法表达力。

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
| `${VAR\|default}` | 环境变量插值。**必须写在引号内**，且需要 `--env`；默认值分隔符推荐 `\|`，同时兼容 `:` 与 ` or `（见下） |
| `{{VAR}}` | 模板变量，配 `verseconf template render/list/validate/generate` |

错误信息带 `文件:行:列`，并给出上下文，例如：

```
app.vcf:2:3: validation error: value 99999 for field 'port' exceeds maximum 65535
```

### 尚未实现

> 这一节存在的原因：早期文档按 SPEC 写了一些**当前实现不支持**的写法。
> 下面这些在 SPEC 里有描述，但照抄会失败，或得到不是预期的值。

| 写法 | 实际行为 |
|------|----------|
| 裸表达式 `${port + 1}`、`${30s + 500ms}` | **词法错误** `unexpected character: '$'`（`parse_error`）。写在引号内不报错，但那会走环境变量插值（需要 `--env`），**不是表达式求值**；AST 里有 `BinaryOp` / `UnitValue` 与求值器，但没有任何语法能产生它们 |
| 裸形式 `${ENV:HOSTNAME}` | **裸形式**与上一条相同：词法错误。**写在引号内可以解析**，所以它不算词法错误；但 `:` 在插值器里是「变量名:默认值」的兼容分隔符，`"${ENV:HOSTNAME}"` 实际被解析成「变量 `ENV`，默认值 `HOSTNAME`」，**不是**「读取环境变量 `HOSTNAME`」。照抄不报错，但取到的值不是 SPEC 想表达的 |
| `#@template` / `#@use` 模板继承 | 未实现。模板是 `{{VAR}}` + `verseconf template` 子命令 |

本节结论由本机实测得出：`validate` 对引号内的
`"${ENV:HOSTNAME}"`、`"${VAR:default}"`、`"${VAR or default}"` 均返回
`valid: true`；对裸形式返回 `parse_error` + `unexpected character: '$'`。
默认值分隔符三种写法都能解析（实现见
`crates/verseconf-core/src/semantic/env_interp.rs`），但文档只推荐 `${VAR|default}`。

完整的语法规范见 [docs/SPECIFICATION.md](docs/SPECIFICATION.md)，
其中标注为未实现的条目请以本节为准。

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

三个命令都走**真实能力**，不经过任何占位实现：

| 命令 | 实际行为 |
|------|----------|
| `verseconf.format` | 由语言服务器的 `documentFormattingProvider` 提供，复用核心库的保注释格式化器；注释与 `#@` 元数据保留、结果幂等。文档无法解析时**不做任何修改**，而不是给出一个猜出来的结果 |
| `verseconf.validate` | 读取语言服务器发布的实时诊断（与编辑器里看到的是同一份），报出首个问题的行列号并可跳到问题面板；服务端未运行时**如实告知**，不会假报"通过" |
| `verseconf.schema.generate` | 由当前文档推断 `#@schema { ... }` 块并插入到文件开头。推断只写能从值确定的类型与 `required`，**不猜** `range` / `enum` / `default` / `pattern`；文档已有 `#@schema` 时拒绝执行（一个文件两份 schema 会解析失败） |

推断逻辑在核心库里（`infer_schema`），同时以自定义请求 `verseconf/generateSchema`
暴露给扩展——扩展只依赖包里自带的语言服务器，不需要用户额外装命令行工具。

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
cargo test -p verseconf-test --test spec_status               # 规范状态标注门禁
```

CI 有 9 个**阻塞**作业：`build-and-test`、`format`、`clippy`、`examples`、
`distribution`、`spec-status`、`fidelity-benchmark`、`performance-benchmark`、
`wasm-distribution`；另有扩展流水线的 5 个作业。

`spec-status` 的作用是防止 [docs/SPECIFICATION.md](docs/SPECIFICATION.md) 的状态标注
再次漂移：它按 [docs/spec-status-probes.json](docs/spec-status-probes.json) 对每条声明
真的跑一次 `parse` / `validate`，并交叉核对速查表把该写法列在 ✅ 还是 🚧 分区。
「实现了标着未实现的特性」和「改坏了标着已实现的特性」都会让它失败。

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
| [docs/SPECIFICATION.md](docs/SPECIFICATION.md) | 语言规范（含已实现/未实现的状态标注） |
| [docs/TUTORIAL.md](docs/TUTORIAL.md) | 入门教程 |
| [docs/MCP.md](docs/MCP.md) | 工具协议契约、错误码表与宿主接入 |
| [crates/verseconf-core/schemas/edit-plan.schema.json](crates/verseconf-core/schemas/edit-plan.schema.json) | **编辑意图契约（公开 schema）** |
| [benchmark/README.md](benchmark/README.md) | 编辑保真度基准的方法论与范围限制 |
| [crates/verseconf-core/README.md](crates/verseconf-core/README.md) | 核心库 API 与最小改动示例 |
| [crates/verseconf-cli/README.md](crates/verseconf-cli/README.md) | 命令行用法 |
| [crates/verseconf-mcp/README.md](crates/verseconf-mcp/README.md) | 工具协议服务端 |
| [crates/verseconf-lsp/README.md](crates/verseconf-lsp/README.md) | 语言服务器 |
| [extensions/verseconf-vscode/README.md](extensions/verseconf-vscode/README.md) | 扩展的开发与打包 |
| [compare/performance_charts.md](compare/performance_charts.md) | 解析性能报告（由基准生成，见附录 A） |
| [examples/](examples/) | 可直接运行的示例配置 |

## 十三、已知限制与路线

**当前限制**

1. **跨 `@include` 的编辑已在核心库、`verseconf edit`、工具协议与基准里落地**
   （fail-closed 定位：目标在多个文件里都能定位到时拒绝，搜索被截断时返回
   `search_incomplete` 而不是宣称唯一）。语料目前是一层 include，
   更深的嵌套链与"目标位于被多层 include 合并出来的表里"尚未覆盖。
2. 基准使用确定性替身策略，不含真实模型 —— 它测的是写法的固有代价，
   不是某个模型的得分。
3. **JS / WebAssembly 分发没有对外发布**：npm 包已放弃发布（发布账号不可用），
   浏览器 CDN 形态也未发布。这两条路径只能从源码构建使用；命令行与库走 crates.io。
4. **解析性能不是本项目的优势**：相对 `serde_json` 慢 2.77–5.65 倍（见附录 A）。
   本项目的证据是编辑保真度，不是解析速度。
5. **合并视图的校验有条件**：只有合并器实际读到的文件集合与 include 图一致时才会返回
   `effective_view: "validated"`。嵌套 include 的基准目录与合并器的解析方式不同，
   此时返回 `not_validated`，`verseconf edit --write` 默认拒绝落盘。
6. **没有测到净收益，本项目已按预登记收尾为「小型可靠配置编辑库」**：确认性复验
   （1422 次真实模型运行，冻结的 holdout 语料 + 开跑前预登记的门槛）里**硬门禁不通过**——
   语义正确 88.8% / 字节保真 88.8%，低于两个对照的 97.5% / 90.7% 与 96.8% / 90.7%；
   静默误改 10 次（对照 9 / 0）；模型侧成本还贵 8.9%。逐条对照与边界见
   [benchmark/confirmation/VERDICT.md](benchmark/confirmation/VERDICT.md)。
7. **门禁在安全轴上第一次有了直接证据，但代价超预算**：消融实验（手写 8 条任务，5/5 挡住）
   与 79 条中立任务（0 误拒）都测不到真实表现，所以另做了一次预登记实验
   （`benchmark/gate-increment/`：12 条真实文档任务 × 2 档位 × 3 重复 = 72 次真实模型生成，
   配对两臂，开跑前冻结主指标与误拒预算）。结果：A 臂 11 次静默安全回归 → B 臂 5 次——
   它挡住了看得见的 6 次，却漏掉 5 次「凭据嵌在 URL 值里」（占全部风险的 45%），
   并把指向环境变量的名字（`token-env = "PANDAS_INTERNAL_UPLOAD_TOKEN"`，**审计建议的写法**）
   判成硬编码凭据；误拒 3.0% > 2% 预算，样本量也不足以证明更低。
   **裁决是「过度拦截」：不能按现状默认开启**（既不是「有增量」，也不是「没有增量」）。
   裁决指出的两条实现缺陷已修（新增阻断规则 `SEC-SENS-002`；环境变量名按引用处理），
   但本轮冻结数据未改——要让新规则产生新证据必须重新预登记再跑。逐条对照与边界见
   [benchmark/gate-increment/VERDICT.md](benchmark/gate-increment/VERDICT.md)。

   修完之后**按同一口径再测了一轮**（[benchmark/gate-increment-round2/](benchmark/gate-increment-round2/)，
   同一语料、开跑前重新预登记，唯一变化的变量是被测实现）：A 臂 9 次 → **B 臂 0 次**——
   结果谓词能看见的风险全部被挡住，第一轮点名的两类缺陷在真实候选上都不再出现
   （模型这轮仍写了 2 次 URL 内嵌凭据，两次都被新规则拦住）。**但预登记的误拒预算仍没达到**：
   评审判定 2 次误拒（3.0%），其中 1 次是确凿的新假阳性（`credential-provider = "cargo:token"`——
   值是凭据**提供者的名字**，不是秘密），另 1 次是把 `SSL_VERIFY = "false"` 这类**确实在关证书校验**
   的环境变量判成误拒，属有争议的口径。所以两轮的裁决都是「过度拦截」，
   但第二轮的数字说明：**门禁是有效的，剩下的问题集中在两处具体形态上，而不是「不起作用」**。
   见 [benchmark/gate-increment-round2/VERDICT.md](benchmark/gate-increment-round2/VERDICT.md)。

   把最后一条假阳性（`credential-provider = "cargo:token"` 这类「值指向凭据机制」的写法）
   也修掉之后，**又按同一口径测了第三轮**（[benchmark/gate-increment-round3/](benchmark/gate-increment-round3/)）：
   A 臂 8 次 → **B 臂 0 次**，三条确定性对照全部满足，开跑前登记的四条预期全部成立
   （B 臂无残留、两类已修误拒都不再出现、发生率同量级）。**但误拒反而变成 3 次（4.5%）**——
   三次全是**同一条刻意未改的判定**（模型把 `SSL_VERIFY = "false"` / `GIT_SSL_NO_VERIFY = "true"`
   这类环境变量写进配置，SEC-005 触发；它们确实在关证书校验）。三轮的 7 次「误拒」里
   6 次是这一条口径、1 次是已修的实现缺陷。
   所以结论是：**实现缺陷已经清零，剩下的全是口径争议**；在 2% 预算的口径不改的前提下
   这个预算达不到——要么把该形态登记为「预期拒绝」，要么接受预算不达标，两者都是产品决定。
   三轮冻结数据均未改动；要让新口径产生新证据必须重新预登记。
   见 [benchmark/gate-increment-round3/VERDICT.md](benchmark/gate-increment-round3/VERDICT.md)。

   为此又做了**第四轮**（[benchmark/gate-increment-round4/](benchmark/gate-increment-round4/)）：
   门禁实现不动，只把「误拒」的测量口径拆成两类——「规则说错了」（实现假阳性，
   计入 2% 预算）与「形态确实存在、只是用户不同意被拦」（政策拒绝，单独报告）。
   结果**证伪了我的预期、也证伪了这个新口径本身**：事实核对把 9 次拒绝里的 **7 次**
   判成"规则说错了"，而逐条人工核对后，其中至少 2 次明显是误标
   （一条是货真价实的 URL 内嵌凭据、一条是 `ghp_` 开头的字面量令牌），
   另 3 次落在"结构在、值是占位符"的灰区。同一批数据上旧口径只判 1 次误拒、新口径判 7 次、
   人工核对认为两边都有错——**测量工具自相矛盾，本轮没有回答"实现假阳性率是否在预算内"**。
   按预登记规则（实现假阳性 ≥2 即超预算）裁决仍是「过度拦截」，
   但结论里必须写明：**"门禁有 7 个实现假阳性"这个说法不成立**。
   四轮合起来能站住的只有一条：**有效性**——B 臂残留 5 → 0 → 0 → 0，
   模型自发写出的 15 次高风险全部被拦住。成本那一条量至今没被可靠测出，
   问题出在测量方法（三轮受政策判定污染、第四轮的 LLM 事实核对偏保守），不是门禁本身；
   要真正回答它必须换成**确定性的形态谓词**并重新预登记。
   见 [benchmark/gate-increment-round4/VERDICT.md](benchmark/gate-increment-round4/VERDICT.md)。

   测量这一环随后做完了（[classify.mjs](benchmark/gate-increment/classify.mjs)）：
   逐规则、逐实例核对「规则声称的形态到底在不在」，可复核、可复现，自检含**回放四轮全部真实拒绝**
   （零"无法判定"，第四轮被 LLM 误标的两条都判对了）。它顺带指出真实缺陷并已修：
   按当前实现重判四轮历史候选，**实现假阳性全部归零**——那些假阳性几乎全是**占位文案**
   （`your-password`、`REPLACE_WITH_...`、`YOUR_USERNAME:YOUR_PASSWORD`），
   门禁现在把「写着请替换的模板」按引用处理，而真凭据形态（`ghp_...`、`AKIA...`、
   `cargo-user:cargo-password`）仍然阻断。**但可引用的误拒率仍要等第五轮带 `--classify` 跑出来**：
   回顾性重判不是新测量。见 [benchmark/gate-increment-round4/VERDICT.md](benchmark/gate-increment-round4/VERDICT.md) 第七节。

   于是做了**第五轮**（[benchmark/gate-increment-round5/](benchmark/gate-increment-round5/)），
   成本只用确定性分类器计量、不调 LLM 评审。**这是五轮里第一次四条裁决规则同时满足**：
   A 臂 9 次静默安全回归 → B 臂 4 次（挡掉 5 次：写死的凭据 3 次、关闭证书校验 2 次），
   18 条对照全部满足，**实现假阳性 0 次**（预算 ≤1），5 次拒绝全部是「规则说对了、
   用户可能不同意被拦」的政策拒绝，逐个列出在裁决里。裁决是「**有增量**」。
   两条必须一起读的边界：(1) B 臂那 4 次残留是**占位模板 URL**
   （`sparse+https://username:password@…`）——正是占位豁免**故意**不再拦的那一类，
   而开跑前冻结的结果谓词 P2 的朴素正则仍把它们当风险；按实现现在对风险形态的定义残留是 0，
   两个数都写在裁决第三节，引用时要说明用哪一个——这笔**测量口径的债已修**：
   运行器新增 `--p2-aligned`，把 P2 换成与实现同一套判据（占位/角色词不算凭据），
   默认仍是历史口径所以前五轮可原样复现，口径切换要在新一轮预登记里声明；
   回归测试把两版的差钉在第五轮那 4 次残留上；
   (2) 「有增量」说的是**门禁挡住了模型自发的部分真实风险、且没有把任何拒绝判成规则说错**，
   不是说用户不会觉得被拦（5 次政策拒绝仍在），也**不是**说可以默认开启——
   那取决于这些拒绝是不是用户能接受的代价（见 TF-0110）。
   见 [benchmark/gate-increment-round5/VERDICT.md](benchmark/gate-increment-round5/VERDICT.md)。

**路线**

- 把跨 `@include` 的语料扩到多层嵌套与"合并出来的表"（核心库、CLI、工具协议与基准已支持单层，见三）
- **不再计划**：发布 npm 包。修复后的产物留在仓库里，构建与打包验收都在 CI 里跑，
  但发布账号已不可用
- 把 schema 推断的覆盖面扩大（例如由多个文件推断共享 schema）

## 附录 A：解析性能对比

> 这一节与「Agent 编辑配置」的主线无关，保留只是为了回答「解析本身要多少钱」。
> **它不是本项目的证据** —— 本项目的证据是 [三、实测数据](#三实测数据编辑保真度)。
> 在「谁解析快」这个维度上 VerseConf 并不占优，如实列出。

本机一次 release 运行（Windows 10 / AMD64 / rustc 1.98.1，每个格式 5 轮取中位数）：

| 数据集 | VerseConf | TOML | JSON | TOML/VCF | VCF/JSON |
|---------|-----------|------|------|----------|----------|
| small (426B) | 9.55μs | 13.51μs | 1.96μs | 1.42x | 4.88x |
| medium (2.5KB) | 78.51μs | 96.99μs | 13.89μs | 1.24x | 5.65x |
| large (24.8KB) | 756.34μs | 950.42μs | 197.13μs | 1.26x | 3.84x |
| xlarge (263KB) | 6.82ms | 8.49ms | 2.46ms | 1.25x | 2.77x |

- VerseConf 的解析速度约为 TOML 的 **1.24–1.42 倍**；
- `serde_json` 比三者都快，VerseConf 比它慢 **2.77–5.65 倍**
  （最大差距在 2.5KB 语料，最小在 263KB 语料）；
- 三种格式的语料是同一份配置的等价写法。

```bash
cargo run -p verseconf-compare --bin generate_test_data
cargo run --release -p verseconf-compare --bin benchmark -- --json compare/benchmark_results.json
python3 compare/generate_charts.py
```

数字的唯一来源是 `compare/benchmark_results.json`；图表脚本不内置任何备用数据，
基准在数据缺失或解析失败时直接报错退出，不会退化成 0。
完整报告见 [compare/performance_charts.md](compare/performance_charts.md)。

## 许可证

MIT OR Apache-2.0 —— 见 [LICENSE-MIT](LICENSE-MIT) 与 [LICENSE-APACHE](LICENSE-APACHE)。

# verseconf-core

VerseConf 配置语言的核心库：解析、AST、校验、安全审计，以及**字符区间级最小改动**的编辑引擎。

这个项目不是「再发明一种配置语法」。它的定位是 **Agent 编辑配置的确定性执行层**：
模型只给出语义意图（改哪个字段、改成什么、为什么改），由确定性代码定位并替换字节区间，
写入前做 schema 与安全双重校验，**无法确定时拒绝而不是猜测**。

## 安装

```bash
cargo add verseconf-core
```

## 快速开始

```rust
use verseconf_core::{format, parse, validate_ast};

let source = "server {\n  port = 8080 # 生产端口\n  host = \"127.0.0.1\"\n}\n";

let ast = parse(source).expect("应当能解析");
validate_ast(&ast).expect("应当通过校验");

// 格式化保留注释与 #@ 元数据，且幂等
println!("{}", format(source).expect("应当能格式化"));
```

## 只改目标值，其余字节零变化

这是核心库与「让模型重写整个文件」的分界线。调用方给出的是意图，不是新文件：

```rust
use verseconf_core::{apply_edit_plan, EditPlan};

let source = "server {\n  port = 8080 # 生产端口\n  host = \"127.0.0.1\"\n}\n";

let plan: EditPlan = serde_json::from_str(r#"{
  "version": "1.0",
  "edits": [
    { "op": "set", "path": ["server", "port"], "value": 9090,
      "expect": { "value": 8080 } }
  ]
}"#).expect("计划应当合法");

let outcome = apply_edit_plan(source, &plan).expect("应当被接受");

assert!(outcome.source.contains("9090"));            // 目标值改对了
assert!(outcome.source.contains("# 生产端口"));       // 行尾注释保留
assert!(outcome.source.contains("127.0.0.1"));       // 区间之外的字节零变化
```

前置条件不符、目标有歧义、改动会破坏 schema 或引入新的安全风险时，
`apply_edit_plan` 返回 `EditRefusal` 而不是猜一个结果——调用方不应写入任何文件。

完整可运行版本：[`crates/verseconf-core/examples/deterministic_edit.rs`](https://github.com/Baixu22/Verse-conf/blob/main/crates/verseconf-core/examples/deterministic_edit.rs)。
它同时是 `cargo test --workspace` 的一部分，所以 README 里的示例不会和实现漂移。

## 公开基准：改配置会不会把别的也改掉

仓库里带一个可第三方重跑的编辑保真度基准（6 篇文档 / 14 个任务，不依赖网络或模型）：

| 策略 | 正确率 | 附带损伤率 | 拒绝准确率 |
|---|---|---|---|
| 意图契约 + 字符区间最小改动 | 8/8 | **0/8** | 6/6 |
| 换值后重写整个文件 | 0/8 | 8/8 | 3/6 |
| 按字段名找第一处匹配行 | 2/8 | 3/8 | 2/6 |

语料、判定脚本与三个策略都在仓库里，重跑得到同一语料指纹 `115811767f1177d0`。
方法与范围限制见 [`benchmark/README.md`](https://github.com/Baixu22/Verse-conf/blob/main/benchmark/README.md)。

## 相关

- [主仓库与命令行](https://github.com/Baixu22/Verse-conf)
- [工具协议服务 verseconf-mcp](https://crates.io/crates/verseconf-mcp) —— 把校验、审计、编辑暴露给 Agent 宿主
- [语言服务器 verseconf-lsp](https://crates.io/crates/verseconf-lsp)

## 许可证

MIT OR Apache-2.0

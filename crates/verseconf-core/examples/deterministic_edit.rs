//! 演示「只改目标值的字节区间，其余字节零变化」。
//!
//! 这段代码同时被 `crates/verseconf-core/README.md` 引用，并且作为
//! `cargo test --workspace` 的一部分被编译——README 里的示例不会和实现漂移。
//!
//! 运行：`cargo run -p verseconf-core --example deterministic_edit`

use verseconf_core::{apply_edit_plan, parse, EditPlan};

fn main() {
    let source = "server {\n  port = 8080 # 生产端口\n  host = \"127.0.0.1\"\n}\n";

    // 1. 先确认它能解析
    let ast = parse(source).expect("源码应当能解析");
    println!("根条目数: {}", ast.root.entries.len());

    // 2. 给出「改哪个字段、改成什么」，而不是给出新文件
    let plan_json = r#"{
      "version": "1.0",
      "edits": [
        {
          "op": "set",
          "path": ["server", "port"],
          "value": 9090,
          "expect": { "value": 8080 }
        }
      ]
    }"#;
    let plan: EditPlan = serde_json::from_str(plan_json).expect("编辑计划应当合法");

    // 3. 由确定性代码完成字符区间级最小改动
    let outcome = apply_edit_plan(source, &plan).expect("应当被接受");

    println!("--- 改动后 ---\n{}", outcome.source);
    println!("已应用 {} 条编辑", outcome.applied.len());

    assert!(outcome.source.contains("9090"), "目标值应当已改对");
    assert!(outcome.source.contains("# 生产端口"), "行尾注释必须保留");
    assert!(
        outcome.source.contains("host = \"127.0.0.1\""),
        "改动之外的字节必须零变化"
    );

    // 4. 前置条件不符时明确拒绝，而不是猜测
    let stale = r#"{
      "version": "1.0",
      "edits": [
        { "op": "set", "path": ["server", "port"], "value": 1, "expect": { "value": 1234 } }
      ]
    }"#;
    let stale_plan: EditPlan = serde_json::from_str(stale).expect("计划应当合法");
    let refusal = apply_edit_plan(source, &stale_plan).expect_err("前置条件不符必须拒绝");
    println!("按预期拒绝: {refusal}");
}

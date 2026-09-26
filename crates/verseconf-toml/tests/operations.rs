//! TF-0078：TOML 适配的 `insert` / `delete` 与复杂值。
//!
//! 这个测试组**刻意与单标量 `set` 的结论分开**（TF-0078 的验收要求）：
//! 单标量 `set` 的通过率不能替 `insert` / `delete` 与复杂值背书。
//!
//! 三种操作各自的语义写在 `src/lib.rs` 的模块文档里，这里逐条钉住：
//!
//! - `insert` 的插入点必须落在**正确的表**里——算错会把新键悄悄放进另一张表，
//!   而结果仍然是合法 TOML；
//! - `delete` 必须整条删掉，跨行的值不能被截断；
//! - 复杂值（多行数组、内联表、多行字符串）的替换必须只动目标值那一段字节。

use serde_json::json;
use toml_edit::ImDocument;
use verseconf_core::{EditPlan, EditRefusal};
use verseconf_toml::apply_toml_edit;

const DOC: &str = "\
# 头注释
top = 1

[alpha]
a = 1
b = [
    1,
    2,
]

[alpha.nested]
c = 2

[beta]
d = 3
";

fn plan(edits: serde_json::Value) -> EditPlan {
    EditPlan::from_json(&json!({ "version": "1.0", "edits": edits }).to_string())
        .expect("测试用的计划应当合法")
}

fn one(op: &str, path: serde_json::Value, value: Option<serde_json::Value>) -> EditPlan {
    let mut edit = json!({ "op": op, "path": path });
    if let Some(value) = value {
        edit["value"] = value;
    }
    plan(json!([edit]))
}

fn apply(source: &str, edit_plan: &EditPlan) -> Result<String, EditRefusal> {
    apply_toml_edit(source, edit_plan).map(|outcome| outcome.source)
}

fn code(refusal: EditRefusal) -> String {
    refusal.code().to_string()
}

// ————————————————————————— insert —————————————————————————

#[test]
fn insert_into_a_table_lands_before_the_next_sub_table() {
    // 这是 insert 唯一真正需要定义清楚的地方：插入点如果落在最后一条子表之后，
    // 新键会被那个子表的 [header] 带走，跑进 alpha.nested 里去。
    let source = apply(DOC, &one("insert", json!(["alpha", "e"]), Some(json!(5)))).unwrap();

    assert_eq!(
        source,
        "\
# 头注释
top = 1

[alpha]
a = 1
b = [
    1,
    2,
]
e = 5

[alpha.nested]
c = 2

[beta]
d = 3
"
    );
}

#[test]
fn insert_into_the_root_lands_before_the_first_header() {
    let source = apply(DOC, &one("insert", json!(["new_top"]), Some(json!(7)))).unwrap();
    assert!(
        source.starts_with("# 头注释\ntop = 1\nnew_top = 7\n\n[alpha]\n"),
        "{source}"
    );
}

#[test]
fn insert_into_the_last_table_appends_at_the_end() {
    let source = apply(DOC, &one("insert", json!(["beta", "e"]), Some(json!(4)))).unwrap();
    assert!(source.ends_with("[beta]\nd = 3\ne = 4\n"), "{source}");
}

#[test]
fn insert_into_a_table_without_direct_keys_uses_its_own_header() {
    let source = apply(
        "[gamma]\n\n[gamma.sub]\nx = 1\n",
        &one("insert", json!(["gamma", "y"]), Some(json!(2))),
    )
    .unwrap();
    assert_eq!(source, "[gamma]\ny = 2\n\n[gamma.sub]\nx = 1\n");
}

#[test]
fn insert_into_a_root_without_direct_keys_goes_first() {
    let source = apply(
        "[a]\nx = 1\n",
        &one("insert", json!(["top"]), Some(json!(9))),
    )
    .unwrap();
    assert_eq!(source, "top = 9\n[a]\nx = 1\n");
}

#[test]
fn insert_keeps_the_files_own_line_ending() {
    let source = apply(
        "[t]\r\nx = 1\r\n",
        &one("insert", json!(["t", "y"]), Some(json!(2))),
    )
    .unwrap();
    assert_eq!(source, "[t]\r\nx = 1\r\ny = 2\r\n");
}

#[test]
fn insert_copies_the_indentation_of_the_line_it_follows() {
    let source = apply(
        "[t]\n    x = 1\n",
        &one("insert", json!(["t", "y"]), Some(json!(2))),
    )
    .unwrap();
    assert_eq!(source, "[t]\n    x = 1\n    y = 2\n");
}

#[test]
fn insert_refuses_a_key_that_already_exists() {
    assert_eq!(
        code(apply(DOC, &one("insert", json!(["alpha", "a"]), Some(json!(9)))).unwrap_err()),
        "target_already_exists"
    );
    // 既有的子表也算「已存在」，不能覆盖成标量
    assert_eq!(
        code(
            apply(
                DOC,
                &one("insert", json!(["alpha", "nested"]), Some(json!(9)))
            )
            .unwrap_err()
        ),
        "target_already_exists"
    );
}

#[test]
fn insert_refuses_a_table_that_does_not_exist() {
    assert_eq!(
        code(apply(DOC, &one("insert", json!(["nope", "x"]), Some(json!(1)))).unwrap_err()),
        "target_not_found"
    );
}

#[test]
fn two_inserts_into_the_same_table_keep_the_plan_order() {
    // 两条编辑的插入点是同一个偏移，所以「从后往前替换」的顺序必须仍然保住计划顺序
    let plan = plan(json!([
        { "op": "insert", "path": ["t", "a"], "value": 1 },
        { "op": "insert", "path": ["t", "b"], "value": 2 }
    ]));
    let source = apply("[t]\nx = 0\n", &plan).unwrap();
    assert_eq!(source, "[t]\nx = 0\na = 1\nb = 2\n");
}

#[test]
fn insert_renders_complex_values_as_valid_toml() {
    let array = apply(
        "[t]\nx = 0\n",
        &one("insert", json!(["t", "arr"]), Some(json!([1, 2, 3]))),
    )
    .unwrap();
    assert_eq!(array, "[t]\nx = 0\narr = [1, 2, 3]\n");

    let inline = apply(
        "[t]\nx = 0\n",
        &one("insert", json!(["t", "inline"]), Some(json!({ "a": 1 }))),
    )
    .unwrap();
    let inserted = inline
        .strip_prefix("[t]\nx = 0\n")
        .expect("只应多出一行")
        .strip_suffix('\n')
        .expect("应当以换行结尾");
    assert!(inserted.starts_with("inline = "), "{inline}");
    ImDocument::parse(inline).expect("结果必须仍是合法 TOML");
}

// ————————————————————————— delete —————————————————————————

#[test]
fn delete_removes_the_whole_line_including_the_trailing_comment() {
    let source = apply("a = 1 # 注释\nb = 2\n", &one("delete", json!(["a"]), None)).unwrap();
    assert_eq!(source, "b = 2\n");
}

#[test]
fn delete_removes_a_multiline_value_entirely() {
    // 只删第一行会留下 `    1,` 这样的残骸，结果直接不合法
    let source = apply(
        "[t]\nb = [\n    1,\n    2,\n]\nc = 3\n",
        &one("delete", json!(["t", "b"]), None),
    )
    .unwrap();
    assert_eq!(source, "[t]\nc = 3\n");
}

#[test]
fn delete_removes_a_multiline_string_entirely() {
    let source = apply(
        "[t]\ns = \"\"\"x\ny\"\"\"\nc = 3\n",
        &one("delete", json!(["t", "s"]), None),
    )
    .unwrap();
    assert_eq!(source, "[t]\nc = 3\n");
}

#[test]
fn delete_keeps_the_files_own_line_ending() {
    let source = apply(
        "[t]\r\nx = 1\r\ny = 2\r\n",
        &one("delete", json!(["t", "x"]), None),
    )
    .unwrap();
    assert_eq!(source, "[t]\r\ny = 2\r\n");
}

#[test]
fn a_hash_inside_a_value_is_not_mistaken_for_a_comment() {
    // 删除范围靠值的 span 决定，所以字符串里的 `#` 不能影响判定
    let source = apply(
        "[t]\nurl = \"http://x#y\"\nu = 3\n",
        &one("delete", json!(["t", "url"]), None),
    )
    .unwrap();
    assert_eq!(source, "[t]\nu = 3\n");
}

#[test]
fn delete_refuses_a_key_that_does_not_exist() {
    assert_eq!(
        code(apply("a = 1\n", &one("delete", json!(["nope"]), None)).unwrap_err()),
        "target_not_found"
    );
}

#[test]
fn delete_respects_the_precondition() {
    let wrong = plan(json!([{ "op": "delete", "path": ["a"], "expect": { "value": 2 } }]));
    assert_eq!(
        code(apply("a = 1\n", &wrong).unwrap_err()),
        "expectation_mismatch"
    );

    let right = plan(json!([{ "op": "delete", "path": ["a"], "expect": { "value": 1 } }]));
    assert_eq!(apply("a = 1\n", &right).unwrap(), "");
}

#[test]
fn delete_refuses_a_whole_table_instead_of_calling_it_missing() {
    // 键存在、只是形状是表；报 target_not_found 会让人以为键名写错了
    assert_eq!(
        code(apply(DOC, &one("delete", json!(["alpha"]), None)).unwrap_err()),
        "unsupported_target"
    );
}

#[test]
fn set_refuses_a_whole_table_instead_of_rewriting_the_structure() {
    let refusal = apply(DOC, &one("set", json!(["alpha"]), Some(json!(1)))).unwrap_err();
    assert_eq!(refusal.code(), "unsupported_target");
    assert!(
        refusal.to_string().contains("不重写结构"),
        "拒绝理由要说明这是结构重写，而不是值替换：{refusal}"
    );
}

// ————————————————— 复杂值：替换的字节保真 —————————————————

#[test]
fn set_replaces_a_multiline_array_without_touching_anything_else() {
    let source = "# 注释\r\n[t]\r\nbefore = 1\r\narr = [\r\n    1,\r\n    2,\r\n]\r\nafter = 2\r\n";
    let outcome = apply(source, &one("set", json!(["t", "arr"]), Some(json!([3])))).unwrap();

    assert_eq!(
        outcome,
        "# 注释\r\n[t]\r\nbefore = 1\r\narr = [3]\r\nafter = 2\r\n"
    );
    assert!(outcome.contains("\r\n"), "CRLF 必须保持");
}

#[test]
fn set_replaces_a_multiline_string() {
    let source = "[t]\ns = \"\"\"x\ny\"\"\"\nu = 3\n";
    let outcome = apply(source, &one("set", json!(["t", "s"]), Some(json!("z")))).unwrap();
    assert_eq!(outcome, "[t]\ns = \"z\"\nu = 3\n");
}

#[test]
fn set_replaces_an_inline_table() {
    let source = "[t]\ninner = { a = 1, b = 2 }\nu = 3\n";
    let outcome = apply(
        source,
        &one("set", json!(["t", "inner"]), Some(json!({ "a": 9 }))),
    )
    .unwrap();

    assert!(outcome.starts_with("[t]\ninner = "), "{outcome}");
    assert!(outcome.ends_with("\nu = 3\n"), "{outcome}");
    assert!(outcome.contains("a = 9"), "{outcome}");
    assert!(
        !outcome.contains("b = 2"),
        "旧值必须被整体替换掉：{outcome}"
    );
}

#[test]
fn a_plan_mixing_all_three_ops_keeps_everything_else_byte_identical() {
    let plan = plan(json!([
        { "op": "set", "path": ["t", "a"], "value": 10 },
        { "op": "delete", "path": ["t", "b"] },
        { "op": "insert", "path": ["t", "c"], "value": 3 }
    ]));
    let source = apply("[t]\na = 1\nb = 2\n", &plan).unwrap();
    assert_eq!(source, "[t]\na = 10\nc = 3\n");
}

// ——————————————— [[key]] 元素的按字段匹配定位 ———————————————

const NAMED: &str = "\
[[servers]]
name = \"primary\"
port = 1

[[servers]]
name = \"replica\"
port = 2
";

fn named(key: &str, field: &str, value: serde_json::Value) -> serde_json::Value {
    json!({ "key": key, "match": { field: value } })
}

#[test]
fn a_named_path_reaches_the_matched_array_table_element() {
    let source = apply(
        NAMED,
        &one(
            "set",
            json!([named("servers", "name", json!("replica")), "port"]),
            Some(json!(9090)),
        ),
    )
    .unwrap();
    assert_eq!(source, NAMED.replace("port = 2", "port = 9090"));
}

#[test]
fn a_named_path_that_matches_nothing_is_refused() {
    let refusal = apply(
        NAMED,
        &one(
            "set",
            json!([named("servers", "name", json!("nope")), "port"]),
            Some(json!(1)),
        ),
    )
    .unwrap_err();
    assert_eq!(code(refusal), "target_not_found");
}

#[test]
fn a_named_path_that_matches_a_missing_field_is_refused() {
    let refusal = apply(
        NAMED,
        &one(
            "set",
            json!([named("servers", "region", json!("eu")), "port"]),
            Some(json!(1)),
        ),
    )
    .unwrap_err();
    assert_eq!(code(refusal), "target_not_found");
}

#[test]
fn a_named_path_that_matches_two_elements_is_ambiguous() {
    // 匹配不唯一时不能猜一个元素改——与 .vcf 路径同一套语义
    const DUPLICATE: &str = "[[s]]\nname = \"a\"\nport = 1\n\n[[s]]\nname = \"a\"\nport = 2\n";
    let refusal = apply(
        DUPLICATE,
        &one(
            "set",
            json!([named("s", "name", json!("a")), "port"]),
            Some(json!(3)),
        ),
    )
    .unwrap_err();
    assert_eq!(code(refusal), "target_ambiguous");
}

#[test]
fn a_named_path_on_a_plain_table_is_refused_with_a_reason() {
    let refusal = apply(
        "[t]\na = 1\n",
        &one(
            "set",
            json!([named("t", "a", json!(1)), "a"]),
            Some(json!(2)),
        ),
    )
    .unwrap_err();
    assert_eq!(refusal.code(), "unsupported_target");
    assert!(refusal.to_string().contains("数组表"), "{refusal}");
}

#[test]
fn insert_into_a_matched_element_lands_inside_that_element() {
    let source = apply(
        NAMED,
        &one(
            "insert",
            json!([named("servers", "name", json!("primary")), "weight"]),
            Some(json!(7)),
        ),
    )
    .unwrap();
    assert_eq!(
        source,
        "\
[[servers]]
name = \"primary\"
port = 1
weight = 7

[[servers]]
name = \"replica\"
port = 2
"
    );
}

#[test]
fn delete_from_a_matched_element_removes_only_that_elements_line() {
    let source = apply(
        NAMED,
        &one(
            "delete",
            json!([named("servers", "name", json!("replica")), "port"]),
            None,
        ),
    )
    .unwrap();
    assert_eq!(
        source,
        "\
[[servers]]
name = \"primary\"
port = 1

[[servers]]
name = \"replica\"
"
    );
}

#[test]
fn a_named_segment_cannot_be_the_last_step() {
    // 「把整个元素换成一个值」没有明确定义，必须拒绝而不是猜
    let refusal = apply(
        NAMED,
        &one(
            "set",
            json!([named("servers", "name", json!("primary"))]),
            Some(json!(1)),
        ),
    )
    .unwrap_err();
    assert_eq!(code(refusal), "unsupported_target");
}

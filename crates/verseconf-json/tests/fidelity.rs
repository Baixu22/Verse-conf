//! 保真编辑的回归（TF-0096 的第二条验收标准：注释与键序在编辑后保持）。
//!
//! 这一组测的是**改动之外的字节零变化**：注释、键序、缩进、换行风格、
//! 尾随逗号都不需要被「保住」——它们从来没被重建过。所以这里逐字节断言
//! 改动前后的文本，而不是断言「解析出来的键值对一样」：后者对
//! 「把注释全删掉再重新序列化」的实现同样成立，而那正是要排除的。

use std::collections::BTreeMap;

use verseconf_core::{EditIntent, EditOp, EditValue, PathSegment};
use verseconf_json::{
    apply_json_edit_plan, check_write_json_with, replace_json_range, set_json_value, JsonFlavor,
    JsonGuard,
};

fn key(path: &str) -> Vec<PathSegment> {
    path.split('.')
        .map(|part| PathSegment::Key(part.to_string()))
        .collect()
}

fn set(source: &str, path: &str, value: EditValue) -> String {
    set_json_value(source, &key(path), &value, JsonFlavor::Jsonc)
        .expect("这次改动应当被允许")
        .source
}

#[test]
fn setting_a_value_replaces_only_that_value() {
    let source = r#"{"port": 8080, "host": "localhost"}"#;
    assert_eq!(
        set(source, "port", EditValue::Integer(9090)),
        r#"{"port": 9090, "host": "localhost"}"#
    );
}

#[test]
fn comments_and_key_order_survive_an_edit() {
    // 这是一条真实形状的 settings.json：文件头注释、行尾注释、尾随逗号都有。
    // 用「解析 → 改 → 重新序列化」的实现会在这里把注释全丢掉。
    let source = "{\n  // 传输层\n  \"tls_verify\": true, // 生产必须开\n  \"port\": 8080,\n}\n";
    let edited = set(source, "port", EditValue::Integer(9090));

    assert_eq!(
        edited,
        "{\n  // 传输层\n  \"tls_verify\": true, // 生产必须开\n  \"port\": 9090,\n}\n"
    );
    assert!(edited.contains("// 传输层"), "注释不能被丢掉");
    assert!(edited.contains("// 生产必须开"), "行尾注释不能被丢掉");
    assert!(edited.contains(",\n}"), "尾随逗号不能被抹掉");
}

#[test]
fn key_order_is_not_reordered() {
    // 键序是用户写的顺序；把它按字典序重排会让 diff 变成整文件重写
    let source = r#"{"zeta": 1, "alpha": 2, "middle": 3}"#;
    let edited = set(source, "alpha", EditValue::Integer(9));
    assert_eq!(edited, r#"{"zeta": 1, "alpha": 9, "middle": 3}"#);
}

#[test]
fn crlf_line_endings_are_not_normalized() {
    // 与 TOML 侧同源的问题：重新序列化会把 CRLF 变成 LF，即使一次都没修改。
    // 这里的编辑不经过任何序列化器，所以换行符不会被碰。
    let source = "{\r\n  \"port\": 8080,\r\n  \"host\": \"a\"\r\n}\r\n";
    let edited = set(source, "port", EditValue::Integer(9090));

    assert_eq!(
        edited,
        "{\r\n  \"port\": 9090,\r\n  \"host\": \"a\"\r\n}\r\n"
    );
    assert_eq!(
        edited.matches("\r\n").count(),
        source.matches("\r\n").count()
    );
}

#[test]
fn indentation_with_tabs_is_preserved() {
    let source = "{\n\t\"port\": 8080,\n\t\"host\": \"a\"\n}\n";
    assert_eq!(
        set(source, "port", EditValue::Integer(9090)),
        "{\n\t\"port\": 9090,\n\t\"host\": \"a\"\n}\n"
    );
}

#[test]
fn setting_a_nested_value_touches_only_that_value() {
    let source = "{\n  \"server\": {\n    \"tls_verify\": true,\n    \"port\": 8080\n  }\n}\n";
    let edited = set(source, "server.port", EditValue::Integer(9090));
    assert_eq!(
        edited,
        "{\n  \"server\": {\n    \"tls_verify\": true,\n    \"port\": 9090\n  }\n}\n"
    );
}

#[test]
fn setting_a_value_inside_an_array_element_works() {
    let source = r#"{"servers": [{"name": "a", "port": 1}, {"name": "b", "port": 2}]}"#;
    let path = vec![
        PathSegment::Named {
            key: "servers".to_string(),
            r#match: BTreeMap::from([("name".to_string(), EditValue::String("b".to_string()))]),
        },
        PathSegment::Key("port".to_string()),
    ];
    let edited = set_json_value(source, &path, &EditValue::Integer(3), JsonFlavor::Json)
        .expect("唯一命中时必须改成功")
        .source;

    assert_eq!(
        edited,
        r#"{"servers": [{"name": "a", "port": 1}, {"name": "b", "port": 3}]}"#
    );
}

#[test]
fn an_expectation_mismatch_refuses_instead_of_overwriting() {
    let source = r#"{"port": 8080}"#;
    let intent = EditIntent {
        op: EditOp::Set,
        path: key("port"),
        value: Some(EditValue::Integer(9090)),
        reason: None,
        expect: Some(verseconf_core::EditExpectation {
            value: Some(EditValue::Integer(1)),
        }),
    };

    let refusal = apply_json_edit_plan(source, &intent, &JsonGuard::default())
        .expect_err("前置条件不符必须拒绝");
    assert_eq!(refusal.code(), "expectation_mismatch");
}

#[test]
fn a_duplicate_key_is_refused_rather_than_guessed() {
    // 同一个键在 JSONC 里出现两次语法合法（后者覆盖前者）。改哪一个没有定义，
    // 「改第一个」会让用户以为改的是生效的那个，所以按有歧义拒绝。
    let source = r#"{"a": 1, "a": 2}"#;
    match set_json_value(source, &key("a"), &EditValue::Integer(3), JsonFlavor::Jsonc) {
        Err(refusal) => assert_eq!(refusal.code(), "target_ambiguous"),
        Ok(outcome) => panic!("重复键必须拒绝，实际改成：{}", outcome.source),
    }
}

#[test]
fn a_byte_order_mark_does_not_shift_the_edit() {
    // Windows 上的编辑器会写出带 BOM 的 UTF-8，而 `jsonc_parser` 不认 BOM。
    // 直接把它交给解析器会让门禁在这类真实配置上不可用；忽略 BOM 但忘记平移
    // 区间则更糟——每次编辑整体错位一个字节。这条测试把两件事一起钉住：
    // BOM 文档能过，且改动之后逐字节保真。
    let source = "\u{feff}{\n  \"a\": 1,\n  \"b\": 2\n}\n";
    let outcome = set_json_value(source, &key("b"), &EditValue::Integer(3), JsonFlavor::Jsonc)
        .expect("带 BOM 的文档应当能用");
    assert_eq!(outcome.source, "\u{feff}{\n  \"a\": 1,\n  \"b\": 3\n}\n");

    // 严格 JSON 下同样成立（BOM 不是「注释风格」那一类差异）
    let strict = set_json_value(
        "\u{feff}{\"a\": 1}",
        &key("a"),
        &EditValue::Integer(2),
        JsonFlavor::Json,
    )
    .expect("方向 BOM 不该让严格 JSON 失效");
    assert_eq!(strict.source, "\u{feff}{\"a\": 2}");
}

// ---------------------------------------------------------------------------
// insert / delete
// ---------------------------------------------------------------------------

fn apply(source: &str, op: EditOp, path: &str, value: Option<EditValue>) -> String {
    let intent = EditIntent {
        op,
        path: key(path),
        value,
        reason: None,
        expect: None,
    };
    apply_json_edit_plan(source, &intent, &JsonGuard::default())
        .expect("这次改动应当被允许")
        .source
}

#[test]
fn insert_into_a_multi_line_object_keeps_its_shape() {
    let source = "{\n  \"a\": 1,\n  \"b\": 2\n}\n";
    let edited = apply(source, EditOp::Insert, "c", Some(EditValue::Integer(3)));
    assert_eq!(edited, "{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3\n}\n");
}

#[test]
fn insert_respects_an_existing_trailing_comma() {
    // 原文本来就有尾随逗号：再补一个就会造出 `3,,`，那不是合法 JSONC
    let source = "{\n  \"a\": 1,\n}\n";
    let edited = apply(source, EditOp::Insert, "b", Some(EditValue::Integer(2)));
    assert_eq!(edited, "{\n  \"a\": 1,\n  \"b\": 2\n}\n");
    assert!(!edited.contains(",,"), "不能出现两个连续逗号");
}

#[test]
fn insert_into_a_single_line_object_stays_single_line() {
    let source = r#"{"a": 1}"#;
    let edited = apply(source, EditOp::Insert, "b", Some(EditValue::Integer(2)));
    assert_eq!(edited, r#"{"a": 1, "b": 2}"#);
}

#[test]
fn insert_into_an_empty_object_fills_it() {
    assert_eq!(
        apply("{}", EditOp::Insert, "a", Some(EditValue::Integer(1))),
        r#"{"a": 1}"#
    );
    // 多行空对象的属性落在两行花括号中间，缩进比 `}` 深一级
    assert_eq!(
        apply("{\n}\n", EditOp::Insert, "a", Some(EditValue::Integer(1))),
        "{\n  \"a\": 1\n}\n"
    );
}

#[test]
fn insert_is_not_blocked_by_a_comment_belonging_to_a_later_sibling() {
    // 独立复核发现的缺陷：行尾注释扫描没有在对象结尾停下，于是读进了
    // **下一个兄弟属性**的字符串值，把 `"// not a comment"` 当成注释，
    // 报出「最后一条属性行尾有注释」——而那个对象里根本没有注释。
    let source = r#"{"outer": {"a": 1}, "z": "// not a comment"}"#;
    let edited = apply(
        source,
        EditOp::Insert,
        "outer.b",
        Some(EditValue::Integer(2)),
    );
    assert_eq!(
        edited,
        r#"{"outer": {"a": 1, "b": 2}, "z": "// not a comment"}"#
    );
}

#[test]
fn insert_after_a_trailing_comment_is_refused_rather_than_corrupting_it() {
    // 逗号必须插在注释**之前**，而这需要两次互不相邻的拼接。
    // 与其产出一份 `"b": 2 // note,`（逗号被注释吃掉、JSON 变成非法），
    // 不如明确说做不到——这条路径宿主可以自己处理那一行再插入。
    let intent = EditIntent {
        op: EditOp::Insert,
        path: key("c"),
        value: Some(EditValue::Integer(3)),
        reason: None,
        expect: None,
    };
    let refusal = apply_json_edit_plan(
        "{\n  \"a\": 1,\n  \"b\": 2 // 注释\n}\n",
        &intent,
        &JsonGuard::default(),
    )
    .expect_err("行尾注释挡住插入点时必须明确拒绝");
    assert_eq!(refusal.code(), "unsupported_target");
    assert!(refusal.to_string().contains("注释"));
}

#[test]
fn insert_into_a_nested_object_lands_in_that_object() {
    let source = "{\n  \"outer\": {\n    \"a\": 1\n  }\n}\n";
    let edited = apply(
        source,
        EditOp::Insert,
        "outer.b",
        Some(EditValue::Bool(true)),
    );
    assert_eq!(
        edited,
        "{\n  \"outer\": {\n    \"a\": 1,\n    \"b\": true\n  }\n}\n"
    );
}

#[test]
fn insert_over_an_existing_key_is_refused() {
    let intent = EditIntent {
        op: EditOp::Insert,
        path: key("a"),
        value: Some(EditValue::Integer(2)),
        reason: None,
        expect: None,
    };
    let refusal = apply_json_edit_plan(r#"{"a": 1}"#, &intent, &JsonGuard::default())
        .expect_err("键已存在时必须拒绝，而不是悄悄覆盖");
    assert_eq!(refusal.code(), "target_already_exists");
}

#[test]
fn delete_of_a_middle_key_leaves_no_orphan_comma() {
    let source = "{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3\n}\n";
    let edited = apply(source, EditOp::Delete, "b", None);
    assert_eq!(edited, "{\n  \"a\": 1,\n  \"c\": 3\n}\n");
}

#[test]
fn delete_of_the_last_key_takes_the_previous_comma() {
    let source = "{\n  \"a\": 1,\n  \"b\": 2\n}\n";
    let edited = apply(source, EditOp::Delete, "b", None);
    assert_eq!(edited, "{\n  \"a\": 1\n}\n");
}

#[test]
fn delete_of_the_only_key_leaves_an_empty_object() {
    // 删掉唯一那条属性时，行首缩进必须跟它一起走：否则留下的是
    // `{\n  \n}`（空对象 + 一行缩进残迹），看起来像被谁改坏过
    assert_eq!(
        apply("{\n  \"a\": 1\n}\n", EditOp::Delete, "a", None),
        "{\n}\n"
    );
    assert_eq!(apply(r#"{"a": 1}"#, EditOp::Delete, "a", None), "{}");
}

#[test]
fn delete_of_the_only_key_keeps_an_unrelated_comment() {
    let source = "{\n  // 这一行说的是下一行的键\n  \"a\": 1\n}\n";
    let edited = apply(source, EditOp::Delete, "a", None);
    assert_eq!(edited, "{\n  // 这一行说的是下一行的键\n}\n");
}

#[test]
fn insert_into_a_single_line_object_with_a_trailing_comma() {
    // 单行 + 尾随逗号：新属性要落在逗号之后，否则逗号会变成
    // 新属性与 `}` 之间的分隔符，而上一条属性后面反而没有逗号
    assert_eq!(
        apply(
            r#"{"a": 1,}"#,
            EditOp::Insert,
            "b",
            Some(EditValue::Integer(2))
        ),
        r#"{"a": 1, "b": 2}"#
    );
}

#[test]
fn delete_works_on_single_line_objects() {
    // 独立复核发现的缺陷：单行对象没有「行」可删，按行删会算出反向区间
    // （start > end）并产出损坏文本，门禁随后拒绝——调用方看到的是
    // 「一次普通删除莫名其妙 validation_failed」。
    for (path, expected) in [
        ("a", r#"{"b":2,"c":3}"#),
        ("b", r#"{"a":1,"c":3}"#),
        ("c", r#"{"a":1,"b":2}"#),
    ] {
        assert_eq!(
            apply(r#"{"a":1,"b":2,"c":3}"#, EditOp::Delete, path, None),
            expected,
            "删除 {path}"
        );
    }
    assert_eq!(apply(r#"{ "a": 1 }"#, EditOp::Delete, "a", None), "{}");
}

#[test]
fn delete_of_a_property_sharing_its_line_with_the_next_one_actually_deletes() {
    // 独立复核发现的缺陷：目标与下一个属性同行时算出的区间为空，
    // 于是返回 Ok 而原文一个字节都没变——「成功」的静默 no-op。
    let source = "{\n  \"a\": 1, \"b\": 2\n}\n";
    let edited = apply(source, EditOp::Delete, "a", None);
    assert_ne!(edited, source, "删除不能是静默 no-op");
    assert_eq!(edited, "{\n  \"b\": 2\n}\n");
}

#[test]
fn delete_keeps_the_rest_of_the_document_intact() {
    let source = "{\n  // 保留我\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3\n}\n";
    let edited = apply(source, EditOp::Delete, "b", None);
    assert_eq!(edited, "{\n  // 保留我\n  \"a\": 1,\n  \"c\": 3\n}\n");
}

#[test]
fn delete_of_a_missing_key_is_refused() {
    let intent = EditIntent {
        op: EditOp::Delete,
        path: key("zzz"),
        value: None,
        reason: None,
        expect: None,
    };
    let refusal = apply_json_edit_plan(r#"{"a": 1}"#, &intent, &JsonGuard::default())
        .expect_err("键不存在时必须拒绝");
    assert_eq!(refusal.code(), "target_not_found");
}

#[test]
fn a_path_through_a_non_object_is_refused_with_a_distinct_code() {
    // 「结构不对」与「键不存在」必须分开：调用方要能区分
    // 「换个路径」和「先补结构」
    let intent = EditIntent {
        op: EditOp::Set,
        path: key("a.b"),
        value: Some(EditValue::Integer(1)),
        reason: None,
        expect: None,
    };
    let refusal = apply_json_edit_plan(r#"{"a": 1}"#, &intent, &JsonGuard::default())
        .expect_err("路径穿过标量必须拒绝");
    assert_eq!(refusal.code(), "unsupported_target");
}

#[test]
fn every_single_property_delete_either_deletes_or_says_the_key_is_missing() {
    // 形状矩阵：独立复核在 delete 上找出两个缺陷（单行对象算出反向区间、
    // 与相邻属性同行时算出空区间变成静默 no-op），它们都不是某一种文档独有的。
    // 这条测试把「文档形状 × 键」的组合都走一遍，只允许两种结果：
    // 成功且真的删掉了东西且结果仍可解析，或者明确报 target_not_found。
    let documents = [
        r#"{"a":1}"#,
        r#"{"a":1,"b":2}"#,
        r#"{"a": 1, "b": 2, "c": 3}"#,
        r#"{ "a": 1 }"#,
        "{\n  \"a\": 1\n}\n",
        "{\n  \"a\": 1,\n  \"b\": 2\n}\n",
        "{\n  \"a\": 1, \"b\": 2\n}\n",
        "{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3\n}\n",
        "{\n  // 前置注释\n  \"a\": 1,\n  \"b\": 2\n}\n",
        "{\n  \"a\": 1, // 行尾注释\n  \"b\": 2\n}\n",
        "{\n\t\"a\": 1,\n\t\"b\": 2\n}\n",
        "{\r\n  \"a\": 1,\r\n  \"b\": 2\r\n}\r\n",
        "{\n  \"outer\": {\n    \"a\": 1\n  }\n}\n",
        "{\n  \"a\": 1, \"b\": 2, \"c\": 3\n}\n",
    ];

    for source in documents {
        for name in ["a", "b", "c", "outer"] {
            let intent = EditIntent {
                op: EditOp::Delete,
                path: key(name),
                value: None,
                reason: None,
                expect: None,
            };
            match apply_json_edit_plan(source, &intent, &JsonGuard::default()) {
                Ok(outcome) => {
                    assert_ne!(
                        outcome.source, source,
                        "删除不能是静默 no-op：{source:?} 删 {name}"
                    );
                    // 结果必须仍然是一份合法文档（拿它自己当候选，只看合法性）
                    check_write_json_with(&outcome.source, &outcome.source, &JsonGuard::default())
                        .unwrap_or_else(|refusal| {
                            panic!(
                                "删除 {name} 之后不再是合法文档：{source:?} → {:?}（{refusal}）",
                                outcome.source
                            )
                        });
                }
                Err(refusal) => assert_eq!(
                    refusal.code(),
                    "target_not_found",
                    "删除 {name} 被意外拒绝：{source:?} → {refusal}"
                ),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 区间原语
// ---------------------------------------------------------------------------

#[test]
fn the_range_primitive_replaces_exactly_that_span() {
    let source = r#"{"port": 8080}"#;
    let edited = replace_json_range(source, 9, 13, "9090", JsonFlavor::Json)
        .expect("区间替换应当被允许")
        .source;
    assert_eq!(edited, r#"{"port": 9090}"#);
}

#[test]
fn the_range_primitive_refuses_an_out_of_bounds_span() {
    let refusal = replace_json_range(r#"{"a": 1}"#, 0, 999, "x", JsonFlavor::Json)
        .expect_err("越界区间必须拒绝");
    assert_eq!(refusal.code(), "unsupported_target");
}

#[test]
fn the_range_primitive_still_runs_the_gate() {
    // 区间原语不是绕过门禁的后门：它跑的是同一份写入前校验
    let source = r#"{"tls_verify": true}"#;
    let start = source.find("true").unwrap();
    let refusal = replace_json_range(source, start, start + 4, "false", JsonFlavor::Json)
        .expect_err("区间原语也必须被门禁拦住");
    assert_eq!(refusal.code(), "security_rejected");
}

#[test]
fn a_jsonc_file_keeps_its_comment_through_a_range_edit() {
    let source = "// 头部注释\n{\n  \"a\": 1\n}\n";
    let start = source.find('1').unwrap();
    let edited = replace_json_range(source, start, start + 1, "2", JsonFlavor::Jsonc)
        .expect("区间替换应当被允许")
        .source;
    assert_eq!(edited, "// 头部注释\n{\n  \"a\": 2\n}\n");
}

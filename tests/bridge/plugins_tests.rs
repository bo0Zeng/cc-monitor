//! 〔RM1b · 第四波〕读 marketplace 那一段搬进了后端（`src/backend/observe/plugins_query.rs`，
//! `P8a` 那几条判据跟着本体一起搬去了 `tests/backend/observe/plugins_query_tests.rs`：三条出口 ·
//! null 不是 0 · 一条坏的不毁整表 · 超限说清 · 不数快照目录）。
//! 本文件只剩 monitor 这一侧的事：**线上形状的收口**（恰一行 · 拒收未知字段 · 每个字段必填）
//! 与「本进程不再自己读盘」。夹具只造结构，不采任何真数据。

use super::*;

/// 后端那一侧的字段集（异源：抄自后端 `plugins_query::MarketplaceEntry`，那一侧有一条同形判据）。
fn full_entry() -> serde_json::Value {
    serde_json::json!({
        "id": "mk",
        "source": "github:a/b",
        "install_location": "/tmp/mk",
        "last_updated": null,
        "declared_plugins": null,
        "declared_error": "落点里没有清单"
    })
}

fn one_line(v: serde_json::Value) -> Vec<String> {
    vec![v.to_string()]
}

#[test]
fn the_exact_wire_shape_parses_and_null_stays_null() {
    let s = parse_survey_lines(&one_line(serde_json::json!({
        "entries": [full_entry()],
        "file_absent": false
    })))
    .expect("恰好是后端那一份形状");
    assert!(!s.file_absent);
    assert_eq!(s.entries.len(), 1);
    // ★ `null` 过了线还是 `None`，不是 `Some(0)`（P8a-Y2 那条病的线上一半）。
    assert_eq!(s.entries[0].declared_plugins, None);
    assert_eq!(
        s.entries[0].declared_error.as_deref(),
        Some("落点里没有清单")
    );
    // 诚实的空也原样过来。
    let empty = parse_survey_lines(&one_line(
        serde_json::json!({"entries": [], "file_absent": true}),
    ))
    .unwrap();
    assert!(empty.file_absent && empty.entries.is_empty());
}

/// ★ 两端一漂就当场报错 —— 多一格、`Option` 缺席（不是 `null`）、少 `file_absent` 都要红。
#[test]
fn any_drift_in_the_wire_shape_is_an_error_not_a_quiet_default() {
    let mut extra = full_entry();
    extra["installed"] = serde_json::json!(39);
    let mut missing = full_entry();
    missing.as_object_mut().unwrap().remove("declared_plugins");
    for (what, entry) in [("多一格", extra), ("Option 缺席", missing)] {
        let got = parse_survey_lines(&one_line(serde_json::json!({
            "entries": [entry],
            "file_absent": false
        })));
        assert!(got.is_err(), "{what} 被静默收下了：{got:?}");
    }
    let no_flag = parse_survey_lines(&one_line(serde_json::json!({"entries": []})));
    assert!(no_flag.is_err(), "缺 file_absent 被静默收下了");
}

#[test]
fn it_must_be_exactly_one_line() {
    assert!(parse_survey_lines(&[]).is_err());
    let l = serde_json::json!({"entries": [], "file_absent": true}).to_string();
    assert!(parse_survey_lines(&[l.clone(), l]).is_err());
}

/// ★ 本机读实现真的退役了：生产段一处读盘都不许有（零命中 ＋ 正控）。
#[test]
fn this_module_no_longer_reads_the_disk_itself() {
    let prod = guard_core::production_code(include_str!("../../src/bridge/src/plugins.rs"));
    // 针运行时拼：字面量会让本文件自己变成「裸遍历目录」那一族扫描判据的语料。
    let needles = [
        format!("fs::{}(", "metadata"),
        format!("read_to_{}(", "string"),
        format!("read_{}(", "dir"),
        format!("resolve_claude_{}(", "dir"),
    ];
    let hits: Vec<&String> = needles
        .iter()
        .filter(|n| prod.contains(n.as_str()))
        .collect();
    assert!(hits.is_empty(), "plugins.rs 生产段又自己读盘了：{hits:?}");
    // 正控：搬家前那几行的形状，同一把尺子必须数得到。
    let before = format!(
        "let meta = match std::fs::{}(path) {{ match std::fs::read_to_{}(path) {{ crate::paths::resolve_claude_{}()",
        "metadata", "string", "dir"
    );
    assert_eq!(
        needles
            .iter()
            .filter(|n| before.contains(n.as_str()))
            .count(),
        3,
        "尺子瞎了"
    );
}

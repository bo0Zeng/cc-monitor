//! 策略设置的声明（`轮换-v5-分层.md` §6.4.1）：后端只照格的种类校验形状与范围，不认格的意思；24 小时条上刷一段是纯算术。

use super::*;
use serde_json::json;

/// 判据夹具那一份声明（排 · 用量上限 · 上限类型 · 启用延迟）。
fn fixture_form() -> Vec<Cell> {
    let src = std::fs::read_to_string(
        crate::guard_support::repo_root().join("tests/__fixtures__/rotation-by-reset.rhai"),
    )
    .expect("判据夹具");
    super::super::script::parse(&src).expect("读得懂").head.form
}

/// 夹具那一份 ＋ 两格百分数（一格总体、范围 0–50；一格每号、能空），把五种格都凑齐。
fn full_form() -> Vec<Cell> {
    let mut f = fixture_form();
    let extra: Vec<Cell> = serde_json::from_value(json!([
        {"key": "level", "kind": "percent", "scope": "all", "label": "x", "min": 0, "max": 50, "default": 10},
        {"key": "keep", "kind": "percent", "scope": "each", "label": "y", "nullable": "z", "default": null}
    ]))
    .expect("格");
    f.extend(extra);
    f
}

fn codes(form: &[Cell], v: serde_json::Value) -> Vec<(String, &'static str)> {
    check(form, &v)
        .into_iter()
        .map(|b| (b.cell, b.code))
        .collect()
}

/// ★ 一份合声明的设置收得下；每种格各错一处 ⇒ 恰好那一格、那一个码（分排沿号列表减了 ⇒ `order`）。
#[test]
fn each_kind_of_cell_is_checked_by_its_shape_only() {
    let f = full_form();
    let ok = json!({
        "caps": {"5h": [{"from": 0, "to": 600, "v": 80}, {"from": 600, "to": 1440, "v": 0}]},
        "capKind": "hard",
        "level": 10,
        "accounts": [
            {"start": true},
            {"id": "lab", "row": 1, "delay": 30, "keep": 20, "capKind": "soft", "caps": {"7d": []}},
            {"id": "team", "row": 1, "keep": null},
            {"id": "api", "row": 3},
        ],
    });
    assert_eq!(codes(&f, ok), []);
    for (v, want) in [
        (json!({"level": 60}), ("level", "range")),
        (json!({"level": "x"}), ("level", "type")),
        (json!({"capKind": "mid"}), ("capKind", "of")),
        (json!({"delay": 5}), ("delay", "unknown")),
        (json!({"nope": 1}), ("nope", "unknown")),
        (
            json!({"caps": {"5h": [{"from": 0, "to": 2000, "v": 1}]}}),
            ("caps.5h[0]", "range"),
        ),
        (json!({"caps": {"1h": []}}), ("caps.1h", "unknown")),
        (
            json!({"caps": {"5h": [{"from": 0, "to": 600, "v": 1}, {"from": 500, "to": 700, "v": 1}]}}),
            ("caps.5h[1]", "overlap"),
        ),
        (
            json!({"accounts": [{"id": "a", "delay": 1.5}]}),
            ("accounts[0].delay", "type"),
        ),
        (
            json!({"accounts": [{"id": "a", "keep": 101}]}),
            ("accounts[0].keep", "range"),
        ),
        (
            json!({"accounts": [{"id": "a", "level": 1}]}),
            ("accounts[0].level", "unknown"),
        ),
        (
            json!({"accounts": [{"id": "a", "row": 1.5}]}),
            ("accounts[0].row", "type"),
        ),
        (
            json!({"accounts": [{"id": "a", "row": 2}, {"id": "b", "row": 1}]}),
            ("accounts[1].row", "order"),
        ),
        (
            json!({"accounts": [{"id": "a", "row": 1}, {"id": "b"}]}),
            ("accounts[1].row", "order"),
        ),
        (json!({"row": 1}), ("row", "unknown")),
        (
            json!({"accounts": [{"id": "a"}, {"id": "a"}]}),
            ("accounts[1]", "dup"),
        ),
        (
            json!({"accounts": [{"start": false}]}),
            ("accounts[0]", "account"),
        ),
        (
            json!({"accounts": [{"id": "../x"}]}),
            ("accounts[0]", "account"),
        ),
    ] {
        assert_eq!(codes(&f, v.clone()), [(want.0.to_string(), want.1)], "{v}");
    }
    assert_eq!(codes(&f, json!([1])), [(String::new(), "type")]);
}

/// 声明本身：键重 · 占号列表那一格 · 选项空 · `inherit` 不在每号那一层 ⇒ 读文件那一下就拒。
#[test]
fn a_bad_form_is_refused_when_the_file_is_read() {
    let mut f = full_form();
    assert!(check_form(&f).is_ok());
    f.push(f[0].clone());
    assert!(check_form(&f).is_err());
    let mut f = full_form();
    f[0].key = ACCOUNTS.into();
    assert!(check_form(&f).is_err());
    let mut f = full_form();
    let i = f
        .iter()
        .position(|c| c.kind == Kind::OneOf)
        .expect("有一格选项");
    f[i].choices.clear();
    assert!(check_form(&f).is_err());
    let mut f = full_form();
    let i = f.iter().position(|c| c.inherit).expect("有一格跟总体");
    f[i].scope = Scope::All;
    assert!(check_form(&f).is_err());
    let mut f = full_form();
    let i = f
        .iter()
        .position(|c| c.kind == Kind::CardRow)
        .expect("有分排那一格");
    f[i].scope = Scope::All;
    assert!(check_form(&f).is_err(), "分排只能每号一份");
}

/// ★ 说明写法：判据夹具合规；标签里写了第二人称或讲结构的字 ⇒ 不合规（设置照样能用）。
#[test]
fn second_person_or_structure_words_in_labels_are_flagged() {
    assert!(!words_bad(&fixture_form()));
    for bad in ["你的上限", "参数", "Your limit"] {
        let mut f = full_form();
        f[0].label = super::super::script::Text::One(bad.into());
        assert!(words_bad(&f), "{bad}");
    }
}

/// ★ 24 小时条上刷一段：刷进去、跨午夜拆两截、`null` 删那一截、连着一样的并成一段。
#[test]
fn painting_a_span_splits_merges_and_wraps() {
    assert_eq!(day_paint(&[], 1200, 1380, Some(0.0)), [(1200, 1380, 0.0)]);
    assert_eq!(
        day_paint(&[(0, 1440, 80.0)], 1320, 120, Some(50.0)),
        [(0, 120, 50.0), (120, 1320, 80.0), (1320, 1440, 50.0)]
    );
    assert_eq!(
        day_paint(&[(0, 600, 80.0), (600, 1440, 0.0)], 600, 900, Some(80.0)),
        [(0, 900, 80.0), (900, 1440, 0.0)]
    );
    assert_eq!(
        day_paint(&[(0, 600, 80.0)], 300, 900, None),
        [(0, 300, 80.0)]
    );
    assert_eq!(day_paint(&[], 60, 60, Some(10.0)), [(0, 1440, 10.0)]);
}

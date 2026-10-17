//! 策略设置的声明：后端只照格的种类校验形状与范围，不认格的意思；24 小时条上刷一段是纯算术。

use super::*;
use serde_json::json;

/// 升级材料「离重置近的权重高」那一份声明（用量上限 · 上限类型 · 权重容差 · 强制切换阈值 · 耗尽后启用 · 启用延迟 · 单次用量上限）：五种格都在里面。
fn full_form() -> Vec<Cell> {
    super::super::script::parse(super::super::upgrade::material(1))
        .expect("读得懂")
        .head
        .form
}

fn codes(form: &[Cell], v: serde_json::Value) -> Vec<(String, &'static str)> {
    check(form, &v)
        .into_iter()
        .map(|b| (b.cell, b.code))
        .collect()
}

/// ★ 一份合声明的设置收得下；每种格各错一处 ⇒ 恰好那一格、那一个码；分排那一格没有了（`row` ⇒ `unknown`）。
#[test]
fn each_kind_of_cell_is_checked_by_its_shape_only() {
    let f = full_form();
    let kinds: Vec<Kind> = f.iter().map(|c| c.kind).collect();
    for k in [
        Kind::Percent,
        Kind::Minutes,
        Kind::Toggle,
        Kind::OneOf,
        Kind::DaySegments,
    ] {
        assert!(kinds.contains(&k), "{k:?}");
    }
    let ok = json!({
        "caps": {"5h": [{"from": 0, "to": 600, "v": 80}, {"from": 600, "to": 1440, "v": 0}]},
        "capKind": "hard",
        "close": 25,
        "force": null,
        "accounts": [
            {"start": true},
            {"id": "lab", "fallback": true, "delay": 30, "stint": 20, "capKind": "soft", "caps": {"7d": []}},
            {"id": "team", "stint": null},
            {"id": "api", "fallback": false},
        ],
    });
    assert_eq!(codes(&f, ok), []);
    for (v, want) in [
        (json!({"close": 160}), ("close", "range")),
        (json!({"close": "x"}), ("close", "type")),
        (json!({"close": null}), ("close", "type")),
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
            json!({"accounts": [{"id": "a", "stint": 101}]}),
            ("accounts[0].stint", "range"),
        ),
        (
            json!({"accounts": [{"id": "a", "fallback": 1}]}),
            ("accounts[0].fallback", "type"),
        ),
        (
            json!({"accounts": [{"id": "a", "close": 1}]}),
            ("accounts[0].close", "unknown"),
        ),
        (
            json!({"accounts": [{"id": "a", "row": 1}]}),
            ("accounts[0].row", "unknown"),
        ),
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

/// ★ 格的种类只有五种：`row` 那种读不进来（声明里写了 ⇒ 这份策略读不懂）。
#[test]
fn a_row_cell_is_not_a_kind_any_more() {
    let r: Result<Cell, _> =
        serde_json::from_value(json!({"key": "row", "kind": "row", "scope": "each", "label": "x"}));
    assert!(r.is_err());
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
}

/// ★ 说明写法：升级材料合规；标签里写了第二人称或讲结构的字 ⇒ 不合规（设置照样能用）。
#[test]
fn second_person_or_structure_words_in_labels_are_flagged() {
    for n in 0..3 {
        let f = super::super::script::parse(super::super::upgrade::material(n))
            .expect("读")
            .head
            .form;
        assert!(!words_bad(&f), "材料 {n}");
    }
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

fn cell<'a>(f: &'a [Cell], key: &str) -> &'a Cell {
    f.iter().find(|c| c.key == key).expect(key)
}

/// ★ 继承只在这里解：号那一行有这一格（值是 `null` 也算有）⇒ 自己的；声明说跟总体 ⇒ 总体的（总体也没设 ⇒ 默认）；否则默认。
/// 24 小时条按窗解：那一窗是段组才算设了。
#[test]
fn a_cell_resolves_own_then_all_then_default_and_null_counts_as_set() {
    let f = full_form();
    let s = json!({
        "force": null,
        "capKind": "hard",
        "caps": {"5h": [{"from": 0, "to": 1440, "v": 80}]},
        "accounts": [
            {"start": true, "stint": null, "capKind": "soft", "caps": {"7d": [{"from": 0, "to": 600, "v": 30}]}},
            {"id": "b"},
        ],
    });
    assert_eq!(
        value_of(cell(&f, "force"), &s, None),
        (Value::Null, RotFrom::Own),
        "总体设成空的那一项"
    );
    assert_eq!(
        value_of(cell(&f, "close"), &s, None),
        (json!(0), RotFrom::Default)
    );
    assert_eq!(
        value_of(cell(&f, "stint"), &s, Some(0)),
        (Value::Null, RotFrom::Own),
        "号那一行设成空的那一项"
    );
    assert_eq!(
        value_of(cell(&f, "stint"), &s, Some(1)),
        (Value::Null, RotFrom::Default),
        "不跟总体 ⇒ 默认"
    );
    assert_eq!(
        value_of(cell(&f, "capKind"), &s, Some(0)),
        (json!("soft"), RotFrom::Own)
    );
    assert_eq!(
        value_of(cell(&f, "capKind"), &s, Some(1)),
        (json!("hard"), RotFrom::All)
    );
    assert_eq!(
        value_of(
            cell(&f, "capKind"),
            &json!({"accounts": [{"id": "b"}]}),
            Some(0)
        ),
        (json!("soft"), RotFrom::All),
        "总体也没设 ⇒ 默认，仍是跟总体"
    );
    let caps = cell(&f, "caps");
    assert_eq!(
        day_of(caps, &s, None, "5h"),
        (vec![(0, 1440, 80.0)], RotFrom::Own)
    );
    assert_eq!(day_of(caps, &s, None, "7d"), (vec![], RotFrom::Default));
    assert_eq!(
        day_of(caps, &s, Some(0), "7d"),
        (vec![(0, 600, 30.0)], RotFrom::Own)
    );
    assert_eq!(
        day_of(caps, &s, Some(0), "5h"),
        (vec![(0, 1440, 80.0)], RotFrom::All),
        "号设了别的窗，这一窗照样跟总体"
    );
    assert_eq!(
        day_of(caps, &s, Some(1), "5h"),
        (vec![(0, 1440, 80.0)], RotFrom::All)
    );
}

/// ★ 改设置交意图：设一格 · 刷段（照此刻生效的那组刷、写成那一行自己的；刷空 ⇒ 删那一窗、那一行全空 ⇒ 删那一格回到跟总体）·
/// 拖号牌（排到落点前头）· 加 · 移出 · 不动；行号越界 · 不是 24 小时条 ⇒ 入参不对。
#[test]
fn an_edit_is_applied_where_the_form_says() {
    let f = full_form();
    let s = json!({"caps": {"5h": [{"from": 0, "to": 1440, "v": 80}]},
                   "accounts": [{"start": true}, {"id": "a"}, {"id": "b", "delay": 5}]});
    let ap = |e: RotEdit| apply(&f, &s, &e);
    let painted = ap(RotEdit::Day {
        key: "caps".into(),
        row: Some(1),
        window: "5h".into(),
        from: 0,
        to: 600,
        v: Some(50.0),
    })
    .expect("ok");
    assert_eq!(
        painted["accounts"][1]["caps"],
        json!({"5h": [{"from": 0, "to": 600, "v": 50}, {"from": 600, "to": 1440, "v": 80}]}),
        "照跟着的总体那组刷"
    );
    assert_eq!(painted["caps"], s["caps"], "总体没动");
    let erased = apply(
        &f,
        &painted,
        &RotEdit::Day {
            key: "caps".into(),
            row: Some(1),
            window: "5h".into(),
            from: 0,
            to: 0,
            v: None,
        },
    )
    .expect("ok");
    assert_eq!(
        erased["accounts"][1],
        json!({"id": "a"}),
        "刷空 ⇒ 回到跟总体"
    );
    let total = ap(RotEdit::Day {
        key: "caps".into(),
        row: None,
        window: "5h".into(),
        from: 0,
        to: 0,
        v: None,
    })
    .expect("ok");
    assert_eq!(total["caps"], json!({}), "总体那一格刷空只删那一窗");
    assert_eq!(
        ap(RotEdit::Set {
            key: "force".into(),
            row: None,
            v: Value::Null
        })
        .expect("ok")["force"],
        Value::Null
    );
    assert_eq!(
        ap(RotEdit::Set {
            key: "delay".into(),
            row: Some(2),
            v: json!(9)
        })
        .expect("ok")["accounts"][2]["delay"],
        json!(9)
    );
    let ids = |v: Value| -> Vec<Value> {
        v["accounts"]
            .as_array()
            .expect("排")
            .iter()
            .map(|r| r.get("id").cloned().unwrap_or(json!("start")))
            .collect()
    };
    assert_eq!(
        ids(ap(RotEdit::Move { from: 2, to: 0 }).expect("ok")),
        [json!("b"), json!("start"), json!("a")]
    );
    assert_eq!(
        ids(ap(RotEdit::Move { from: 0, to: 2 }).expect("ok")),
        [json!("a"), json!("start"), json!("b")]
    );
    assert_eq!(
        ids(ap(RotEdit::Add {
            account: "c".into()
        })
        .expect("ok")),
        [json!("start"), json!("a"), json!("b"), json!("c")]
    );
    assert_eq!(
        ids(ap(RotEdit::Remove { row: 0 }).expect("ok")),
        [json!("a"), json!("b")]
    );
    assert_eq!(ap(RotEdit::Keep).expect("ok"), s);
    for bad in [
        RotEdit::Remove { row: 3 },
        RotEdit::Move { from: 0, to: 3 },
        RotEdit::Set {
            key: "delay".into(),
            row: Some(7),
            v: json!(1),
        },
        RotEdit::Day {
            key: "close".into(),
            row: None,
            window: "5h".into(),
            from: 0,
            to: 60,
            v: Some(1.0),
        },
        RotEdit::Day {
            key: "caps".into(),
            row: None,
            window: "5h".into(),
            from: 0,
            to: 2000,
            v: Some(1.0),
        },
    ] {
        assert!(apply(&f, &s, &bad).is_err(), "{bad:?}");
    }
}

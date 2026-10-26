//! 计划读面的帧面宿主：参数形状（一格的 agent 视角点到才起 pb：`dump_tests` 里真起假 pb 判）。

use super::*;
use crate::wire::typed::tests::ask;
use serde_json::json;

#[test]
fn missing_args_are_bad_args() {
    assert_eq!(ask("plan-read", json!({})).unwrap_err().code, "bad_args");
    assert_eq!(
        ask("plan-cell-view", json!({"workspace": "/w"}))
            .unwrap_err()
            .code,
        "bad_args"
    );
    assert_eq!(
        ask("plan-list", json!({"dirs": "x"})).unwrap_err().code,
        "bad_args"
    );
}

/// 入参按类型收：多一个键 ⇒ `bad_args`（不静默不认）。
#[test]
fn an_unknown_arg_is_refused() {
    let f = ask("plan-read", json!({"workspace": "/w", "fresh": true})).unwrap_err();
    assert_eq!(f.code, "bad_args");
    let f = ask(
        "plan-ack",
        json!({"workspace": "/w", "slice": "a", "key": "k", "x": 1}),
    )
    .unwrap_err();
    assert_eq!(f.code, "bad_args");
}

/// `slice` 只跟 `take` 走：`take` 不带 · 别的动词带了 ⇒ `bad_args`（在起 pb 之前）。
#[test]
fn slice_goes_with_take_and_only_with_it() {
    let f = ask("plan-command", json!({"workspace": "/w", "cmd": "take"})).unwrap_err();
    assert_eq!(f.code, "bad_args");
    let f = ask(
        "plan-command",
        json!({"workspace": "/w", "cmd": "pause", "slice": "a"}),
    )
    .unwrap_err();
    assert_eq!(f.code, "bad_args");
}

/// ★ 不重问时（`plan-list {}` · `sessions-needs` 计划那一截）同一个目录只起一次 pb：认出是工作区 · 不是工作区 · 读不成（pb 失败 · 不认得）
/// 都记在本子里，连问两次第二次照记下的那个结局；读不成的那一种照旧说 pb 不认得。`fresh` ⇒ 重起。
#[test]
fn an_unfresh_list_runs_pb_once_per_dir_whatever_it_said() {
    use crate::plan::dump::Ran;
    let tz = crate::Tz::default();
    let cases: [(&str, fn() -> Ran); 3] = [
        ("不是工作区", || Ran::NotWorkspace("x".into())),
        ("读不成", || Ran::Failed {
            said: "坏了".into(),
            raw: None,
        }),
        ("不认得", || Ran::Unsupported("老 pb".into())),
    ];
    for (name, ran) in cases {
        let b = crate::plan::book::Book::default();
        let runs = std::cell::Cell::new(0);
        let read = |dir: &Path| {
            runs.set(runs.get() + 1);
            b.ingest(ran(), dir, &crate::plan::fixture::who, 1)
        };
        let first = list_in(&b, vec!["/d".into()], false, &read, &tz);
        let again = list_in(&b, vec!["/d".into()], false, &read, &tz);
        assert_eq!(runs.get(), 1, "{name}：同一个目录连问两次只起一次 pb");
        assert_eq!(
            serde_json::to_value(&first).unwrap(),
            serde_json::to_value(&again).unwrap(),
            "{name}：第二次照记下的结局"
        );
        list_in(&b, vec!["/d".into()], true, &read, &tz);
        assert_eq!(runs.get(), 2, "{name}：fresh ⇒ 重起");
    }
}

/// `record` 要 `slice` · `summary` · `quotes` 都在；别的动词带了 `summary` / `quotes` ⇒ `bad_args`（在起 pb 之前）。
#[test]
fn record_takes_slice_summary_and_quotes_and_nothing_else_does() {
    for bad in [
        json!({"workspace": "/w", "cmd": "record", "summary": "x", "quotes": ["y"]}),
        json!({"workspace": "/w", "cmd": "record", "slice": "a"}),
        json!({"workspace": "/w", "cmd": "pause", "summary": "x"}),
        json!({"workspace": "/w", "cmd": "take", "slice": "a", "quotes": ["y"]}),
    ] {
        assert_eq!(
            ask("plan-command", bad.clone()).unwrap_err().code,
            "bad_args",
            "{bad}"
        );
    }
}

/// `plan-refs {sid, texts}`：入参按类型收（缺 · 多 ⇒ `bad_args`）；认不出片 ⇒ 两样 `null`、一段对一段地空，不起 pb。
#[test]
fn plan_refs_takes_sid_and_texts_and_answers_one_list_per_text() {
    assert_eq!(
        ask("plan-refs", json!({"texts": ["A1"]})).unwrap_err().code,
        "bad_args"
    );
    assert_eq!(
        ask("plan-refs", json!({"sid": "s", "texts": [], "x": 1}))
            .unwrap_err()
            .code,
        "bad_args"
    );
    let r = ask(
        "plan-refs",
        json!({"sid": "00000000-no-such-session", "texts": ["看 A1", "", "B2"]}),
    )
    .unwrap();
    assert_eq!(
        r,
        Some(json!({"workspace": null, "slice": null, "refs": [[], [], []]}))
    );
}

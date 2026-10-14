//! 计划读面的帧面宿主：参数形状 · 没读过的格没有 agent 视角。

use super::*;
use crate::wire::typed::tests::ask;

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

#[test]
fn a_cell_never_read_has_no_view() {
    let f = ask(
        "plan-cell-view",
        json!({"workspace": "/nowhere-read", "slice": "a", "id": "A1"}),
    )
    .unwrap_err();
    assert_eq!(f.code, "no_view");
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

//! 计划读面的帧面宿主：参数形状 · 没读过的格没有 agent 视角。

use super::*;

#[test]
fn missing_args_are_bad_args() {
    assert_eq!(
        answer("plan-read", &json!({}), &Default::default())
            .unwrap_err()
            .code,
        "bad_args"
    );
    assert_eq!(
        answer(
            "plan-cell-view",
            &json!({"workspace": "/w"}),
            &Default::default()
        )
        .unwrap_err()
        .code,
        "bad_args"
    );
    assert_eq!(
        answer("plan-list", &json!({"dirs": "x"}), &Default::default())
            .unwrap_err()
            .code,
        "bad_args"
    );
}

#[test]
fn a_cell_never_read_has_no_view() {
    let f = answer(
        "plan-cell-view",
        &json!({"workspace": "/nowhere-read", "slice": "a", "id": "A1"}),
        &Default::default(),
    )
    .unwrap_err();
    assert_eq!(f.code, "no_view");
}

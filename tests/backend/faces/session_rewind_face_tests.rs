//! `session-rewind-options`：入参形状 · 交给那台时换成本机 · 这台没有那条会话的记录 ⇒ `no_such_session`。

use super::*;

#[test]
fn the_request_takes_exactly_machine_and_sid() {
    let ok: SessionRewindOptionsReq = serde_json::from_value(serde_json::json!({
        "machine": "<local>",
        "sid": "00000000-0000-4000-8000-0000000000a1",
    }))
    .expect("两格就够");
    assert!(ok.machine.is_local());
    let extra = serde_json::from_value::<SessionRewindOptionsReq>(serde_json::json!({
        "machine": "<local>",
        "sid": "00000000-0000-4000-8000-0000000000a1",
        "how": "conversation",
    }));
    assert!(extra.is_err(), "多一格 ⇒ 拒（选项只问，不带选了什么）");
}

#[test]
fn handing_to_the_other_machine_rewrites_it_as_local() {
    let req = SessionRewindOptionsReq {
        machine: Machine::named("box2"),
        sid: "00000000-0000-4000-8000-0000000000a1".parse().unwrap(),
    };
    assert!(!req.machine().is_local());
    let there = req.here();
    assert!(there.machine.is_local());
    assert_eq!(there.sid, req.sid);
}

/// 测试沙箱的家里没有这条会话 ⇒ `no_such_session`（不照缺省那一家瞎给选项）。
#[test]
fn a_session_without_a_record_here_is_no_such_session() {
    let got = here("00000000-0000-4000-8000-0000000000a9").expect_err("沙箱里没有这条会话");
    assert_eq!(got.code, "no_such_session");
}

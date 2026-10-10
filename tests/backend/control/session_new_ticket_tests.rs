//! 起新会话那一趟的票：同一张票再问 ⇒ 不起第二个（起好了回原样那一份 · 还在起回 pending）；没起成 ⇒ 再问照常起；没票 ⇒ 每次都起。
use super::*;
use serde_json::json;
use std::cell::Cell;

fn pending() -> &'static str {
    "launch_pending"
}

#[test]
fn the_same_ticket_after_a_start_gets_the_same_answer_and_starts_nothing() {
    let t = Tickets::default();
    let runs = Cell::new(0);
    let start = || {
        runs.set(runs.get() + 1);
        Ok(json!({"outcome": "started", "session": "work"}))
    };
    let a = t.with(Some("t-1"), pending, start);
    let b = t.with(Some("t-1"), pending, || {
        runs.set(runs.get() + 1);
        Ok(json!({"outcome": "started", "session": "work-2"}))
    });
    assert_eq!(a, b, "同一张票第二次问回的是第一次那一份");
    assert_eq!(runs.get(), 1, "同一张票只起一次");
}

#[test]
fn a_ticket_still_starting_answers_pending_instead_of_starting_again() {
    let t = Tickets::default();
    let inner = t.with(Some("t-2"), pending, || {
        // 第一趟还没回来时，同一张票的第二趟到了。
        let second = t.with(Some("t-2"), pending, || panic!("还在起的那一趟被重起了"));
        assert_eq!(second, Err("launch_pending"));
        Ok(json!({"outcome": "started"}))
    });
    assert!(inner.is_ok());
}

#[test]
fn a_failed_start_forgets_its_ticket_so_asking_again_starts_it() {
    let t = Tickets::default();
    let a: Run<&str> = t.with(Some("t-3"), pending, || Err("tmux_taken"));
    assert_eq!(a, Err("tmux_taken"));
    let b = t.with(Some("t-3"), pending, || Ok(json!({"outcome": "started"})));
    assert_eq!(
        b,
        Ok(json!({"outcome": "started"})),
        "没起成的那一趟不挡同一张票再起"
    );
}

#[test]
fn without_a_ticket_every_ask_starts() {
    let t = Tickets::default();
    let runs = Cell::new(0);
    for _ in 0..2 {
        let _: Run<&str> = t.with(None, pending, || {
            runs.set(runs.get() + 1);
            Ok(json!({}))
        });
    }
    assert_eq!(runs.get(), 2);
}

#[test]
fn only_the_last_few_tickets_are_kept() {
    let t = Tickets::default();
    for i in 0..=KEEP {
        let _: Run<&str> = t.with(Some(&format!("k-{i}")), pending, || Ok(json!({ "n": i })));
    }
    let oldest: Run<&str> = t.with(Some("k-0"), pending, || Ok(json!({"n": "again"})));
    assert_eq!(
        oldest,
        Ok(json!({"n": "again"})),
        "最早那张被挤掉了 ⇒ 当没见过"
    );
    let newest: Run<&str> = t.with(Some(&format!("k-{KEEP}")), pending, || {
        Ok(json!({"n": "again"}))
    });
    assert_eq!(newest, Ok(json!({ "n": KEEP })));
}

#[test]
fn ticket_shape() {
    assert!(ticket_ok("0b6f0c1e-4c1d-4f0a-9e7a-2b1d3c4e5f60"));
    for bad in ["", "a b", "x;rm", &"x".repeat(65)] {
        assert!(!ticket_ok(bad), "{bad}");
    }
}

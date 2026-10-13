//! 往会话说一句：在等人 ⇒ 不送；不在等 ⇒ 照 `terminal-input` 那一条送（带回车，身份门同一份）。

use super::*;
use crate::control::terminals_wire::{Sent, SentResult, TerminalInputReq};
use std::cell::RefCell;

fn req(sid: &str, text: &str) -> SessionSayReq {
    SessionSayReq {
        sid: sid.into(),
        text: text.into(),
        client: Some("phone".into()),
    }
}

const SID: &str = "11111111-2222-3333-4444-555555555555";

/// ★ 那台说在等人（批准 · 回答）⇒ 一个字都不送，回被拒 ＋ `waiting` ＋ 那一句。
#[test]
fn a_session_that_waits_for_someone_gets_nothing_typed_into_it() {
    let sent = RefCell::new(Vec::<TerminalInputReq>::new());
    let send = |r: &TerminalInputReq| {
        sent.borrow_mut().push(r.clone());
        Ok(Sent {
            result: SentResult::Delivered,
            why: None,
            said: None,
            screen: None,
        })
    };
    let got = say_with(&req(SID, "继续"), &|_| true, &send).unwrap();
    assert_eq!(got.result, SentResult::Refused);
    assert_eq!(got.why, Some(crate::control::terminals::Refused::Waiting));
    assert_eq!(
        got.said.as_deref(),
        Some(copy_core::copy_text("beTerminal.no.waiting", &[]).as_str())
    );
    assert!(sent.borrow().is_empty(), "在等人时送了字");
}

/// ★ 不在等 ⇒ 按 sid 送这一句、补回车，自报的前端照交（过同一道身份门）；送的结局原样回。
#[test]
fn a_session_that_does_not_wait_gets_the_line_with_enter() {
    let sent = RefCell::new(Vec::<TerminalInputReq>::new());
    let send = |r: &TerminalInputReq| {
        sent.borrow_mut().push(r.clone());
        Ok(Sent {
            result: SentResult::Unsure,
            why: None,
            said: None,
            screen: None,
        })
    };
    let got = say_with(&req(SID, "/compact"), &|_| false, &send).unwrap();
    assert_eq!(got.result, SentResult::Unsure);
    let sent = sent.borrow();
    assert_eq!(sent.len(), 1);
    let r = &sent[0];
    assert_eq!(r.sid.as_deref(), Some(SID));
    assert_eq!(r.text.as_deref(), Some("/compact"));
    assert_eq!(r.enter, Some(true));
    assert_eq!(r.client.as_deref(), Some("phone"));
    assert!(r.terminal.is_none() && r.key.is_none() && r.seen_screen.is_none());
}

/// ★ 不是会话 id ⇒ `bad_args`，不问在不在等、不送。
#[test]
fn a_malformed_sid_is_refused_before_anything_is_asked() {
    let asked = RefCell::new(0);
    let got = say_with(
        &req("../x", "hi"),
        &|_| {
            *asked.borrow_mut() += 1;
            false
        },
        &|_| panic!("送了"),
    );
    assert_eq!(got.unwrap_err().code, "bad_args");
    assert_eq!(*asked.borrow(), 0);
}

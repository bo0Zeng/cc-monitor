//! 往会话说一句：在等人 ⇒ 不送；不在等 ⇒ 照 `terminal-input` 那一条送（带回车，身份门同一份）。

use super::*;
use crate::control::session_open::{Landed, Opened};
use crate::control::terminals::Refused;
use crate::control::terminals_wire::{Sent, SentResult, TerminalInputReq};
use std::cell::RefCell;

fn req(sid: &str, text: &str) -> SessionSayReq {
    SessionSayReq {
        sid: sid.into(),
        text: text.into(),
        client: Some("phone".into()),
        seen_screen: None,
    }
}

const SID: &str = "11111111-2222-3333-4444-555555555555";

/// 不该走到恢复那一支。
fn no_resume(_: &SessionSayReq) -> Option<Reopened> {
    panic!("恢复了")
}

fn refused(why: Refused) -> Sent {
    Sent {
        result: SentResult::Refused,
        why: Some(why),
        said: Some(crate::control::terminals::no_said(why)),
        screen: None,
        resumed: false,
    }
}

/// 恢复落在 `landed`（`session` ＝ 落在的那个 tmux 会话）。
fn at(landed: Landed, session: Option<&str>) -> Reopened {
    Ok(Opened {
        landed,
        session: session.map(str::to_string),
        line: None,
        sid: Some(SID.into()),
        agent: None,
        cwd: None,
        account: None,
        note: None,
        prompt: None,
    })
}

/// 恢复没成，码 `code`。
fn not(code: &'static str) -> Reopened {
    Err((code, "那一句".into(), None, Some("stderr".into())))
}

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
            resumed: false,
        })
    };
    let got = say_with(&req(SID, "继续"), &|_| true, &send, &no_resume).unwrap();
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
            resumed: false,
        })
    };
    let got = say_with(&req(SID, "/compact"), &|_| false, &send, &no_resume).unwrap();
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
        &|_| panic!("恢复了"),
    );
    assert_eq!(got.unwrap_err().code, "bad_args");
    assert_eq!(*asked.borrow(), 0);
}

/// ★ 终端没了（名单里没有 · 送的那一刻没了）⇒ 按记下的号恢复，那一句当第一句交出去；成了 ⇒ 交到了 ＋ `resumed`。
#[test]
fn a_session_whose_terminal_is_gone_is_resumed_with_the_line() {
    for gone in [Refused::NotKnown, Refused::Ended] {
        let asked = RefCell::new(Vec::<(String, String)>::new());
        let resume = |r: &SessionSayReq| {
            asked.borrow_mut().push((r.sid.clone(), r.text.clone()));
            Some(at(Landed::Started, Some("p-cc")))
        };
        let got = say_with(
            &req(SID, "接着做"),
            &|_| false,
            &|_| Ok(refused(gone)),
            &resume,
        )
        .unwrap();
        assert_eq!(got.result, SentResult::Delivered, "{gone:?}");
        assert!(
            got.resumed && got.why.is_none() && got.said.is_none(),
            "{got:?}"
        );
        assert_eq!(
            *asked.borrow(),
            vec![(SID.to_string(), "接着做".to_string())]
        );
        let wire = serde_json::to_value(&got).unwrap();
        assert_eq!(wire["resumed"], true);
    }
    // 送到了的那一形不带这一格。
    let wire = serde_json::to_value(Sent {
        result: SentResult::Delivered,
        why: None,
        said: None,
        screen: None,
        resumed: false,
    })
    .unwrap();
    assert!(wire.get("resumed").is_none(), "{wire}");
}

/// ★ 恢复没成：开在 tmux 之外的终端里（另有活进程在写）⇒ `elsewhere`；别的没成 ⇒ `not_resumed`；都带那一句、都不是交到了。
/// 这台找不到那条记录（恢复那一支答不出）⇒ 照送的那一下原样回。
#[test]
fn a_resume_that_does_not_happen_says_why_and_delivers_nothing() {
    let cases = [
        (Some(not("already_live")), Refused::Elsewhere),
        (Some(not("place_unavailable")), Refused::NotResumed),
        (Some(not("start_failed")), Refused::NotResumed),
        (None, Refused::NotKnown),
    ];
    for (a, want) in cases {
        let got = say_with(
            &req(SID, "接着做"),
            &|_| false,
            &|_| Ok(refused(Refused::NotKnown)),
            &|_| a.clone(),
        )
        .unwrap();
        assert_eq!(got.result, SentResult::Refused, "{want:?}");
        assert_eq!(got.why, Some(want));
        assert!(!got.resumed);
        assert_eq!(
            got.said.as_deref(),
            Some(crate::control::terminals::no_said(want).as_str())
        );
    }
}

/// ★ 别的被拒（别的前端的 · 不归我们管 · 多个 · 画面变了）不恢复，原样回。
#[test]
fn other_refusals_are_passed_back_without_resuming() {
    for why in [
        Refused::NotYours,
        Refused::NotManaged,
        Refused::Ambiguous,
        Refused::ScreenChanged,
    ] {
        let got = say_with(
            &req(SID, "hi"),
            &|_| false,
            &|_| Ok(refused(why)),
            &no_resume,
        )
        .unwrap();
        assert_eq!(got.why, Some(why));
    }
}

/// ★ 带了看见的那一屏的指纹 ⇒ 原样交给 `terminal-input`（画面变了它就不送，原样回）；
/// 送的那一刻终端没了也不恢复：看见的那一屏已经不在了，恢复出来的不是调用方看见的那一条。
#[test]
fn a_seen_screen_rides_along_and_a_gone_terminal_is_not_resumed() {
    let with_seen = |sid: &str| SessionSayReq {
        seen_screen: Some("0123456789abcdef".into()),
        ..req(sid, "看一下收件箱")
    };
    let sent = RefCell::new(Vec::<TerminalInputReq>::new());
    let changed = |r: &TerminalInputReq| {
        sent.borrow_mut().push(r.clone());
        Ok(Sent {
            screen: Some("fedcba9876543210".into()),
            ..refused(Refused::ScreenChanged)
        })
    };
    let got = say_with(&with_seen(SID), &|_| false, &changed, &no_resume).unwrap();
    assert_eq!(got.why, Some(Refused::ScreenChanged));
    assert_eq!(
        sent.borrow()[0].seen_screen.as_deref(),
        Some("0123456789abcdef"),
        "看见的那一屏的指纹没交给送字那一条"
    );
    for gone in [Refused::NotKnown, Refused::Ended] {
        let got = say_with(
            &with_seen(SID),
            &|_| false,
            &|_| Ok(refused(gone)),
            &no_resume,
        )
        .unwrap();
        assert_eq!((got.result, got.why), (SentResult::Refused, Some(gone)));
    }
}

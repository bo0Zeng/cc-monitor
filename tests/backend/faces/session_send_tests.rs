//! 往会话送进一句（投递那一下）：在等人 ⇒ 不送；不在等 ⇒ 照 `terminal-input` 那一条送（带回车，身份门同一份）；
//! 终端没了 ⇒ 按记下的号恢复、那一句当第一句；带附件 ⇒ 照那一家的写法拼，有一份不在 ⇒ 一个字都不送。

use super::*;
use crate::control::session_open::{Landed, Opened};
use crate::control::terminals::Refused;
use crate::control::terminals_wire::{Sent, SentResult, TerminalInputReq};
use std::cell::RefCell;

fn req(sid: &str, text: &str) -> Say {
    Say {
        sid: sid.parse().unwrap(),
        text: text.into(),
        client: Some("phone".into()),
        attachments: Vec::new(),
    }
}

/// 不带附件那一句原样送（没有单独粘的那几段）。
fn say_with(
    say: &Say,
    waiting: &dyn Fn(&str) -> bool,
    send: &dyn Fn(&TerminalInputReq) -> Result<Sent, Said>,
    resume: &dyn Fn(&Say) -> Option<Reopened>,
) -> Result<Delivery, Said> {
    let send = |r: &TerminalInputReq, pastes: &[String]| {
        assert!(pastes.is_empty(), "不带附件那一句粘了东西：{pastes:?}");
        send(r)
    };
    say_typed(say, &plain(&say.text), waiting, &send, resume)
}

const SID: &str = "11111111-2222-3333-4444-555555555555";

/// 不该走到恢复那一支。
fn no_resume(_: &Say) -> Option<Reopened> {
    panic!("恢复了")
}

fn refused(why: Refused) -> Sent {
    crate::control::terminals::input_reply(SentResult::Refused, Some(why), None)
}

/// 送那一下的结局（没被拒）。
fn sent_as(result: SentResult) -> Sent {
    crate::control::terminals::input_reply(result, None, None)
}

/// 恢复落在 `landed`（`session` ＝ 落在的那个 tmux 会话）。
fn at(landed: Landed, session: Option<&str>) -> Reopened {
    Ok(Opened {
        landed,
        session: session.map(str::to_string),
        at: None,
        line: None,
        sid: Some(SID.into()),
        agent: None,
        cwd: None,
        account: None,
        note: None,
        prompt: None,
        arrival: None,
    })
}

/// 恢复没成，码 `code`。
fn not(code: &'static str) -> Reopened {
    Err(crate::common::said::Said::new(code, "那一句".into()).and_raw(Some("stderr")))
}

/// ★ 那台说在等人（批准 · 回答）⇒ 一个字都不送，结局「在等人」（照排着）。
#[test]
fn a_session_that_waits_for_someone_gets_nothing_typed_into_it() {
    let sent = RefCell::new(Vec::<TerminalInputReq>::new());
    let send = |r: &TerminalInputReq| {
        sent.borrow_mut().push(r.clone());
        Ok(sent_as(SentResult::Delivered))
    };
    let got = say_with(&req(SID, "继续"), &|_| true, &send, &no_resume).unwrap();
    assert_eq!(got, Delivery::Waiting);
    assert!(sent.borrow().is_empty(), "在等人时送了字");
}

/// ★ 不在等 ⇒ 按 sid 送这一句、补回车，自报的前端照交（过同一道身份门）；送的结局原样回。
#[test]
fn a_session_that_does_not_wait_gets_the_line_with_enter() {
    let sent = RefCell::new(Vec::<TerminalInputReq>::new());
    let send = |r: &TerminalInputReq| {
        sent.borrow_mut().push(r.clone());
        Ok(sent_as(SentResult::Unsure))
    };
    let got = say_with(&req(SID, "/compact"), &|_| false, &send, &no_resume).unwrap();
    assert_eq!(
        got,
        Delivery::In { resumed: false },
        "不知道送没送到也算送进去了（不重发）"
    );
    let sent = sent.borrow();
    assert_eq!(sent.len(), 1);
    let r = &sent[0];
    assert_eq!(r.sid.as_deref(), Some(SID));
    assert_eq!(r.text.as_deref(), Some("/compact"));
    assert_eq!(r.enter, Some(true));
    assert_eq!(r.client.as_deref(), Some("phone"));
    assert!(r.terminal.is_none() && r.key.is_none() && r.seen_screen.is_none());
}

/// ★ 终端没了（名单里没有 · 送的那一刻没了）⇒ 按记下的号恢复，那一句当第一句交出去；成了 ⇒ 送进去了（恢复后交出去的）。
#[test]
fn a_session_whose_terminal_is_gone_is_resumed_with_the_line() {
    for gone in [Refused::NotKnown, Refused::Ended] {
        let asked = RefCell::new(Vec::<(String, String)>::new());
        let resume = |r: &Say| {
            asked.borrow_mut().push((r.sid.to_string(), r.text.clone()));
            Some(at(Landed::Started, Some("p-cc")))
        };
        let got = say_with(
            &req(SID, "接着做"),
            &|_| false,
            &|_| Ok(refused(gone)),
            &resume,
        )
        .unwrap();
        assert_eq!(got, Delivery::In { resumed: true }, "{gone:?}");
        assert_eq!(
            *asked.borrow(),
            vec![(SID.to_string(), "接着做".to_string())]
        );
    }
}

/// ★ 恢复没成：开在 tmux 之外的终端里（另有活进程在写）⇒ 另有活进程；别的没成 ⇒ 恢复没成；都送不进去。
/// 这台找不到那条记录（恢复那一支答不出）⇒ 照送的那一下原样（送字那一侧的被拒）。
#[test]
fn a_resume_that_does_not_happen_says_why_and_delivers_nothing() {
    let cases = [
        (Some(not("already_live")), Undeliverable::Unseen),
        (
            Some(at(Landed::Running, Some("p-cc"))),
            Undeliverable::Unseen,
        ),
        (Some(not("place_unavailable")), Undeliverable::NotResumed),
        (Some(not("start_failed")), Undeliverable::NotResumed),
        (None, Undeliverable::Refused(refused(Refused::NotKnown))),
    ];
    for (a, want) in cases {
        let got = say_with(
            &req(SID, "接着做"),
            &|_| false,
            &|_| Ok(refused(Refused::NotKnown)),
            &|_| a.clone(),
        )
        .unwrap();
        assert_eq!(got, Delivery::No(want));
    }
}

/// ★ 每一种送不进各说一句、写清哪一步没成：非空 · 不是缺键的占位 · 互不相同；送字被拒的照送字那一侧写好的那一句，附件的点名编号。
/// 按变体穷举：`kind_of` 是一个不带通配的 match，加一个变体这里就编不过，逼着补一个样本。
#[test]
fn every_undeliverable_says_which_step_failed() {
    const KINDS: usize = 5;
    fn kind_of(u: &Undeliverable) -> usize {
        match u {
            Undeliverable::Refused(_) => 0,
            Undeliverable::AttachmentGone(_) => 1,
            Undeliverable::NotResumed => 2,
            Undeliverable::Unseen => 3,
            Undeliverable::NotRewound(_) => 4,
        }
    }
    let not_yours = refused(Refused::NotYours);
    let samples = [
        Undeliverable::Refused(not_yours.clone()),
        Undeliverable::AttachmentGone(vec!["1027-1200-a3f2".into(), "1027-1201-b4e3".into()]),
        Undeliverable::NotResumed,
        Undeliverable::Unseen,
        Undeliverable::NotRewound("回退菜单对不上".into()),
    ];
    let mut kinds: Vec<usize> = samples.iter().map(kind_of).collect();
    kinds.sort_unstable();
    assert_eq!(
        kinds,
        (0..KINDS).collect::<Vec<_>>(),
        "每个变体恰好一个样本"
    );
    let said: Vec<String> = samples.iter().map(Undeliverable::said).collect();
    for (u, s) in samples.iter().zip(&said) {
        assert!(!s.trim().is_empty(), "{u:?} 一个字都没说");
        assert!(!s.contains('〔'), "{u:?} 用的键不在文案表里：{s}");
    }
    let mut uniq = said.clone();
    uniq.sort();
    uniq.dedup();
    assert_eq!(uniq.len(), said.len(), "两种送不进说成了同一句：{said:?}");
    assert_eq!(
        Some(said[0].clone()),
        not_yours.said.clone(),
        "送字被拒照送字那一侧那一句"
    );
    assert!(
        said[1].contains("1027-1200-a3f2") && said[1].contains("1027-1201-b4e3"),
        "附件那一句没点名编号：{}",
        said[1]
    );
    assert_eq!(said[4], "回退菜单对不上");
}

/// ★ 别的被拒（别的前端的 · 不归我们管 · 多个）不恢复，原样回。
#[test]
fn other_refusals_are_passed_back_without_resuming() {
    for why in [Refused::NotYours, Refused::NotManaged, Refused::Ambiguous] {
        let got = say_with(
            &req(SID, "hi"),
            &|_| false,
            &|_| Ok(refused(why)),
            &no_resume,
        )
        .unwrap();
        assert_eq!(got, Delivery::No(Undeliverable::Refused(refused(why))));
    }
}

// ── 带附件的那一句 ────────────────────────────────────────────

/// 一个会话工作目录：`ids` 各落一份（编号目录里一份 `<名>`）。
fn workdir(tag: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ccm-send-att-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    for (id, name) in files {
        let at = d.join(".ccm-attach").join(id);
        std::fs::create_dir_all(&at).expect("建编号目录");
        std::fs::write(at.join(name), b"x").expect("摆一份");
    }
    d
}

/// ★ 带一张图一份文件（Claude）：送字那一条收到「图的绝对路径单独粘」＋「正文后面一行 `@相对路径`」。
#[test]
fn a_line_with_attachments_types_what_the_agent_adapter_wrote() {
    let cwd = workdir(
        "ok",
        &[("1023-1529-a3f2", "a.png"), ("1023-1529-7c01", "login.css")],
    );
    let mut r = req(SID, "截图在这");
    r.attachments = vec!["1023-1529-a3f2".into(), "1023-1529-7c01".into()];
    let typed = typed_of(&r, crate::agents::claudecode::AGENT_KIND, &cwd).expect("都在");
    let abs = cwd
        .join(".ccm-attach/1023-1529-a3f2/a.png")
        .display()
        .to_string();
    assert_eq!(typed.pastes, std::slice::from_ref(&abs));
    assert_eq!(
        typed.text,
        "截图在这\n@.ccm-attach/1023-1529-7c01/login.css"
    );
    let sent = RefCell::new(Vec::<(TerminalInputReq, Vec<String>)>::new());
    let send = |t: &TerminalInputReq, pastes: &[String]| {
        sent.borrow_mut().push((t.clone(), pastes.to_vec()));
        Ok(sent_as(SentResult::Delivered))
    };
    let got = say_typed(&r, &typed, &|_| false, &send, &no_resume).unwrap();
    assert_eq!(got, Delivery::In { resumed: false });
    let (s, pastes) = &sent.borrow()[0];
    assert_eq!(pastes, &[abs]);
    assert_eq!(s.text.as_deref(), Some(typed.text.as_str()));
    let _ = std::fs::remove_dir_all(&cwd);
}

/// 有一份那台已清掉 ⇒ 一个字都不送：送不进去 ＋ 附件已不在，列出那几个编号（与投进队那一刻同一份判法）。
#[test]
fn a_cleared_attachment_refuses_the_whole_line_and_names_it() {
    let cwd = workdir("gone", &[("1023-1529-a3f2", "a.png")]);
    let mut r = req(SID, "再看下");
    r.attachments = vec!["1023-1529-a3f2".into(), "1023-1530-e95b".into()];
    let no = typed_of(&r, crate::agents::claudecode::AGENT_KIND, &cwd).expect_err("该拒");
    assert_eq!(
        no,
        Delivery::No(Undeliverable::AttachmentGone(vec!["1023-1530-e95b".into()]))
    );
    assert_eq!(
        crate::common::attachment_dir::missing(&cwd, &r.attachments),
        ["1023-1530-e95b"]
    );
    let _ = std::fs::remove_dir_all(&cwd);
}

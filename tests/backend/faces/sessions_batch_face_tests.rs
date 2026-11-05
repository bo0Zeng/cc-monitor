//! `sessions-batch` 合各台：照原序摆好逐条结局、远端称呼套机器名、那台没成 ⇒ 那一组每项都带那台那一句；入参不对 ⇒ 拒。

use super::*;
use crate::dial::remote_ask::test_peers::{until, Peers};

fn item(machine: &str, sid: &str) -> SessionItem {
    SessionItem {
        machine: if machine == "here" {
            Machine::local()
        } else {
            Machine::named(machine)
        },
        sid: sid.parse().expect("sid"),
    }
}

fn stop(items: Vec<SessionItem>) -> SessionsBatchReq {
    SessionsBatchReq {
        op: SessionsOp::Stop,
        items,
        client: None,
    }
}

fn outcome(sid: &str, state: SessionState, said: Option<&str>, detail: &str) -> SessionOutcome {
    SessionOutcome {
        machine: Machine::local(),
        sid: sid.parse().expect("sid"),
        outcome: state,
        why: said.map(|_| "io_failed".to_string()),
        said: said.map(str::to_string),
        copy_detail: detail.to_string(),
        title: sid.to_string(),
        session: None,
    }
}

/// 替身本机：那一组每项都成了（记下交进来的那几个 id）。
fn all_done(
    seen: &std::sync::Mutex<Vec<String>>,
) -> impl FnOnce(SessionsBatchReq) -> std::future::Ready<Result<SessionsBatchReply, Said>> + '_ {
    move |r: SessionsBatchReq| {
        let results: Vec<SessionOutcome> = r
            .items
            .iter()
            .map(|i| {
                seen.lock().unwrap().push(i.sid.to_string());
                outcome(i.sid.as_str(), SessionState::Done, None, "")
            })
            .collect();
        std::future::ready(Ok(sessions_reply(&verb_of(&SessionsOp::Stop), results)))
    }
}

fn head(n: &str, ok: &str, bad: &str) -> String {
    let action = copy_text("tabBatch.verb.stop", &[]);
    [
        copy_text("beFiles.batch.head", &[("action", &action), ("n", n)]),
        copy_text("beFiles.batch.segOk", &[("ok", ok)]),
        copy_text("beFiles.batch.segBad", &[("bad", bad)]),
    ]
    .join(&copy_text("kit.text.sep", &[]))
}

/// ★ 三项跨两台（本机 s-1 · s-3，box2 的 s-2）：本机都成、box2 那一项没成 ⇒ 照原序三条、合好那一句、
/// 那一行带机器名、复制详情只有 box2 那一段；box2 只被问它自己那一项。
#[tokio::test]
async fn two_machines_merge_in_item_order() {
    let box2 = sessions_reply(
        &verb_of(&SessionsOp::Stop),
        vec![outcome(
            "s-2",
            SessionState::Failed,
            Some("tmux silent"),
            "exit 1",
        )],
    );
    let peers = Peers::new(&[("box2", serde_json::to_string(&box2).unwrap())]);
    let table = Peers::table(&["box2"]);
    let seen = std::sync::Mutex::new(Vec::new());
    let r = across(
        stop(vec![
            item("here", "s-1"),
            item("box2", "s-2"),
            item("here", "s-3"),
        ]),
        all_done(&seen),
        &crate::common::time::Tz::default(),
        until(5_000),
        &table,
        &peers,
    )
    .await
    .expect("答得出");
    let sids: Vec<String> = r.results.iter().map(|o| o.sid.to_string()).collect();
    assert_eq!(sids, ["s-1", "s-2", "s-3"]);
    assert_eq!(*seen.lock().unwrap(), ["s-1", "s-3"], "本机只做本机那一组");
    assert_eq!(r.results[1].machine, Machine::named("box2"));
    let title = copy_text(
        "tabBatch.line.remote",
        &[("title", "s-2"), ("machine", "box2")],
    );
    assert_eq!(r.results[1].title, title);
    assert_eq!(r.said, head("3", "2", "1"));
    assert!(r.failed);
    assert_eq!(
        r.lines,
        vec![copy_text(
            "tabBatch.result.failedLine",
            &[("title", &title), ("why", "tmux silent")]
        )]
    );
    assert_eq!(r.copy_detail, "tmux silent\nexit 1");
    let asked = peers.asked();
    assert_eq!(asked.len(), 1);
    let items = &asked[0].1["args"]["items"];
    assert_eq!(
        items,
        &serde_json::json!([{"machine": cmd_wire::LOCAL, "sid": "s-2"}]),
        "那台只拿到它自己那一项，机器写成它眼里的本机"
    );
}

/// ★ box2 常驻不在 ⇒ 那一项没成、带那台那一句与复制详情；本机那两项照成。
#[tokio::test]
async fn a_machine_that_cannot_answer_fails_only_its_own_items() {
    let peers = Peers::new(&[]);
    let table = Peers::table(&["box2"]);
    let seen = std::sync::Mutex::new(Vec::new());
    let r = across(
        stop(vec![
            item("here", "s-1"),
            item("here", "s-2"),
            item("box2", "s-3"),
        ]),
        all_done(&seen),
        &crate::common::time::Tz::default(),
        until(5_000),
        &table,
        &peers,
    )
    .await
    .expect("答得出");
    let states: Vec<SessionState> = r.results.iter().map(|o| o.outcome).collect();
    assert_eq!(
        states,
        [SessionState::Done, SessionState::Done, SessionState::Failed]
    );
    let s3 = &r.results[2];
    assert_eq!(
        s3.said.as_deref(),
        Some(crate::wire::listen::call_absent("box2").as_str())
    );
    assert!(!s3.copy_detail.is_empty(), "没成的那一项带复制详情");
    assert_eq!(r.said, head("3", "2", "1"));
    assert_eq!(r.lines.len(), 1);
}

/// 机器表里没有那台 ⇒ 那一项没成、码 `unreachable`；别的台照做。
#[tokio::test]
async fn a_machine_not_in_the_table_is_unreachable() {
    let peers = Peers::new(&[]);
    let table = crate::dial::remote_ask::test_peers::empty();
    let seen = std::sync::Mutex::new(Vec::new());
    let r = across(
        stop(vec![item("here", "s-1"), item("nowhere", "s-2")]),
        all_done(&seen),
        &crate::common::time::Tz::default(),
        until(5_000),
        &table,
        &peers,
    )
    .await
    .expect("答得出");
    assert_eq!(r.results[0].outcome, SessionState::Done);
    assert_eq!(r.results[1].outcome, SessionState::Failed);
    assert_eq!(r.results[1].why.as_deref(), Some("unreachable"));
    assert!(peers.asked().is_empty());
}

/// 入参不对 ⇒ 拒：一项都没有 · 同一台同一条写了两次 · 不是停。
#[tokio::test]
async fn bad_batches_are_refused() {
    let peers = Peers::new(&[]);
    let table = crate::dial::remote_ask::test_peers::empty();
    let seen = std::sync::Mutex::new(Vec::new());
    let dup = stop(vec![item("here", "s-1"), item("here", "s-1")]);
    let empty = stop(vec![]);
    for bad in [dup, empty] {
        let e = across(
            bad,
            all_done(&seen),
            &crate::common::time::Tz::default(),
            None,
            &table,
            &peers,
        )
        .await;
        assert!(e.is_err());
    }
    assert!(seen.lock().unwrap().is_empty(), "拒了还做了");
}

fn start(items: Vec<SessionItem>, run: cmd_wire::session::Launch) -> SessionsBatchReq {
    SessionsBatchReq {
        op: SessionsOp::Start {
            run,
            relayed: false,
        },
        items,
        client: None,
    }
}

fn window(machine: Machine, sid: &str, line: &str) -> OpenWindow {
    OpenWindow {
        machine,
        sid: sid.parse().expect("sid"),
        line: line.to_string(),
        cwd: Some("/w".to_string()),
    }
}

/// 那一句的头 ＋ 几段（照给的先后）。
fn said_line(action: &str, n: &str, segs: &[(&str, &str, &str)]) -> String {
    let mut parts = vec![copy_text(
        "beFiles.batch.head",
        &[("action", action), ("n", n)],
    )];
    parts.extend(segs.iter().map(|(k, arg, v)| copy_text(k, &[(arg, v)])));
    parts.join(&copy_text("kit.text.sep", &[]))
}

/// ★ 起：两台各两项、本机一项本来在跑 ⇒ 那一句照「恢复」那个词合（完成 3 · 跳过 1）；
/// 本机那一组照界面交的（不是转来的），box2 那一组带 `relayed`；开窗那几行照原序、远端那一行写那台。
#[tokio::test]
async fn start_across_two_machines_merges_and_relays() {
    use cmd_wire::session::Launch;
    let op = SessionsOp::Start {
        run: Launch::Direct,
        relayed: false,
    };
    let box2 = SessionsBatchReply {
        windows: vec![window(Machine::local(), "s-2", "line-2")],
        ..sessions_reply(
            &verb_of(&op),
            vec![
                outcome("s-2", SessionState::Done, None, ""),
                outcome("s-4", SessionState::Done, None, ""),
            ],
        )
    };
    let peers = Peers::new(&[("box2", serde_json::to_string(&box2).unwrap())]);
    let table = Peers::table(&["box2"]);
    let got_op = std::sync::Mutex::new(None);
    let running = copy_text("tabBatch.why.running", &[("name", "proj-cc")]);
    let here = |r: SessionsBatchReq| {
        *got_op.lock().unwrap() = Some(r.op);
        let results = vec![
            SessionOutcome {
                why: Some("running".into()),
                ..outcome("s-1", SessionState::Skipped, Some(&running), "")
            },
            outcome("s-3", SessionState::Done, None, ""),
        ];
        std::future::ready(Ok(SessionsBatchReply {
            windows: vec![window(Machine::local(), "s-3", "line-3")],
            ..sessions_reply(&verb_of(&r.op), results)
        }))
    };
    let r = across(
        start(
            vec![
                item("here", "s-1"),
                item("box2", "s-2"),
                item("here", "s-3"),
                item("box2", "s-4"),
            ],
            Launch::Direct,
        ),
        here,
        &crate::common::time::Tz::default(),
        until(5_000),
        &table,
        &peers,
    )
    .await
    .expect("答得出");
    let action = copy_text("tabBatch.verb.startWindow", &[]);
    assert_eq!(
        r.said,
        said_line(
            &action,
            "4",
            &[
                ("beFiles.batch.segOk", "ok", "3"),
                ("beFiles.batch.segSkip", "skip", "1")
            ]
        )
    );
    assert!(!r.failed);
    assert_eq!(r.results[0].said.as_deref(), Some(running.as_str()));
    assert_eq!(*got_op.lock().unwrap(), Some(op), "本机那一组照界面交的");
    let asked = peers.asked();
    assert_eq!(
        asked[0].1["args"]["op"],
        serde_json::json!({"start": {"run": "direct", "relayed": true}}),
        "转给别的台那一组：开窗的不是它"
    );
    assert_eq!(
        r.windows,
        vec![
            window(Machine::named("box2"), "s-2", "line-2"),
            window(Machine::local(), "s-3", "line-3"),
        ],
        "开窗那几行照原序，远端那一行写那台"
    );
}

/// 起一项：会话脸说恢复不了 ⇒ 跳过（那一格那一句，不问 `session-open`）；本来就在跑 ⇒ 跳过（在哪个 tmux 会话里跑）；
/// 落在窗口里 ⇒ 成了、带开窗那一行；那台拒了 ⇒ 没成，那台那一句 ＋ 复制详情。
#[test]
fn one_start_maps_the_open_answer() {
    use cmd_wire::session::{Landed, Opened};
    let opened = |landed, session: Option<&str>, line: Option<&str>| Opened {
        landed,
        session: session.map(str::to_string),
        host: session.map(|_| crate::wire::TerminalHost::Tmux),
        line: line.map(str::to_string),
        sid: Some("s-1".into()),
        agent: None,
        cwd: Some("/w".into()),
        account: None,
        note: None,
        prompt: None,
        arrival: None,
    };
    let sid: cmd_wire::Sid = "s-1".parse().unwrap();
    let no = crate::wire::Allow::No {
        why: crate::common::cells::Words("那台说·恢复不了".into()),
    };
    let asked = std::cell::Cell::new(false);
    let (o, w) = start_one(&sid, "proj 起头".into(), no, || {
        asked.set(true);
        Ok(opened(Landed::Started, None, None))
    });
    assert!(!asked.get(), "脸上恢复不了 ⇒ 不问");
    assert_eq!(o.outcome, SessionState::Skipped);
    assert_eq!(o.said.as_deref(), Some("那台说·恢复不了"));
    assert_eq!(o.title, "proj 起头");
    assert!(w.is_none());

    let yes = || crate::wire::Allow::Yes;
    let (o, _) = start_one(&sid, "t".into(), yes(), || {
        Ok(opened(
            Landed::Running,
            Some("proj-cc"),
            Some("tmux attach"),
        ))
    });
    assert_eq!(o.outcome, SessionState::Skipped);
    assert_eq!(o.why.as_deref(), Some("running"));
    assert_eq!(
        o.said,
        Some(copy_text("tabBatch.why.running", &[("name", "proj-cc")]))
    );

    let (o, w) = start_one(&sid, "t".into(), yes(), || {
        Ok(opened(Landed::Window, None, Some("claude --resume s-1")))
    });
    assert_eq!(o.outcome, SessionState::Done);
    assert_eq!(o.said, None);
    assert_eq!(
        w,
        Some(window(Machine::local(), "s-1", "claude --resume s-1"))
    );

    let (o, w) = start_one(&sid, "t".into(), yes(), || {
        Ok(opened(
            Landed::Started,
            Some("proj-cc"),
            Some("tmux attach"),
        ))
    });
    assert_eq!(o.outcome, SessionState::Done);
    assert_eq!(o.session.as_deref(), Some("proj-cc"));
    assert!(w.is_none(), "落在 tmux 里不开窗");

    let (o, _) = start_one(&sid, "t".into(), yes(), || {
        Err(Said::new("launch_pending", "那一趟还在起".into()))
    });
    assert_eq!(o.outcome, SessionState::Failed);
    assert_eq!(o.why.as_deref(), Some("launch_pending"));
    assert_eq!(o.said.as_deref(), Some("那一趟还在起"));
    assert!(!o.copy_detail.is_empty(), "没成的带复制详情");
}

fn ask(op: SessionsOp, items: Vec<SessionItem>) -> SessionsBatchReq {
    SessionsBatchReq {
        op,
        items,
        client: None,
    }
}

/// ★ 隐藏：三项跨两台（box2 那一项那台写注解没成）⇒ 那一句照「隐藏」那个词合（完成 2 · 失败 ×1）；
/// 撤销那几问照各台答回的合起来：同一样的并成一问、照原序、远端那一项写那台。
#[tokio::test]
async fn hide_across_two_machines_merges_the_said_and_the_undo_asks() {
    let local = |sid: &str| item("here", sid);
    let box2 = SessionsBatchReply {
        undo: vec![ask(SessionsOp::ClearHide, vec![local("s-2")])],
        ..sessions_reply(
            &verb_of(&SessionsOp::Hide),
            vec![
                outcome("s-2", SessionState::Done, None, ""),
                outcome("s-4", SessionState::Failed, Some("写不下去"), "io"),
            ],
        )
    };
    let peers = Peers::new(&[("box2", serde_json::to_string(&box2).unwrap())]);
    let table = Peers::table(&["box2"]);
    let here = |r: SessionsBatchReq| {
        assert_eq!(r.op, SessionsOp::Hide);
        let results = r
            .items
            .iter()
            .map(|i| outcome(i.sid.as_str(), SessionState::Done, None, ""))
            .collect();
        std::future::ready(Ok(SessionsBatchReply {
            undo: vec![
                ask(SessionsOp::ClearHide, vec![local("s-1")]),
                ask(SessionsOp::Unhide, vec![local("s-3")]),
            ],
            ..sessions_reply(&verb_of(&r.op), results)
        }))
    };
    let r = across(
        ask(
            SessionsOp::Hide,
            vec![
                local("s-1"),
                item("box2", "s-2"),
                local("s-3"),
                item("box2", "s-4"),
            ],
        ),
        here,
        &crate::common::time::Tz::default(),
        until(5_000),
        &table,
        &peers,
    )
    .await
    .expect("答得出");
    let action = copy_text("history.menu.hide", &[]);
    assert_eq!(
        r.said,
        said_line(
            &action,
            "4",
            &[
                ("beFiles.batch.segOk", "ok", "3"),
                ("beFiles.batch.segBad", "bad", "1")
            ]
        )
    );
    assert_eq!(
        peers.asked()[0].1["args"]["op"],
        serde_json::json!("hide"),
        "隐藏那一组照原样转"
    );
    assert_eq!(
        r.undo,
        vec![
            ask(
                SessionsOp::ClearHide,
                vec![local("s-1"), item("box2", "s-2")]
            ),
            ask(SessionsOp::Unhide, vec![local("s-3")]),
        ]
    );
}

/// 隐藏一项：写成了 ⇒ 成了、撤销那一问照改之前那一格（没有 ⇒ 回到规则 · `off` ⇒ 取消隐藏 · `on` ⇒ 隐藏）；
/// 取消隐藏 · 回到规则成了 ⇒ 撤不回；写不下去 ⇒ 没成（那一句 ＋ 复制详情）。
#[test]
fn one_hide_maps_the_write() {
    use cmd_wire::session::Hide;
    let sid: cmd_wire::Sid = "s-1".parse().unwrap();
    for (prev, undo) in [
        (None, SessionsOp::ClearHide),
        (Some(Hide::Off), SessionsOp::Unhide),
        (Some(Hide::On), SessionsOp::Hide),
    ] {
        let (o, u) = hide_one(&sid, "t".into(), SessionsOp::Hide, || Ok(prev));
        assert_eq!(o.outcome, SessionState::Done);
        assert_eq!(u, Some(undo), "改之前 {prev:?}");
    }
    for op in [SessionsOp::Unhide, SessionsOp::ClearHide] {
        let (o, u) = hide_one(&sid, "t".into(), op, || Ok(Some(Hide::On)));
        assert_eq!(o.outcome, SessionState::Done);
        assert_eq!(u, None);
    }
    let (o, u) = hide_one(&sid, "proj 起头".into(), SessionsOp::Hide, || {
        Err(Said::new("io_failed", "写不下去".into()))
    });
    assert_eq!(o.outcome, SessionState::Failed);
    assert_eq!(o.why.as_deref(), Some("io_failed"));
    assert_eq!(o.said.as_deref(), Some("写不下去"));
    assert_eq!(o.title, "proj 起头");
    assert!(!o.copy_detail.is_empty());
    assert_eq!(u, None);
}

/// ★ 固定：三项一项那台写注解失败 ⇒ 那一项没成、那一句照「固定」那个词合（完成 2 · 失败 ×1）；撤不回 ⇒ 没有撤销那几问。
#[tokio::test]
async fn pin_across_two_machines_merges_the_said() {
    let box2 = sessions_reply(
        &verb_of(&SessionsOp::Pin),
        vec![outcome("s-2", SessionState::Failed, Some("写不下去"), "io")],
    );
    let peers = Peers::new(&[("box2", serde_json::to_string(&box2).unwrap())]);
    let table = Peers::table(&["box2"]);
    let here = |r: SessionsBatchReq| {
        let results = r
            .items
            .iter()
            .map(|i| outcome(i.sid.as_str(), SessionState::Done, None, ""))
            .collect();
        std::future::ready(Ok(sessions_reply(&verb_of(&r.op), results)))
    };
    let r = across(
        ask(
            SessionsOp::Pin,
            vec![
                item("here", "s-1"),
                item("box2", "s-2"),
                item("here", "s-3"),
            ],
        ),
        here,
        &crate::common::time::Tz::default(),
        until(5_000),
        &table,
        &peers,
    )
    .await
    .expect("答得出");
    let action = copy_text("tabMenu.pin.pin", &[]);
    assert_eq!(
        r.said,
        said_line(
            &action,
            "3",
            &[
                ("beFiles.batch.segOk", "ok", "2"),
                ("beFiles.batch.segBad", "bad", "1")
            ]
        )
    );
    assert_eq!(r.results[1].outcome, SessionState::Failed);
    assert!(r.undo.is_empty());
}

/// 固定一项：注解里已经是那一格 ⇒ 跳过（已固定 · 未固定那一句，不写）；不然写那一格。
#[test]
fn one_pin_skips_what_is_already_there() {
    let sid: cmd_wire::Sid = "s-1".parse().unwrap();
    let wrote = std::cell::Cell::new(false);
    let o = pin_one(&sid, "t".into(), true, true, || {
        wrote.set(true);
        Ok(())
    });
    assert!(!wrote.get(), "已固定 ⇒ 不写");
    assert_eq!(o.outcome, SessionState::Skipped);
    assert_eq!(o.said, Some(copy_text("tabBatch.why.pinned", &[])));
    let o = pin_one(&sid, "t".into(), false, false, || Ok(()));
    assert_eq!(o.outcome, SessionState::Skipped);
    assert_eq!(o.said, Some(copy_text("tabBatch.why.notPinned", &[])));
    let o = pin_one(&sid, "t".into(), true, false, || {
        wrote.set(true);
        Ok(())
    });
    assert!(wrote.get());
    assert_eq!(o.outcome, SessionState::Done);
    let o = pin_one(&sid, "t".into(), true, false, || {
        Err(Said::new("io_failed", "写不下去".into()))
    });
    assert_eq!(o.outcome, SessionState::Failed);
    assert_eq!(o.said.as_deref(), Some("写不下去"));
}

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
        std::future::ready(Ok(sessions_reply(&stop_verb(), results)))
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
        &stop_verb(),
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
        &crate::Tz::default(),
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
        &crate::Tz::default(),
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
        &crate::Tz::default(),
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
            &crate::Tz::default(),
            None,
            &table,
            &peers,
        )
        .await;
        assert!(e.is_err());
    }
    assert!(seen.lock().unwrap().is_empty(), "拒了还做了");
}

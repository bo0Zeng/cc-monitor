//! `terminals-end` 合各台：照原序摆好逐条结局、远端标题套机器名、那台没成 ⇒ 那一组每项都带那台那一句；入参不对 ⇒ 拒。
//! 本机那一项：结束成了 ⇒ 成了；那台拒了 ⇒ 没成（码 · 那一句 · 复制详情）。

use super::*;
use crate::dial::remote_ask::test_peers::{until, Peers};

fn item(machine: &str, terminal: &str) -> TerminalsEndItem {
    TerminalsEndItem {
        machine: if machine == "here" {
            Machine::local()
        } else {
            Machine::named(machine)
        },
        terminal: terminal.to_string(),
    }
}

fn end(items: Vec<TerminalsEndItem>) -> TerminalsEndReq {
    TerminalsEndReq {
        items,
        client: None,
    }
}

/// 替身本机：那一组每项都成了（记下交进来的那几个句柄）。
fn all_done(
    seen: &std::sync::Mutex<Vec<String>>,
) -> impl FnOnce(TerminalsEndReq) -> std::future::Ready<Result<TerminalsEndReply, Said>> + '_ {
    move |r: TerminalsEndReq| {
        let results = r
            .items
            .iter()
            .map(|i| {
                seen.lock().unwrap().push(i.terminal.clone());
                one(&i.terminal, i.terminal.clone(), || {
                    Ok(TerminalEnded {
                        terminal: i.terminal.clone(),
                    })
                })
            })
            .collect();
        std::future::ready(Ok(reply_of(results)))
    }
}

/// ★ 三行跨两台（本机 t1 · t2 成，box2 的 t3 那台答 `other_windows`）⇒ 照原序三条、合好那一句、
/// 那一行带「（box2）」、复制详情只有 box2 那一段；box2 只被问它自己那一项。
#[tokio::test]
async fn two_machines_merge_in_item_order() {
    let box2 = reply_of(vec![one("t3", "t3".to_string(), || {
        Err(Said::new("other_windows", "还开着别的窗口".into()).and_raw(Some("2 clients")))
    })]);
    let peers = Peers::new(&[("box2", serde_json::to_string(&box2).unwrap())]);
    let table = Peers::table(&["box2"]);
    let seen = std::sync::Mutex::new(Vec::new());
    let r = across(
        end(vec![item("here", "t1"), item("box2", "t3"), item("here", "t2")]),
        all_done(&seen),
        &crate::Tz::default(),
        until(5_000),
        &table,
        &peers,
    )
    .await
    .expect("答得出");
    let ts: Vec<&str> = r.results.iter().map(|o| o.terminal.as_str()).collect();
    assert_eq!(ts, ["t1", "t3", "t2"]);
    assert_eq!(*seen.lock().unwrap(), ["t1", "t2"], "本机只做本机那一组");
    assert_eq!(r.results[1].machine, Machine::named("box2"));
    assert_eq!(r.results[1].why.as_deref(), Some("other_windows"));
    assert_eq!(r.said, "结束 3 项 · 完成 2 · 失败 ×1");
    assert!(r.failed);
    assert_eq!(r.lines.len(), 1);
    assert!(r.lines[0].contains("（box2）"), "{:?}", r.lines);
    assert!(r.lines[0].contains("还开着别的窗口"), "{:?}", r.lines);
    assert!(r.copy_detail.starts_with("还开着别的窗口\n"), "{}", r.copy_detail);
    assert!(r.copy_detail.contains("2 clients"), "{}", r.copy_detail);
    let asked = peers.asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(
        asked[0].1["args"]["items"],
        serde_json::json!([{"machine": cmd_wire::LOCAL, "terminal": "t3"}]),
        "那台只拿到它自己那一项，机器写成它眼里的本机"
    );
}

/// ★ box2 常驻不在 ⇒ t3 没成、带那台那一句与复制详情；本机那两项照成。
#[tokio::test]
async fn a_machine_that_cannot_answer_fails_only_its_own_items() {
    let peers = Peers::new(&[]);
    let table = Peers::table(&["box2"]);
    let seen = std::sync::Mutex::new(Vec::new());
    let r = across(
        end(vec![item("here", "t1"), item("here", "t2"), item("box2", "t3")]),
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
    let t3 = &r.results[2];
    assert_eq!(
        t3.said.as_deref(),
        Some(crate::wire::listen::call_absent("box2").as_str())
    );
    assert!(!t3.copy_detail.is_empty(), "没成的那一项带复制详情");
    assert!(t3.title.contains("box2"), "{}", t3.title);
    assert_eq!(r.said, "结束 3 项 · 完成 2 · 失败 ×1");
}

/// 一项成了 ⇒ 点名那一句（同单个结束）；没成 ⇒ 点名「结束失败」那一句。
#[tokio::test]
async fn one_item_names_its_title() {
    let peers = Peers::new(&[]);
    let table = crate::dial::remote_ask::test_peers::empty();
    let seen = std::sync::Mutex::new(Vec::new());
    let r = across(
        end(vec![item("here", "t1")]),
        all_done(&seen),
        &crate::Tz::default(),
        None,
        &table,
        &peers,
    )
    .await
    .expect("答得出");
    assert_eq!(
        r.said,
        copy_text("sessionState.kill.done", &[("title", "t1")])
    );
    assert!(!r.failed);
    assert!(r.lines.is_empty());
}

/// 本机那一项：结束被拒 ⇒ 没成，码 · 那一句 · 复制详情照那台写的。
#[test]
fn a_refused_end_is_failed_with_the_said() {
    let o = one("t1", "proj".to_string(), || {
        Err(Said::new("wrong_owner", "不是这个前端起的".into()).and_raw(Some("owner=x")))
    });
    assert_eq!(o.outcome, SessionState::Failed);
    assert_eq!(o.why.as_deref(), Some("wrong_owner"));
    assert_eq!(o.said.as_deref(), Some("不是这个前端起的"));
    assert!(o.copy_detail.contains("owner=x"), "{}", o.copy_detail);
    assert_eq!(o.title, "proj");
}

/// 入参不对 ⇒ 拒：一项都没有 · 同一台同一个终端写了两次。
#[tokio::test]
async fn bad_batches_are_refused() {
    let peers = Peers::new(&[]);
    let table = crate::dial::remote_ask::test_peers::empty();
    let seen = std::sync::Mutex::new(Vec::new());
    for bad in [
        end(vec![item("here", "t1"), item("here", "t1")]),
        end(vec![]),
    ] {
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
    assert!(seen.lock().unwrap().is_empty());
}

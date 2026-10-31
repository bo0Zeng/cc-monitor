//! 计划退回：先判目标（已结束 / 认不出 ⇒ 只给复制；已结束而人点了恢复 ⇒ 照投）· 能投就投进那条会话的输入队
//! （在等人 · 一轮在跑也照投，投递自己等）、排进去了记一条。

use super::*;
use crate::plan::fixture::{as_json, dump, MAIN, SUB};
use crate::plan::{Live, Whose};
use cmd_wire::queue::QueuePutOutcome;
use serde_json::{json, Value};
use std::cell::RefCell;

/// 回包的线上那一形（判据按键看）。
type Out = Result<Value, Said>;

fn doc_with(who: &dyn Fn(&str) -> Whose) -> PlanRead {
    let b = crate::plan::book::Book::default();
    let d = dump("/w");
    let raw = d.to_string().into_bytes();
    b.ingest(
        crate::plan::dump::Ran::Dump { doc: d, raw },
        std::path::Path::new("/w"),
        who,
        1,
    )
    .unwrap()
}

fn live(activity: crate::agents::SessionActivity) -> impl Fn(&str) -> Whose {
    move |id: &str| {
        let l = Some(Live {
            activity: Some(activity),
            needs: None,
        });
        match id {
            MAIN => Whose::Session {
                sid: MAIN.into(),
                live: l,
            },
            SUB => Whose::Subagent {
                parent: MAIN.into(),
                parent_title: "甲会话".into(),
                live: l,
            },
            _ => Whose::Unknown,
        }
    }
}

type Rec = (String, String, String, Returned);

/// 投一次：`put` 的结局由判据给；交回回包 · 投了哪几条 · 记了哪几条。
fn run_with(
    doc: &PlanRead,
    args: Value,
    put: &dyn Fn(&SessionQueuePutReq) -> Result<QueuePut, Said>,
) -> (Out, Vec<SessionQueuePutReq>, Vec<Rec>) {
    let sent = RefCell::new(Vec::new());
    let kept = RefCell::new(Vec::new());
    let on_put = |r: &SessionQueuePutReq| {
        sent.borrow_mut().push(r.clone());
        put(r)
    };
    let record = |ws: &str, sl: &str, id: &str, r: Returned| {
        kept.borrow_mut().push((ws.into(), sl.into(), id.into(), r));
        Ok(())
    };
    let req: PlanReturnReq = serde_json::from_value(args).expect("入参解得出");
    let out = return_with(&req, doc, &on_put, &record, 42).map(|r| as_json(&r));
    (out, sent.into_inner(), kept.into_inner())
}

fn queued(_: &SessionQueuePutReq) -> Result<QueuePut, Said> {
    Ok(QueuePut {
        outcome: QueuePutOutcome::Queued,
        seq: Some(1),
        place: Some(1),
        said: String::new(),
    })
}

fn run(doc: &PlanRead, args: Value) -> (Out, Vec<SessionQueuePutReq>, Vec<Rec>) {
    run_with(doc, args, &queued)
}

fn args(id: &str) -> Value {
    json!({"workspace": "/w", "slice": "alpha", "id": id, "text": "单位写错了", "client": "monitor"})
}

#[test]
fn a_working_owner_gets_the_line_queued_and_the_return_is_kept() {
    let doc = doc_with(&live(crate::agents::SessionActivity::Working));
    let (out, sent, kept) = run(&doc, args("A2-1"));
    let out = out.unwrap();
    assert_eq!(out["result"], "queued");
    assert_eq!(out["line"], "人 · A2-1 乙的一半：单位写错了");
    assert_eq!(out["to"]["kind"], "subagent");
    // 子 agent 接手 ⇒ 投给它的父会话；发自自报的那一端；id 是这一刻 ＋ 片 ＋ 格。
    assert_eq!(sent.len(), 1);
    let p = &sent[0];
    assert_eq!(
        (
            p.sid.as_str(),
            p.text.as_str(),
            p.end.as_str(),
            p.id.as_str()
        ),
        (
            MAIN,
            "人 · A2-1 乙的一半：单位写错了",
            "monitor",
            "plan-42-alpha-A2-1"
        )
    );
    assert!(p.machine.is_local() && p.n.is_none() && p.attachments.is_empty());
    assert_eq!(kept.len(), 1);
    let (ws, sl, id, r) = &kept[0];
    assert_eq!(
        (ws.as_str(), sl.as_str(), id.as_str()),
        ("/w", "alpha", "A2-1")
    );
    assert_eq!(
        (r.at, r.to.as_str(), r.result.as_str()),
        (42, MAIN, "queued")
    );
    assert!(r.children.is_empty());
    let cell = &doc.slices[0].cells[7];
    assert_eq!(cell.id, "A2-1");
    assert_eq!(
        r.body,
        crate::plan::needs::body_digest(cell.body.as_deref())
    );
}

/// 在等人（批准 · 回答）也照投：投递等它答完再送，这里不拒。投不进去（队满 · 读不出）⇒ 那个码照回、不记退回。
#[test]
fn an_owner_waiting_for_you_is_queued_and_a_failed_put_keeps_nothing() {
    let doc = doc_with(&live(crate::agents::SessionActivity::NeedsYou));
    let (out, sent, kept) = run(&doc, args("A2"));
    assert_eq!(out.unwrap()["result"], "queued");
    assert_eq!((sent.len(), kept.len()), (1, 1));
    assert_eq!(
        kept[0].3.children,
        vec!["A2-1".to_string(), "A2-2".to_string()]
    );
    let full = |_: &SessionQueuePutReq| Err(crate::wire::spec::fail("queue_full", "满了".to_string()));
    let (out, _, kept) = run_with(&doc, args("A2"), &full);
    assert_eq!(out.unwrap_err().code, "queue_full");
    assert!(kept.is_empty());
}

#[test]
fn an_ended_or_unknown_owner_only_gets_a_line_to_copy() {
    let doc = doc_with(&ended);
    // A1 归主会话（已结束）；A2 归子 agent（这张表里认不出）。
    let (out, sent, kept) = run(&doc, args("A1"));
    let out = out.unwrap();
    assert_eq!(out["result"], "copy");
    assert_eq!(out["why"], "ended");
    assert_eq!(out["line"], "人 · A1 甲功能：单位写错了");
    let (out2, sent2, _) = run(&doc, args("A2"));
    let out2 = out2.unwrap();
    assert_eq!(out2["result"], "copy");
    assert_eq!(out2["why"], "unknown");
    assert!(sent.is_empty() && sent2.is_empty() && kept.is_empty());
}

#[test]
fn the_signer_is_a_target_only_when_pb_gives_one() {
    let doc = doc_with(&live(crate::agents::SessionActivity::Working));
    // beta 的 B1：没人接着，pb 给的 signer 是 null ⇒ 没有这一项。
    let mut a = args("B1");
    a["slice"] = json!("beta");
    a["to"] = json!("signer");
    let (out, _, _) = run(&doc, a.clone());
    assert_eq!(out.unwrap_err().code, "no_target");
    // A2-1 的元数据写在 A2 那个权力范围 ⇒ signer 是它的负责人（子 agent）⇒ 投给它的父会话。
    let mut b = args("A2-1");
    b["to"] = json!("signer");
    let (out, sent, _) = run(&doc, b);
    assert_eq!(out.unwrap()["to"]["id"], SUB);
    assert_eq!(sent[0].sid.as_str(), MAIN);
    a["to"] = json!("boss");
    assert_eq!(run(&doc, a).0.unwrap_err().code, "bad_args");
}

#[test]
fn bad_shapes_and_missing_cells_are_errors() {
    let doc = doc_with(&live(crate::agents::SessionActivity::Working));
    let mut a = args("A2-1");
    a["text"] = json!("   ");
    assert_eq!(run(&doc, a).0.unwrap_err().code, "bad_args");
    assert_eq!(run(&doc, args("Z9")).0.unwrap_err().code, "no_such_cell");
    let mut a = args("A2-1");
    a["slice"] = json!("beta");
    assert_eq!(run(&doc, a).0.unwrap_err().code, "no_such_cell");
    let mut a = args("A2-1");
    a["seen_screen"] = json!("abc");
    assert!(
        serde_json::from_value::<PlanReturnReq>(a).is_err(),
        "看见的那一屏不再是入参"
    );
}

/// project 在 pb 里可寻址（`pb see project`）⇒ 退回它 ＝ 投给 project 的负责人；那一行与普通格同一处拼，标题是仓库名；
/// 记下的子格是顶层那几格（落地看顶层多出新的一格）。project 谁都签不了 ⇒ 没有「签它的」那一项。
#[test]
fn the_top_block_is_addressed_as_project_and_goes_to_its_owner() {
    let doc = doc_with(&live(crate::agents::SessionActivity::Working));
    let (out, sent, kept) = run(&doc, args("project"));
    let out = out.unwrap();
    assert_eq!(out["result"], "queued");
    assert_eq!(out["line"], "人 · project alpha：单位写错了");
    assert_eq!(out["to"]["kind"], "session");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].sid.as_str(), MAIN);
    assert_eq!(sent[0].text, "人 · project alpha：单位写错了");
    let (_, sl, id, r) = &kept[0];
    assert_eq!((sl.as_str(), id.as_str()), ("alpha", "project"));
    assert_eq!(
        r.children,
        ["A1", "A2", "A3", "A4", "A5", "A6", "A7"].map(String::from)
    );
    let mut a = args("project");
    a["to"] = json!("signer");
    assert_eq!(run(&doc, a).0.unwrap_err().code, "no_target");
}

fn ended(id: &str) -> Whose {
    match id {
        MAIN => Whose::Session {
            sid: MAIN.into(),
            live: None,
        },
        _ => Whose::Unknown,
    }
}

/// 接手的会话已结束、人点了恢复（`resume: true`）⇒ 照投（投递那一下按记下的号恢复它、这一行当第一句）；记一条退回。
/// 认不出的接手点了恢复也只给复制。
#[test]
fn an_ended_owner_is_queued_when_asked_to_resume() {
    let doc = doc_with(&ended);
    let mut a = args("A1");
    a["resume"] = json!(true);
    let (out, sent, kept) = run(&doc, a);
    let out = out.unwrap();
    assert_eq!(out["result"], "queued");
    assert_eq!(out["to"]["sid"], MAIN);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].text, "人 · A1 甲功能：单位写错了");
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].3.to, MAIN);
    let mut a = args("A2");
    a["resume"] = json!(true);
    let (out, sent, _) = run(&doc, a);
    assert_eq!(out.unwrap()["result"], "copy");
    assert!(sent.is_empty());
}

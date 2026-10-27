//! 计划退回：先判能不能送（在等你 ⇒ 拒；已结束 / 认不出 ⇒ 只给复制；已结束而人点了恢复 ⇒ 恢复它再送）·
//! 能送走说一句那条路、送到了 / 送达未知记一条。

use super::*;
use crate::plan::fixture::{as_json, dump, MAIN, SUB};
use crate::plan::{Live, Whose};
use std::cell::RefCell;

/// 回包的线上那一形（判据按键看）。
type Out = Result<Value, Fail>;

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

type Sent = RefCell<Vec<(Value, bool)>>;
type Rec = (String, String, String, Returned);
type Kept = RefCell<Vec<Rec>>;

fn run(doc: &PlanRead, args: Value, reply: Value) -> (Out, Vec<Value>, Vec<Rec>) {
    let (out, sent, kept) = run_resuming(doc, args, reply);
    assert!(sent.iter().all(|(_, r)| !r), "没点恢复就不许恢复");
    (out, sent.into_iter().map(|(v, _)| v).collect(), kept)
}

fn run_resuming(doc: &PlanRead, args: Value, reply: Value) -> (Out, Vec<(Value, bool)>, Vec<Rec>) {
    run_waiting(doc, args, reply, &|_| false)
}

/// 活会话表那一判由判据给（`waiting(sid)`）。
fn run_waiting(
    doc: &PlanRead,
    args: Value,
    reply: Value,
    waiting: &dyn Fn(&str) -> bool,
) -> (Out, Vec<(Value, bool)>, Vec<Rec>) {
    let sent: Sent = RefCell::new(Vec::new());
    let kept: Kept = RefCell::new(Vec::new());
    let send = |a: &Value, resume: bool| {
        sent.borrow_mut().push((a.clone(), resume));
        Ok(reply.clone())
    };
    let record = |ws: &str, sl: &str, id: &str, r: Returned| {
        kept.borrow_mut().push((ws.into(), sl.into(), id.into(), r));
        Ok(())
    };
    let req: PlanReturnReq = serde_json::from_value(args).expect("入参解得出");
    let out = return_with(&req, doc, &send, &record, waiting, 42).map(|r| as_json(&r));
    (out, sent.into_inner(), kept.into_inner())
}

fn args(id: &str) -> Value {
    json!({"workspace": "/w", "slice": "alpha", "id": id, "text": "单位写错了", "client": "monitor", "seen_screen": "abc"})
}

#[test]
fn a_working_owner_gets_the_line_said_to_it_and_the_return_is_kept() {
    let doc = doc_with(&live(crate::agents::SessionActivity::Working));
    let (out, sent, kept) = run(&doc, args("A2-1"), json!({"result": "delivered"}));
    let out = out.unwrap();
    assert_eq!(out["result"], "delivered");
    assert_eq!(out["line"], "人 · A2-1 乙的一半：单位写错了");
    assert_eq!(out["to"]["kind"], "subagent");
    assert_eq!(out["resumed"], false);
    // 子 agent 接手 ⇒ 说给它的父会话。
    assert_eq!(
        sent,
        vec![
            json!({"sid": MAIN, "text": "人 · A2-1 乙的一半：单位写错了", "client": "monitor", "seen_screen": "abc"})
        ]
    );
    assert_eq!(kept.len(), 1);
    let (ws, sl, id, r) = &kept[0];
    assert_eq!(
        (ws.as_str(), sl.as_str(), id.as_str()),
        ("/w", "alpha", "A2-1")
    );
    assert_eq!(r.at, 42);
    assert_eq!(r.to, MAIN);
    assert_eq!(r.result, "delivered");
    assert!(r.children.is_empty());
    let cell = &doc.slices[0].cells[7];
    assert_eq!(cell.id, "A2-1");
    assert_eq!(
        r.body,
        crate::plan::needs::body_digest(cell.body.as_deref())
    );
}

#[test]
fn not_knowing_whether_it_arrived_is_kept_too_but_a_refusal_is_not() {
    let doc = doc_with(&live(crate::agents::SessionActivity::Idle));
    let (_, _, kept) = run(&doc, args("A2"), json!({"result": "unsure"}));
    assert_eq!(kept[0].3.result, "unsure");
    assert_eq!(
        kept[0].3.children,
        vec!["A2-1".to_string(), "A2-2".to_string()]
    );
    let (out, _, kept) = run(
        &doc,
        args("A2"),
        json!({"result": "refused", "why": "screen_changed", "said": "x", "screen": "def"}),
    );
    let out = out.unwrap();
    assert_eq!(out["result"], "refused");
    assert_eq!(out["why"], "screen_changed");
    assert_eq!(out["screen"], "def");
    assert!(kept.is_empty());
}

#[test]
fn an_owner_waiting_for_you_is_refused_without_typing_anything() {
    // 在不在等人只问活会话表（脸上说在跑也一样）。
    let doc = doc_with(&live(crate::agents::SessionActivity::Working));
    let (out, sent, kept) =
        run_waiting(&doc, args("A2-1"), json!({"result": "delivered"}), &|sid| {
            sid == MAIN
        });
    let out = out.unwrap();
    assert_eq!(out["result"], "refused");
    assert_eq!(out["why"], "waiting");
    assert!(out["said"].as_str().is_some_and(|s| !s.is_empty()));
    assert_eq!(out["line"], "人 · A2-1 乙的一半：单位写错了");
    assert!(sent.is_empty() && kept.is_empty());
}

#[test]
fn an_ended_or_unknown_owner_only_gets_a_line_to_copy() {
    let doc = doc_with(&ended);
    // A1 归主会话（已结束）；A2 归子 agent（这张表里认不出）。
    let (out, sent, kept) = run(&doc, args("A1"), json!({"result": "delivered"}));
    let out = out.unwrap();
    assert_eq!(out["result"], "copy");
    assert_eq!(out["why"], "ended");
    assert_eq!(out["line"], "人 · A1 甲功能：单位写错了");
    let (out2, sent2, _) = run(&doc, args("A2"), json!({"result": "delivered"}));
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
    let (out, _, _) = run(&doc, a.clone(), json!({"result": "delivered"}));
    assert_eq!(out.unwrap_err().code, "no_target");
    // A2-1 的元数据写在 A2 那个权力范围 ⇒ signer 是它的负责人（子 agent）⇒ 说给它的父会话。
    let mut b = args("A2-1");
    b["to"] = json!("signer");
    let (out, sent, _) = run(&doc, b, json!({"result": "delivered"}));
    assert_eq!(out.unwrap()["to"]["id"], SUB);
    assert_eq!(sent[0]["sid"], MAIN);
    a["to"] = json!("boss");
    assert_eq!(run(&doc, a, json!({})).0.unwrap_err().code, "bad_args");
}

#[test]
fn bad_shapes_and_missing_cells_are_errors() {
    let doc = doc_with(&live(crate::agents::SessionActivity::Working));
    let mut a = args("A2-1");
    a["text"] = json!("   ");
    assert_eq!(run(&doc, a, json!({})).0.unwrap_err().code, "bad_args");
    assert_eq!(
        run(&doc, args("Z9"), json!({})).0.unwrap_err().code,
        "no_such_cell"
    );
    let mut a = args("A2-1");
    a["slice"] = json!("beta");
    assert_eq!(run(&doc, a, json!({})).0.unwrap_err().code, "no_such_cell");
}

/// project 在 pb 里可寻址（`pb see project`）⇒ 退回它 ＝ 送给 project 的负责人；那一行与普通格同一处拼，标题是仓库名；
/// 记下的子格是顶层那几格（落地看顶层多出新的一格）。project 谁都签不了 ⇒ 没有「签它的」那一项。
#[test]
fn the_top_block_is_addressed_as_project_and_goes_to_its_owner() {
    let doc = doc_with(&live(crate::agents::SessionActivity::Working));
    let (out, sent, kept) = run(&doc, args("project"), json!({"result": "delivered"}));
    let out = out.unwrap();
    assert_eq!(out["result"], "delivered");
    assert_eq!(out["line"], "人 · project alpha：单位写错了");
    assert_eq!(out["to"]["kind"], "session");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["sid"], MAIN);
    assert_eq!(sent[0]["text"], "人 · project alpha：单位写错了");
    let (_, sl, id, r) = &kept[0];
    assert_eq!((sl.as_str(), id.as_str()), ("alpha", "project"));
    assert_eq!(
        r.children,
        ["A1", "A2", "A3", "A4", "A5", "A6", "A7"].map(String::from)
    );
    let mut a = args("project");
    a["to"] = json!("signer");
    assert_eq!(run(&doc, a, json!({})).0.unwrap_err().code, "no_target");
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

/// 接手的会话已结束、人点了恢复（`resume: true`）⇒ 恢复它、那一行当它的第一句（不带看见的那一屏：那一屏已经不在了）；
/// 回包说恢复了、接手照旧是它；送到了记一条退回。
#[test]
fn an_ended_owner_is_resumed_with_the_line_when_asked() {
    let doc = doc_with(&ended);
    let mut a = args("A1");
    a["resume"] = json!(true);
    let (out, sent, kept) = run_resuming(&doc, a, json!({"result": "delivered", "resumed": true}));
    let out = out.unwrap();
    assert_eq!(out["result"], "delivered");
    assert_eq!(out["resumed"], true);
    assert_eq!(out["to"]["sid"], MAIN);
    assert_eq!(
        sent,
        vec![(
            json!({"sid": MAIN, "text": "人 · A1 甲功能：单位写错了", "client": "monitor"}),
            true
        )]
    );
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].3.to, MAIN);
}

/// 恢复没成 ⇒ 原因照回（恢复不了 · 另有进程在写它），不记退回；认不出的接手点了恢复也只给复制。
#[test]
fn a_resume_that_fails_says_why_and_keeps_nothing() {
    let doc = doc_with(&ended);
    let mut a = args("A1");
    a["resume"] = json!(true);
    let (out, _, kept) = run_resuming(
        &doc,
        a,
        json!({"result": "refused", "why": "not_resumed", "said": "x", "resumed": false}),
    );
    let out = out.unwrap();
    assert_eq!(
        (out["result"].as_str(), out["why"].as_str()),
        (Some("refused"), Some("not_resumed"))
    );
    assert!(kept.is_empty());
    let mut a = args("A2");
    a["resume"] = json!(true);
    let (out, sent, _) = run_resuming(&doc, a, json!({"result": "delivered"}));
    assert_eq!(out.unwrap()["result"], "copy");
    assert!(sent.is_empty());
}

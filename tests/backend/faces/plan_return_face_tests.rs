//! 计划退回：先判能不能送（在等你 ⇒ 拒；已结束 / 认不出 ⇒ 只给复制）· 能送走送字那条路、送到了 / 送达未知记一条。

use super::*;
use crate::plan::fixture::{dump, MAIN, SUB};
use crate::plan::{Live, Whose};
use std::cell::RefCell;

fn doc_with(who: &dyn Fn(&str) -> Whose) -> Value {
    let b = crate::plan::book::Book::default();
    let d = dump("/w");
    let raw = d.to_string().into_bytes();
    b.take(
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
                live: l,
            },
            _ => Whose::Unknown,
        }
    }
}

type Sent = RefCell<Vec<Value>>;
type Rec = (String, String, String, Returned);
type Kept = RefCell<Vec<Rec>>;

fn run(doc: &Value, args: Value, reply: Value) -> (Answer, Vec<Value>, Vec<Rec>) {
    let sent: Sent = RefCell::new(Vec::new());
    let kept: Kept = RefCell::new(Vec::new());
    let send = |a: &Value| {
        sent.borrow_mut().push(a.clone());
        Ok(reply.clone())
    };
    let record = |ws: &str, sl: &str, id: &str, r: Returned| {
        kept.borrow_mut().push((ws.into(), sl.into(), id.into(), r));
        Ok(())
    };
    let out = return_with(&args, doc, &send, &record, 42);
    (out, sent.into_inner(), kept.into_inner())
}

fn args(id: &str) -> Value {
    json!({"workspace": "/w", "slice": "alpha", "id": id, "text": "单位写错了", "client": "monitor", "seen_screen": "abc"})
}

#[test]
fn a_working_owner_gets_the_line_through_terminal_input_and_the_return_is_kept() {
    let doc = doc_with(&live(crate::agents::SessionActivity::Working));
    let (out, sent, kept) = run(&doc, args("A1-2"), json!({"result": "delivered"}));
    let out = out.unwrap();
    assert_eq!(out["result"], "delivered");
    assert_eq!(out["line"], "人 · A1-2 甲的写出：单位写错了");
    assert_eq!(out["to"]["kind"], "subagent");
    // 子 agent 接手 ⇒ 送进它的父会话那个终端。
    assert_eq!(
        sent,
        vec![
            json!({"sid": MAIN, "text": "人 · A1-2 甲的写出：单位写错了", "enter": true, "client": "monitor", "seen_screen": "abc"})
        ]
    );
    assert_eq!(kept.len(), 1);
    let (ws, sl, id, r) = &kept[0];
    assert_eq!(
        (ws.as_str(), sl.as_str(), id.as_str()),
        ("/w", "alpha", "A1-2")
    );
    assert_eq!(r.at, 42);
    assert_eq!(r.to, MAIN);
    assert_eq!(r.result, "delivered");
    assert!(r.children.is_empty());
    let cell = &doc["slices"][0]["cells"][2];
    assert_eq!(r.body, crate::plan::needs::body_digest(cell));
}

#[test]
fn not_knowing_whether_it_arrived_is_kept_too_but_a_refusal_is_not() {
    let doc = doc_with(&live(crate::agents::SessionActivity::Idle));
    let (_, _, kept) = run(&doc, args("A1"), json!({"result": "unsure"}));
    assert_eq!(kept[0].3.result, "unsure");
    assert_eq!(
        kept[0].3.children,
        vec!["A1-1".to_string(), "A1-2".to_string()]
    );
    let (out, _, kept) = run(
        &doc,
        args("A1"),
        json!({"result": "refused", "why": "screen-changed", "said": "x", "screen": "def"}),
    );
    let out = out.unwrap();
    assert_eq!(out["result"], "refused");
    assert_eq!(out["why"], "screen-changed");
    assert_eq!(out["screen"], "def");
    assert!(kept.is_empty());
}

#[test]
fn an_owner_waiting_for_you_is_refused_without_typing_anything() {
    let doc = doc_with(&live(crate::agents::SessionActivity::NeedsYou));
    let (out, sent, kept) = run(&doc, args("A1-2"), json!({"result": "delivered"}));
    let out = out.unwrap();
    assert_eq!(out["result"], "refused");
    assert_eq!(out["why"], "waiting");
    assert!(out["said"].as_str().is_some_and(|s| !s.is_empty()));
    assert_eq!(out["line"], "人 · A1-2 甲的写出：单位写错了");
    assert!(sent.is_empty() && kept.is_empty());
}

#[test]
fn an_ended_or_unknown_owner_only_gets_a_line_to_copy() {
    let ended = |id: &str| match id {
        MAIN => Whose::Session {
            sid: MAIN.into(),
            live: None,
        },
        _ => Whose::Unknown,
    };
    let doc = doc_with(&ended);
    // A2 归主会话（已结束）；A1 归子 agent（这张表里认不出）。
    let (out, sent, kept) = run(&doc, args("A2"), json!({"result": "delivered"}));
    let out = out.unwrap();
    assert_eq!(out["result"], "copy");
    assert_eq!(out["why"], "ended");
    assert_eq!(out["line"], "人 · A2 乙功能：单位写错了");
    let (out2, sent2, _) = run(&doc, args("A1"), json!({"result": "delivered"}));
    let out2 = out2.unwrap();
    assert_eq!(out2["result"], "copy");
    assert_eq!(out2["why"], "unknown");
    assert!(sent.is_empty() && sent2.is_empty() && kept.is_empty());
}

#[test]
fn the_signer_is_a_target_only_when_pb_gives_one() {
    let doc = doc_with(&live(crate::agents::SessionActivity::Working));
    let mut a = args("A1-2");
    a["to"] = json!("signer");
    let (out, _, _) = run(&doc, a.clone(), json!({"result": "delivered"}));
    assert_eq!(out.unwrap_err().code, "no_target");
    a["to"] = json!("boss");
    assert_eq!(run(&doc, a, json!({})).0.unwrap_err().code, "bad_args");
}

#[test]
fn bad_shapes_and_missing_cells_are_errors() {
    let doc = doc_with(&live(crate::agents::SessionActivity::Working));
    let mut a = args("A1-2");
    a["text"] = json!("   ");
    assert_eq!(run(&doc, a, json!({})).0.unwrap_err().code, "bad_args");
    assert_eq!(
        run(&doc, args("Z9"), json!({})).0.unwrap_err().code,
        "no_such_cell"
    );
    let mut a = args("A1-2");
    a["slice"] = json!("beta");
    assert_eq!(run(&doc, a, json!({})).0.unwrap_err().code, "no_such_cell");
}

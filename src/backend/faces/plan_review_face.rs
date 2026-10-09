//! 计划审面认可那一半（`plan-ack` · `plan-unack`）的帧面宿主；退回那一半（`plan-return`，要起 tmux）住 [`super::plan_return_face`]。
//! 判定在 [`crate::plan::needs`]，认可的记录住后端自己那份文件（[`crate::plan::review`] 的认可写口只从本文件进）；计划仓一个字节都不写。
//!
//! - `plan-ack {workspace, slice, key}`：认可一条要你看（键此刻在、不是问人那一种才收）；推一帧 `plan_changed` 带新的数。
//! - `plan-unack {workspace, slice, key}`：撤掉（8 秒撤销那一下）；同上推一帧。

use crate::plan::needs;
use crate::stream::inbound::spec::Fail;
use copy_core::copy_text;
use serde_json::{json, Value};

type Answer = Result<Value, Fail>;

fn bad(what: &str) -> Fail {
    Fail::new("bad_args", crate::common::contract::malformed(what))
}

fn str_arg<'a>(args: &'a Value, k: &str) -> Result<&'a str, Fail> {
    args.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| bad(&format!("missing `{k}` (a string)")))
}

fn slice_of<'a>(doc: &'a Value, name: &str) -> Option<&'a Value> {
    doc.get("slices")?
        .as_array()?
        .iter()
        .find(|s| s.get("name").and_then(Value::as_str) == Some(name))
}

fn io(e: (&'static str, String)) -> Fail {
    Fail::new(e.0, e.1)
}

/// 这个键此刻能不能认可：不在 ⇒ `no_such_need`；问人那一种 ⇒ `not_ackable`。
pub(crate) fn check_ack(sl: &Value, key: &str) -> Result<(), Fail> {
    match needs::ackable(sl, key) {
        None => Err(Fail::new(
            "no_such_need",
            copy_text("bePlan.review.noSuchNeed", &[]),
        )),
        Some(false) => Err(Fail::new(
            "not_ackable",
            copy_text("bePlan.review.notAckable", &[]),
        )),
        Some(true) => Ok(()),
    }
}

/// 这个工作区上一次读好的那一份（没读过 ⇒ `not_read`）。
fn last(ws: &str) -> Result<Value, Fail> {
    crate::plan::book::book()
        .last(ws)
        .ok_or_else(|| Fail::new("not_read", copy_text("bePlan.review.notRead", &[])))
}

/// 认可改了之后：这个工作区此刻要你看的数，推一帧 `plan_changed`（别的窗口跟着改数）。
fn after_ack(ws: &str, key: &str, acked: bool) -> Answer {
    let doc = crate::faces::plan_face::annotated(last(ws)?);
    let n = doc.get("needCount").and_then(Value::as_u64).unwrap_or(0);
    if let Some((rev, n)) = crate::faces::plan_face::seen_of(&doc) {
        // 此刻没有流连接订它 ⇒ 发不出是正常的，客户端重问 `plan-read` 就有。
        let _ = crate::plan::watch::changes().send((ws.to_string(), rev, n));
    }
    Ok(json!({"key": key, "acked": acked, "needCount": n}))
}

fn ack(args: &Value) -> Answer {
    let (ws, slice, key) = (
        str_arg(args, "workspace")?,
        str_arg(args, "slice")?,
        str_arg(args, "key")?,
    );
    let doc = last(ws)?;
    let sl = slice_of(&doc, slice)
        .ok_or_else(|| Fail::new("no_such_need", copy_text("bePlan.review.noSuchNeed", &[])))?;
    check_ack(sl, key)?;
    crate::plan::review::answer_ack(ws, slice, key, &needs::keys_of(sl)).map_err(io)?;
    after_ack(ws, key, true)
}

fn unack(args: &Value) -> Answer {
    let (ws, slice, key) = (
        str_arg(args, "workspace")?,
        str_arg(args, "slice")?,
        str_arg(args, "key")?,
    );
    crate::plan::review::answer_unack(ws, slice, key).map_err(io)?;
    after_ack(ws, key, false)
}

/// 帧面入口：命令名从 `r.cmd` 来。
pub(crate) fn answer(cmd: &str, args: &Value) -> Answer {
    match cmd {
        "plan-ack" => {
            ack(args).and_then(crate::faces::plan_face::wired::<crate::plan::wire::PlanAckReply>)
        }
        "plan-unack" => {
            unack(args).and_then(crate::faces::plan_face::wired::<crate::plan::wire::PlanAckReply>)
        }
        other => Err(bad(&format!("unknown command `{other}`"))),
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/plan_review_face_tests.rs"]
mod tests;

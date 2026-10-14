//! 计划审面认可那一半（`plan-ack` · `plan-unack`）的帧面宿主；退回那一半（`plan-return`，要起 tmux）住 [`super::plan_return_face`]。
//! 判定在 [`crate::plan::needs`]，认可的记录住后端自己那份文件（[`crate::plan::review`] 的认可写口只从本文件进）；计划仓一个字节都不写。
//!
//! - `plan-ack {workspace, slice, key}`：认可一条要你看（键此刻在、不是问人那一种才收）；推一帧 `changed {plan}` 带新的数。
//! - `plan-unack {workspace, slice, key}`：撤掉（8 秒撤销那一下）；同上推一帧。

use crate::plan::needs;
use crate::plan::wire;
use crate::wire::spec::Fail;
use crate::wire::typed::{self, Command, Cx, Handle};
use copy_core::copy_text;
use serde_json::{json, Value};

type Answer = Result<Value, Fail>;

fn slice_of<'a>(doc: &'a Value, name: &str) -> Option<&'a Value> {
    doc.get("slices")?
        .as_array()?
        .iter()
        .find(|s| s.get("name").and_then(Value::as_str) == Some(name))
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

/// 认可改了之后：这个工作区此刻要你看的数，推一帧 `changed {plan}`（别的窗口跟着改数）。
fn after_ack(ws: &str, key: &str, acked: bool) -> Answer {
    let doc = crate::faces::plan_face::marked(last(ws)?);
    let n = doc.get("needCount").and_then(Value::as_u64).unwrap_or(0);
    if let Some((rev, n)) = crate::faces::plan_face::seen_of(&doc) {
        // 此刻没有流连接订它 ⇒ 发不出是正常的，客户端重问 `plan-read` 就有。
        let _ = crate::plan::watch::changes().send((ws.to_string(), rev, n));
    }
    Ok(json!({"key": key, "acked": acked, "needCount": n}))
}

fn ack(req: &PlanAckReq, own: &crate::platform::fs::Own) -> Answer {
    let (ws, slice, key) = (req.workspace.as_str(), req.slice.as_str(), req.key.as_str());
    let doc = last(ws)?;
    let sl = slice_of(&doc, slice)
        .ok_or_else(|| Fail::new("no_such_need", copy_text("bePlan.review.noSuchNeed", &[])))?;
    check_ack(sl, key)?;
    crate::plan::review::answer_ack(own, ws, slice, key, &needs::keys_of(sl))?;
    after_ack(ws, key, true)
}

fn unack(req: &PlanAckReq, own: &crate::platform::fs::Own) -> Answer {
    crate::plan::review::answer_unack(own, &req.workspace, &req.slice, &req.key)?;
    after_ack(&req.workspace, &req.key, false)
}

/// `plan-ack` · `plan-unack` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanAckReq {
    /// 工作区根。
    pub(crate) workspace: String,
    /// 片名。
    pub(crate) slice: String,
    /// 那一条的键（`plan-read` 里 `needs[].key`）。
    pub(crate) key: String,
}

typed::codes! {
    /// `plan-ack` 的码。
    pub(crate) enum PlanAckErr {
        IoFailed = "io_failed",
        NoSuchNeed = "no_such_need",
        NotAckable = "not_ackable",
        NotRead = "not_read",
        ReviewUnreadable = "review_unreadable",
    }
}

typed::codes! {
    /// `plan-unack` 的码（没有也不算错）。
    pub(crate) enum PlanUnackErr {
        IoFailed = "io_failed",
        NotRead = "not_read",
        ReviewUnreadable = "review_unreadable",
    }
}

/// `plan-ack`：认可一条要你看（读—改—写后端自己那份小文件 ⇒ 阻塞档、写自家状态）。
pub(crate) struct PlanAckCmd;

impl Command for PlanAckCmd {
    const NAME: &'static str = "plan-ack";
    type Req = PlanAckReq;
    type Reply = wire::PlanAckReply;
    type Err = PlanAckErr;
}

impl Handle for PlanAckCmd {
    type Needs = typed::needs::Own;
    fn run(
        req: PlanAckReq,
        cx: Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::PlanAckReply, PlanAckErr>> + Send + 'static
    {
        async move {
            ack(&req, &cx.needs)
                .and_then(crate::faces::plan_face::wired)
                .map_err(|f| typed::of_fail(f, PlanAckErr::IoFailed))
        }
    }
}

/// `plan-unack`：撤掉一条认可（同上）。
pub(crate) struct PlanUnackCmd;

impl Command for PlanUnackCmd {
    const NAME: &'static str = "plan-unack";
    type Req = PlanAckReq;
    type Reply = wire::PlanAckReply;
    type Err = PlanUnackErr;
}

impl Handle for PlanUnackCmd {
    type Needs = typed::needs::Own;
    fn run(
        req: PlanAckReq,
        cx: Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<wire::PlanAckReply, PlanUnackErr>> + Send + 'static
    {
        async move {
            unack(&req, &cx.needs)
                .and_then(crate::faces::plan_face::wired)
                .map_err(|f| typed::of_fail(f, PlanUnackErr::IoFailed))
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/plan_review_face_tests.rs"]
mod tests;

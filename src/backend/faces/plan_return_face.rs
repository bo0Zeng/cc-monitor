//! 计划审面退回那一半（`plan-return`）的帧面宿主：拼「人 · {编号} {标题}：{原话}」（对象是一格，或顶块 `project`：标题是片名），先判目标 ——
//! 认不出 ⇒ 只给这一行去复制；已结束 ⇒ 只给复制，人点了恢复（`resume`）⇒ 照投；能投就投进那条会话的输入队（`session-queue-put`
//! 那一半，同一个写口：在等人 · 一轮在跑都由投递等，已结束的由投递恢复它、这一行当第一句），
//! 排进去了 ⇒ 记一条「已退回」（[`crate::plan::review`] 的退回写口只从本文件进），
//! 之后 `plan-read` 里那一格带落没落地（[`crate::plan::needs::landing`]）。

use crate::common::said::Said;
use crate::plan::needs::{self, Returned};
use crate::wire::typed::{self, Command, Cx, Handle};
use cmd_wire::plan::{PlanRead, PlanReturnReply, PlanWho};
use cmd_wire::queue::{QueuePut, SessionQueuePutReq};
use copy_core::copy_text;

type Answer = Result<PlanReturnReply, Said>;

fn bad(what: &str) -> Said {
    crate::wire::spec::fail("bad_args", crate::common::contract::malformed(what))
}

fn now_ms() -> u64 {
    crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 投进那条会话的输入队（生产 ＝ `session-queue-put` 在这台的那一半）。
pub(crate) type Put<'a> = &'a dyn Fn(&SessionQueuePutReq) -> Result<QueuePut, Said>;
/// 记一次退回（生产 ＝ [`crate::plan::review`] 的写口）。
pub(crate) type Record<'a> = &'a dyn Fn(&str, &str, &str, Returned) -> Result<(), Said>;

/// 发自哪一端：没自报 ⇒ 这一个。
const END: &str = "plan";

fn reply(
    line: &str,
    to: &PlanWho,
    result: &str,
    why: Option<&str>,
    said: Option<String>,
) -> PlanReturnReply {
    PlanReturnReply {
        line: line.to_string(),
        to: Some(to.clone()),
        result: result.to_string(),
        why: why.map(str::to_string),
        said,
    }
}

/// `plan-return` 的本体：`doc` 是此刻现读、标过的那一份；`now` 是这一刻（毫秒，也拿来铸这一句的 id）。
pub(crate) fn return_with(
    req: &PlanReturnReq,
    doc: &PlanRead,
    put: Put,
    record: Record,
    now: u64,
) -> Answer {
    let (ws, slice, id) = (req.workspace.as_str(), req.slice.as_str(), req.id.as_str());
    let text = req.text.as_str();
    if text.trim().is_empty() {
        return Err(bad("`text` is empty"));
    }
    let role = match req.to.as_deref() {
        None => "owner",
        Some(s) if s == "owner" || s == "signer" => s,
        Some(_) => return Err(bad("`to` must be `owner` or `signer`")),
    };
    let no_cell =
        || crate::wire::spec::fail("no_such_cell", copy_text("bePlan.review.noSuchCell", &[]));
    let sl = doc
        .slices
        .iter()
        .find(|s| s.name == slice)
        .ok_or_else(no_cell)?;
    // 一格，或顶块 `project`（[`needs::target`]）。
    let cell = needs::target(sl, id).ok_or_else(no_cell)?;
    let line = needs::line(id, cell.title.as_deref().unwrap_or_default(), text);
    let Some(to) = cell.who(role) else {
        return Err(crate::wire::spec::fail(
            "no_target",
            copy_text("bePlan.review.noTarget", &[("role", role)]),
        ));
    };
    let Some(sid) = to.sid.as_deref() else {
        return Ok(reply(
            &line,
            to,
            "copy",
            Some("unknown"),
            Some(copy_text("bePlan.return.unknown", &[])),
        ));
    };
    if needs::live_of(to).is_none() && req.resume != Some(true) {
        return Ok(reply(
            &line,
            to,
            "copy",
            Some("ended"),
            Some(copy_text("sessionState.planReturn.ended", &[])),
        ));
    }
    put(&SessionQueuePutReq {
        machine: cmd_wire::Machine::local(),
        sid: sid.parse().map_err(|_| bad("`sid` is not a session id"))?,
        id: format!("{END}-{now}-{slice}-{id}"),
        end: req.client.clone().unwrap_or_else(|| END.to_string()),
        n: None,
        at_ms: i64::try_from(now).unwrap_or(i64::MAX),
        text: line.clone(),
        attachments: Vec::new(),
        rewind: None,
    })?;
    record(
        ws,
        slice,
        id,
        Returned {
            at: now,
            to: sid.to_string(),
            result: "queued".to_string(),
            children: cell.children.clone(),
            body: needs::body_digest(cell.body.as_deref()),
        },
    )?;
    Ok(reply(&line, to, "queued", None, None))
}

fn give_back(req: &PlanReturnReq, own: &crate::platform::fs::Own) -> Answer {
    let ws = req.workspace.as_str();
    // 只拿此刻的接手与状态（不交出去），时刻的字用不上 ⇒ 按 UTC 写的那一份也行。
    let doc = crate::faces::plan_face::read_fresh(ws, &crate::Tz::default())?;
    let put = |r: &SessionQueuePutReq| {
        use super::session_queue_face as face;
        let got = face::put_here(own, r, crate::common::time::now_ms(), &face::cwd_here);
        crate::history::session_queue::moved();
        got
    };
    let record = |ws: &str, sl: &str, id: &str, r: Returned| {
        crate::plan::review::answer_returned(own, ws, sl, id, r)
    };
    return_with(req, &doc, &put, &record, now_ms())
}

/// `plan-return` 的入参。
#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanReturnReq {
    /// 工作区根。
    pub(crate) workspace: String,
    /// 片名。
    pub(crate) slice: String,
    /// 格的编号；顶块 ⇒ `project`（标题是片名，送给顶块接手）。
    pub(crate) id: String,
    /// 人的原话（换行并成空格）。
    pub(crate) text: String,
    /// `owner`（在长它的，缺省）· `signer`（签它的，pb 给了才有）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable, type = "\"owner\" | \"signer\""))]
    pub(crate) to: Option<String>,
    /// 自报的前端（投进队的那一条记作发自它；投递时过 `terminal-input` 那一道身份门）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) client: Option<String>,
    /// `true` ⇒ 目标会话已结束时照投（投递时在这台恢复它、这一行当它的第一句；人点了［恢复并退回］）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) resume: Option<bool>,
}

typed::codes! {
    /// `plan-return` 的码：现读计划（起 pb）· 判目标 · 投进那条会话的输入队 · 记一条退回。
    pub(crate) enum PlanReturnErr {
        AttachmentGone = "attachment_gone",
        ChildTimedOut = "child_timed_out",
        Failed = "failed",
        IoFailed = "io_failed",
        NoPb = "no_pb",
        NoSuchCell = "no_such_cell",
        NoTarget = "no_target",
        NotWorkspace = "not_workspace",
        PbUnsupported = "pb_unsupported",
        QueueFull = "queue_full",
        QueueUnreadable = "queue_unreadable",
        ReviewUnreadable = "review_unreadable",
        TextTooLong = "text_too_long",
    }
}

/// `plan-return`：把人的话投给负责那一格的会话（起 pb ⇒ 阻塞档；投进队 · 记退回写自家状态）。
pub(crate) struct PlanReturnCmd;

impl Command for PlanReturnCmd {
    const NAME: &'static str = "plan-return";
    type Req = PlanReturnReq;
    type Reply = cmd_wire::plan::PlanReturnReply;
    type Err = PlanReturnErr;
}

impl Handle for PlanReturnCmd {
    type Needs = typed::needs::Own;
    fn run(
        req: PlanReturnReq,
        cx: Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<cmd_wire::plan::PlanReturnReply, PlanReturnErr>>
           + std::marker::Send
           + 'static {
        async move {
            let (_, own) = cx.split();
            give_back(&req, &own).map_err(|f| typed::of_fail(f, PlanReturnErr::Failed))
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/plan_return_face_tests.rs"]
mod tests;

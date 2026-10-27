//! 计划审面退回那一半（`plan-return`）的帧面宿主：拼「人 · {编号} {标题}：{原话}」（对象是一格，或顶块 `project`：标题是片名），先判目标能不能送 ——
//! 在等你（批准 · 回答 · 计划）⇒ 拒；认不出 ⇒ 只给这一行去复制；已结束 ⇒ 只给复制，人点了恢复（`resume`）⇒ 在这台恢复它、这一行当第一句；
//! 能送就走 `session-say` 的本体（同一道身份门、同一个「画面变了不送」、agent 退了不往 shell 里打字，不另造路），
//! 送到了 / 送达未知 ⇒ 记一条「已退回」（[`crate::plan::review`] 的退回写口只从本文件进），
//! 之后 `plan-read` 里那一格带落没落地（[`crate::plan::needs::landing`]）。

use crate::plan::needs::{self, Returned};
use crate::plan::wire::{PlanRead, PlanReturnReply, PlanWho};
use crate::wire::spec::Fail;
use crate::wire::typed::{self, Command, Cx, Handle};
use copy_core::copy_text;
use serde_json::{json, Map, Value};

type Answer = Result<PlanReturnReply, Fail>;

fn bad(what: &str) -> Fail {
    Fail::new("bad_args", crate::common::contract::malformed(what))
}

fn now_ms() -> u64 {
    crate::common::time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 说一句（`session-say` 的入参那一形 ＋ 这台已经没有它的终端时恢不恢复它；生产 ＝ `session-say` 的本体）。
pub(crate) type Send<'a> = &'a dyn Fn(&Value, bool) -> Result<Value, Fail>;
/// 记一次退回（生产 ＝ [`crate::plan::review`] 的写口）。
pub(crate) type Record<'a> = &'a dyn Fn(&str, &str, &str, Returned) -> Result<(), Fail>;

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
        screen: None,
        resumed: false,
    }
}

/// `plan-return` 的本体：`doc` 是此刻现读、标过的那一份；`waiting(sid)` ＝ 活会话表说那条会话在等人（在等 ⇒ 拒，只问它）。
pub(crate) fn return_with(
    req: &PlanReturnReq,
    doc: &PlanRead,
    send: Send,
    record: Record,
    waiting: &dyn Fn(&str) -> bool,
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
    let no_cell = || Fail::new("no_such_cell", copy_text("bePlan.review.noSuchCell", &[]));
    let sl = doc
        .slices
        .iter()
        .find(|s| s.name == slice)
        .ok_or_else(no_cell)?;
    // 一格，或顶块 `project`（[`needs::target`]）。
    let cell = needs::target(sl, id).ok_or_else(no_cell)?;
    let line = needs::line(id, cell.title.as_deref().unwrap_or_default(), text);
    let Some(to) = cell.who(role) else {
        return Err(Fail::new(
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
    let resume = req.resume == Some(true);
    let alive = needs::live_of(to).is_some();
    if !alive && !resume {
        return Ok(reply(
            &line,
            to,
            "copy",
            Some("ended"),
            Some(copy_text("sessionState.planReturn.ended", &[])),
        ));
    }
    if waiting(sid) {
        return Ok(reply(
            &line,
            to,
            "refused",
            Some("waiting"),
            Some(copy_text("bePlan.return.waiting", &[])),
        ));
    }
    let mut input = Map::new();
    input.insert("sid".into(), json!(sid));
    input.insert("text".into(), json!(line));
    if let Some(v) = &req.client {
        input.insert("client".into(), json!(v));
    }
    // 恢复的那一个没有可看的那一屏：带着指纹就不恢复（`session-say` 的规矩）。
    if let Some(v) = req.seen_screen.as_ref().filter(|_| alive) {
        input.insert("seen_screen".into(), json!(v));
    }
    let got = send(&Value::Object(input), resume)?;
    let result = got
        .get("result")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if result == "delivered" || result == "unsure" {
        record(
            ws,
            slice,
            id,
            Returned {
                at: now,
                to: sid.to_string(),
                result: result.to_string(),
                children: cell.children.clone(),
                body: needs::body_digest(cell.body.as_deref()),
            },
        )?;
    }
    let text = |k: &str| got.get(k).and_then(Value::as_str).map(str::to_string);
    let mut out = reply(
        &line,
        to,
        result,
        got.get("why").and_then(Value::as_str),
        text("said"),
    );
    out.screen = text("screen");
    out.resumed = got.get("resumed").and_then(Value::as_bool) == Some(true);
    Ok(out)
}

fn give_back(
    req: &PlanReturnReq,
    files: crate::wire::typed::Files,
    own: &crate::platform::fs::Own,
) -> Answer {
    let ws = req.workspace.as_str();
    // 只拿此刻的接手与状态（不交出去），时刻的字用不上 ⇒ 按 UTC 写的那一份也行。
    let doc = crate::faces::plan_face::read_fresh(ws, &crate::Tz::default())?;
    // 说的那一句是本条现拼的（`return_with`）⇒ 按 `session-say` 的入参类型解一次，形状不对同那一条回 `bad_args`。
    let home = crate::history::listing::agent_home();
    let waiting = |sid: &str| crate::history::live_table::now(&home).waiting(sid);
    let send = |a: &Value, resume: bool| {
        use crate::faces::session_say_face as say;
        let req: say::SessionSayReq = typed::take(a)?;
        let again = |r: &say::SessionSayReq| {
            resume
                .then(|| say::resume_here(&r.sid, r.client.as_deref(), Some(&r.text), files))
                .flatten()
        };
        say::say_with(&req, &waiting, &crate::control::terminals_wire::say, &again)
            .and_then(|sent| crate::wire::spec::wire(&sent))
    };
    let record = |ws: &str, sl: &str, id: &str, r: Returned| {
        crate::plan::review::answer_returned(own, ws, sl, id, r)
    };
    return_with(req, &doc, &send, &record, &waiting, now_ms())
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
    /// 自报的前端（过 `terminal-input` 那一道身份门）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) client: Option<String>,
    /// 送之前看到的那一屏的指纹（同 `terminal-input`）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) seen_screen: Option<String>,
    /// `true` ⇒ 目标会话已结束时在这台恢复它、这一行当它的第一句（人点了［恢复并退回］）。
    #[serde(default)]
    #[cfg_attr(test, ts(optional = nullable))]
    pub(crate) resume: Option<bool>,
}

typed::codes! {
    /// `plan-return` 的码：现读计划（起 pb）· 判目标 · 说一句（`session-say` 的本体，起 tmux）· 记一条退回。
    pub(crate) enum PlanReturnErr {
        BadTarget = "bad_target",
        CaptureFailed = "capture_failed",
        ChildTimedOut = "child_timed_out",
        Failed = "failed",
        IoFailed = "io_failed",
        NoPb = "no_pb",
        NoServer = "no_server",
        NoSuchCell = "no_such_cell",
        NoSuchSession = "no_such_session",
        NoTarget = "no_target",
        NoTmux = "no_tmux",
        NotWorkspace = "not_workspace",
        PbUnsupported = "pb_unsupported",
        ReviewUnreadable = "review_unreadable",
        Unobservable = "unobservable",
    }
}

/// `plan-return`：把人的话送给负责那一格的会话（起 pb ＋ tmux ⇒ 阻塞档；记退回写自家状态）。
pub(crate) struct PlanReturnCmd;

impl Command for PlanReturnCmd {
    const NAME: &'static str = "plan-return";
    type Req = PlanReturnReq;
    type Reply = crate::plan::wire::PlanReturnReply;
    type Err = PlanReturnErr;
}

impl Handle for PlanReturnCmd {
    type Needs = typed::needs::FilesOwn;
    fn run(
        req: PlanReturnReq,
        cx: Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<crate::plan::wire::PlanReturnReply, PlanReturnErr>>
           + std::marker::Send
           + 'static {
        async move {
            let (_, (files, own)) = cx.split();
            give_back(&req, files, &own).map_err(|f| typed::of_fail(f, PlanReturnErr::Failed))
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/plan_return_face_tests.rs"]
mod tests;

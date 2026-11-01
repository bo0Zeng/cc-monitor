//! 帧面 `sessions-batch`（typed，只在本机后端答全）：一批会话跨几台的那一条 —— 收全部 id、按台分组、
//! 本机那一组同进程停（`sessions-stop` 的本体），别的台各拿自己那一组问它常驻的 `sessions-batch`（那一份只含它自己的会话），
//! 逐条结局照入参原序摆好，合那一句交 `batch_core::sessions::sessions_reply`（手机核心同一份）。
//!
//! 那台够不着 · 常驻不在 · 太旧 · 答了失败 ⇒ 那一组每一项都是 `failed`，那一句与复制详情照那一台的失败（`cmd_wire::Failed`）写。

use crate::common::said::Said;
use crate::control::session_batch as batch;
use crate::dial::remote_ask::{Remote, Table};
use crate::wire::typed::{codes, Command};
use batch_core::sessions::{sessions_reply, Verb};
use cmd_wire::batch::{
    SessionItem, SessionOutcome, SessionState, SessionsBatchReply, SessionsBatchReq, SessionsOp,
};
use cmd_wire::Machine;
use copy_core::copy_text;
use std::future::Future;

codes! {
    /// `sessions-batch` 自己没有别的码（哪台没成只进那一组的逐条结局）。
    pub(crate) enum SessionsBatchErr {}
}

/// `sessions-batch`：一批会话跨几台（本机同进程做，别的台经常驻问它自己那一组），合成一份再答。
pub(crate) struct SessionsBatchCmd;

impl Command for SessionsBatchCmd {
    const NAME: &'static str = "sessions-batch";
    type Req = SessionsBatchReq;
    type Reply = SessionsBatchReply;
    type Err = SessionsBatchErr;
}

impl crate::wire::typed::Handle for SessionsBatchCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: SessionsBatchReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl Future<Output = Result<SessionsBatchReply, SessionsBatchErr>> + Send + 'static {
        async move {
            let (tz, until) = (cx.tz.clone(), cx.until);
            across(
                req,
                |r| stop_here(r, tz.clone(), until),
                &cx.tz,
                until,
                &crate::dial::remote_ask::REACH,
                &crate::dial::remote_ask::DialRemote,
            )
            .await
            .map_err(SessionsBatchErr::BadArgs)
        }
    }
}

/// 停那一样的字：动作那个词 ＋ 只有一项时点名的两句。
fn stop_verb() -> Verb {
    Verb {
        action: copy_text("tabBatch.verb.stop", &[]),
        one_done: |t| copy_text("sessionState.kill.done", &[("title", t)]),
        one_failed: |t| copy_text("tabSessionActions.kill.failed", &[("title", t)]),
    }
}

/// 本机那一组：同进程跑 `sessions-stop` 的本体（阻塞档 ⇒ 放进阻塞池、带停那一档的期限）。
async fn stop_here(
    req: SessionsBatchReq,
    tz: crate::Tz,
    until: Option<crate::platform::child::Until>,
) -> Result<SessionsBatchReply, Said> {
    use crate::wire::typed::{Codes as _, Handle as _};
    let sids = batch::SidsReq {
        sids: req.items.into_iter().map(|i| i.sid).collect(),
        client: req.client,
    };
    let cx = crate::wire::typed::Cx {
        tz,
        until,
        needs: (),
    };
    let rt = tokio::runtime::Handle::current();
    let done = tokio::task::spawn_blocking(move || {
        let _budget = crate::platform::child::Budget::capped(batch::BATCH_CAP, until);
        rt.block_on(crate::faces::session_batch_face::SessionsStop::run(
            sids, cx,
        ))
    })
    .await
    .map_err(|e| Said::from(e.to_string()))?
    .map_err(|e| e.into_said())?;
    Ok(sessions_reply(&stop_verb(), done.results))
}

/// 合各台：校验 → 按台分组（照第一次出现的先后）→ 每台各问自己那一组 → 逐条照入参原序摆好、远端的称呼套机器名 → 合那一句。
/// `local` ＝ 本机那一组怎么做（生产 [`stop_here`]，判据给替身）；入参不对 ⇒ `Err`（`bad_args` 那一句）。
pub(crate) async fn across<'a, L, F>(
    req: SessionsBatchReq,
    local: L,
    tz: &'a crate::Tz,
    until: Option<crate::platform::child::Until>,
    table: &'a Table,
    remote: &'a dyn Remote,
) -> Result<SessionsBatchReply, String>
where
    L: FnOnce(SessionsBatchReq) -> F,
    F: Future<Output = Result<SessionsBatchReply, Said>> + Send + 'a,
{
    if !matches!(req.op, SessionsOp::Stop) {
        return Err(crate::common::contract::malformed(
            "`op` must be `stop` (starting a batch is not taken here)",
        ));
    }
    if req.items.is_empty() || req.items.len() > batch::MAX_BATCH {
        return Err(crate::common::contract::malformed(&format!(
            "`items` must hold 1..={} entries",
            batch::MAX_BATCH
        )));
    }
    for (i, it) in req.items.iter().enumerate() {
        if req.items[..i].contains(it) {
            return Err(crate::common::contract::malformed(&format!(
                "duplicate item: {} on {}",
                it.sid, it.machine
            )));
        }
    }
    let mut groups: Vec<(Machine, SessionsBatchReq)> = Vec::new();
    for it in &req.items {
        let one = SessionItem {
            machine: Machine::local(),
            sid: it.sid.clone(),
        };
        match groups.iter_mut().find(|(m, _)| *m == it.machine) {
            Some((_, g)) => g.items.push(one),
            None => groups.push((
                it.machine.clone(),
                SessionsBatchReq {
                    op: req.op.clone(),
                    items: vec![one],
                    client: req.client.clone(),
                },
            )),
        }
    }
    let got = crate::dial::remote_ask::ask_apiece::<SessionsBatchCmd, _, _, _>(
        groups, local, tz, until, table, remote,
    )
    .await?;
    let results = req
        .items
        .iter()
        .map(|it| {
            let answered = got.iter().find(|(m, _)| *m == it.machine).map(|(_, r)| r);
            outcome_of(it, answered)
        })
        .collect();
    Ok(sessions_reply(&stop_verb(), results))
}

/// 一项的结局：那台答回来的那一条（换成这一项的那台、远端的称呼套机器名）；那台没成 ⇒ 那台那一份失败。
fn outcome_of(
    it: &SessionItem,
    answered: Option<&Result<SessionsBatchReply, Said>>,
) -> SessionOutcome {
    let found = match answered {
        Some(Ok(r)) => match r.results.iter().find(|o| o.sid == it.sid) {
            Some(o) => Ok(o.clone()),
            None => Err(Said::new(
                "backend_differs",
                copy_core::backend_differs(&it.machine.to_string()),
            )),
        },
        Some(Err(s)) => Err(s.clone()),
        None => Err(Said::from(copy_core::reply_unreadable(
            &it.machine.to_string(),
        ))),
    };
    let mut o = found.unwrap_or_else(|s| {
        let f =
            crate::wire::detail::failed_of(SessionsBatchCmd::NAME, s.code_or_fallback(), s.clone());
        SessionOutcome {
            machine: it.machine.clone(),
            sid: it.sid.clone(),
            outcome: SessionState::Failed,
            why: Some(f.code),
            said: Some(f.said),
            copy_detail: f.detail,
            title: it.sid.as_str().chars().take(8).collect(),
            session: None,
        }
    });
    o.machine = it.machine.clone();
    if let Some(name) = it.machine.host_name() {
        o.title = copy_text(
            "tabBatch.line.remote",
            &[("title", &o.title), ("machine", name)],
        );
    }
    o
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/sessions_batch_face_tests.rs"]
mod tests;

//! 帧面：轮换的切换（typed，基础功能层）—— 拖出来的先后（`rotation-temp`）· 现在用（`rotation-use-now`）。都是切换这一个动作：
//! 只拿事实拦（策略的上限不拦），写进 `rotation.json`。

use crate::accounts::quota::rotation;
use crate::accounts::quota::rules::book::{self, Session};
use crate::accounts::quota::rules::decide;
use crate::accounts::upstream_select::turn;
use crate::faces::rotation_face::Ctx;
use crate::wire::typed::Command;
pub(crate) use cmd_wire::rotation::{RotDone, RotUnusable, RotationTempReply};
use copy_core::copy_text;
use serde::Deserialize;

fn no_session() -> String {
    copy_text("beRotation.session.gone", &[])
}

fn note(e: crate::common::said::Said) -> String {
    crate::common::said::IntoNote::into_note(e)
}

fn session_of(ctx: &Ctx, sid: &str) -> Option<Session> {
    ctx.hop.store.now().sessions.get(sid).cloned()
}

// ── rotation-temp ─────────────────────────────────────────────────────────────────────────

/// 拖出来的「接下来先用谁」怎么改：把一个号放到第 `at` 位（0 起，越界取末尾）· 清掉。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) enum RotTempOp {
    Place { account: String, at: usize },
    Clear,
}

/// `rotation-temp` 的入参。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationTempReq {
    pub(crate) sid: String,
    pub(crate) op: RotTempOp,
}

crate::wire::typed::codes! {
    /// `rotation-temp` 自己的码。
    pub(crate) enum RotationTempErr {
        NoSession = "no_session",
        /// 那个号不在这台的账号里。
        NotListed = "not_listed",
        IoFailed = "io_failed",
    }
}

/// `rotation-temp`：用户在时间轴上拖出来的顺序（到策略说该换的那一刻先用它）。
pub(crate) struct RotationTemp;

impl Command for RotationTemp {
    const NAME: &'static str = "rotation-temp";
    type Req = RotationTempReq;
    type Reply = RotationTempReply;
    type Err = RotationTempErr;
}

impl crate::wire::typed::Handle for RotationTemp {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: RotationTempReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RotationTempReply, RotationTempErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let (_files, own) = (needs.0, &needs.1);

            temp_with(&Ctx::here(), own, &req)
        }
    }
}

/// 这个号在这台的账号里吗（开会话用的号算）。
fn listed(ctx: &Ctx, s: &Session, account: &str) -> bool {
    account == s.start || ctx.hop.library().accounts.iter().any(|a| a.id == account)
}

pub(crate) fn temp_with(
    ctx: &Ctx,
    own: &crate::platform::Own,
    req: &RotationTempReq,
) -> Result<RotationTempReply, RotationTempErr> {
    let s = session_of(ctx, &req.sid).ok_or_else(|| RotationTempErr::NoSession(no_session()))?;
    if let RotTempOp::Place { account, .. } = &req.op {
        if !listed(ctx, &s, account) {
            return Err(RotationTempErr::NotListed(copy_text(
                "beRotation.edit.notListed",
                &[("acct", account)],
            )));
        }
    }
    let temp = rotation::face_change(&ctx.hop.store, own, |b| {
        let s = b.sessions.get_mut(&req.sid)?;
        match &req.op {
            RotTempOp::Place { account, at } => {
                s.temp.retain(|a| a != account);
                let at = (*at).min(s.temp.len());
                s.temp.insert(at, account.clone());
            }
            RotTempOp::Clear => s.temp.clear(),
        }
        Some(s.temp.clone())
    })
    .map_err(|e| RotationTempErr::IoFailed(note(e)))?
    .ok_or_else(|| RotationTempErr::NoSession(no_session()))?;
    Ok(RotationTempReply { temp })
}

// ── rotation-use-now ──────────────────────────────────────────────────────────────────────

/// `rotation-use-now` 的入参。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct RotationUseNowReq {
    pub(crate) sid: String,
    pub(crate) account: String,
}

crate::wire::typed::codes! {
    /// `rotation-use-now` 自己的码。
    pub(crate) enum RotationUseNowErr {
        NoSession = "no_session",
        NotListed = "not_listed",
        /// 那个号事实上发不出去（被拒 · 没登录 · 没 key · 用满）。策略的上限不拦它。
        Unusable = "unusable" => RotUnusable,
        IoFailed = "io_failed",
    }
}

/// `rotation-use-now`：当场换到这个号、放进拖出来的顺序最前（用户的手动操作，只拿事实拦）。
pub(crate) struct RotationUseNow;

impl Command for RotationUseNow {
    const NAME: &'static str = "rotation-use-now";
    type Req = RotationUseNowReq;
    type Reply = RotDone;
    type Err = RotationUseNowErr;
}

impl crate::wire::typed::Handle for RotationUseNow {
    type Needs = crate::wire::typed::needs::FilesOwn;
    fn run(
        req: RotationUseNowReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<RotDone, RotationUseNowErr>> + Send + 'static
    {
        let (_cx, needs) = cx.split();
        async move {
            let (_files, own) = (needs.0, &needs.1);

            use_now_with(&Ctx::here(), own, &req, crate::accounts::quota::now_unix())
        }
    }
}

pub(crate) fn use_now_with(
    ctx: &Ctx,
    own: &crate::platform::Own,
    req: &RotationUseNowReq,
    now: u64,
) -> Result<RotDone, RotationUseNowErr> {
    let s = session_of(ctx, &req.sid).ok_or_else(|| RotationUseNowErr::NoSession(no_session()))?;
    if !listed(ctx, &s, &req.account) {
        return Err(RotationUseNowErr::NotListed(copy_text(
            "beRotation.edit.notListed",
            &[("acct", &req.account)],
        )));
    }
    let plan = ctx.hop.adoptions.adopted(own, &req.sid, now);
    let row = ctx.row(&s.agent);
    let facts = ctx.hop.accts(&s, &plan.settings, &row, now);
    let sigma = match decide::use_now(&turn::sigma_of(&s), &facts, now, &req.account) {
        Ok(x) => x,
        Err(until) => {
            let why = ctx
                .hop
                .fact_one(
                    &s.agent,
                    &ctx.hop.library(),
                    &req.account,
                    &row,
                    now,
                    &crate::Tz::default(),
                )
                .blocked
                .map(|b| b.why);
            return Err(RotationUseNowErr::Unusable(
                copy_text("beRotation.edit.unusable", &[("acct", &req.account)]),
                RotUnusable { why, until },
            ));
        }
    };
    rotation::face_change(&ctx.hop.store, own, |b| {
        if let Some(x) = b.sessions.get_mut(&req.sid) {
            if x.current != req.account {
                let from = std::mem::replace(&mut x.current, req.account.clone());
                x.since = now;
                x.push_log(book::Log {
                    at: now,
                    from,
                    to: req.account.clone(),
                    cause: book::Cause::UseNow,
                    why: None,
                });
            }
            x.temp.clone_from(&sigma.temp);
        }
    })
    .map_err(|e| RotationUseNowErr::IoFailed(note(e)))?;
    Ok(RotDone {})
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/rotation_act_face_tests.rs"]
mod tests;

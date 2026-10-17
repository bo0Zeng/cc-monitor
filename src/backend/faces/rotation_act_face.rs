//! 帧面：会话侧的临时动作（typed，基础功能层）—— 拖出来的先后 · 先不用的号（`rotation-temp`）· 现在用（`rotation-use-now`）。
//! 都只这个会话、不改策略：只拿事实拦（策略的上限不拦），写进 `rotation.json` 会话那一条。

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

/// 会话侧的临时动作：把一个号放到「接下来先用谁」第 `at` 位（0 起，越界取末尾）· 整串换成拖完的那一串 · 清掉 ·
/// 这个会话先不用 / 再用一个号。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) enum RotTempOp {
    Place { account: String, at: usize },
    Set(Vec<String>),
    Clear,
    Off { account: String, off: bool },
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

/// `rotation-temp`：用户拖出来的顺序（到策略说该换的那一刻先用它）· 先不用的号（一判当用不了）。
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
    let named: Vec<&String> = match &req.op {
        RotTempOp::Place { account, .. } | RotTempOp::Off { account, .. } => vec![account],
        RotTempOp::Set(all) => all.iter().collect(),
        RotTempOp::Clear => Vec::new(),
    };
    if let Some(a) = named.into_iter().find(|a| !listed(ctx, &s, a)) {
        return Err(RotationTempErr::NotListed(copy_text(
            "beRotation.edit.notListed",
            &[("acct", a)],
        )));
    }
    let (temp, off) = rotation::face_change(&ctx.hop.store, own, |b| {
        let s = b.sessions.get_mut(&req.sid)?;
        match &req.op {
            RotTempOp::Place { account, at } => {
                s.temp.retain(|a| a != account);
                let at = (*at).min(s.temp.len());
                s.temp.insert(at, account.clone());
            }
            RotTempOp::Set(all) => {
                s.temp.clear();
                for a in all {
                    if !s.temp.contains(a) {
                        s.temp.push(a.clone());
                    }
                }
            }
            RotTempOp::Clear => s.temp.clear(),
            RotTempOp::Off { account, off } => {
                s.off.retain(|a| a != account);
                if *off {
                    s.off.push(account.clone());
                }
            }
        }
        Some((s.temp.clone(), s.off.clone()))
    })
    .map_err(|e| RotationTempErr::IoFailed(note(e)))?
    .ok_or_else(|| RotationTempErr::NoSession(no_session()))?;
    Ok(RotationTempReply { temp, off })
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
    let plan = crate::accounts::quota::rules::Adopted::of(s.uses.as_ref());
    let row = ctx.row(&s.agent);
    let facts = ctx.hop.accts(&s, &plan.settings, &row, now);
    let sigma = match decide::use_now(&turn::sigma_of(&s), &facts, now, &req.account) {
        Ok(x) => x,
        Err(_) if s.off.contains(&req.account) => {
            return Err(RotationUseNowErr::Unusable(
                copy_text("beRotation.edit.offNow", &[("acct", &req.account)]),
                RotUnusable {
                    why: None,
                    until: None,
                },
            ));
        }
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

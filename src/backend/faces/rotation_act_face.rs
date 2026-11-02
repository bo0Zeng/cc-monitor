//! 帧面：会话侧的临时动作（typed，基础功能层）—— 拖出来的先后 · 先不用的号（`rotation-temp`）· 现在用（`rotation-use-now`）。
//! 都只这个会话、不改策略：只拿事实拦（策略的上限不拦），写进 `rotation.json` 会话那一条。

use crate::accounts::quota::rotation;
use crate::accounts::quota::rules::book::{self, Session};
use crate::accounts::quota::rules::decide::{self, Decision};
use crate::accounts::quota::rules::{simulate, standing, Acct, Adopted};
use crate::accounts::upstream_select::turn;
use crate::faces::rotation_face::Ctx;
use crate::wire::typed::Command;
pub(crate) use cmd_wire::rotation::{
    RotDone, RotTempOp, RotUnusable, RotationTempReply, RotationTempReq, RotationUseNowReq,
};
use copy_core::copy_text;

fn no_session() -> String {
    copy_text("beRotation.session.gone", &[])
}

fn note(e: crate::common::said::Said) -> String {
    crate::common::said::IntoNote::into_note(e)
}

fn session_of(ctx: &Ctx, sid: &str) -> Option<Session> {
    ctx.hop.store.now().sessions.get(sid).cloned()
}

// ── 判一次 · 推演（视图与临时动作同用） ────────────────────────────────────────────────────

/// 照一份副本此刻判一次、推演到 `end`：事实 · 那一判 · 推演。
pub(crate) struct Judged {
    pub(crate) facts: Vec<Acct>,
    pub(crate) d: Decision,
    pub(crate) tl: simulate::Timeline,
}

/// 视图与会话侧的临时动作同照这一处判、推演（`sid` 空 ⇒ 设置页，没有会话）。
pub(crate) fn judged(
    ctx: &Ctx,
    sid: &str,
    session: &Session,
    plan: &Adopted,
    row: &dyn Fn(&str) -> Option<bool>,
    now: u64,
    end: u64,
) -> Judged {
    let facts = ctx
        .hop
        .accts(sid, session, &ctx.hop.store.now(), &plan.settings, row, now);
    let d = ctx.hop.judge_session(sid, session, plan, &facts, now, &[]);
    let budget = crate::accounts::quota::rules::code::Budgeted::new(plan);
    let tl = simulate::simulate(
        &ctx.hop.sigma(sid, session, now),
        &facts,
        &plan.settings,
        &budget,
        &|t| crate::accounts::quota::rules::clock_at(&plan.tz, t),
        now,
        end,
    );
    Judged { facts, d, tl }
}

/// 会话的顺序：第一格 ＝ 在用的号，往下照拖出来的 · 策略交的先后，此刻用不了的与先不用的沉底（各照号的先后）。
pub(crate) fn lineup_of(s: &Session, d: &Decision, facts: &[Acct], now: u64) -> Vec<String> {
    lineup_by(s, d.sigma.temp.iter().chain(&d.order), facts, now)
}

/// 不轮换的会话的顺序：第一格 ＝ 在用的号，往下照号的先后，此刻用不了的与先不用的沉底（没有策略交的先后）。
pub(crate) fn plain_lineup(s: &Session, facts: &[Acct], now: u64) -> Vec<String> {
    lineup_by(s, std::iter::empty(), facts, now)
}

/// 第一格在用的号，往下 `prefer` 里发得出去的，再照号的先后：发得出去的在前，余下的沉底。
fn lineup_by<'a>(
    s: &Session,
    prefer: impl Iterator<Item = &'a String>,
    facts: &'a [Acct],
    now: u64,
) -> Vec<String> {
    let ok = |a: &str| {
        !s.off.iter().any(|x| x == a)
            && facts
                .iter()
                .find(|x| x.id == a)
                .is_some_and(|x| standing::sendable(x, now))
    };
    let mut out = vec![s.current.clone()];
    let ids: Vec<&String> = facts.iter().map(|x| &x.id).collect();
    let usable = prefer.chain(ids.iter().copied());
    for a in usable.filter(|a| ok(a)).chain(ids.iter().copied()) {
        if !out.contains(a) {
            out.push(a.clone());
        }
    }
    out
}

/// 这个会话照它载入的副本判一次、推演到 `end`。
fn judged_session(ctx: &Ctx, sid: &str, s: &Session, now: u64, end: u64) -> Judged {
    let plan = Adopted::of(s.uses.as_deref());
    let row = ctx.row(&s.agent);
    judged(ctx, sid, s, &plan, &row, now, end)
}

// ── rotation-temp ─────────────────────────────────────────────────────────────────────────

crate::wire::typed::codes! {
    /// `rotation-temp` 自己的码。
    pub(crate) enum RotationTempErr {
        NoSession = "no_session",
        /// 那个号不在这台的账号里。
        NotListed = "not_listed",
        /// 拖到第一格（现在用它）而它事实上发不出去（同 `rotation-use-now`）。
        Unusable = "unusable" => RotUnusable,
        IoFailed = "io_failed",
    }
}

/// `rotation-temp`：用户拖出来的顺序（到策略说该换的那一刻先用它）· 先不用的号（一判当用不了）。
pub(crate) struct RotationTemp;

impl Command for RotationTemp {
    crate::wire::typed::by_call!(RotationTempReq);
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

            temp_with(&Ctx::here(), own, &req, crate::common::time::now_secs())
        }
    }
}

/// 这个号在轮换的号表里吗（同 `Hop::roster`：账号 0 不在里头；开会话用的号算）。
fn listed(ctx: &Ctx, s: &Session, account: &str) -> bool {
    account == s.start || ctx.hop.roster().iter().any(|a| a == account)
}

/// 顺序 `line` 里把 `account` 拖到第 `to` 格之后，「接下来先用谁」那一串：第二格起到挪动那一截的尽头（第一格在用的号不在里头）。
/// 没挪 · 不在顺序里 · 越界 ⇒ 缺。
fn moved(line: &[String], account: &str, to: usize) -> Option<Vec<String>> {
    let i = line.iter().position(|a| a == account)?;
    if to >= line.len() || to == i {
        return None;
    }
    let mut next: Vec<String> = line.iter().filter(|a| *a != account).cloned().collect();
    next.insert(to, account.to_string());
    Some(next[1..=i.max(to)].to_vec())
}

pub(crate) fn temp_with(
    ctx: &Ctx,
    own: &crate::platform::Own,
    req: &RotationTempReq,
    now: u64,
) -> Result<RotationTempReply, RotationTempErr> {
    let s = session_of(ctx, &req.sid).ok_or_else(|| RotationTempErr::NoSession(no_session()))?;
    let account = match &req.op {
        RotTempOp::Move { account, .. } | RotTempOp::Off { account, .. } => Some(account),
        RotTempOp::Clear => None,
    };
    if let Some(a) = account.filter(|a| !listed(ctx, &s, a)) {
        return Err(RotationTempErr::NotListed(copy_text(
            "beRotation.edit.notListed",
            &[("acct", a)],
        )));
    }
    // 拖到第一格 ⇒ 现在用它（同 `rotation-use-now`）。
    if let RotTempOp::Move { account, to: 0 } = &req.op {
        let use_now = RotationUseNowReq {
            sid: req.sid.clone(),
            account: account.clone(),
        };
        use_now_with(ctx, own, &use_now, now).map_err(|e| match e {
            RotationUseNowErr::NoSession(t) => RotationTempErr::NoSession(t),
            RotationUseNowErr::NotListed(t) => RotationTempErr::NotListed(t),
            RotationUseNowErr::Unusable(t, u) => RotationTempErr::Unusable(t, u),
            RotationUseNowErr::IoFailed(t) => RotationTempErr::IoFailed(t),
            RotationUseNowErr::BadArgs(t) => RotationTempErr::BadArgs(t),
        })?;
        let s =
            session_of(ctx, &req.sid).ok_or_else(|| RotationTempErr::NoSession(no_session()))?;
        return Ok(RotationTempReply {
            temp: s.temp,
            off: s.off,
        });
    }
    // 第几格照这个会话此刻的顺序认（写之前算好，写的那一下只动那一串）。
    let next = match &req.op {
        RotTempOp::Move { account, to } => {
            let j = judged_session(ctx, &req.sid, &s, now, now);
            moved(&lineup_of(&s, &j.d, &j.facts, now), account, *to)
        }
        _ => None,
    };
    let (temp, off) = rotation::face_change(&ctx.hop.store, own, |b| {
        let s = b.sessions.get_mut(&req.sid)?;
        match &req.op {
            RotTempOp::Move { .. } => {
                if let Some(all) = &next {
                    s.temp.clone_from(all);
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
    crate::wire::typed::by_call!(RotationUseNowReq);
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

            use_now_with(&Ctx::here(), own, &req, crate::common::time::now_secs())
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
    let plan = Adopted::of(s.uses.as_deref());
    let row = ctx.row(&s.agent);
    let facts = ctx.hop.accts(
        &req.sid,
        &s,
        &ctx.hop.store.now(),
        &plan.settings,
        &row,
        now,
    );
    let sigma = match decide::use_now(&ctx.hop.sigma(&req.sid, &s, now), &facts, now, &req.account)
    {
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
                    saw: Some(turn::saw_of(&sigma, &facts, now)),
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

//! 帧面 `session-account`（typed）：**一条会话的号那一份成品**，状态栏按钮 · 它的悬停卡 · 标签页上的号徽章 · 手机只照它画。
//! 会话在哪台就问哪台：那台此刻知道的几样（中转记的开会话的号与在用的号 · 进程环境判出的号 · 记着的上次的号 · 这台的默认号 ·
//! 额度账）在这里并成一份；用哪一样、比不比默认号、写成什么字都在这里定，出口不判、不比、不合。
//! 没走中转的会话也答得出（所以不并进 `rotation-view`）。

use crate::wire::typed::Command;
/// 回包住 `cmd-wire`（两侧同一份类型）。
pub(crate) use cmd_wire::accounts::{SessionAccountBadge, SessionAccountReply, SessionAccountWhen};
use cmd_wire::quota::QuotaRead;
use copy_core::copy_text;
use serde::Deserialize;

/// `session-account` 的入参。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub(crate) struct SessionAccountReq {
    pub(crate) sid: String,
}

crate::wire::typed::codes! {
    /// `session-account` 自己的码（只有入参不对那一种）。
    pub(crate) enum SessionAccountErr {}
}

/// `session-account`：读轮换账本 · 活会话表 · 上次的号 · 账号库 · 额度账 ⇒ 阻塞档。
pub(crate) struct SessionAccountCmd;

impl Command for SessionAccountCmd {
    const NAME: &'static str = "session-account";
    type Req = SessionAccountReq;
    type Reply = SessionAccountReply;
    type Err = SessionAccountErr;
}

impl crate::wire::typed::Handle for SessionAccountCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        req: SessionAccountReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<SessionAccountReply, SessionAccountErr>> + Send + 'static
    {
        async move {
            let ctx = crate::faces::rotation_face::Ctx::here();
            let now = crate::accounts::quota::now_unix();
            let quota = crate::faces::rotation_face::quota_read_with(&ctx, now, &cx.tz);
            let facts = facts_here(&ctx, &req.sid);
            Ok(product(&facts, &quota))
        }
    }
}

/// 那台此刻关于这条会话知道的几样（生产由 [`facts_here`] 取；判据直接造）。
#[derive(Debug, Clone, Default)]
pub(crate) struct Facts {
    /// 中转见过这个会话：`(开会话的号, 在用的号)`。
    pub(crate) rot: Option<(String, String)>,
    /// 会话此刻还活着。
    pub(crate) alive: bool,
    /// 活着、进程环境判出的号（会话宣告带的那一个）。
    pub(crate) live: Option<String>,
    /// 这台记着的「上次用哪个号起的」。
    pub(crate) last: Option<String>,
    /// 这台的默认号（选得了的才算）。
    pub(crate) default: Option<String>,
    /// 这台的号查得了。
    pub(crate) queryable: bool,
    /// 上游是用户自己的（[`crate::accounts::upstream_select::session_upstream::own_upstream_of`] 判的）。
    pub(crate) own_upstream: bool,
}

/// 这台的生产那一份。
fn facts_here(ctx: &crate::faces::rotation_face::Ctx, sid: &str) -> Facts {
    let rot = ctx
        .hop
        .store
        .now()
        .sessions
        .get(sid)
        .map(|s| (s.start.clone(), s.current.clone()));
    let home = crate::history::listing::agent_home();
    let rows = crate::observe::accounts_query::session_accounts_here(&home);
    let mine = || {
        rows.iter()
            .filter(|r| r.alive && r.sid.as_deref() == Some(sid))
    };
    let alive = mine().next().is_some();
    let live = mine().find_map(|r| r.account.clone());
    let list = crate::faces::launch_account_face::list_of(
        crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT,
    );
    let default = list
        .meta
        .effective_default
        .as_deref()
        .and_then(|d| list.accounts.iter().find(|a| a.scanned.name == d))
        .filter(|a| a.scanned.selectable)
        .map(|a| a.scanned.name.clone());
    Facts {
        rot,
        alive,
        live,
        last: crate::faces::launch_account_face::last_of(sid),
        default,
        queryable: list.meta.enabled,
        own_upstream: crate::accounts::upstream_select::session_upstream::own_upstream_here(sid),
    }
}

/// 号名表里的叫法。
fn label(account: &str) -> String {
    crate::accounts::quota::name_words::account_text(account)
}

/// ★ 并成一份（纯函数）。号照这个先后取：中转记的在用号 ＞ 进程环境判出的号 ＞ 记着的上次的号 ＞ 不知道。
pub(crate) fn product(f: &Facts, quota: &QuotaRead) -> SessionAccountReply {
    let rot = f.rot.as_ref();
    let account = rot
        .map(|(_, cur)| cur.clone())
        .or_else(|| f.live.clone().filter(|_| f.alive))
        .or_else(|| f.last.clone());
    let when = match (&account, f.alive) {
        (None, _) => SessionAccountWhen::Unknown,
        (Some(_), true) => SessionAccountWhen::Now,
        // 不活了：中转记的在用号 · 记着的上次的号都只是「上次」。
        (Some(_), false) => SessionAccountWhen::Last,
    };
    let seen = |a: &str| quota.accounts.iter().find(|x| x.head.account == a);
    let unseen = |a: &str| quota.unseen.iter().find(|x| x.head.account == a);
    let texts = |rows: &cmd_wire::quota::Rows| -> Vec<Vec<String>> {
        rows.iter()
            .map(|r| r.iter().map(|c| c.text.0.clone()).collect())
            .collect()
    };
    let switch_row = || {
        vec![
            copy_text("acct.hover.switch", &[]),
            copy_text("acct.hover.restartOnly", &[]),
        ]
    };
    let mut from = None;
    let mut from_line = None;
    let own_upstream = f
        .own_upstream
        .then(|| copy_text("acct.own.line", &[]));
    let (usage, stale, hover, sampled) = match (rot, &account) {
        // 上游是自设的：鉴权是用户网关的，说不出是哪个号的额度 ⇒ 不画那个号的用量（账上有也不画）；换号不适用。
        _ if own_upstream.is_some() => {
            let mut head = Vec::new();
            head.extend(account.as_deref().map(label));
            (
                None,
                false,
                vec![
                    head,
                    vec![
                        copy_text("acct.hover.usage", &[]),
                        copy_text("acct.own.usage", &[]),
                    ],
                    vec![
                        copy_text("acct.hover.switch", &[]),
                        copy_text("acct.own.switch", &[]),
                    ],
                ],
                None,
            )
        }
        // 走中转：用量 · 悬停照额度账里在用那个号那一段；换过号 ⇒ 末尾一行从哪个号换来。
        (Some((start, cur)), _) => {
            let led = seen(cur);
            let mut rows = led
                .map(|l| texts(&l.rows))
                .or_else(|| unseen(cur).map(|u| texts(&u.rows)))
                .unwrap_or_else(|| vec![vec![label(cur)]]);
            // 额度账那一段的末行是采样那一格。
            let sampled = led.and_then(|_| rows.len().checked_sub(1));
            if start != cur {
                from = Some(start.clone());
                let line = copy_text("rot.hover.from", &[("account", &label(start))]);
                rows.push(vec![line.clone()]);
                from_line = Some(line);
            }
            (
                led.map(|l| l.head.show.usage.clone()),
                led.is_some_and(|l| l.head.show.stale),
                rows,
                sampled,
            )
        }
        // 没走中转、号知道：名 · 类型 / 用量 — 未实时显示 / 切换 仅重启切换。
        (None, Some(a)) => {
            let kind = seen(a)
                .map(|l| &l.rows)
                .or_else(|| unseen(a).map(|u| &u.rows))
                .and_then(|r| r.first())
                .and_then(|r| r.get(1))
                .map(|c| c.text.0.clone());
            let mut head = vec![label(a)];
            head.extend(kind);
            (
                None,
                false,
                vec![
                    head,
                    vec![
                        copy_text("acct.hover.usage", &[]),
                        copy_text("acct.val.none", &[]),
                        copy_text("acct.hover.noRelay", &[]),
                    ],
                    switch_row(),
                ],
                None,
            )
        }
        (None, None) => (
            None,
            false,
            vec![vec![copy_text("acct.hover.noRelay", &[])], switch_row()],
            None,
        ),
    };
    // 自设上游：与默认号比没有意义（用的不是那个号的额度）⇒ 不比、不出徽章；标签页悬停卡那一行写那一句。
    let differs = own_upstream.is_none()
        && matches!((&account, &f.default), (Some(a), Some(d)) if a != d);
    let account_line = own_upstream.clone().or_else(|| {
        account
            .as_deref()
            .map(|a| copy_text("tabBar.hover.account", &[("name", a)]))
    });
    let badge = (differs && f.queryable).then(|| SessionAccountBadge {
        ghost: when == SessionAccountWhen::Last,
        title: account_line.clone().unwrap_or_default(),
    });
    SessionAccountReply {
        account_line,
        from_line,
        name: account
            .as_deref()
            .map_or_else(|| copy_text("acct.hover.noRelay", &[]), label),
        account,
        when,
        from,
        usage,
        stale,
        hover,
        sampled,
        differs,
        badge,
        queryable: f.queryable,
        own_upstream,
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_account_face_tests.rs"]
mod tests;

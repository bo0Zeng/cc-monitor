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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
pub struct SessionAccountReq {
    pub(crate) sid: String,
}

crate::wire::typed::codes! {
    /// `session-account` 自己的码（只有入参不对那一种）。
    pub enum SessionAccountErr {}
}

/// `session-account`：读轮换账本 · 活会话表 · 上次的号 · 账号库 · 额度账 ⇒ 阻塞档。
pub struct SessionAccountCmd;

impl Command for SessionAccountCmd {
    const NAME: &'static str = "session-account";
    type Req = SessionAccountReq;
    type Reply = SessionAccountReply;
    type Err = SessionAccountErr;
}

impl crate::wire::typed::Handle for SessionAccountCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        req: SessionAccountReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<SessionAccountReply, SessionAccountErr> {
        let ctx = crate::faces::rotation_face::Ctx::here();
        let now = crate::common::time::now_secs();
        let quota = crate::faces::rotation_face::quota_read_with(&ctx, now, &cx.tz);
        let facts = facts_here(&ctx, &req.sid);
        Ok(product(&facts, &quota))
    }
}

/// 那台此刻关于这条会话知道的几样（生产由 [`facts_here`] 取；判据直接造）。
#[derive(Debug, Clone, Default)]
pub(crate) struct Facts {
    /// 这条会话那一家（号名照它问；[`agent_among`]）：按 sid 找到它的那一家；这台找不到它的记录 ⇒ 活会话表里的那一家；都认不出 ⇒ 空串。
    pub(crate) agent: String,
    /// 中转见过这个会话：`(开会话的号, 在用的号)`。
    pub(crate) rot: Option<(String, String)>,
    /// 会话此刻还活着。
    pub(crate) alive: bool,
    /// 活着、进程环境判出的号（会话宣告带的那一个）。
    pub(crate) live: Option<String>,
    /// 这台记着的「上次用哪个号起的」。
    pub(crate) last: Option<String>,
    /// 这台的默认号（这条会话那一家的、选得了的才算：[`default_of`]）。
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
    let home = crate::agents::local_homes();
    let (alive, live) = standing_at(&home, sid);
    let at = crate::observe::query::locate::session_at(&home, sid).ok();
    let live_kind = crate::history::live_table::now(&home).kind_of(sid);
    let (kind, agent) = agent_among(crate::agents::REGISTRY, at.as_ref(), live_kind);
    let list = crate::faces::launch_account_face::list_of(agent);
    let default = kind.and_then(|k| default_of(&list, &cmd_wire::AgentKind::new(k)));
    Facts {
        agent: agent.to_string(),
        rot,
        alive,
        live,
        last: crate::faces::launch_account_face::last_of(sid),
        default,
        queryable: list.meta.enabled,
        own_upstream: crate::accounts::upstream_select::session_upstream::own_upstream_here(sid),
    }
}

/// 这条会话此刻活没活 · 进程环境判出的号：活没活照脸上那一判（`SessionAccount.live`，启动时刻核不上的照样算活）；
/// 号只认身份核上了的那一行（`alive`）。家目录是参数（判据拿夹具喂它）。
pub(crate) fn standing_at(
    home: impl crate::agents::IntoHomes,
    sid: &str,
) -> (bool, Option<String>) {
    let home_v = crate::agents::IntoHomes::into_homes(home);
    let rows = crate::accounts::manage::library::session_accounts_here(&home_v);
    let mine = || rows.iter().filter(|r| r.sid.as_deref() == Some(sid));
    let alive = mine().any(|r| r.live);
    let account = mine().filter(|r| r.alive).find_map(|r| r.account.clone());
    (alive, account)
}

/// 会话是哪一家照[`crate::observe::accounts_query::session_kind`]（先记录、再活会话表）；认不出 ⇒ `None`（不比默认号：别家的默认号不算）。问清单那几格（查得了没有）· 号名的那一家：就是它；
/// 不知道是哪一家 · 那一家不由我们起 ⇒ 空串（不退到默认那一家）。
pub(crate) fn agent_among(
    registry: &[crate::agents::Adapter],
    at: Option<&crate::agents::SessionAt>,
    live: Option<&'static str>,
) -> (Option<&'static str>, &'static str) {
    let kind = crate::observe::accounts_query::session_kind(at, live);
    let agent = kind
        .and_then(|k| crate::agents::pick_kind_among(registry, Some(k)).ok())
        .map_or("", |(k, _)| k);
    (kind, agent)
}

/// 这台的默认号，且是 `agent` 那一家、选得了的：照起会话「跟随」没有上次的号那一判（[`crate::control::launch_account::pick`]），
/// 别家的默认号 ⇒ 没有。
pub(crate) fn default_of(
    list: &crate::accounts::manage::library::AccountsList,
    agent: &cmd_wire::AgentKind,
) -> Option<String> {
    use crate::control::launch_account::{pick, AccountAsk, Settled};
    match pick(&AccountAsk::Follow, list, None, agent) {
        Ok(Settled::Account(a)) => Some(a.name),
        _ => None,
    }
}

/// ★ 并成一份（纯函数）。号照这个先后取：中转记的在用号 ＞ 进程环境判出的号 ＞ 记着的上次的号 ＞ 不知道。
pub(crate) fn product(f: &Facts, quota: &QuotaRead) -> SessionAccountReply {
    // 号名表里的叫法：照这条会话那一家问。
    let label =
        |account: &str| crate::accounts::quota::name_words::account_text_of(&f.agent, account);
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
    // 自己拼的那几行没有语气：一格一个常规语气的字。额度账那一段照它写好的格（连语气）原样带过去。
    let plain = |rows: Vec<Vec<String>>| -> cmd_wire::quota::Rows {
        rows.into_iter()
            .map(|r| {
                r.into_iter()
                    .map(|t| cmd_wire::quota::Cell {
                        text: crate::common::cells::Words(t),
                        tone: crate::common::cells::Tone::Plain,
                    })
                    .collect()
            })
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
    let own_upstream = f.own_upstream.then(|| copy_text("acct.own.line", &[]));
    let (usage, stale, hover, sampled) = match (rot, &account) {
        // 上游是自设的：鉴权是用户网关的，说不出是哪个号的额度 ⇒ 不画那个号的用量（账上有也不画）；换号不适用。
        _ if own_upstream.is_some() => {
            let mut head = Vec::new();
            head.extend(account.as_deref().map(label));
            (
                None,
                false,
                plain(vec![
                    head,
                    vec![
                        copy_text("acct.hover.usage", &[]),
                        copy_text("acct.own.usage", &[]),
                    ],
                    vec![
                        copy_text("acct.hover.switch", &[]),
                        copy_text("acct.own.switch", &[]),
                    ],
                ]),
                None,
            )
        }
        // 走中转：用量 · 悬停照额度账里在用那个号那一段；换过号 ⇒ 末尾一行从哪个号换来。
        (Some((start, cur)), _) => {
            let led = seen(cur);
            let mut rows = led
                .map(|l| l.rows.clone())
                .or_else(|| unseen(cur).map(|u| u.rows.clone()))
                .unwrap_or_else(|| plain(vec![vec![label(cur)]]));
            // 几点采的：额度账那一条写好的那一句（不成行）。
            let sampled = led.map(|l| l.sampled.0.clone());
            if start != cur {
                from = Some(start.clone());
                let line = copy_text("rot.hover.from", &[("account", &label(start))]);
                rows.extend(plain(vec![vec![line.clone()]]));
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
                plain(vec![
                    head,
                    vec![
                        copy_text("acct.hover.usage", &[]),
                        copy_text("acct.val.none", &[]),
                        copy_text("acct.hover.noRelay", &[]),
                    ],
                    switch_row(),
                ]),
                None,
            )
        }
        (None, None) => (
            None,
            false,
            plain(vec![
                vec![copy_text("acct.hover.noRelay", &[])],
                switch_row(),
            ]),
            None,
        ),
    };
    // 自设上游：与默认号比没有意义（用的不是那个号的额度）⇒ 不比、不出徽章；标签页悬停卡那一行写那一句。
    let differs =
        own_upstream.is_none() && matches!((&account, &f.default), (Some(a), Some(d)) if a != d);
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
        own_upstream,
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/session_account_face_tests.rs"]
mod tests;

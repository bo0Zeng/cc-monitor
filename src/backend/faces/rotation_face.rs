//! 换号那一族的帧面宿主：把账号库（`accounts/manage`）读成上游选择换号要的那几格交进去、把配置编辑层（`quota/setup/`）交给基础层当
//! 「会话用哪份策略问谁」—— 上游选择与账号库管理、基础层与配置编辑层互不认识（后者只许配置层问基础层），都只在这里接上。另住 `quota-read`（额度账的显示态）。
//! typed 命令：基础层 `rotation_facts_face` · `rotation_act_face`；配置编辑层 `rotation_view_face` · `rotation_edit_face` · `rotation_try_face`。

use crate::accounts::upstream_select::rotate::BookAt;
use crate::platform::fs::Own;
use acct_core::AccountBook;

/// 交给中转装配与到点重读（`accounts/quota/reread.rs`）的那一份读法：这台的号表（账号库那一个读口），换号那一侧账号 0 恒在、排第一个。
pub fn books() -> BookAt {
    std::sync::Arc::new(|home| {
        crate::accounts::manage::scan::book_at(home).map(AccountBook::with_bare_first)
    })
}

/// **常驻后端里起中转**（进程入口 `stream/entry.rs` 调）：上游选择的装配口 ＋ 这台的账号库 · 配置编辑层（「会话用哪份策略问谁」住 [`crate::accounts::quota::adoptions_at`]）。
pub fn host_relay(tap: std::sync::Arc<dyn crate::relay::TapPort>, own: Own) -> String {
    crate::accounts::upstream_select::host_relay(
        books(),
        crate::accounts::quota::adoptions_at(),
        history_ports(),
        tap,
        own,
    )
}

/// 交给换号的 history 那两口（生产那一份，唯一一处）：判活照活会话表（pidfile 里的会话 id ＋ 进程还是同一个）· 需手动的铃。
pub(crate) fn history_ports() -> crate::accounts::upstream_select::rotate::HistoryPorts {
    crate::accounts::upstream_select::rotate::HistoryPorts {
        live: std::sync::Arc::new(crate::history::live_table::live_here),
        ring: std::sync::Arc::new(crate::history::needs_changed::ring),
    }
}

/// 交给登录盯防的那一份：账号库里每个号的配置目录（`accounts::quota::nodes::watch_logins_here` 起时递进去）。
pub fn account_dirs() -> crate::accounts::quota::nodes::AccountDirs {
    std::sync::Arc::new(crate::accounts::manage::scan::config_dirs)
}

use crate::accounts::quota::ledger;
use crate::accounts::quota::setup::uses::Setup;
use crate::accounts::quota::show;
use crate::accounts::upstream_select::rotate::Hop;
use crate::common::time::TextClock;
use std::sync::Arc;

/// `(agent, 号)` → 这台 key 表里那一行：没有 ⇒ `None`；有 ⇒ 接不接得上。
type Rows = Box<dyn Fn(&str, &str) -> Option<bool> + Send + Sync>;

/// 一条命令要的几样：基础层那一份（轮换的账本 · 额度账 · 账号库，都在 `hop` 里）· 配置编辑层那一份（`setup`，`hop` 问的就是它）·
/// 这台 key 表里有哪几行。
pub(crate) struct Ctx {
    pub(crate) hop: Hop,
    pub(crate) setup: Arc<Setup>,
    pub(crate) rows: Rows,
}

impl Ctx {
    /// 这台的生产那一份：数据目录 · 家 · key 表那份文件都照这台推（[`Self::at`]）。
    /// 只读那几问也用它，不拿写句柄：数据目录照读那一侧的推法（`platform::paths::data_home`，与启动时造写句柄同一个出处）。
    pub(crate) fn here() -> Self {
        Self::at(
            crate::platform::paths::data_home().as_deref(),
            crate::platform::paths::home_dir(),
            crate::accounts::upstream_select::file_face::machine_key_path().as_deref(),
        )
    }

    /// 落点照交进来的：账本 · 额度账（现读盘）住 `data`，账号库照家 `home` 读，key 表照 `keys` 那份文件答（推不出 ⇒ 零行）。
    pub(crate) fn at(
        data: Option<&std::path::Path>,
        home: Option<std::path::PathBuf>,
        keys: Option<&std::path::Path>,
    ) -> Self {
        let rows = keys
            .map(crate::accounts::upstream_select::file_face::rows_at)
            .unwrap_or_default();
        let hop = Hop::here(
            data,
            home,
            books(),
            &crate::accounts::quota::adoptions_at(),
            history_ports(),
        );
        Self {
            setup: Arc::new(Setup::of(Arc::clone(&hop.store))),
            hop,
            rows: Box::new(move |agent, a| rows.of(agent).iter().any(|r| r == a).then_some(true)),
        }
    }

    /// 这一家这个号在 key 表里那一行（同 [`Self::rows`]，绑好那一家）。
    pub(crate) fn row<'a>(&'a self, agent: &'a str) -> impl Fn(&str) -> Option<bool> + 'a {
        move |a| (self.rows)(agent, a)
    }
}

// ── quota-read ───────────────────────────────────────────────────────────────────────

crate::wire::typed::codes! {
    /// `quota-read` 没有别的码：读不出来是一个状态（`state: "unreadable"` ＋ `reason`），不是一次失败。
    pub enum QuotaReadErr {}
}

/// `quota-read`：这台的额度账，每条带上显示态；另给账号库里从没出过数的号、此刻发得出去的号、最早回来的那个。
/// 出口那一下给每个时刻添好显示的字、每号添好几行与开窗那一判（读一份文件 ＋ 账号库 ⇒ 阻塞档）。
pub struct QuotaReadCmd;

impl crate::wire::typed::Command for QuotaReadCmd {
    crate::wire::typed::by_call!(cmd_wire::quota::QuotaReadReq);
    const TAKES_INPUT: bool = false;
    type Err = QuotaReadErr;
}

impl crate::wire::typed::Handle for QuotaReadCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    async fn run(
        _req: cmd_wire::quota::QuotaReadReq,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> Result<QuotaRead, QuotaReadErr> {
        Ok(quota_read_now(&Ctx::here(), &cx))
    }
}

pub(crate) use crate::faces::quota_read::QuotaRead;

/// [`quota_read_with`]，照请求信封 `cx` 的时区、此刻的钟。
pub(crate) fn quota_read_now(ctx: &Ctx, cx: &crate::wire::typed::Cx) -> QuotaRead {
    quota_read_with(ctx, crate::common::time::now_secs(), &cx.tz)
}

/// 读这台的额度账、给每个号判显示态，交出口写成成品。
pub(crate) fn quota_read_with(ctx: &Ctx, now: u64, tz: &crate::common::time::Tz) -> QuotaRead {
    use crate::faces::quota_read::{reply_of, Base, Seen, Unseen, UnseenHead};
    let base = ledger::answer_of(ctx.hop.quota.path(), now);
    let seen = &base.accounts;
    let lib = ctx.hop.book();
    let shown = |agent: &str, account: &str, o: Option<&ledger::Observed>| {
        let slot = crate::agents::route_named(agent).and_then(crate::agents::window_slot_of);
        let key = crate::agents::route_named(agent).and_then(crate::agents::window_key_of);
        let slot = |w: &str| slot.and_then(|f| f(w));
        let key = |w: &str| key.and_then(|f| f(w));
        let mut sh = show::show(
            o.map(|o| (&o.reading, o.seen_at)),
            ctx.hop
                .show_facts(agent, &lib, account, &|a| (ctx.rows)(agent, a), now),
            now,
            &slot,
        );
        if let Some(o) = o {
            sh.windows = show::windows_of(o, &key, now);
        }
        sh
    };
    // 号按（家, 号名）认：两家同名的号各算各的。
    let mut usable: Vec<(String, String)> = Vec::new();
    let mut earliest: Option<(u64, (String, String))> = None;
    // 「可发」与「几点回来」照事实那一格（`blocked` 由 `rules::standing` 那一处判），显示态不另判；开窗那一判也照这一份事实。
    let mut judge = |agent: &str, account: &str| {
        let f = ctx
            .hop
            .fact_one(agent, &lib, account, &|a| (ctx.rows)(agent, a), now, tz);
        let key = (agent.to_string(), account.to_string());
        match &f.blocked {
            None => usable.push(key),
            Some(b) => {
                if let Some(at) = b
                    .until
                    .filter(|at| earliest.as_ref().is_none_or(|(t, _)| at < t))
                {
                    earliest = Some((at, key));
                }
            }
        }
        crate::accounts::quota::rules::decide::warm_verdict(
            &crate::accounts::quota::rules::Acct::of_fact(agent, &f),
            now,
        )
    };
    let mut rows: Vec<Seen> = Vec::new();
    for o in seen {
        let sh = shown(&o.agent, &o.account, Some(o));
        let warm = judge(&o.agent, &o.account);
        rows.push(Seen {
            agent: o.agent.clone(),
            account: o.account.clone(),
            seen_at: o.seen_at,
            reading: o.reading.clone(),
            show: sh,
            warm,
        });
    }
    // 账号库里每个号照它自己那一家认（（家, 号名）：别家有同名的号出过数，不算这个号出过数）。
    let unseen: Vec<Unseen> = lib
        .accounts
        .iter()
        .filter_map(|m| {
            Some((
                crate::accounts::upstream_select::rotate::family_of(m)?,
                m.route_id()?,
            ))
        })
        .filter(|(agent, id)| !seen.iter().any(|o| o.agent == *agent && o.account == *id))
        .map(|(agent, id)| {
            let sh = shown(agent, &id, None);
            let warm = judge(agent, &id);
            Unseen {
                head: UnseenHead {
                    agent: crate::agents::kind_of_route(agent),
                    account: id,
                    kind: sh.kind,
                    login: sh.login,
                    usage: sh.usage.into(),
                },
                warm,
            }
        })
        .collect();
    reply_of(
        Base {
            state: base.state,
            reason: base.reason.clone(),
            detail: base.detail.clone(),
            path: base.path.clone(),
            now: base.now,
            usable,
            earliest: earliest.map(|(at, key)| (key, at)),
        },
        rows,
        unseen,
        &TextClock::new(now, tz),
    )
}

#[cfg(test)]
#[path = "../../../tests/backend/faces/rotation_face_tests.rs"]
pub(crate) mod tests;

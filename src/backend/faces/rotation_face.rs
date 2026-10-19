//! 换号那一族的帧面宿主：把账号库（`accounts/manage`）读成上游选择换号要的那几格交进去、把配置编辑层（`quota/setup/`）交给基础层当
//! 「会话用哪份策略问谁」—— 上游选择与账号库管理、基础层与配置编辑层互不认识（后者只许配置层问基础层），都只在这里接上。另住 `quota-read`（额度账的显示态）。
//! typed 命令：基础层 `rotation_facts_face` · `rotation_act_face`；配置编辑层 `rotation_view_face` · `rotation_edit_face` · `rotation_try_face`。

use crate::accounts::upstream_select::rotate::{LibAccount, Library, LibraryRead};
use crate::platform::fs::Own;
use std::path::Path;

/// 这台的账号库：清单里每个号（配置目录的末段 ＝ 路由第 2 段）＋ 账号 0。没建账号库 / 清单读不懂 ⇒ 空。
pub(crate) fn library_at(home: &Path) -> Library {
    let manifest = crate::platform::paths::home_join(home, relay_route_core::ACCOUNTS_DIR_REL)
        .join(relay_route_core::ACCOUNTS_MANIFEST_NAME);
    let Some(m) = crate::accounts::manage::scan::manifest_text_at(&manifest)
        .ok()
        .and_then(|t| crate::accounts::manage::model::Manifest::parse(&t).ok())
    else {
        return Library::default();
    };
    let mut accounts = vec![LibAccount {
        id: crate::accounts::manage::model::ACCOUNT_ZERO.to_string(),
        dir: None,
        api: false,
    }];
    accounts.extend(m.managed().filter_map(|a| {
        Some(LibAccount {
            id: acct_core::apikey_account_id_of_dir(&a.config_dir)?,
            dir: Some(a.config_dir.clone().into()),
            api: a.is_api_key(),
        })
    }));
    Library {
        enabled: true,
        accounts,
    }
}

/// 交给中转装配的那一份读法。
pub(crate) fn library() -> LibraryRead {
    std::sync::Arc::new(library_at)
}

/// 交给中转装配的「会话用哪份策略问谁」：跟着那份 `rotation.json` 的配置编辑层。
pub(crate) fn adoptions_at() -> crate::accounts::quota::rules::AdoptionsAt {
    std::sync::Arc::new(|store| std::sync::Arc::new(Setup::of(std::sync::Arc::clone(store))))
}

/// **常驻后端里起中转**（`main.rs` 调）：上游选择的装配口 ＋ 这台的账号库 · 配置编辑层；接上了中转那一刻，
/// 盘上的轮换若还是旧版，升级写回、播种（全机只这一处写回）。
pub fn host_relay(tap: std::sync::Arc<dyn crate::relay::TapPort>, own: Own) -> String {
    let up = own.clone();
    crate::accounts::upstream_select::host_relay(library(), adoptions_at(), tap, own, move || {
        let lib = crate::platform::paths::home_dir()
            .map(|h| library_at(&h))
            .unwrap_or_default();
        let rest = crate::accounts::upstream_select::rotate::rest_members(
            lib.accounts.iter().map(|a| a.id.as_str()),
            &[],
            |x| lib.accounts.iter().any(|a| a.id == x && !a.api),
        );
        let live = crate::history::live::live_here();
        uses::migrate_here(
            &up,
            &uses::UpgradeEnv {
                live: &|sid| live.contains(sid),
                rest,
                title: &|_| None,
            },
        );
    })
}

/// 交给登录盯防的那一份：账号库里每个号的配置目录（`accounts::quota::nodes::watch_logins_here` 起时递进去）。
pub(crate) fn account_dirs() -> crate::accounts::quota::nodes::AccountDirs {
    std::sync::Arc::new(|home| {
        library_at(home)
            .accounts
            .into_iter()
            .filter_map(|a| a.dir)
            .collect()
    })
}

use crate::accounts::quota::ledger::{self, Ledger};
use crate::accounts::quota::rotation::{self, RotationStore};
use crate::accounts::quota::setup::uses::{self, Setup};
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
    /// 这台的生产那一份：账本 · 额度账（现读盘）· 家 · 账号库 · key 表都从真环境取。
    pub(crate) fn here() -> Self {
        let get = |k: &str| std::env::var(k).ok();
        let quota = Arc::new(Ledger::at(ledger::path_from(&get)));
        let home = crate::platform::paths::home_dir_from(&|k| get(k).map(Into::into));
        let rows = crate::accounts::upstream_select::file_face::machine_rows();
        let store = Arc::new(RotationStore::at(rotation::path_now()));
        let setup = Arc::new(Setup::of(Arc::clone(&store)));
        let adoptions: crate::accounts::quota::rules::AdoptionsAt = {
            let s = Arc::clone(&setup);
            Arc::new(move |_| Arc::clone(&s) as Arc<dyn crate::accounts::quota::rules::Adoptions>)
        };
        Self {
            setup,
            hop: Hop::new(
                store,
                &adoptions,
                quota,
                home,
                library(),
                Arc::new(crate::history::live::live_here),
                None,
            ),
            rows: Box::new(move |agent, a| {
                (agent == crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT
                    && rows.iter().any(|r| r == a))
                .then_some(true)
            }),
        }
    }

    /// 这一家这个号在 key 表里那一行（同 [`Self::rows`]，绑好那一家）。
    pub(crate) fn row<'a>(&'a self, agent: &'a str) -> impl Fn(&str) -> Option<bool> + 'a {
        move |a| (self.rows)(agent, a)
    }
}

/// 凭据文件那一家（账号库是它的）：没有会话的那几问（设置页 · 试跑）按它判种类与事实。
pub(crate) const LIBRARY_AGENT: &str = crate::accounts::upstream_select::CREDENTIALS_FILE_AGENT;

// ── quota-read ───────────────────────────────────────────────────────────────────────

crate::wire::typed::codes! {
    /// `quota-read` 没有别的码：读不出来是一个状态（`state: "unreadable"` ＋ `reason`），不是一次失败。
    pub(crate) enum QuotaReadErr {}
}

/// `quota-read`：这台的额度账，每条带上显示态；另给账号库里从没出过数的号、此刻发得出去的号、最早回来的那个。
/// 出口那一下给每个时刻添好显示的字、每号添好几行与开窗那一判（读一份文件 ＋ 账号库 ⇒ 阻塞档）。
pub(crate) struct QuotaReadCmd;

impl crate::wire::typed::Command for QuotaReadCmd {
    const NAME: &'static str = "quota-read";
    const TAKES_INPUT: bool = false;
    type Req = crate::wire::typed::NoArgs;
    type Reply = QuotaRead;
    type Err = QuotaReadErr;
}

impl crate::wire::typed::Handle for QuotaReadCmd {
    type Needs = crate::wire::typed::needs::Nothing;
    fn run(
        _req: crate::wire::typed::NoArgs,
        cx: crate::wire::typed::Cx<Self::Needs>,
    ) -> impl std::future::Future<Output = Result<QuotaRead, QuotaReadErr>> + Send + 'static {
        async move {
            Ok(quota_read_with(
                &Ctx::here(),
                crate::accounts::quota::now_unix(),
                &cx.tz,
            ))
        }
    }
}

pub(crate) use crate::faces::quota_read::QuotaRead;

/// 读这台的额度账、给每个号判显示态，交出口写成成品。
pub(crate) fn quota_read_with(ctx: &Ctx, now: u64, tz: &crate::Tz) -> QuotaRead {
    use crate::faces::quota_read::{reply_of, Base, Seen, UnseenHead};
    let base = ledger::answer_of(ctx.hop.quota.path(), now);
    let seen = &base.accounts;
    let lib = ctx.hop.library();
    let shown = |agent: &str, account: &str, o: Option<&ledger::Observed>| {
        let slot = crate::agents::window_slot_of(agent);
        let key = crate::agents::window_key_of(agent);
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
            show::per_pct_of_slots(&mut sh);
        }
        sh
    };
    let mut usable: Vec<String> = Vec::new();
    let mut earliest: Option<(u64, String)> = None;
    let mut rows: Vec<Seen> = Vec::new();
    for o in seen {
        let sh = shown(&o.agent, &o.account, Some(o));
        if show::usable(&sh) {
            usable.push(o.account.clone());
        }
        if let Some(at) = show::back_at(&sh, &o.reading, now) {
            if earliest.as_ref().is_none_or(|(t, _)| at < *t) {
                earliest = Some((at, o.account.clone()));
            }
        }
        rows.push(Seen {
            agent: o.agent.clone(),
            account: o.account.clone(),
            seen_at: o.seen_at,
            reading: o.reading.clone(),
            windows_seen: o.windows_seen.clone(),
            show: sh,
        });
    }
    let unseen: Vec<UnseenHead> = lib
        .accounts
        .iter()
        .filter(|a| {
            !seen
                .iter()
                .any(|o| o.agent == LIBRARY_AGENT && o.account == a.id)
        })
        .map(|a| {
            let sh = shown(LIBRARY_AGENT, &a.id, None);
            if show::usable(&sh) {
                usable.push(a.id.clone());
            }
            UnseenHead {
                agent: crate::agents::kind_of_route(LIBRARY_AGENT),
                account: a.id.clone(),
                kind: sh.kind,
                login: sh.login,
                sub_id: sh.sub_id,
                usage: sh.usage.into(),
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
            usable_now: usable,
            earliest: earliest.map(|(at, account)| (account, at)),
        },
        rows,
        unseen,
        &TextClock::new(now, tz),
    )
}

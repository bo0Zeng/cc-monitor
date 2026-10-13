//! 上游选择里「这个会话这一发该走哪个号」那一问：问轮换（钉在哪 · 满了换谁），拿那个号的令牌与身份，备好这一发的去处。
//!
//! - 判换不换、换谁归策略（[`crate::accounts::quota::rules::decide::turn`] 叫它）；只在节点上判、结论缓存到下一个节点（[`super::turn`]）。
//!   这里只备料（令牌 · 身份 · 改写后的请求体）。
//! - 起会话的号照今天走（[`Go::Start`]，那张决策表一个字节不变）；只有换到**别的号**时才由这里备料：
//!   订阅号 ⇒ 鉴权头换成那个号的访问令牌（账号域 `oauth` 给，快过期就续），请求体里账号身份那一格一起换；
//!   按量号 ⇒ 照它在 key 表里那一行，身份格换成空串。
//! - 账号库（哪些号 · 配置目录 · 是不是按量号）由宿主读好交进来（[`LibraryRead`]）：上游选择不认识账号库管理。
//! - 令牌只在备料时拿到、包在秘密类型里；拿不到只记原因码，那一句不带令牌。

use super::table::RoutingTable;
use crate::accounts::oauth::{self, TokenEndpoint};
use crate::accounts::quota::facts::Kind;
use crate::accounts::quota::ledger::Ledger;
use crate::accounts::quota::rotation::RotationStore;
use crate::accounts::quota::show::{self, LoginState};
use crate::agents::{IdentityCell, LoginFace};
use crate::platform::fs::{Own, Ro};
use crate::relay::{AuthSwap, Destination, Mode, RouteKey};
use creds_core::SecretKey;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 续登录令牌那一发的期限。
const TOKEN_DEADLINE: std::time::Duration = std::time::Duration::from_millis(15_000);

/// 账号库里的一个号（换号要的那几格）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibAccount {
    /// 路由第 2 段（配置目录的末段；账号 0 是 `0`）。
    pub id: String,
    /// 配置目录；`None` ＝ 账号 0（这一家的默认配置目录，身份在家目录下）。
    pub dir: Option<PathBuf>,
    /// 按量号（API key）。
    pub api: bool,
}

/// 这台的账号库。`enabled = false` ⇒ 没建账号库（只能用起会话那个号）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Library {
    pub enabled: bool,
    pub accounts: Vec<LibAccount>,
}

impl Library {
    fn find(&self, id: &str) -> Option<&LibAccount> {
        self.accounts.iter().find(|a| a.id == id)
    }
}

/// 家目录 → 这台的账号库（宿主给）。
pub type LibraryRead = Arc<dyn Fn(&Path) -> Library + Send + Sync>;

/// 能当轮换里的号：过路由段闸、不是「没说是哪个号」那一格。
pub(crate) fn account_ok(a: &str) -> bool {
    crate::relay::segment_is_safe(a) && a != super::endpoint::UNDECLARED_ACCOUNT_SEGMENT
}

/// 「账号库里其余的订阅号」此刻是哪几个：库里没点名（`named`）、名字过得了路由段的订阅号，按名字排。
/// 挑号（[`Rotation`] 排候选）与配置编辑器那一行写成员都照它。
pub(crate) fn rest_members<'a>(
    ids: impl IntoIterator<Item = &'a str>,
    named: &[String],
    is_sub: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut rest: Vec<String> = ids
        .into_iter()
        .filter(|x| !named.iter().any(|n| n == x) && account_ok(x) && is_sub(x))
        .map(str::to_string)
        .collect();
    rest.sort();
    rest
}

/// 备好的这一发去处（自有值：令牌与改写后的请求体跨出判定、在上游选择的锁里交给中转）。
pub(crate) enum Go {
    /// 照起会话那个号走（那张决策表）。
    Start,
    /// 订阅号：换上它的令牌；`body` 是身份格换好的整份请求体（`None` ＝ 请求体里没有那一格，原样发）。
    Sub {
        account: String,
        token: SecretKey,
        body: Option<Vec<u8>>,
    },
    /// 按量号：照它在 key 表里那一行。
    Api {
        account: String,
        body: Option<Vec<u8>>,
    },
    /// 硬上限卡住：这一发不发上游，回这一家的「用满」回包。
    Hold { reply: crate::agents::LimitReply },
}

/// 为什么一个号接不上（备料那一下）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unready {
    /// 拿不到这个号能用的登录（没登录 · 续期失败 · 读不出账号身份）。
    NeedsLogin,
    /// 按量号在这台的 key 表里没有 key。
    NeedsKey,
    /// 这一发的请求体里账号身份那一格认不准，换不了。
    UnsureBody,
}

/// 一家的窗口名 → 语义位。
pub(super) fn slot_fn(agent: &str) -> impl Fn(&str) -> Option<&'static str> {
    let f = crate::agents::window_slot_of(agent);
    move |w: &str| f.and_then(|f| f(w))
}

/// 一发请求在上游选择这一侧的事实（判的时候要的；按量号那几格由调用方从 key 表答）。
pub(crate) struct Turn<'a> {
    pub(crate) agent: &'a str,
    /// 起会话的号（路由第 2 段）。
    pub(crate) start: &'a str,
    /// 会话 id（流标签）。
    pub(crate) sid: &'a str,
    pub(crate) body: &'a [u8],
    /// 这个号在这台 key 表里那一行：没有 ⇒ `None`；有 ⇒ 接不接得上。
    pub(crate) row: &'a dyn Fn(&str) -> Option<bool>,
    pub(crate) now: u64,
}

/// 读身份的缓存：`.claude.json` 可能很大，按那份文件的戳缓存（文件动了再读）。
/// 这台此刻活着的会话（sid 集合）。判活只认一处：生产那一份是 [`crate::observe::accounts_query::live_here`]（pidfile 里的会话 id ＋ 进程还是同一个）。
pub(crate) type Live = Arc<dyn Fn() -> std::collections::BTreeSet<String> + Send + Sync>;

type IdentityCache = Mutex<BTreeMap<PathBuf, (Option<(std::time::SystemTime, u64)>, String)>>;

/// 换号要的几样：轮换的账本 · 额度账 · 家目录 · 账号库 · 令牌端点（缺省用那一家登记的）。
pub(crate) struct Hop {
    pub(crate) store: Arc<RotationStore>,
    pub(crate) quota: Arc<Ledger>,
    pub(crate) home: Option<PathBuf>,
    pub(crate) library: LibraryRead,
    pub(crate) token: Option<TokenEndpoint>,
    /// 这台此刻活着的会话。
    pub(crate) live: Live,
    identities: IdentityCache,
    /// 每个会话上一判的结论，缓存到下一个节点（[`super::turn`]）。
    pub(super) cache: Mutex<super::turn::Cache>,
}

impl Hop {
    /// `token`：续令牌发到哪（`None` ＝ 那一家登记的端点；判据一律给一个本机的假端点）。
    pub(crate) fn new(
        store: Arc<RotationStore>,
        quota: Arc<Ledger>,
        home: Option<PathBuf>,
        library: LibraryRead,
        live: Live,
        token: Option<TokenEndpoint>,
    ) -> Self {
        Self {
            store,
            quota: quota.clone(),
            home,
            library,
            token,
            cache: Mutex::new(super::turn::Cache::new(quota.nodes())),
            live,
            identities: Mutex::new(BTreeMap::new()),
        }
    }

    pub(crate) fn library(&self) -> Library {
        self.home
            .as_deref()
            .map(|h| (self.library)(h))
            .unwrap_or_default()
    }

    fn endpoint(&self, face: &LoginFace) -> Option<TokenEndpoint> {
        match &self.token {
            Some(t) => Some(TokenEndpoint {
                base: t.base.clone(),
                rest: t.rest.clone(),
                deadline: t.deadline,
            }),
            None => TokenEndpoint::of(face, TOKEN_DEADLINE),
        }
    }

    /// 号 → （配置目录, 账号 0 时的家目录）。
    fn dir_of(
        &self,
        face: &LoginFace,
        lib: &Library,
        id: &str,
    ) -> Option<(PathBuf, Option<PathBuf>)> {
        let home = self.home.as_deref()?;
        // 起会话时没说是哪个号（`_`）不是哪一份登录：不拿默认配置目录（账号 0）那份顶。
        match &lib.find(id)?.dir {
            Some(d) => Some((d.clone(), None)),
            None => Some(((face.base_dir)(home), Some(home.to_path_buf()))),
        }
    }

    /// ★ 起会话时没说是哪个号（`_`）的这一发实际在用哪个号：请求体里的账号身份恰好对上账号库里**一个**订阅号 ⇒ 那个号的 id
    /// （账号 0 ⇒ `0`）；一个都没对上 · 对上不止一个 · 请求体里没有这一格 · 没建账号库 ⇒ `None`（照旧记 `_`，不猜）。
    /// 每一发都比一次（身份按文件戳缓存）：会话里 `/login` 换了号，下一发就认成新号。
    pub(crate) fn recognize(&self, agent: &str, body: &[u8]) -> Option<String> {
        let face = crate::agents::login_of(agent)?;
        let uuid = (face.identity_of_body)(body)?;
        let lib = self.library();
        let mut hit = lib.accounts.iter().filter(|x| !x.api).filter(|x| {
            self.dir_of(&face, &lib, &x.id)
                .and_then(|(dir, base)| self.identity(&face, &dir, base.as_deref()))
                .is_some_and(|who| who == uuid)
        });
        let one = hit.next()?;
        hit.next().is_none().then(|| one.id.clone())
    }

    fn identity(&self, face: &LoginFace, dir: &Path, base: Option<&Path>) -> Option<String> {
        let file = (face.identity_file)(dir, base);
        let stamp = Ro::metadata(&file)
            .ok()
            .and_then(|m| Some((m.modified().ok()?, m.len())));
        let mut g = self.identities.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((s, who)) = g.get(&file) {
            if stamp.is_some() && *s == stamp {
                return Some(who.clone());
            }
        }
        let who = (face.identity_in)(&file)?;
        g.insert(file, (stamp, who.clone()));
        Some(who)
    }

    /// 订阅号此刻能用的令牌与身份；拿不到 ⇒ 原因码（那一句只进日志，不带令牌）。
    fn login(
        &self,
        own: &Own,
        agent: &str,
        lib: &Library,
        account: &str,
        now: u64,
    ) -> Result<(SecretKey, String), Unready> {
        let face = crate::agents::login_of(agent).ok_or(Unready::NeedsLogin)?;
        let (dir, base) = self
            .dir_of(&face, lib, account)
            .ok_or(Unready::NeedsLogin)?;
        let ep = self.endpoint(&face).ok_or(Unready::NeedsLogin)?;
        let token = oauth::access_token(own, &dir, &face, &ep, now.saturating_mul(1000), account)
            .map_err(|u| {
            tracing::warn!("[rotate] {}", u.said(account).logged());
            self.quota
                .notify(super::super::quota::nodes::Source::Login, None);
            Unready::NeedsLogin
        })?;
        let who = self
            .identity(&face, &dir, base.as_deref())
            .ok_or(Unready::NeedsLogin)?;
        Ok((token, who))
    }

    /// ★ 一个号此刻接不接得上（只读，不续令牌）—— 帧面（[`Self::view`]）只问它，真换号（[`Self::prepare`]）先问它再拿令牌，
    /// 两边同一处判：起会话的号 ⇒ 接得上；按量号 ⇒ key 表里那一行接得上；订阅号 ⇒ 盘上有登录、读得出账号身份。
    pub(super) fn reach(
        &self,
        a: &Turn<'_>,
        lib: &Library,
        account: &str,
    ) -> Result<Reach, Unready> {
        if account == a.start {
            return Ok(Reach::Start);
        }
        match kind_of(a, lib, account) {
            Kind::Api if (a.row)(account) == Some(true) => Ok(Reach::Api),
            Kind::Api => Err(Unready::NeedsKey),
            Kind::Sub => self.who_on_disk(a.agent, lib, account).map(|_| Reach::Sub),
        }
    }

    /// 备好走 `account` 的这一发（起会话的号 ⇒ [`Go::Start`]）。
    pub(super) fn prepare(
        &self,
        own: &Own,
        a: &Turn<'_>,
        lib: &Library,
        account: &str,
    ) -> Result<Go, Unready> {
        let rewrite = |who: &str| -> Result<Option<Vec<u8>>, Unready> {
            let face = crate::agents::login_of(a.agent).ok_or(Unready::UnsureBody)?;
            match (face.rewrite_identity)(a.body, who) {
                IdentityCell::Absent => Ok(None),
                IdentityCell::Rewritten(b) => Ok(Some(b)),
                IdentityCell::Unsure => Err(Unready::UnsureBody),
            }
        };
        match self.reach(a, lib, account)? {
            Reach::Start => Ok(Go::Start),
            Reach::Api => Ok(Go::Api {
                account: account.to_string(),
                body: rewrite("")?,
            }),
            Reach::Sub => {
                let (token, who) = self.login(own, a.agent, lib, account, a.now)?;
                Ok(Go::Sub {
                    account: account.to_string(),
                    token,
                    body: rewrite(&who)?,
                })
            }
        }
    }

    fn who_on_disk(&self, agent: &str, lib: &Library, account: &str) -> Result<String, Unready> {
        let face = crate::agents::login_of(agent).ok_or(Unready::NeedsLogin)?;
        let (dir, base) = self
            .dir_of(&face, lib, account)
            .ok_or(Unready::NeedsLogin)?;
        if !matches!(
            oauth::store::read(&dir, &face),
            oauth::store::Read::Present(_)
        ) {
            return Err(Unready::NeedsLogin);
        }
        self.identity(&face, &dir, base.as_deref())
            .ok_or(Unready::NeedsLogin)
    }

    /// 一个号显示态要的几格：种类 · 登录（只读，不续令牌）· 订阅标识（账号身份的散列，原值不出这里）。
    pub(crate) fn show_facts(
        &self,
        agent: &str,
        lib: &Library,
        account: &str,
        row: &dyn Fn(&str) -> Option<bool>,
    ) -> show::Facts {
        let kind = kind_with(row, lib, account);
        let (login, sub_id) = match kind {
            // 起会话时没说是哪个号：是哪一份登录说不准 ⇒ 不带订阅标识；登录照起它的那个会话（它自己发得出去）。
            Kind::Sub if account == super::endpoint::UNDECLARED_ACCOUNT_SEGMENT => {
                (LoginState::Ok, None)
            }
            Kind::Api => (
                if row(account) == Some(true) {
                    LoginState::Ok
                } else {
                    LoginState::NeedsKey
                },
                None,
            ),
            Kind::Sub => match self.who_on_disk(agent, lib, account) {
                Ok(who) => (LoginState::Ok, Some(show::sub_id_of(&who))),
                Err(_) => (LoginState::NeedsLogin, None),
            },
        };
        show::Facts {
            kind,
            login,
            sub_id,
        }
    }

    /// ★ 这台这一家每个号此刻的事实（`rotation-facts` 与判 · 推演同用这一处；只读：不续令牌、不记）。号照账号库的先后。
    pub(crate) fn facts(
        &self,
        agent: &str,
        row: &dyn Fn(&str) -> Option<bool>,
        now: u64,
        tz: &crate::Tz,
    ) -> Vec<crate::accounts::quota::facts::Fact> {
        let lib = self.library();
        lib.accounts
            .iter()
            .map(|a| self.fact_one(agent, &lib, &a.id, row, now, tz))
            .collect()
    }

    /// 一个号的事实（不在账号库里的号也拼得出：开会话用的号没说是哪个号时）。
    pub(crate) fn fact_one(
        &self,
        agent: &str,
        lib: &Library,
        id: &str,
        row: &dyn Fn(&str) -> Option<bool>,
        now: u64,
        tz: &crate::Tz,
    ) -> crate::accounts::quota::facts::Fact {
        let slot = slot_fn(agent);
        let when = |t: u64| {
            crate::common::time::fmt_at(
                i64::try_from(t).unwrap_or(i64::MAX),
                i64::try_from(now).unwrap_or(i64::MAX),
                tz,
            )
        };
        let o = self.quota.entry(agent, id);
        let rate = |name: &str| o.as_ref().and_then(|o| o.rate(name, now));
        crate::accounts::quota::facts::fact_of(
            id,
            o.as_ref().map(|o| &o.reading),
            o.as_ref().map(|o| o.seen_at),
            kind_with(row, lib, id),
            self.show_facts(agent, lib, id, row).login,
            now,
            &slot,
            &rate,
            &when,
        )
    }
}

/// 号的种类：key 表里有行、或账号库说它是按量号 ⇒ 按量号；其余是订阅号。
fn kind_of(a: &Turn<'_>, lib: &Library, account: &str) -> Kind {
    kind_with(a.row, lib, account)
}

fn kind_with(row: &dyn Fn(&str) -> Option<bool>, lib: &Library, account: &str) -> Kind {
    if row(account).is_some() || lib.find(account).is_some_and(|x| x.api) {
        Kind::Api
    } else {
        Kind::Sub
    }
}

/// [`Hop::reach`] 的答：接得上的是哪一种。
pub(super) enum Reach {
    Start,
    Api,
    Sub,
}

/// 中转按 [`Go`] 发：起会话的号走那张决策表；按量号照它那一行；订阅号换上它的令牌（上游 ＝ 这一家的默认上游）。
pub(crate) fn dispatch_go(
    table: &RoutingTable,
    upstreams: &super::Upstreams,
    mode: Mode,
    key: &RouteKey,
    names: &[&str],
    go: Go,
    act: &mut dyn FnMut(Destination<'_>),
) {
    let agent = key.seg1.as_str();
    match go {
        Go::Start => super::decide(table, upstreams, mode, key, names, act),
        Go::Api { account, body } => match table.lookup(agent, &account) {
            Some(row) => super::dispatch_auth(row, &account, body.as_deref(), act),
            None => super::decide(table, upstreams, mode, key, names, act),
        },
        Go::Sub {
            account,
            token,
            body,
        } => match (
            upstreams.of(agent, names),
            super::auth_header_of(creds_core::store::AuthStyle::Bearer),
        ) {
            (Some(upstream), Some((name, prefix))) => {
                let value = super::header_value(prefix, &token);
                act(Destination::Substitute {
                    upstream,
                    auth: AuthSwap {
                        clear: super::headers_to_clear(),
                        write: Some((name, value.as_str())),
                    },
                    body: body.as_deref(),
                    tag: &account,
                });
            }
            _ => super::decide(table, upstreams, mode, key, names, act),
        },
        Go::Hold { reply } => act(Destination::Reply {
            status: reply.status,
            reason: "at-limit",
            headers: &reply.headers,
            body: &reply.body,
        }),
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/upstream_select/rotate_tests.rs"]
mod tests;

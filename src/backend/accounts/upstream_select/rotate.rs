//! 上游选择里「这个会话这一发该走哪个号」那一问：问轮换（钉在哪 · 满了换谁），拿那个号的令牌与身份，备好这一发的去处。
//!
//! - 判换不换、换谁归策略（[`crate::accounts::quota::rules::decide::turn`] 叫它）；只在节点上判、结论缓存到下一个节点（[`super::turn`]）。
//!   这里只备料（令牌 · 身份 · 改写后的请求体）。
//! - 起会话的号照今天走（[`Go::Start`]，那张决策表一个字节不变）；只有换到**别的号**时才由这里备料：
//!   订阅号 ⇒ 鉴权头换成那个号的访问令牌（账号域 `oauth` 给，快过期就续），请求体里账号身份那一格一起换；
//!   按量号 ⇒ 照它在 key 表里那一行，身份格换成空串。
//! - 账号库（哪些号 · 配置目录 · 是不是按量号）由宿主读好交进来（[`BookAt`]）：上游选择不认识账号库管理。
//! - 令牌只在备料时拿到、包在秘密类型里；拿不到只记原因码，那一句不带令牌。

use super::table::RoutingTable;
use crate::accounts::oauth::{self, TokenEndpoint};
use crate::accounts::quota::facts::Kind;
use crate::accounts::quota::ledger::Ledger;
use crate::accounts::quota::rotation::RotationStore;
use crate::accounts::quota::rules::{decide, Adoptions, AdoptionsAt};
use crate::accounts::quota::show::{self, LoginState};
use crate::agents::{Adapter, CredCell, IdentityCell, LoginFace, SwapHead};
use crate::platform::fs::{Own, Ro};
use crate::relay::{AuthSwap, Destination, Mode, RouteKey};
use acct_core::{AccountBook, ManifestAccount};
use creds_core::SecretKey;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 家目录 → 这台的号表（宿主给：全仓那一张 `acct_core::AccountBook`，账号 0 排第一个）。`None` ⇒ 没建账号库 / 读不懂（只能用起会话那个号）。
/// 号的路由段 · 配置目录 · 是不是按量号都照那一行现算（`route_id` · `config_dir` · `is_api_key`），这里不另存一张表。
pub type BookAt = Arc<dyn Fn(&Path) -> Option<AccountBook> + Send + Sync>;

/// 备好的这一发去处（自有值：令牌与改写后的请求体跨出判定、在上游选择的锁里交给中转）。
pub(crate) enum Go {
    /// 照起会话那个号走（那张决策表）。
    Start,
    /// 订阅号：照那一家的换号头（[`crate::agents::SwapHead`]）写上那个号的几个头；`body` 是身份格换好的整份请求体（`None` ＝ 请求体里没有那一格，原样发）。
    Sub {
        account: String,
        heads: Vec<(&'static str, HeadValue)>,
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

/// 换号头的一个值：令牌那一格到交中转那一刻才拼成明文（[`super::header_value`]）；凭据文件里点名的那一格原样。
pub(crate) enum HeadValue {
    Token {
        prefix: &'static str,
        token: SecretKey,
    },
    Plain(String),
}

/// ★ 订阅号那一发照那一家的换号头备料（唯一一处，不认哪一家）：`swap` 空 ⇒ 换号不成立、照起会话那个号走（[`Go::Start`]，
/// 不登录、不续令牌）；否则 `login` 交（访问令牌, 身份格换好的请求体），每个头照它的 [`CredCell`] 取值 ——
/// 访问令牌 ⇒ 令牌（带前缀）；凭据文件里点名的一格 ⇒ `field(那一格)`（带前缀），取不到 ⇒ 要登录。
pub(crate) fn swap_go(
    account: &str,
    swap: &[SwapHead],
    login: &mut dyn FnMut() -> Result<(SecretKey, Option<Vec<u8>>), Unready>,
    field: &dyn Fn(&str) -> Option<String>,
) -> Result<Go, Unready> {
    if swap.is_empty() {
        return Ok(Go::Start);
    }
    let (token, body) = login()?;
    let heads = swap
        .iter()
        .map(|h| {
            let v = match h.cell {
                CredCell::Access => HeadValue::Token {
                    prefix: h.prefix,
                    token: token.clone(),
                },
                CredCell::Field(f) => HeadValue::Plain(format!("{}{}", h.prefix, field(f)?)),
            };
            Some((h.name, v))
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(Unready::NeedsLogin)?;
    Ok(Go::Sub {
        account: account.to_string(),
        heads,
        body,
    })
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
    /// 那一家声明账号 0 不续不写，这个账号 0 的令牌又快过期 / 过期了（[`oauth::unrenewed_base`]）：不是要重新登录。
    BaseUntouched,
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

/// 这台此刻活着的会话（sid 集合）。判活住 history，由宿主递进来（生产那一份见 `faces::rotation_face`）。
pub(crate) type Live = Arc<dyn Fn() -> std::collections::BTreeSet<String> + Send + Sync>;

/// 需手动的铃：会话外那几项可能变了（续令牌没成 ⇒「需重新登录」可能进了清单）。铃住 history，由宿主递进来。
pub(crate) type Ring = Arc<dyn Fn() + Send + Sync>;

/// 宿主递进来的 history 那两口（上游选择不认识 history）：这台活着的会话 · 需手动的铃。
#[derive(Clone)]
pub struct HistoryPorts {
    pub live: Live,
    pub ring: Ring,
}

/// 请求体里的账号身份对着路由写明的那个号（[`Hop::speaks_for`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Speaks {
    /// 就是它的。
    Yes,
    /// 不是它的。
    No,
    /// 它盘上的身份读不到（没登录 · 文件不在）：说不出是不是它。
    Blank,
    /// 无从比（请求体里没有身份这一格 · 那一家没有登录面 · 没建账号库 · 它不是账号库里这一家的订阅号：
    /// 按量号的鉴权是 key，身份那一格说明不了用的是谁的额度）。
    Mute,
}

/// 读身份的缓存：`.claude.json` 可能很大，按那份文件的戳缓存（文件动了再读）。
type IdentityCache = Mutex<BTreeMap<PathBuf, (Option<(std::time::SystemTime, u64)>, String)>>;

/// 换号要的几样：轮换的账本 · 新会话载入哪份问谁 · 额度账 · 家目录 · 账号库 · 令牌端点（缺省用那一家登记的）。
pub struct Hop {
    pub store: Arc<RotationStore>,
    pub adoptions: Arc<dyn Adoptions>,
    pub quota: Arc<Ledger>,
    pub(crate) home: Option<PathBuf>,
    pub book: BookAt,
    pub(crate) token: Option<TokenEndpoint>,
    /// 这台此刻活着的会话。
    pub live: Live,
    /// 需手动的铃。
    ring: Ring,
    identities: IdentityCache,
    /// 每个会话上一判的结论，缓存到下一个节点（[`super::turn`]）。
    pub(super) cache: Mutex<super::turn::Cache>,
    /// 认哪几家（生产那一份 ＝ [`crate::agents::REGISTRY`]；判据经 [`Self::among`] 喂合成的）。
    registry: &'static [Adapter],
}

impl Hop {
    /// `token`：续令牌发到哪（`None` ＝ 那一家登记的端点；判据一律给一个本机的假端点）。
    /// `adoptions`：新会话载入哪份问谁（宿主交进来，照这份 `store` 给出那一位）。
    pub fn new(
        store: Arc<RotationStore>,
        adoptions: &AdoptionsAt,
        quota: Arc<Ledger>,
        home: Option<PathBuf>,
        book: BookAt,
        history: HistoryPorts,
        token: Option<TokenEndpoint>,
    ) -> Self {
        Self {
            adoptions: adoptions(&store),
            store,
            quota: quota.clone(),
            home,
            book,
            token,
            cache: Mutex::new(super::turn::Cache::new(quota.nodes())),
            live: history.live,
            ring: history.ring,
            identities: Mutex::new(BTreeMap::new()),
            registry: crate::agents::REGISTRY,
        }
    }

    /// 换一张注册表认家（判据喂替身家用；生产那一份恒是 [`crate::agents::REGISTRY`]）。
    pub fn among(mut self, registry: &'static [Adapter]) -> Self {
        self.registry = registry;
        self
    }

    /// 路由名 `agent` 那一家的默认上游（照 [`Self::among`] 那张注册表；认不出 ⇒ `None`）。按路由名取的那几格都从它取。
    pub(super) fn upstream(&self, agent: &str) -> Option<&'static crate::agents::DefaultUpstream> {
        self.registry
            .iter()
            .filter_map(|a| a.upstream.as_ref())
            .find(|u| u.route_id == agent)
    }

    /// 那一家的订阅号登录格式（没有 ⇒ 这一家换不了号）。
    pub(super) fn login_face(&self, agent: &str) -> Option<LoginFace> {
        self.upstream(agent).and_then(|u| u.login)
    }

    /// 那一家换号时写的那几个头（没登记 ⇒ 空：换号不成立）。
    fn swap_heads(&self, agent: &str) -> &'static [SwapHead] {
        self.upstream(agent).map_or(&[], |u| u.swap_heads)
    }

    /// 那一家的窗口名 → 语义位。
    pub(super) fn slot(&self, agent: &str) -> impl Fn(&str) -> Option<&'static str> {
        let f = self.upstream(agent).and_then(|u| u.window_slot);
        move |w: &str| f.and_then(|f| f(w))
    }

    /// 那一家的号什么时候由我方续（`base` ⇒ 账号 0）：续期条件照这张注册表里那一家的账号面。
    fn when(&self, agent: &str, base: bool) -> oauth::When {
        let kind = crate::agents::kind_of_route_among(self.registry, agent);
        oauth::When {
            renew: crate::agents::renew_among(self.registry, kind),
            ..oauth::When::here(kind, base)
        }
    }

    /// 号表里一个号是哪一家的号（[`decide::family_among`]，照这张注册表）。
    fn family(&self, m: &ManifestAccount) -> Option<&'static str> {
        decide::family_among(self.registry, m)
    }

    /// 号表里 `agent` 那一家号名 `id` 的那一条（[`decide::in_family_among`]，照这张注册表）。
    fn in_family<'a>(
        &self,
        lib: &'a AccountBook,
        agent: &str,
        id: &str,
    ) -> Option<&'a ManifestAccount> {
        decide::in_family_among(self.registry, lib, agent, id)
    }

    /// 号的种类：key 表里有行、或账号库说它是按量号 ⇒ 按量号；其余是订阅号。
    fn kind_with(
        &self,
        row: &dyn Fn(&str) -> Option<bool>,
        agent: &str,
        lib: &AccountBook,
        account: &str,
    ) -> Kind {
        if row(account).is_some()
            || self
                .in_family(lib, agent, account)
                .is_some_and(ManifestAccount::is_api_key)
        {
            Kind::Api
        } else {
            Kind::Sub
        }
    }

    /// ★ 生产那一份装配（唯一一处：中转那一路 · 帧面那一份都调它）：轮换账本与额度账住数据目录 `data`
    /// （`rotation::path_under` · `ledger::path_under`；推不出 ⇒ `None`，两份都不落盘）·
    /// 家目录 · 号表怎么读 · 新会话载入哪份问谁 · history 那两口都由宿主给；令牌端点用那一家登记的。
    pub fn here(
        data: Option<&Path>,
        home: Option<PathBuf>,
        book: BookAt,
        adoptions: &AdoptionsAt,
        history: HistoryPorts,
    ) -> Self {
        Self::new(
            Arc::new(RotationStore::at(
                data.map(crate::accounts::quota::rotation::path_under),
            )),
            adoptions,
            Arc::new(Ledger::at(
                data.map(crate::accounts::quota::ledger::path_under),
            )),
            home,
            book,
            history,
            None,
        )
    }

    /// 这台的号表；没建账号库 / 读不懂 / 没有家目录 ⇒ `None`（只能用起会话那个号）。
    pub(crate) fn book_here(&self) -> Option<AccountBook> {
        self.home.as_deref().and_then(|h| (self.book)(h))
    }

    /// [`Self::book_here`]，没有就是空表（列事实、拼一个号的样子用；判换不换先看 `book_here`）。
    pub fn book(&self) -> AccountBook {
        self.book_here().unwrap_or_default()
    }

    /// 轮换里的号：号表里每个号的路由段，照表里的先后；路由段推不出来的那一行不算。
    /// 账号 0（不设配置目录的那一份 `~/.claude`，账号库写清单时合成那一条）不算：它不经选号（`acct_core::account_selectable`），
    /// 往往与库里某个号是同一份订阅；会话恰好起在它上面时，开会话的号那一条由调用方另拼（[`Self::fact_one`]）。
    /// 每个号带着它是哪一家的号（[`Self::family`]；说不出是哪一家的那一行不算）：号按（家, 号名）认。
    fn ids<'a>(
        &'a self,
        lib: &'a AccountBook,
    ) -> impl Iterator<Item = (&'static str, String)> + 'a {
        lib.accounts
            .iter()
            .filter_map(|m| Some((self.family(m)?, m.route_id()?)))
            .filter(|(_, id)| id != acct_core::BARE_ACCOUNT)
    }

    /// 轮换里的号名（[`Self::ids`]，几家同名的只列一次）：新策略的号列表 · 清掉库里没有的号都照它。
    pub fn roster(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (_, id) in self.ids(&self.book()) {
            if !out.contains(&id) {
                out.push(id);
            }
        }
        out
    }

    /// 号表里不是 `agent` 那一家的号（照号的先后），各照自己那一家拼成判看的那一份。交给判的那一排接在本家的号后面，
    /// 只为让一判认出策略选了别家的号（`decide::turn`）；判 · 顺序 · 推演都只看本家的号。key 表那一行只认会话那一家，别家的号不问它。
    pub fn elsewhere(&self, agent: &str, now: u64) -> Vec<crate::accounts::quota::rules::Acct> {
        let lib = self.book();
        let tz = crate::common::time::Tz::default();
        self.ids(&lib)
            .filter(|(a, _)| *a != agent)
            .map(|(a, id)| {
                crate::accounts::quota::rules::Acct::of_fact(
                    a,
                    &self.fact_one(a, &lib, &id, &|_| None, now, &tz),
                )
            })
            .collect()
    }

    fn endpoint(&self, face: &LoginFace) -> Option<TokenEndpoint> {
        match &self.token {
            Some(t) => Some(TokenEndpoint {
                base: t.base.clone(),
                rest: t.rest.clone(),
                deadline: t.deadline,
            }),
            None => TokenEndpoint::of(face, oauth::TOKEN_DEADLINE),
        }
    }

    /// 号 → （配置目录, 账号 0 时的家目录）；账号 0 的配置目录照那一家账号面那一格（`AccountsFace::shared_root`）。
    fn dir_of(
        &self,
        agent: &str,
        lib: &AccountBook,
        id: &str,
    ) -> Option<(PathBuf, Option<PathBuf>)> {
        let home = self.home.as_deref()?;
        // 起会话时没说是哪个号（`_`）不是哪一份登录：不拿默认配置目录（账号 0）那份顶。
        match self.in_family(lib, agent, id)?.config_dir.as_deref() {
            Some(d) => Some((PathBuf::from(d), None)),
            None => {
                let kind = crate::agents::kind_of_route_among(self.registry, agent);
                let base = crate::agents::accounts_face_among(self.registry, kind)?.shared_root;
                Some((base(home), Some(home.to_path_buf())))
            }
        }
    }

    /// ★ 这一发实际在用哪个号：请求体里的账号身份恰好对上账号库里**一个**订阅号 ⇒ 那个号的 id
    /// （账号 0 ⇒ `0`）；一个都没对上 · 对上不止一个 · 请求体里没有这一格 · 没建账号库 ⇒ `None`（不猜）。
    /// 每一发都比一次（身份按文件戳缓存）：会话里 `/login` 换了号，下一发就认成新号。
    pub(crate) fn recognize(&self, agent: &str, body: &[u8]) -> Option<String> {
        let face = self.login_face(agent)?;
        let uuid = (face.identity_of_body)(body)?;
        let lib = self.book();
        let mut hit = lib
            .accounts
            .iter()
            .filter(|x| !x.is_api_key() && self.family(x) == Some(agent))
            .filter_map(ManifestAccount::route_id)
            .filter(|id| self.is_identity(&face, agent, &lib, id, &uuid) == Some(true));
        let one = hit.next()?;
        hit.next().is_none().then_some(one)
    }

    /// ★ 请求体里的账号身份是不是路由写明的号 `route` 的（[`Speaks`]）。
    pub(crate) fn speaks_for(&self, agent: &str, body: &[u8], route: &str) -> Speaks {
        let Some(face) = crate::agents::route_named(agent).and_then(crate::agents::login_of) else {
            return Speaks::Mute;
        };
        let Some(uuid) = (face.identity_of_body)(body) else {
            return Speaks::Mute;
        };
        let Some(lib) = self.book_here() else {
            return Speaks::Mute;
        };
        if self
            .in_family(&lib, agent, route)
            .is_none_or(ManifestAccount::is_api_key)
        {
            return Speaks::Mute;
        }
        match self.is_identity(&face, agent, &lib, route, &uuid) {
            Some(true) => Speaks::Yes,
            Some(false) => Speaks::No,
            None => Speaks::Blank,
        }
    }

    /// 号 `id` 盘上登录的账号身份是不是 `uuid`；盘上的身份读不到（没登录 · 文件不在）⇒ `None`。
    fn is_identity(
        &self,
        face: &LoginFace,
        agent: &str,
        lib: &AccountBook,
        id: &str,
        uuid: &str,
    ) -> Option<bool> {
        let (dir, base) = self.dir_of(agent, lib, id)?;
        Some(self.identity(face, &dir, base.as_deref())? == uuid)
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
        lib: &AccountBook,
        account: &str,
        now: u64,
    ) -> Result<(SecretKey, String), Unready> {
        let face = self.login_face(agent).ok_or(Unready::NeedsLogin)?;
        let (dir, base) = self
            .dir_of(agent, lib, account)
            .ok_or(Unready::NeedsLogin)?;
        // 缺省令牌端点写不出也照样问：凭据里记了出处的号照出处续；两样都没有 ⇒ 那一句照实进日志（`Unusable::CannotRenew`）。
        let ep = self.endpoint(&face);
        // 那一家什么时候让我方续（只在没有活会话用这个号时续的那一家，有会话在用就留给它自己；账号 0 照那一家声明的续或不续）。
        let when = self.when(agent, base.is_some());
        let token = oauth::access_token(
            own,
            &dir,
            &face,
            ep.as_ref(),
            now.saturating_mul(1000),
            account,
            when,
        )
        .map_err(|u| {
            // 续不上已成那个号的事实（`oauth::missed`）：发一件节点，判照新的事实重来；不算号的事实的（`Unusable::is_account_fact`）不发。
            if u.is_account_fact() {
                self.quota
                    .notify(super::super::quota::nodes::Source::Login, None);
                // 「需重新登录」那一项可能进了需手动清单。
                (self.ring)();
            }
            match u {
                oauth::Unusable::BaseUntouched => Unready::BaseUntouched,
                _ => Unready::NeedsLogin,
            }
        })?;
        let who = self
            .identity(&face, &dir, base.as_deref())
            .ok_or(Unready::NeedsLogin)?;
        Ok((token, who))
    }

    /// ★ 一个号此刻接不接得上（只读，不续令牌）—— 帧面（[`Self::view`]）只问它，真换号（[`Self::prepare`]）先问它再拿令牌，
    /// 两边同一处判：起会话的号 ⇒ 接得上；按量号 ⇒ key 表里那一行接得上；订阅号 ⇒ 只认 [`Self::sub_login`] 那一答：
    /// 登录正常、又不在等续令牌再试的那一刻，才接得上。
    pub(super) fn reach(
        &self,
        a: &Turn<'_>,
        lib: &AccountBook,
        account: &str,
    ) -> Result<Reach, Unready> {
        if account == a.start {
            return Ok(Reach::Start);
        }
        match self.kind_with(a.row, a.agent, lib, account) {
            Kind::Api if (a.row)(account) == Some(true) => Ok(Reach::Api),
            Kind::Api => Err(Unready::NeedsKey),
            Kind::Sub => match self.sub_login_of(a.agent, lib, account, a.now) {
                show::Facts {
                    login: LoginState::Ok,
                    retry_at: None,
                    ..
                } => Ok(Reach::Sub),
                show::Facts {
                    login: LoginState::BaseUntouched,
                    ..
                } => Err(Unready::BaseUntouched),
                _ => Err(Unready::NeedsLogin),
            },
        }
    }

    /// 账号库里 `agent` 那一家的订阅号 `account` 此刻的登录（[`Self::sub_login`]；那一家没有登录面 · 号不在账号库里 ⇒ 要登录）。
    fn sub_login_of(&self, agent: &str, lib: &AccountBook, account: &str, now: u64) -> show::Facts {
        let Some(face) = self.login_face(agent) else {
            return sub_facts(LoginState::NeedsLogin, None);
        };
        let Some((dir, base)) = self.dir_of(agent, lib, account) else {
            return sub_facts(LoginState::NeedsLogin, None);
        };
        let when = self.when(agent, base.is_some());
        self.sub_login(&face, &dir, base.as_deref(), when, now)
    }

    /// ★ 一个订阅号此刻的登录（只读：不续令牌、不拿锁、不写）—— 帧面（[`Self::show_facts`]）、事实（[`Self::fact_one`]）与接不接得上（[`Self::reach`]）
    /// 同问这一处：盘上没有登录 · 读不出账号身份 ⇒ 要登录；那一家不归我方续的账号 0、令牌又快过期 / 过期了（[`oauth::unrenewed_base`]，同拿令牌那一判）
    /// ⇒ [`LoginState::BaseUntouched`]；续令牌被拒过 ⇒ 要登录；一时没续上 ⇒ 正常、带着几点再试（到点之前用不了）；其余 ⇒ 正常。
    fn sub_login(
        &self,
        face: &LoginFace,
        dir: &Path,
        base: Option<&Path>,
        when: oauth::When,
        now: u64,
    ) -> show::Facts {
        if !matches!(
            oauth::store::read(dir, face),
            oauth::store::Read::Present(_)
        ) || self.identity(face, dir, base).is_none()
        {
            return sub_facts(LoginState::NeedsLogin, None);
        }
        if oauth::unrenewed_base(dir, face, when, now.saturating_mul(1000)) {
            return sub_facts(LoginState::BaseUntouched, None);
        }
        match oauth::missed::missed(dir, face, now) {
            Some(oauth::missed::Unrenewed::Manual) => sub_facts(LoginState::NeedsLogin, None),
            Some(oauth::missed::Unrenewed::RetryAt(t)) => sub_facts(LoginState::Ok, Some(t)),
            None => sub_facts(LoginState::Ok, None),
        }
    }

    /// 备好走 `account` 的这一发（起会话的号 ⇒ [`Go::Start`]）。
    pub(super) fn prepare(
        &self,
        own: &Own,
        a: &Turn<'_>,
        lib: &AccountBook,
        account: &str,
    ) -> Result<Go, Unready> {
        let rewrite = |who: &str| -> Result<Option<Vec<u8>>, Unready> {
            let face = self.login_face(a.agent).ok_or(Unready::UnsureBody)?;
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
                let swap = self.swap_heads(a.agent);
                let mut login = || {
                    let (token, who) = self.login(own, a.agent, lib, account, a.now)?;
                    Ok((token, rewrite(&who)?))
                };
                let field = |f: &str| self.cred_field(a.agent, lib, account, f);
                swap_go(account, swap, &mut login, &field)
            }
        }
    }

    /// 这个号凭据文件里那一家登录那一节点名的一格（串）；读不到 · 不是串 ⇒ `None`。
    fn cred_field(
        &self,
        agent: &str,
        lib: &AccountBook,
        account: &str,
        field: &str,
    ) -> Option<String> {
        let face = self.login_face(agent)?;
        let (dir, _) = self.dir_of(agent, lib, account)?;
        match oauth::store::read(&dir, &face) {
            oauth::store::Read::Present(m) => m
                .get(face.section)?
                .get(field)?
                .as_str()
                .map(str::to_string),
            _ => None,
        }
    }

    /// 这个号上一次续令牌没成、此刻还作数（[`oauth::missed`]）。
    fn missed(
        &self,
        agent: &str,
        lib: &AccountBook,
        account: &str,
        now: u64,
    ) -> Option<oauth::missed::Unrenewed> {
        let face = self.login_face(agent)?;
        let (dir, _) = self.dir_of(agent, lib, account)?;
        oauth::missed::missed(&dir, &face, now)
    }

    /// 账号库里续令牌被拒、要重新登录的那几个号（需手动清单列它们）。
    pub fn needs_relogin(&self, agent: &str, now: u64) -> Vec<String> {
        let lib = self.book();
        lib.accounts
            .iter()
            .filter(|a| !a.is_api_key() && self.family(a) == Some(agent))
            .filter_map(ManifestAccount::route_id)
            .filter(|id| {
                self.missed(agent, &lib, id, now) == Some(oauth::missed::Unrenewed::Manual)
            })
            .collect()
    }

    /// 一个号显示态与事实要的几格：种类 · 登录（只读，不续令牌；续令牌被拒过 ⇒ 要重新登录）· 续令牌一时没成时几点再试（订阅号照 [`Self::sub_login`]）。
    pub fn show_facts(
        &self,
        agent: &str,
        lib: &AccountBook,
        account: &str,
        row: &dyn Fn(&str) -> Option<bool>,
        now: u64,
    ) -> show::Facts {
        match self.kind_with(row, agent, lib, account) {
            // 起会话时没说是哪个号：是哪一份登录说不准 ⇒ 登录照起它的那个会话（它自己发得出去）。
            Kind::Sub if account == relay_route_core::UNDECLARED_ACCOUNT_SEGMENT => {
                sub_facts(LoginState::Ok, None)
            }
            Kind::Sub => self.sub_login_of(agent, lib, account, now),
            Kind::Api => show::Facts {
                kind: Kind::Api,
                login: if row(account) == Some(true) {
                    LoginState::Ok
                } else {
                    LoginState::NeedsKey
                },
                retry_at: None,
            },
        }
    }

    /// ★ 这台这一家每个号此刻的事实（`rotation-facts` 与判 · 推演同用这一处；只读：不续令牌、不记）。号照账号库的先后。
    pub fn facts(
        &self,
        agent: &str,
        row: &dyn Fn(&str) -> Option<bool>,
        now: u64,
        tz: &crate::common::time::Tz,
    ) -> Vec<crate::accounts::quota::facts::Fact> {
        let lib = self.book();
        self.ids(&lib)
            .filter(|(a, _)| *a == agent)
            .map(|(_, id)| self.fact_one(agent, &lib, &id, row, now, tz))
            .collect()
    }

    /// 一个号的事实（不在账号库里的号也拼得出：开会话用的号没说是哪个号时）。
    pub fn fact_one(
        &self,
        agent: &str,
        lib: &AccountBook,
        id: &str,
        row: &dyn Fn(&str) -> Option<bool>,
        now: u64,
        tz: &crate::common::time::Tz,
    ) -> crate::accounts::quota::facts::Fact {
        let slot = self.slot(agent);
        let when = |t: u64| {
            crate::common::time::fmt_at(
                i64::try_from(t).unwrap_or(i64::MAX),
                i64::try_from(now).unwrap_or(i64::MAX),
                tz,
            )
        };
        let o = self.quota.entry(agent, id);
        let rate = |name: &str| o.as_ref().and_then(|o| o.rate(name, now));
        let per_pct = |name: &str| o.as_ref().and_then(|o| o.tokens_per_pct(name, now));
        let reading = o
            .as_ref()
            .map(|o| crate::accounts::quota::facts::current(o, now).reading);
        let shown = self.show_facts(agent, lib, id, row, now);
        crate::accounts::quota::facts::fact_of(
            id,
            reading.as_ref(),
            shown.kind,
            shown.login,
            shown.retry_at,
            now,
            &slot,
            &crate::accounts::quota::facts::Measured {
                rate: &rate,
                per_pct: &per_pct,
            },
            &when,
        )
    }

    /// ★ 号换窗的历史交给策略的那两样（判 · 试跑拼完 `Acct` 同问这一处）：周窗重置时剩几点 · 开窗取整往哪边；
    /// 照 [`crate::accounts::quota::facts::pick`] 此刻挑的周窗 · 5 小时窗的原名问额度账，账上没这个号 ⇒ 照旧空着。
    pub fn with_past(
        &self,
        agent: &str,
        all: &mut [crate::accounts::quota::rules::Acct],
        now: u64,
    ) {
        let slot = self.slot(agent);
        for x in all {
            let Some(o) = self.quota.entry(agent, &x.id) else {
                continue;
            };
            let reading = crate::accounts::quota::facts::current(&o, now).reading;
            let name = |s: &str| {
                crate::accounts::quota::facts::pick(&reading, s, now, &slot).map(|w| w.name.clone())
            };
            x.past = crate::accounts::quota::rules::Past {
                weekly_left_at_reset: name(cmd_wire::quota::SLOT_7D)
                    .map(|n| o.weekly_left_at_reset(&n))
                    .unwrap_or_default(),
                grid_up: name(cmd_wire::quota::SLOT_5H)
                    .and_then(|n| o.window_grid(&n, crate::accounts::quota::rules::WINDOW_SECS[0])),
            };
        }
    }
}

/// 号表里一个号是哪一家的号（路由名）：生产注册表上的 [`decide::family_among`]。
pub fn family_of(m: &ManifestAccount) -> Option<&'static str> {
    decide::family_among(crate::agents::REGISTRY, m)
}

/// 订阅号的那几格（[`Hop::sub_login`] 的答）。
fn sub_facts(login: LoginState, retry_at: Option<u64>) -> show::Facts {
    show::Facts {
        kind: Kind::Sub,
        login,
        retry_at,
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
            heads,
            body,
        } => match upstreams.of(agent, names) {
            Some(upstream) => {
                let values: Vec<(&'static str, String)> = heads
                    .iter()
                    .map(|(name, v)| {
                        let value = match v {
                            HeadValue::Token { prefix, token } => {
                                super::header_value(prefix, token)
                            }
                            HeadValue::Plain(s) => s.clone(),
                        };
                        (*name, value)
                    })
                    .collect();
                let write: Vec<(&'static str, &str)> =
                    values.iter().map(|(n, v)| (*n, v.as_str())).collect();
                // 剥的名单：鉴权头全集并上这一发要写的几个头名（同名的下游头一条不留）。
                let mut clear: Vec<&'static str> = super::headers_to_clear().to_vec();
                for (n, _) in &heads {
                    if !clear.iter().any(|c| c.eq_ignore_ascii_case(n)) {
                        clear.push(n);
                    }
                }
                act(Destination::Substitute {
                    upstream,
                    auth: AuthSwap {
                        clear: &clear,
                        write: &write,
                    },
                    body: body.as_deref(),
                    tag: &account,
                });
            }
            None => super::decide(table, upstreams, mode, key, names, act),
        },
        Go::Hold { reply } => act(Destination::Reply {
            status: reply.status,
            reason: "at-limit",
            headers: &reply.headers,
            body: &reply.body,
        }),
    }
}

// 只给测试用的那几样（号表里的一行）：本包判据与上层判据（`test-support`）共用。
#[cfg(any(test, feature = "test-support"))]
#[path = "../../../../tests/backend/accounts/upstream_select/rotate_support.rs"]
#[expect(
    clippy::disallowed_methods,
    reason = "测试档夹具在沙箱家目录里写替身家的凭据（同各包测试段放开碰盘）"
)]
pub mod test_support;

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/upstream_select/rotate_tests.rs"]
mod tests;

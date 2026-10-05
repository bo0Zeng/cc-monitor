//! 上游选择里「这个会话这一发该走哪个号」那一问：问轮换（钉在哪 · 满了换谁），拿那个号的令牌与身份，备好这一发的去处。
//!
//! - 判换不换、换谁只在 [`crate::accounts::quota::decide`]；这里只备料（令牌 · 身份 · 改写后的请求体）、钉号、记一条。
//! - 起会话的号照今天走（[`Go::Start`]，那张决策表一个字节不变）；只有换到**别的号**时才由这里备料：
//!   订阅号 ⇒ 鉴权头换成那个号的访问令牌（账号域 `oauth` 给，快过期就续），请求体里账号身份那一格一起换；
//!   按量号 ⇒ 照它在 key 表里那一行，身份格换成空串。
//! - 账号库（哪些号 · 配置目录 · 是不是按量号）由宿主读好交进来（[`LibraryRead`]）：上游选择不认识账号库管理。
//! - 令牌只在备料时拿到、包在秘密类型里；拿不到只记原因码，那一句不带令牌。

use super::table::RoutingTable;
use crate::accounts::oauth::{self, TokenEndpoint};
use crate::accounts::quota::decide::{self, Facts, Kind, Verdict};
use crate::accounts::quota::ledger::Ledger;
use crate::accounts::quota::rotation::{
    self, AccountAt, AccountCell, Blocked, Book, InPlace, RotationStore, SessionEntry,
    SessionRotation, SessionRotationState, SwitchRecord, SwitchWhy, Unready,
};
use crate::agents::{IdentityCell, LoginFace, QuotaReading};
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
type IdentityCache = Mutex<BTreeMap<PathBuf, (Option<(std::time::SystemTime, u64)>, String)>>;

/// 换号要的几样：轮换的账本 · 额度账 · 家目录 · 账号库 · 令牌端点（缺省用那一家登记的）。
pub(crate) struct Hop {
    pub(crate) store: Arc<RotationStore>,
    pub(crate) quota: Arc<Ledger>,
    pub(crate) home: Option<PathBuf>,
    pub(crate) library: LibraryRead,
    pub(crate) token: Option<TokenEndpoint>,
    identities: IdentityCache,
}

impl Hop {
    /// `token`：续令牌发到哪（`None` ＝ 那一家登记的端点；判据一律给一个本机的假端点）。
    pub(crate) fn new(
        store: Arc<RotationStore>,
        quota: Arc<Ledger>,
        home: Option<PathBuf>,
        library: LibraryRead,
        token: Option<TokenEndpoint>,
    ) -> Self {
        Self {
            store,
            quota,
            home,
            library,
            token,
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
        match &lib.find(id)?.dir {
            Some(d) => Some((d.clone(), None)),
            None => Some(((face.base_dir)(home), Some(home.to_path_buf()))),
        }
    }

    fn identity(&self, face: &LoginFace, dir: &Path, base: Option<&Path>) -> Option<String> {
        let file = (face.identity_file)(dir, base);
        let stamp = std::fs::metadata(&file)
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
        let token =
            oauth::access_token(&dir, &face, &ep, now.saturating_mul(1000)).map_err(|u| {
                tracing::warn!("[rotate] {}", u.said(account));
                Unready::NeedsLogin
            })?;
        let who = self
            .identity(&face, &dir, base.as_deref())
            .ok_or(Unready::NeedsLogin)?;
        Ok((token, who))
    }

    /// 备好走 `account` 的这一发（起会话的号 ⇒ [`Go::Start`]）。
    fn prepare(&self, a: &Turn<'_>, lib: &Library, account: &str) -> Result<Go, Unready> {
        if account == a.start {
            return Ok(Go::Start);
        }
        let rewrite = |who: &str| -> Result<Option<Vec<u8>>, Unready> {
            let face = crate::agents::login_of(a.agent).ok_or(Unready::UnsureBody)?;
            match (face.rewrite_identity)(a.body, who) {
                IdentityCell::Absent => Ok(None),
                IdentityCell::Rewritten(b) => Ok(Some(b)),
                IdentityCell::Unsure => Err(Unready::UnsureBody),
            }
        };
        if kind_of(a, lib, account) == Kind::Api {
            if (a.row)(account) != Some(true) {
                return Err(Unready::NeedsKey);
            }
            return Ok(Go::Api {
                account: account.to_string(),
                body: rewrite("")?,
            });
        }
        let (token, who) = self.login(a.agent, lib, account, a.now)?;
        Ok(Go::Sub {
            account: account.to_string(),
            token,
            body: rewrite(&who)?,
        })
    }

    /// 这一家有可换的订阅号登录、这一发带着会话 id ⇒ 这个会话在这台的轮换状态（第一次看见 / 换了起它的号 ⇒ 记下）。
    fn entry(&self, a: &Turn<'_>) -> Option<(Book, SessionEntry)> {
        if a.sid.is_empty() || crate::agents::login_of(a.agent).is_none() {
            return None;
        }
        let mut book = self.store.now();
        let known = book
            .sessions
            .get(a.sid)
            .is_some_and(|s| s.start == a.start && s.agent == a.agent);
        if !known {
            match rotation::relay_change(&self.store, |b| {
                b.saw(a.sid, a.agent, a.start, a.now);
                b.clone()
            }) {
                Ok(b) => book = b,
                Err(e) => {
                    tracing::warn!("[rotate] {e}");
                    return None;
                }
            }
        }
        let s = book.sessions.get(a.sid)?.clone();
        Some((book, s))
    }

    /// 判一次 ⇒ 结论 ＋ 判的时候备好的那一发（换成了才有）。
    #[allow(clippy::too_many_arguments)]
    fn judge(
        &self,
        a: &Turn<'_>,
        lib: &Library,
        book: &Book,
        s: &SessionEntry,
        current: &str,
        heard: Option<&QuotaReading>,
        tried: &[String],
    ) -> (Verdict, Option<Go>) {
        let rot = book.rotation_of(s);
        let pool = rot.pool(&s.start);
        let slot = crate::agents::window_slot_of(a.agent);
        let seen = |x: &str| self.quota.entry(a.agent, x).map(|o| o.reading);
        let kind = |x: &str| kind_of(a, lib, x);
        let slot_of = |w: &str| slot.and_then(|f| f(w));
        let f = Facts {
            pool: &pool,
            when: rot.when,
            current,
            now: a.now,
            heard,
            seen: &seen,
            kind: &kind,
            slot: &slot_of,
            tried,
        };
        let mut ready: Option<Go> = None;
        let v = decide::decide(&f, &mut |x| {
            self.prepare(a, lib, x).map(|g| {
                ready = Some(g);
            })
        });
        (v, ready)
    }

    fn pin(&self, sid: &str, rec: SwitchRecord, skipped: &[(String, Unready)]) {
        if let Err(e) = rotation::relay_change(&self.store, |b| b.pin(sid, rec, skipped)) {
            tracing::warn!("[rotate] {e}");
        }
    }

    fn stuck(&self, sid: &str, rec: SwitchRecord, skipped: &[(String, Unready)]) {
        if let Err(e) = rotation::relay_change(&self.store, |b| b.note_stuck(sid, rec, skipped)) {
            tracing::warn!("[rotate] {e}");
        }
    }

    /// 结论落账：换成了 ⇒ 钉住、记一条，交出备好的那一发；没换成 ⇒ 记该记的，`None`。
    fn settle(&self, a: &Turn<'_>, current: &str, v: Verdict, ready: Option<Go>) -> Option<Go> {
        match v {
            Verdict::Stay => None,
            Verdict::Switch {
                to,
                why,
                from_resets_at,
                skipped,
            } => {
                let rec = SwitchRecord {
                    at: a.now,
                    from: current.to_string(),
                    to,
                    why,
                    from_resets_at,
                };
                self.pin(a.sid, rec, &skipped);
                ready
            }
            Verdict::Stuck {
                why,
                from_resets_at,
                skipped,
            } => {
                let rec = SwitchRecord {
                    at: a.now,
                    from: current.to_string(),
                    to: current.to_string(),
                    why,
                    from_resets_at,
                };
                self.stuck(a.sid, rec, &skipped);
                None
            }
        }
    }

    /// ★ 发之前：这个会话此刻钉在哪个号；额度账上它满着（或到阈值 · 超额在兜）⇒ 先按轮换换好再发。
    /// 钉着的号接不上了（登录过期续不上…）⇒ 回到起会话的号、记一条为什么。
    pub(crate) fn steer(&self, a: &Turn<'_>) -> Go {
        let Some((book, s)) = self.entry(a) else {
            return Go::Start;
        };
        let lib = self.library();
        let (v, ready) = self.judge(a, &lib, &book, &s, &s.current, None, &[]);
        if let Some(go) = self.settle(a, &s.current, v, ready) {
            return go;
        }
        if s.current == s.start {
            return Go::Start;
        }
        match self.prepare(a, &lib, &s.current) {
            Ok(go) => go,
            Err(reason) => {
                let rec = SwitchRecord {
                    at: a.now,
                    from: s.current.clone(),
                    to: s.start.clone(),
                    why: SwitchWhy::Skipped {
                        account: s.current.clone(),
                        reason,
                    },
                    from_resets_at: None,
                };
                self.pin(a.sid, rec, &[]);
                Go::Start
            }
        }
    }

    /// ★ 回包被拒：问轮换要下一个号（`tried` ＝ 这一发已经发过的号，`heard_from` 是答这一发的那个）。
    /// 别的请求刚把这个会话换走了 ⇒ 跟上它（不另记）。没得换 ⇒ `None`（中转原样交回拒绝）。
    pub(crate) fn on_refused(
        &self,
        a: &Turn<'_>,
        heard_from: &str,
        heard: &QuotaReading,
        tried: &[String],
    ) -> Option<Go> {
        let (book, s) = self.entry(a)?;
        let lib = self.library();
        if s.current != heard_from && !tried.contains(&s.current) {
            if let Ok(go) = self.prepare(a, &lib, &s.current) {
                return Some(go);
            }
        }
        let (v, ready) = self.judge(a, &lib, &book, &s, heard_from, Some(heard), tried);
        self.settle(a, heard_from, v, ready)
    }

    /// 订阅号「拿得到登录」的只读那一版（不续令牌）：配置目录在 · 有凭据文件 · 读得出身份。
    fn login_on_disk(&self, agent: &str, lib: &Library, account: &str) -> Result<(), Unready> {
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
            .map(|_| ())
            .ok_or(Unready::NeedsLogin)
    }

    /// 帧面「现在就换」：目标号此刻接不接得上（订阅号要真拿到令牌 —— 快过期就续，续不上照实报）。
    pub(crate) fn check_target(
        &self,
        agent: &str,
        start: &str,
        target: &str,
        row: &dyn Fn(&str) -> Option<bool>,
        now: u64,
    ) -> Result<(), Unready> {
        let a = Turn {
            agent,
            start,
            sid: "",
            body: b"",
            row,
            now,
        };
        self.prepare(&a, &self.library(), target).map(|_| ())
    }

    /// 帧面：一个会话的那一份（「账号」格 ＋ 下一个 · 卡住 · 可用按量号）。只读：不记、不续令牌。
    pub(crate) fn view(
        &self,
        book: &Book,
        sid: &str,
        row: &dyn Fn(&str) -> Option<bool>,
        now: u64,
    ) -> SessionRotationState {
        let lib = self.library();
        let Some(s) = book.sessions.get(sid) else {
            return SessionRotationState::Absent {
                in_place: InPlace::NoRelay,
            };
        };
        let in_place = if crate::agents::login_of(&s.agent).is_none() {
            InPlace::AgentHasNoAccounts
        } else if !lib.enabled {
            InPlace::MachineNotMulti
        } else {
            InPlace::Ok
        };
        let a = Turn {
            agent: &s.agent,
            start: &s.start,
            sid,
            body: b"",
            row,
            now,
        };
        let rot = book.rotation_of(s);
        let pool = rot.pool(&s.start);
        let slot = crate::agents::window_slot_of(&s.agent);
        let seen = |x: &str| self.quota.entry(&s.agent, x).map(|o| o.reading);
        let kind = |x: &str| kind_of(&a, &lib, x);
        let slot_of = |w: &str| slot.and_then(|f| f(w));
        let f = Facts {
            pool: &pool,
            when: rot.when,
            current: &s.current,
            now,
            heard: None,
            seen: &seen,
            kind: &kind,
            slot: &slot_of,
            tried: &[],
        };
        let mut ready = |x: &str| -> Result<(), Unready> {
            match kind_of(&a, &lib, x) {
                _ if x == s.start => Ok(()),
                Kind::Api => (row(x) == Some(true))
                    .then_some(())
                    .ok_or(Unready::NeedsKey),
                Kind::Sub => self.login_on_disk(&s.agent, &lib, x),
            }
        };
        let next = decide::next_of(&f, &mut ready);
        let blocked = (decide::refused_now(&f) && next.is_none()).then(|| Blocked {
            earliest: pool
                .iter()
                .filter_map(|x| decide::back_at(x, &f).map(|at| (at, x)))
                .min()
                .map(|(at, x)| AccountAt {
                    account: x.clone(),
                    at,
                }),
        });
        let fallback_api = lib
            .accounts
            .iter()
            .map(|x| x.id.as_str())
            .filter(|x| {
                !pool.iter().any(|p| p == x)
                    && kind_of(&a, &lib, x) == Kind::Api
                    && row(x) == Some(true)
                    && !decide::spent(x, &f)
            })
            .map(str::to_string)
            .next();
        SessionRotationState::Present(Box::new(SessionRotation {
            agent: s.agent.clone(),
            follow: s.follow,
            custom: s.custom.clone(),
            account: AccountCell {
                start: s.start.clone(),
                current: s.current.clone(),
                since: s.since,
                history: s.history.clone(),
                in_place,
            },
            next,
            blocked,
            fallback_api,
        }))
    }
}

/// 号的种类：key 表里有行、或账号库说它是按量号 ⇒ 按量号；其余是订阅号。
fn kind_of(a: &Turn<'_>, lib: &Library, account: &str) -> Kind {
    if (a.row)(account).is_some() || lib.find(account).is_some_and(|x| x.api) {
        Kind::Api
    } else {
        Kind::Sub
    }
}

/// 中转按 [`Go`] 发：起会话的号走那张决策表；按量号照它那一行；订阅号换上它的令牌（上游 ＝ 这一家的默认上游）。
pub(crate) fn dispatch_go(
    table: &RoutingTable,
    upstreams: &super::Upstreams,
    mode: Mode,
    key: &RouteKey,
    go: Go,
    act: &mut dyn FnMut(Destination<'_>),
) {
    let agent = key.seg1.as_str();
    match go {
        Go::Start => super::decide(table, upstreams, mode, key, act),
        Go::Api { account, body } => match table.lookup(agent, &account) {
            Some(row) => super::dispatch_auth(row, &account, body.as_deref(), act),
            None => super::decide(table, upstreams, mode, key, act),
        },
        Go::Sub {
            account,
            token,
            body,
        } => match (
            upstreams.of(agent),
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
            _ => super::decide(table, upstreams, mode, key, act),
        },
    }
}

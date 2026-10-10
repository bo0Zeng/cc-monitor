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
    self, AccountAt, AccountCell, AtLimit, Base, Baseline, Blocked, Book, InPlace, Rotation,
    RotationStore, SegmentShow, SessionEntry, SessionRotation, SessionRotationState, SwitchRecord,
    SwitchWhy, Unready,
};
use crate::accounts::quota::show::{self, LoginState};
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
    /// 硬上限卡住：这一发不发上游，回这一家的「用满」回包。
    Hold { reply: crate::agents::LimitReply },
}

/// 这台后端此刻的本地钟比 UTC 快几秒（按时段写的上限按它取）；读不出 ⇒ 按 UTC，不猜（同 `--text` 排时刻那一处）。
pub(crate) fn local_offset(now: u64) -> i64 {
    crate::platform::local_tz::offset_secs(now).unwrap_or(0)
}

/// 一家的窗口名 → 窗口键（这一家没给 ⇒ 一个都没有）。
fn key_fn(agent: &str) -> impl Fn(&str) -> Option<String> {
    let f = crate::agents::window_key_of(agent);
    move |w: &str| f.and_then(|f| f(w))
}

/// 一家的窗口名 → 语义位。
fn slot_fn(agent: &str) -> impl Fn(&str) -> Option<&'static str> {
    let f = crate::agents::window_slot_of(agent);
    move |w: &str| f.and_then(|f| f(w))
}

/// 这份轮换的「到上限」此刻实际照哪一档办：说 `stop`、这一家却给不出「用满」回包 ⇒ `continue`。
pub(crate) fn at_limit_in_effect(agent: &str, said: AtLimit) -> AtLimit {
    match said {
        AtLimit::Stop if crate::agents::limit_reply_of(agent).is_some() => AtLimit::Stop,
        _ => AtLimit::Continue,
    }
}

/// [`Hop::plan_view`] 的结果。
pub(crate) struct PlanView {
    pub(crate) steps: Vec<decide::PlanStep>,
    pub(crate) lanes: Vec<PlanLane>,
    /// 号 → 窗口键（`*` ＝ 全部窗口）→ （此刻取的, 这一格不算时往下一层取到的）。
    pub(crate) effective: BTreeMap<String, BTreeMap<String, (decide::CapAt, decide::CapAt)>>,
}

/// 预览里一个号的泳道：不能用的那几段 · 重置时刻（`(语义位或窗口键, 时刻)`）。
pub(crate) struct PlanLane {
    pub(crate) account: String,
    pub(crate) spans: Vec<decide::LaneSpan>,
    pub(crate) resets: Vec<(String, u64)>,
    /// 此刻卡人的那个窗口（语义位或窗口键）与用了多少（%）；没出过数 ⇒ `None`。
    pub(crate) pinch: Option<(String, u32)>,
}

/// 一发请求在上游选择这一侧的事实（判的时候要的；按量号那几格由调用方从 key 表答）。
pub(crate) struct Turn<'a> {
    pub(crate) agent: &'a str,
    /// 起会话的号（路由第 2 段）。
    pub(crate) start: &'a str,
    /// 会话 id（流标签）。
    pub(crate) sid: &'a str,
    /// 会话血缘里它的父（`lineage.rs`；中转那一路每一发先认过）。新会话第一次被看见时按它默认跟随父会话。
    pub(crate) parent: Option<String>,
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

    /// 额度账上这个号此刻各窗口的用量（换进它那一刻记成这一段的基线）；没见过 ⇒ 空（之后第一次说得出时补）。
    pub(crate) fn baseline_of(&self, agent: &str, account: &str, now: u64) -> Baseline {
        let key = key_fn(agent);
        self.quota
            .entry(agent, account)
            .map(|o| {
                o.reading
                    .windows
                    .iter()
                    .filter_map(|w| {
                        Some((
                            key(&w.name)?,
                            Base {
                                used: decide::used_now(w, now),
                                resets_at: w.resets_at,
                            },
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 换进 `target` 那一刻挡在它前面的号（池里排在它前面、此刻不能用的），随这一段记下（`preempt` 只等它们回来）。
    pub(crate) fn above_at(
        &self,
        book: &Book,
        s: &SessionEntry,
        target: &str,
        row: &dyn Fn(&str) -> Option<bool>,
        now: u64,
    ) -> Vec<String> {
        let lib = self.library();
        let a = Turn {
            agent: &s.agent,
            start: &s.start,
            sid: "",
            parent: None,
            body: b"",
            row,
            now,
        };
        let rot = book.rotation_of(s);
        let pool = rot.pool(&s.start);
        let seen = |x: &str| self.quota.entry(&s.agent, x).map(|o| o.reading);
        let kind = |x: &str| kind_of(&a, &lib, x);
        let (slot, key) = (slot_fn(&s.agent), key_fn(&s.agent));
        let f = self.facts(
            &rot,
            &pool,
            s,
            target,
            now,
            None,
            &[],
            &seen,
            &kind,
            &slot,
            &key,
        );
        decide::blocked_above(&f, target)
    }

    /// 这个会话判一次要的事实（`current` · `heard` · `tried` 由调用方给）。
    #[allow(clippy::too_many_arguments)]
    fn facts<'f>(
        &self,
        rot: &'f Rotation,
        pool: &'f [String],
        s: &'f SessionEntry,
        current: &'f str,
        now: u64,
        heard: Option<&'f QuotaReading>,
        tried: &'f [String],
        seen: &'f dyn Fn(&str) -> Option<QuotaReading>,
        kind: &'f dyn Fn(&str) -> Kind,
        slot: &'f dyn Fn(&str) -> Option<&'static str>,
        key: &'f dyn Fn(&str) -> Option<String>,
    ) -> Facts<'f> {
        Facts {
            pool,
            when: rot.when,
            cap: &rot.cap,
            stint: &rot.stint,
            preempt: rot.preempt,
            at_limit: at_limit_in_effect(&s.agent, rot.at_limit),
            fallback: &rot.fallback,
            wait: rot.wait,
            can_hold: crate::agents::limit_reply_of(&s.agent).is_some(),
            current,
            base: &s.baseline,
            above: &s.blocked_above,
            now,
            offset: local_offset(now),
            heard,
            seen,
            kind,
            slot,
            key,
            tried,
        }
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
        if id == super::endpoint::UNDECLARED_ACCOUNT_SEGMENT {
            // 起会话时没说是哪个号 ⇒ 那一家的默认配置目录（同账号 0）。
            return Some(((face.base_dir)(home), Some(home.to_path_buf())));
        }
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
        let token = oauth::access_token(&dir, &face, &ep, now.saturating_mul(1000), account)
            .map_err(|u| {
                tracing::warn!("[rotate] {}", u.said(account).logged());
                Unready::NeedsLogin
            })?;
        let who = self
            .identity(&face, &dir, base.as_deref())
            .ok_or(Unready::NeedsLogin)?;
        Ok((token, who))
    }

    /// ★ 一个号此刻接不接得上（只读，不续令牌）—— 「下一个」（[`Self::view`]）只问它，真换号（[`Self::prepare`]）先问它再拿令牌，
    /// 两边同一处判：起会话的号 ⇒ 接得上；按量号 ⇒ key 表里那一行接得上；订阅号 ⇒ 盘上有登录、读得出账号身份。
    fn reach(&self, a: &Turn<'_>, lib: &Library, account: &str) -> Result<Reach, Unready> {
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
    fn prepare(&self, a: &Turn<'_>, lib: &Library, account: &str) -> Result<Go, Unready> {
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
                let (token, who) = self.login(a.agent, lib, account, a.now)?;
                Ok(Go::Sub {
                    account: account.to_string(),
                    token,
                    body: rewrite(&who)?,
                })
            }
        }
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
            let base = self.baseline_of(a.agent, a.start, a.now);
            let mut next = book.clone();
            next.saw_child(a.sid, a.agent, a.start, a.now, a.parent.as_deref());
            let above = next
                .sessions
                .get(a.sid)
                .map(|s| self.above_at(&next, s, a.start, a.row, a.now))
                .unwrap_or_default();
            match rotation::relay_change(&self.store, |b| {
                b.saw_child(a.sid, a.agent, a.start, a.now, a.parent.as_deref());
                b.rebase(a.sid, &base);
                b.block_above(a.sid, &above);
                b.clone()
            }) {
                Ok(b) => book = b,
                Err(e) => {
                    tracing::warn!("[rotate] {}", e.logged());
                    return None;
                }
            }
        } else if book
            .sessions
            .get(a.sid)
            .is_some_and(|s| a.now.saturating_sub(s.last_seen()) >= rotation::SEEN_REFRESH)
        {
            // 已知的会话每天头一发刷新「看见」的时刻（清旧会话按它；只动这一格，钉号 · 基线 · 挡在前面的都不碰）。写不成只出声。
            match rotation::relay_change(&self.store, |b| {
                b.saw(a.sid, a.agent, a.start, a.now);
                b.clone()
            }) {
                Ok(b) => book = b,
                Err(e) => tracing::warn!("[rotate] {}", e.logged()),
            }
        }
        let s = book.sessions.get(a.sid)?.clone();
        Some((book, s))
    }

    /// 判一次 ⇒ 结论 ＋ 判的时候备好的那一发（换成了才有）＋ 换成了的话挡在新号前面的那几个。
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
    ) -> (Verdict, Option<Go>, Vec<String>) {
        let rot = book.rotation_of(s);
        let pool = rot.pool(&s.start);
        let seen = |x: &str| self.quota.entry(a.agent, x).map(|o| o.reading);
        let kind = |x: &str| kind_of(a, lib, x);
        let (slot, key) = (slot_fn(a.agent), key_fn(a.agent));
        let f = self.facts(
            &rot, &pool, s, current, a.now, heard, tried, &seen, &kind, &slot, &key,
        );
        let mut ready: Option<Go> = None;
        let v = decide::decide(&f, &mut |x| {
            self.prepare(a, lib, x).map(|g| {
                ready = Some(g);
            })
        });
        let above = match &v {
            Verdict::Switch { to, .. } => self.above_at(book, s, to, a.row, a.now),
            _ => Vec::new(),
        };
        (v, ready, above)
    }

    /// 钉到 `rec.to`、记一条；这一段的基线从额度账上那个号此刻的用量记起，挡在它前面的号（`above`）一并记下。
    fn pin(
        &self,
        agent: &str,
        sid: &str,
        rec: SwitchRecord,
        skipped: &[(String, Unready)],
        above: &[String],
    ) {
        let base = self.baseline_of(agent, &rec.to, rec.at);
        if let Err(e) = rotation::relay_change(&self.store, |b| {
            b.pin(sid, rec, skipped);
            b.rebase(sid, &base);
            b.block_above(sid, above);
        }) {
            tracing::warn!("[rotate] {}", e.logged());
        }
    }

    fn stuck(&self, sid: &str, rec: SwitchRecord, skipped: &[(String, Unready)]) {
        if let Err(e) = rotation::relay_change(&self.store, |b| b.note_stuck(sid, rec, skipped)) {
            tracing::warn!("[rotate] {}", e.logged());
        }
    }

    /// 结论落账：换成了 ⇒ 钉住、记一条，交出备好的那一发；没换成 ⇒ 记该记的，`None`。
    fn settle(
        &self,
        a: &Turn<'_>,
        current: &str,
        (v, ready, above): (Verdict, Option<Go>, Vec<String>),
    ) -> Option<Go> {
        match v {
            Verdict::Stay => None,
            Verdict::Switch {
                to,
                why,
                from_resets_at,
                skipped,
            } => {
                let rec = SwitchRecord {
                    at_text: None,
                    from_resets_at_text: None,
                    at: a.now,
                    from: current.to_string(),
                    to,
                    why,
                    from_resets_at,
                };
                self.pin(a.agent, a.sid, rec, &skipped, &above);
                ready
            }
            Verdict::Stuck {
                why,
                from_resets_at,
                skipped,
            } => {
                let rec = SwitchRecord {
                    at_text: None,
                    from_resets_at_text: None,
                    at: a.now,
                    from: current.to_string(),
                    to: current.to_string(),
                    why,
                    from_resets_at,
                };
                self.stuck(a.sid, rec, &skipped);
                None
            }
            Verdict::Hold { n, back, skipped } => {
                let rec = SwitchRecord {
                    at_text: None,
                    from_resets_at_text: None,
                    at: a.now,
                    from: current.to_string(),
                    to: current.to_string(),
                    why: SwitchWhy::Held { n },
                    from_resets_at: Some(back.at),
                };
                self.stuck(a.sid, rec, &skipped);
                let reply = crate::agents::limit_reply_of(a.agent)?;
                Some(Go::Hold {
                    reply: reply(back.at, a.now, back.slot.as_deref()),
                })
            }
            Verdict::Wait {
                instead,
                back,
                skipped,
            } => {
                let rec = SwitchRecord {
                    at_text: None,
                    from_resets_at_text: None,
                    at: a.now,
                    from: current.to_string(),
                    to: current.to_string(),
                    why: SwitchWhy::Wait {
                        account: back.account.clone(),
                        instead,
                    },
                    from_resets_at: Some(back.at),
                };
                self.stuck(a.sid, rec, &skipped);
                let reply = crate::agents::limit_reply_of(a.agent)?;
                Some(Go::Hold {
                    reply: reply(back.at, a.now, back.slot.as_deref()),
                })
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
        let judged = self.judge(a, &lib, &book, &s, &s.current, None, &[]);
        if let Some(go) = self.settle(a, &s.current, judged) {
            return go;
        }
        // 换进来时说不出的窗口，这会儿额度账上有了 ⇒ 补进这一段的基线。
        let base = self.baseline_of(a.agent, &s.current, a.now);
        if base.keys().any(|k| !s.baseline.contains_key(k)) {
            if let Err(e) = rotation::relay_change(&self.store, |b| b.rebase(a.sid, &base)) {
                tracing::warn!("[rotate] {}", e.logged());
            }
        }
        if s.current == s.start {
            return Go::Start;
        }
        match self.prepare(a, &lib, &s.current) {
            Ok(go) => go,
            Err(reason) => {
                let rec = SwitchRecord {
                    at_text: None,
                    from_resets_at_text: None,
                    at: a.now,
                    from: s.current.clone(),
                    to: s.start.clone(),
                    why: SwitchWhy::Skipped {
                        account: s.current.clone(),
                        reason,
                    },
                    from_resets_at: None,
                };
                let above = self.above_at(&book, &s, &s.start, a.row, a.now);
                self.pin(a.agent, a.sid, rec, &[], &above);
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
        let judged = self.judge(a, &lib, &book, &s, heard_from, Some(heard), tried);
        self.settle(a, heard_from, judged)
    }

    /// 订阅号「拿得到登录」的只读那一版（不续令牌）：配置目录在 · 有凭据文件 · 读得出身份（交回身份）。
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
            parent: None,
            body: b"",
            row,
            now,
        };
        self.prepare(&a, &self.library(), target).map(|_| ())
    }

    /// 一个号此刻卡人的窗口与用了多少（%）：额度账说卡在哪个窗口就取它，没说 ⇒ 用得最多的那个；没出过数 ⇒ `None`。
    /// 窗口按语义位写（`5h` · `7d`），没有语义位的照这一家的窗口键。
    pub(crate) fn pinch(&self, agent: &str, account: &str, now: u64) -> Option<(String, u32)> {
        self.pinch_of(agent, account, now)
            .map(|(_, _, name, pct)| (name, pct))
    }

    /// [`Hop::pinch`] ＋ 那一条账与那个窗口在这一家的原名。
    fn pinch_of(
        &self,
        agent: &str,
        account: &str,
        now: u64,
    ) -> Option<(
        crate::accounts::quota::ledger::Observed,
        String,
        String,
        u32,
    )> {
        let o = self.quota.entry(agent, account)?;
        let (slot, key) = (slot_fn(agent), key_fn(agent));
        let ws = &o.reading.windows;
        let w = o
            .reading
            .limiting
            .as_deref()
            .and_then(|l| ws.iter().find(|w| w.name == l && w.used.is_some()))
            .or_else(|| {
                ws.iter()
                    .filter(|w| w.used.is_some())
                    .max_by(|a, b| decide::used_now(a, now).total_cmp(&decide::used_now(b, now)))
            })?;
        let name = slot(&w.name).map_or_else(|| key(&w.name), |s| Some(s.to_string()))?;
        let pct = (decide::used_now(w, now) * 100.0).round().max(0.0) as u32;
        let raw = w.name.clone();
        Some((o, raw, name, pct))
    }

    /// 这个号卡人的那个窗口按目前的涨法几点用到 `target` %（额度账的走势，[`crate::accounts::quota::ledger::Observed::eta`]）；没根据 ⇒ `None`。
    pub(crate) fn eta(&self, agent: &str, account: &str, target: u32, now: u64) -> Option<u64> {
        let (o, raw, _, _) = self.pinch_of(agent, account, now)?;
        o.eta(&raw, f64::from(target) / 100.0, now)
    }

    /// 帧面「这份轮换接下来会怎么走」（`rotation-plan`）：从 `now` 到 `until` 的预览 · 池里各号不能用的那几段与重置时刻 ·
    /// 各号各窗口此刻取的上限。`current` / `above` ＝ 此刻的号与挡在它前面的（规则 / 草稿没有会话 ⇒ 起始账号 · 空）。只读。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn plan_view(
        &self,
        rot: &Rotation,
        agent: &str,
        start: &str,
        current: &str,
        above: &[String],
        row: &dyn Fn(&str) -> Option<bool>,
        now: u64,
        until: u64,
    ) -> PlanView {
        let lib = self.library();
        let a = Turn {
            agent,
            start,
            sid: "",
            parent: None,
            body: b"",
            row,
            now,
        };
        let pool = rot.pool(start);
        let seen = |x: &str| self.quota.entry(agent, x).map(|o| o.reading);
        let kind = |x: &str| kind_of(&a, &lib, x);
        let (slot, key) = (slot_fn(agent), key_fn(agent));
        let f = Facts {
            pool: &pool,
            when: rot.when,
            cap: &rot.cap,
            stint: &rot.stint,
            preempt: rot.preempt,
            at_limit: at_limit_in_effect(agent, rot.at_limit),
            fallback: &rot.fallback,
            wait: rot.wait,
            can_hold: crate::agents::limit_reply_of(agent).is_some(),
            current,
            base: &BTreeMap::new(),
            above,
            now,
            offset: local_offset(now),
            heard: None,
            seen: &seen,
            kind: &kind,
            slot: &slot,
            key: &key,
            tried: &[],
        };
        let mut ready = |x: &str| self.reach(&a, &lib, x).map(|_| ());
        let steps = decide::plan(&f, until, &mut ready);
        let lanes = pool
            .iter()
            .map(|x| {
                let resets = seen(x)
                    .map(|r| {
                        r.windows
                            .iter()
                            .filter_map(|w| {
                                let at = w.resets_at.filter(|t| *t > now && *t < until)?;
                                Some((
                                    slot(&w.name)
                                        .map_or_else(|| key(&w.name), |s| Some(s.to_string()))?,
                                    at,
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                PlanLane {
                    account: x.clone(),
                    spans: decide::lane(&f, x, until),
                    resets,
                    pinch: self.pinch(agent, x, now),
                }
            })
            .collect();
        let effective = pool
            .iter()
            .map(|x| {
                let mut keys: Vec<String> = ["5h", "7d"].iter().map(|k| k.to_string()).collect();
                keys.extend(rot.cap.get(x).into_iter().flat_map(|m| m.keys().cloned()));
                keys.push(crate::accounts::quota::rotation::ALL_WINDOWS.to_string());
                keys.dedup();
                let cells = keys.into_iter().fold(BTreeMap::new(), |mut m, k| {
                    m.entry(k.clone())
                        .or_insert_with(|| decide::effective_cap(&f, x, &k));
                    m
                });
                (x.clone(), cells)
            })
            .collect();
        PlanView {
            steps,
            lanes,
            effective,
        }
    }

    /// 帧面：一个会话的那一份（「账号」格 ＋ 下一个 · 卡住 · 可用按量号 · 此刻那个号的显示态）。只读：不记、不续令牌。
    /// `live(sid)` ＝ 这个会话的进程此刻还活着。
    pub(crate) fn view(
        &self,
        book: &Book,
        sid: &str,
        parent: Option<&str>,
        row: &dyn Fn(&str) -> Option<bool>,
        live: &dyn Fn(&str) -> bool,
        now: u64,
    ) -> SessionRotationState {
        let lib = self.library();
        let Some(s) = book.sessions.get(sid) else {
            return SessionRotationState::Absent {
                in_place: InPlace::NoRelay,
            };
        };
        let in_place = if !live(sid) {
            InPlace::Ended
        } else if crate::agents::login_of(&s.agent).is_none() {
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
            parent: None,
            body: b"",
            row,
            now,
        };
        let rot = book.rotation_of(s);
        let pool = rot.pool(&s.start);
        let seen = |x: &str| self.quota.entry(&s.agent, x).map(|o| o.reading);
        let kind = |x: &str| kind_of(&a, &lib, x);
        let (slot_of, key) = (slot_fn(&s.agent), key_fn(&s.agent));
        let f = self.facts(
            &rot,
            &pool,
            s,
            &s.current,
            now,
            None,
            &[],
            &seen,
            &kind,
            &slot_of,
            &key,
        );
        let mut ready = |x: &str| self.reach(&a, &lib, x).map(|_| ());
        let next = decide::next_of(&f, &mut ready);
        let held = match decide::decide(&f, &mut ready) {
            Verdict::Hold { back, .. } | Verdict::Wait { back, .. } => Some(back),
            _ => None,
        };
        let blocked = if let Some(back) = held {
            Some(Blocked {
                earliest: Some(AccountAt {
                    at_text: None,
                    account: back.account,
                    at: back.at,
                }),
            })
        } else {
            (decide::refused_now(&f) && next.is_none()).then(|| Blocked {
                earliest: pool
                    .iter()
                    .filter_map(|x| decide::back_at(x, &f).map(|at| (at, x)))
                    .min()
                    .map(|(at, x)| AccountAt {
                        at_text: None,
                        account: x.clone(),
                        at,
                    }),
            })
        };
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
        let held = self.quota.entry(&s.agent, &s.current);
        let mut quota = show::show(
            held.as_ref().map(|o| (&o.reading, o.seen_at)),
            self.show_facts(&s.agent, &lib, &s.current, row),
            show::near_of(rot.when),
            now,
            &slot_of,
        );
        if let Some(o) = &held {
            quota.windows = show::windows_of(o, &key, now);
        }
        let pct = |x: f64| (x * 100.0).round().max(0.0) as u32;
        let segment = decide::segment(&f)
            .into_iter()
            .map(|(w, base, spent, stint)| SegmentShow {
                w,
                base: pct(base),
                spent: pct(spent),
                stint,
            })
            .collect();
        SessionRotationState::Present(Box::new(SessionRotation {
            agent: s.agent.clone(),
            source: s.source.clone(),
            rule_name: book
                .rule_of(s)
                .and_then(|id| book.rules.get(id))
                .map(|r| r.name.clone()),
            explain: crate::accounts::quota::rule_text::explain(&rot),
            parent: parent.map(str::to_string),
            parent_missing: matches!(
                s.source,
                crate::accounts::quota::rotation::Source::Parent(_)
            ) && book.decider(s).is_none(),
            custom: s.custom.clone(),
            account: AccountCell {
                since_text: None,
                start: s.start.clone(),
                current: s.current.clone(),
                since: s.since,
                history: s.history.clone(),
                in_place,
                segment,
                blocked_above: s.blocked_above.clone(),
            },
            next,
            blocked,
            fallback_api,
            at_limit: at_limit_in_effect(&s.agent, rot.at_limit),
            quota,
        }))
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
enum Reach {
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

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
use crate::accounts::quota::decide::{self, Kind, Verdict};
use crate::accounts::quota::ledger::Ledger;
use crate::accounts::quota::rotation::{
    self, Base, Baseline, Book, ConfigBody, InPlace, Leave, Params, Pick, RotationStore,
    SessionEntry, SwitchRecord, Unready, Who,
};
use crate::accounts::quota::show::{self, LoginState};
use crate::accounts::quota::strategy::ctx::{Facts, Stint};
use crate::agents::{IdentityCell, LoginFace, QuotaReading};
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
            Unready::NeedsLogin
        })?;
        let who = self
            .identity(&face, &dir, base.as_deref())
            .ok_or(Unready::NeedsLogin)?;
        Ok((token, who))
    }

    /// ★ 一个号此刻接不接得上（只读，不续令牌）—— 帧面（[`Self::view`]）只问它，真换号（[`Self::prepare`]）先问它再拿令牌，
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
    fn prepare(
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

    /// ★ 一个会话此刻的场面：打开的那份配置展开成参与的号（起会话的号 · 点名的号 · 其余订阅号），每个号带事实与这一行的参数；
    /// 在用号这一段用了几点 · 用速。`heard` ＝ 刚回来的那个回包（它答的那个号按它判）。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn scene(
        &self,
        book: &Book,
        sid: Option<&str>,
        agent: &str,
        start: &str,
        current: &str,
        body: &ConfigBody,
        lib: &Library,
        row: &dyn Fn(&str) -> Option<bool>,
        now: u64,
        heard: Option<(&str, &QuotaReading)>,
        baseline: &Baseline,
        since: u64,
    ) -> Scene {
        let a = Turn {
            agent,
            start,
            sid: sid.unwrap_or(""),
            body: b"",
            row,
            now,
        };
        let slot = slot_fn(agent);
        let mut named: Vec<String> = body
            .accounts
            .iter()
            .filter_map(|r| match &r.who {
                Who::Id(x) => Some(x.clone()),
                _ => None,
            })
            .collect();
        named.push(start.to_string());
        let mut out: Vec<(Facts, Params)> = Vec::new();
        let mut from_rest = Vec::new();
        let mut push = |id: &str, p: &Params, rest: bool, out: &mut Vec<(Facts, Params)>| {
            if out.iter().any(|(f, _)| f.id == id) {
                return;
            }
            let kind = kind_of(&a, lib, id);
            let reading = match heard {
                Some((h, r)) if h == id => Some(r.clone()),
                _ => self.quota.entry(agent, id).map(|o| o.reading),
            };
            let others = book
                .sessions
                .iter()
                .filter(|(x, s)| Some(x.as_str()) != sid && s.agent == agent && s.current == id)
                .count() as u32;
            let login_ok = self.reach(&a, lib, id).is_ok();
            let f = decide::facts_of(
                id,
                reading.as_ref(),
                heard.is_some_and(|(h, _)| h == id),
                kind,
                login_ok,
                others,
                now,
                &slot,
            );
            out.push((f, p.clone()));
            if rest {
                from_rest.push(id.to_string());
            }
        };
        for r in &body.accounts {
            match &r.who {
                Who::Start => push(start, &r.params, false, &mut out),
                Who::Id(x) => push(x, &r.params, false, &mut out),
                Who::Rest => {
                    let rest =
                        rest_members(lib.accounts.iter().map(|x| x.id.as_str()), &named, |x| {
                            kind_of(&a, lib, x) == Kind::Sub
                        });
                    for x in &rest {
                        push(x, &r.params, true, &mut out);
                    }
                }
            }
        }
        let (stint, rate) = match self.pinch_of(agent, current, now) {
            Some((o, raw, _, pct)) => {
                let key = key_fn(agent)(&raw);
                let base = key
                    .as_deref()
                    .and_then(|k| baseline.get(k))
                    .map_or(f64::from(pct), |b| (b.used * 100.0).max(0.0));
                (
                    Stint {
                        since,
                        used: (f64::from(pct) - base).max(0.0),
                    },
                    o.rate(&raw, now),
                )
            }
            None => (Stint { since, used: 0.0 }, None),
        };
        Scene {
            accounts: out,
            from_rest,
            stint,
            rate,
        }
    }

    /// 这一家有可换的订阅号登录、这一发带着会话 id ⇒ 这个会话在这台的轮换状态（第一次看见 / 换了起它的号 ⇒ 记下）。
    fn entry(&self, own: &Own, a: &Turn<'_>) -> Option<(Book, SessionEntry)> {
        if a.sid.is_empty() || crate::agents::login_of(a.agent).is_none() {
            return None;
        }
        let book = self.store.now();
        let fresh = !book
            .sessions
            .get(a.sid)
            .is_some_and(|s| s.start == a.start && s.agent == a.agent);
        let stale_seen = book
            .sessions
            .get(a.sid)
            .is_some_and(|s| a.now.saturating_sub(s.last_seen()) >= rotation::SEEN_REFRESH);
        if !fresh && !stale_seen {
            let s = book.sessions.get(a.sid)?.clone();
            return Some((book, s));
        }
        let base = fresh.then(|| self.baseline_of(a.agent, a.start, a.now));
        match rotation::relay_change(&self.store, own, |b| {
            b.saw(a.sid, a.agent, a.start, a.now);
            if let Some(base) = &base {
                b.rebase(a.sid, base);
            }
            b.clone()
        }) {
            Ok(b) => {
                let s = b.sessions.get(a.sid)?.clone();
                Some((b, s))
            }
            Err(e) => {
                tracing::warn!("[rotate] {}", e.logged());
                None
            }
        }
    }

    /// 判一次：场面 → 跑策略 → 有效优先级 → 挑。回结论 ＋ 判的时候备好的那一发（换成了才有）＋ 这一判的备忘 ＋ 热换要不要清。
    #[allow(clippy::too_many_arguments)]
    fn judge(
        &self,
        own: &Own,
        a: &Turn<'_>,
        lib: &Library,
        book: &Book,
        s: &SessionEntry,
        current: &str,
        heard: Option<(&str, &QuotaReading)>,
        tried: &[String],
    ) -> Option<(Verdict, Option<Go>, Option<serde_json::Value>, bool, bool)> {
        let o = book.opened(s)?;
        let sc = self.scene(
            book,
            Some(a.sid),
            a.agent,
            &s.start,
            current,
            o.body,
            lib,
            a.row,
            a.now,
            heard,
            &s.baseline,
            s.since,
        );
        let all = super::super::quota::strategy::all(self.store.strategy_dir().as_deref());
        let st = super::super::quota::strategy::find(&all, &o.body.strategy);
        let memo = s.memo.clone().unwrap_or(serde_json::Value::Null);
        let j = decide::judge(&decide::Judge {
            strategy: st.as_deref(),
            body: o.body,
            accounts: &sc.accounts,
            now: a.now,
            current,
            stint: sc.stint,
            rate: sc.rate,
            memo: &memo,
        });
        if let Some(e) = &j.error {
            tracing::warn!(
                "[rotate] 策略「{}」这一判照内置策略 1 排：{}",
                o.body.strategy,
                e.msg
            );
        }
        let (eff, clear_hot) = decide::effective(&j.rows, &s.manual, &sc.accounts);
        let mut ready: Option<Go> = None;
        let v = decide::decide(
            &eff,
            &decide::Pickable {
                facts: &sc.accounts,
                current,
                tried,
                can_hold: crate::agents::limit_reply_of(a.agent).is_some(),
            },
            &j.strategy,
            &mut |x| {
                self.prepare(own, a, lib, x).map(|g| {
                    ready = Some(g);
                })
            },
        );
        let all_shut = !sc.accounts.iter().any(|(f, _)| f.ok);
        Some((v, ready, j.memo, clear_hot, all_shut))
    }

    /// 结论落账：换成了 ⇒ 钉住、记一条（只在真换成的那一下记），交出备好的那一发；停发 ⇒ 记一条、交出「用满」回包；
    /// 备忘 · 清热换跟着这一下写。
    fn settle(
        &self,
        own: &Own,
        a: &Turn<'_>,
        current: &str,
        (v, ready, memo, clear_hot): (Verdict, Option<Go>, Option<serde_json::Value>, bool),
    ) -> Option<Go> {
        let rec = |to: &str, leave: Leave, pick: Option<Pick>, back: Option<u64>| SwitchRecord {
            at: a.now,
            at_text: None,
            from: current.to_string(),
            to: to.to_string(),
            leave,
            pick,
            from_resets_at: back,
            from_resets_at_text: None,
            text: None,
        };
        let from_back = self
            .quota
            .entry(a.agent, current)
            .and_then(|o| o.reading.resets_at);
        match v {
            Verdict::Stay { overage } => {
                let r = overage.then(|| rec(current, Leave::Overage, None, from_back));
                self.write(own, |b| {
                    let mut changed = b.set_memo(a.sid, memo.clone());
                    if clear_hot {
                        changed |= b.clear_hot(a.sid);
                    }
                    if let Some(r) = &r {
                        changed |= b.note_stuck(a.sid, r.clone());
                    }
                    changed
                });
                None
            }
            Verdict::Switch { to, leave, pick } => {
                let base = self.baseline_of(a.agent, &to, a.now);
                let r = rec(&to, leave, Some(pick), from_back);
                self.write(own, |b| {
                    b.set_memo(a.sid, memo.clone());
                    if clear_hot {
                        b.clear_hot(a.sid);
                    }
                    if b.pin(a.sid, r.clone()) {
                        b.rebase(a.sid, &base);
                    }
                    true
                });
                ready
            }
            Verdict::Hold { back, w } => {
                let Some((_, at)) = back else {
                    self.write(own, |b| b.set_memo(a.sid, memo.clone()));
                    return None;
                };
                let r = rec(current, Leave::Held, None, Some(at));
                self.write(own, |b| {
                    let mut changed = b.set_memo(a.sid, memo.clone());
                    if clear_hot {
                        changed |= b.clear_hot(a.sid);
                    }
                    changed | b.note_stuck(a.sid, r.clone())
                });
                let reply = crate::agents::limit_reply_of(a.agent)?;
                Some(Go::Hold {
                    reply: reply(at, a.now, w.as_deref()),
                })
            }
        }
    }

    /// 中转那一路写一下：先在内存里那一份上试，没改就不进锁、不写盘；改了才在锁里照同一个改法写。写不成只出声。
    fn write(&self, own: &Own, f: impl Fn(&mut Book) -> bool) {
        let mut probe = self.store.now();
        let before = probe.clone();
        f(&mut probe);
        if probe == before {
            return;
        }
        if let Err(e) = rotation::relay_change(&self.store, own, |b| {
            f(b);
        }) {
            tracing::warn!("[rotate] {}", e.logged());
        }
    }

    /// ★ 发之前：这个会话此刻照打开的那份配置判一次；换成了 ⇒ 走新号；停发 ⇒ 回「用满」。
    /// 钉着的号接不上了（登录过期续不上…）⇒ 回到起会话的号、记一条为什么。
    pub(crate) fn steer(&self, a: &Turn<'_>, own: &Own) -> Go {
        let Some((book, s)) = self.entry(own, a) else {
            return Go::Start;
        };
        let lib = self.library();
        if let Some((v, ready, memo, clear, _)) =
            self.judge(own, a, &lib, &book, &s, &s.current, None, &[])
        {
            if let Some(go) = self.settle(own, a, &s.current, (v, ready, memo, clear)) {
                return go;
            }
        }
        let s = self.store.now().sessions.get(a.sid).cloned().unwrap_or(s);
        // 换进来时说不出的窗口，这会儿额度账上有了 ⇒ 补进这一段的基线。
        let base = self.baseline_of(a.agent, &s.current, a.now);
        if base.keys().any(|k| !s.baseline.contains_key(k)) {
            if let Err(e) = rotation::relay_change(&self.store, own, |b| b.rebase(a.sid, &base)) {
                tracing::warn!("[rotate] {}", e.logged());
            }
        }
        if s.current == s.start {
            return Go::Start;
        }
        match self.prepare(own, a, &lib, &s.current) {
            Ok(go) => go,
            Err(_) => {
                let rec = SwitchRecord {
                    at: a.now,
                    at_text: None,
                    from: s.current.clone(),
                    to: s.start.clone(),
                    leave: Leave::Unusable {
                        why: serde_json::json!({"k": "@login"}),
                    },
                    pick: None,
                    from_resets_at: None,
                    from_resets_at_text: None,
                    text: None,
                };
                let base = self.baseline_of(a.agent, &s.start, a.now);
                if let Err(e) = rotation::relay_change(&self.store, own, |b| {
                    if b.pin(a.sid, rec) {
                        b.rebase(a.sid, &base);
                    }
                }) {
                    tracing::warn!("[rotate] {}", e.logged());
                }
                Go::Start
            }
        }
    }

    /// ★ 回包被拒：照打开的那份配置再判一次（`tried` ＝ 这一发已经发过的号，`heard_from` 是答这一发的那个）。
    /// 别的请求刚把这个会话换走了 ⇒ 跟上它（不另记）。没得换 ⇒ `None`（中转原样交回拒绝）。
    pub(crate) fn on_refused(
        &self,
        a: &Turn<'_>,
        heard_from: &str,
        heard: &QuotaReading,
        tried: &[String],
        own: &Own,
    ) -> Option<Go> {
        let (book, s) = self.entry(own, a)?;
        let lib = self.library();
        if s.current != heard_from && !tried.contains(&s.current) {
            if let Ok(go) = self.prepare(own, a, &lib, &s.current) {
                return Some(go);
            }
        }
        let judged = self.judge(
            own,
            a,
            &lib,
            &book,
            &s,
            heard_from,
            Some((heard_from, heard)),
            tried,
        )?;
        // 一个号事实上都不能用（都被拒着）⇒ 上游那份拒绝就是真的「用满」：照记一条，原样交回（不另造一份）。
        let all_shut = matches!(judged.0, Verdict::Hold { .. }) && judged.4;
        let go = self.settle(own, a, heard_from, (judged.0, judged.1, judged.2, judged.3));
        if all_shut {
            return None;
        }
        go
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
        own: &Own,
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
        self.prepare(own, &a, &self.library(), target).map(|_| ())
    }

    /// 一个号此刻卡人的窗口：那一条账 · 那个窗口在这一家的原名 · 语义位（没有 ⇒ 窗口键）· 用了多少（%）。额度账说卡在哪个窗口就取它，没说 ⇒ 用得最多的那个；没出过数 ⇒ `None`。
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

    /// 帧面：一个会话的那一份（打开的配置 · 场面 · 有效优先级 · 结论 · 此刻那个号的显示态）。只读：不记、不续令牌。
    /// `live(sid)` ＝ 这个会话的进程此刻还活着。中转没见过它 ⇒ `Err(能不能不重启换)`。
    pub(crate) fn view(
        &self,
        book: &Book,
        sid: &str,
        row: &dyn Fn(&str) -> Option<bool>,
        live: &dyn Fn(&str) -> bool,
        now: u64,
    ) -> Result<SessionView, InPlace> {
        let lib = self.library();
        let Some(s) = book.sessions.get(sid) else {
            return Err(InPlace::NoRelay);
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
        let o = book.opened(s).ok_or(InPlace::NoRelay)?;
        let (id, rev, dirty, body) = (o.id.to_string(), o.rev, o.dirty, o.body.clone());
        let sc = self.scene(
            book,
            Some(sid),
            &s.agent,
            &s.start,
            &s.current,
            &body,
            &lib,
            row,
            now,
            None,
            &s.baseline,
            s.since,
        );
        let all = super::super::quota::strategy::all(self.store.strategy_dir().as_deref());
        let strategy = super::super::quota::strategy::find(&all, &body.strategy);
        let memo = s.memo.clone().unwrap_or(serde_json::Value::Null);
        let judged = decide::judge(&decide::Judge {
            strategy: strategy.as_deref(),
            body: &body,
            accounts: &sc.accounts,
            now,
            current: &s.current,
            stint: sc.stint,
            rate: sc.rate,
            memo: &memo,
        });
        let a = Turn {
            agent: &s.agent,
            start: &s.start,
            sid,
            body: b"",
            row,
            now,
        };
        let (eff, hot_stale) = decide::effective(&judged.rows, &s.manual, &sc.accounts);
        let verdict = decide::decide(
            &eff,
            &decide::Pickable {
                facts: &sc.accounts,
                current: &s.current,
                tried: &[],
                can_hold: crate::agents::limit_reply_of(&s.agent).is_some(),
            },
            &judged.strategy,
            &mut |x| self.reach(&a, &lib, x).map(|_| ()),
        );
        let slot_of = slot_fn(&s.agent);
        let key = key_fn(&s.agent);
        let held = self.quota.entry(&s.agent, &s.current);
        let cur_params = sc
            .accounts
            .iter()
            .find(|(f, _)| f.id == s.current)
            .map(|(_, p)| p.clone());
        let mut quota = show::show(
            held.as_ref().map(|o| (&o.reading, o.seen_at)),
            self.show_facts(&s.agent, &lib, &s.current, row),
            &show::lines_of(cur_params.as_ref(), body.clock_at(now), &slot_of),
            now,
            &slot_of,
        );
        if let Some(o) = &held {
            quota.windows = show::windows_of(o, &key, now);
        }
        Ok(SessionView {
            entry: s.clone(),
            in_place,
            config_id: id,
            rev,
            dirty,
            body,
            scene: sc,
            judged,
            eff,
            hot_stale,
            verdict,
            quota,
        })
    }

    /// 帧面：一份配置（或草稿）此刻在这台的场面（推演 · 试跑用；没有会话 ⇒ 起会话的号取 `start`，在用的号取有效优先级第一个能用的）。
    pub(crate) fn scene_of(
        &self,
        book: &Book,
        agent: &str,
        start: &str,
        current: &str,
        body: &ConfigBody,
        row: &dyn Fn(&str) -> Option<bool>,
        now: u64,
    ) -> Scene {
        let lib = self.library();
        self.scene(
            book,
            None,
            agent,
            start,
            current,
            body,
            &lib,
            row,
            now,
            None,
            &Baseline::new(),
            now,
        )
    }

    /// 一个号接不接得上（只读）。
    pub(crate) fn ready(
        &self,
        agent: &str,
        start: &str,
        account: &str,
        row: &dyn Fn(&str) -> Option<bool>,
        now: u64,
    ) -> bool {
        let a = Turn {
            agent,
            start,
            sid: "",
            body: b"",
            row,
            now,
        };
        self.reach(&a, &self.library(), account).is_ok()
    }
}

/// 一个会话此刻的场面：参与的号（按写下的先后，事实 ＋ 这一行的参数）· 哪几个是从「其余的订阅号」那一行来的 · 这一段 · 用速。
#[derive(Debug, Clone)]
pub(crate) struct Scene {
    pub(crate) accounts: Vec<(Facts, Params)>,
    pub(crate) from_rest: Vec<String>,
    pub(crate) stint: Stint,
    pub(crate) rate: Option<f64>,
}

/// [`Hop::view`] 的结果。
pub(crate) struct SessionView {
    pub(crate) entry: SessionEntry,
    pub(crate) in_place: InPlace,
    pub(crate) config_id: String,
    pub(crate) rev: u64,
    pub(crate) dirty: bool,
    pub(crate) body: ConfigBody,
    pub(crate) scene: Scene,
    pub(crate) judged: decide::Judged,
    pub(crate) eff: Vec<decide::Ranked>,
    /// 热换的号已经到线 / 不能用了（下一发会清掉）。
    pub(crate) hot_stale: bool,
    pub(crate) verdict: Verdict,
    pub(crate) quota: show::QuotaShow,
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

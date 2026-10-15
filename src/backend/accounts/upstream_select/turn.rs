//! ★ 中转发之前那一判（第五版）：叫会话用的那份策略、照它交的先后做动作；只在节点上判，
//! 结论缓存到下一个节点 —— 节点之间一发都不叫策略（算法 #28）。
//!
//! - 判（[`decide::turn`]）是纯的；这里拼它要的：会话那一条（`rotation.json`）· 那份策略文件（[`shelf`]）· 每个号的事实。
//! - 缓存作废：过了下一个已知节点 `tn` · 会话那一条的快照变了 · 策略文件的版本变了 · 事实那几种节点（外来的读数 · 被拒 · 续期失败 ·
//!   `quota.json` · 账号库 / 登录文件）与策略目录 ⇒ 全部；这个会话自己的读数 ⇒ 只在越过它记着的 `wake.when` 某一条时。
//! - 结论跟盘上那一条不同才写（先在内存里试，没变不进锁）。

use super::rotate::{Go, Hop, Library, Turn};
use crate::accounts::quota::nodes::{Node, Nodes, Source};
use crate::accounts::quota::rotation;
use crate::accounts::quota::rules::book::{self, Session, Uses};
use crate::accounts::quota::rules::decide::{
    self, Ctx, Decision, Pick, Sigma, Strategy, Verdict, WakeWhen,
};
use crate::accounts::quota::rules::{script, shelf, standing, Acct};
use crate::accounts::quota::strategy::{Fail, FailKind};
use crate::agents::QuotaReading;
use crate::platform::fs::Own;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// 会话那一条里，判要看的那几格（`seen` · 换号记录 · 「从几点起」不算）。
#[derive(Debug, Clone, PartialEq)]
struct Snap {
    agent: String,
    start: String,
    current: String,
    temp: Vec<String>,
    uses: Uses,
    memo: Option<Value>,
}

fn snap(s: &Session) -> Snap {
    Snap {
        agent: s.agent.clone(),
        start: s.start.clone(),
        current: s.current.clone(),
        temp: s.temp.clone(),
        uses: s.strategy.clone(),
        memo: s.memo.clone(),
    }
}

struct Cached {
    snap: Snap,
    rev: String,
    pick: Pick,
    tn: Option<u64>,
    when: Vec<WakeWhen>,
    /// 来过这个会话自己的读数：下一发比一下 `when`。
    own_reading: bool,
}

/// 每个进程一份：会话 id → 上一判的结论。
pub(crate) struct Cache {
    rx: tokio::sync::broadcast::Receiver<Node>,
    map: HashMap<String, Cached>,
    /// 这个进程里重跑过几次判（判据数「节点之间不重跑」）。
    pub(crate) runs: u64,
}

impl Cache {
    pub(crate) fn new(tx: &Nodes) -> Self {
        Self {
            rx: tx.subscribe(),
            map: HashMap::new(),
            runs: 0,
        }
    }

    /// 把攒下的节点过一遍。
    fn drain(&mut self) {
        use tokio::sync::broadcast::error::TryRecvError;
        loop {
            match self.rx.try_recv() {
                Ok(n) => self.node(n),
                Err(TryRecvError::Lagged(_)) => self.map.clear(),
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            }
        }
    }

    fn node(&mut self, n: Node) {
        match (n.source, n.sid) {
            // 会话那一条变了：比快照就知道（本进程写的、别的进程写的一样），不靠这一件。
            (Source::Book, _) => {}
            // 这个会话自己的读数：只对它自己记一笔，下一发比 `when`；对别的会话是外来的事实。
            (Source::Reading, Some(sid)) => {
                self.map.retain(|k, _| *k == sid);
                if let Some(c) = self.map.get_mut(&sid) {
                    c.own_reading = true;
                }
            }
            _ => self.map.clear(),
        }
    }

    fn forget(&mut self, sid: &str) {
        self.map.remove(sid);
    }
}

/// 会话用的那份策略此刻：读懂了 ⇒ 那份文件；没了 · 读不懂 ⇒ 判的时候走兜底（带原因）。
pub(crate) struct Loaded {
    pub(crate) id: String,
    pub(crate) shelved: Option<Arc<shelf::Shelved>>,
    /// 生效的设置：会话有草稿 ⇒ 草稿；否则文件头里的。
    pub(crate) settings: Value,
    pub(crate) tz: crate::Tz,
}

impl Loaded {
    pub(crate) fn rev(&self) -> &str {
        self.shelved.as_ref().map_or("", |s| s.rev.as_str())
    }

    pub(crate) fn file(&self) -> Option<&script::File> {
        self.shelved.as_ref().and_then(|s| s.file.as_ref().ok())
    }

    /// 读不出的原因（文件不在 ⇒ `Gone`）。
    pub(crate) fn broken(&self) -> Option<Fail> {
        match &self.shelved {
            None => Some(Fail {
                kind: FailKind::Gone,
                line: None,
                col: None,
                msg: format!("strategy file `{}` is not there", self.id),
                why: crate::accounts::quota::strategy::FailWhy::Gone,
            }),
            Some(s) => s.file.as_ref().err().cloned(),
        }
    }
}

impl Strategy for Loaded {
    fn decide(&self, ctx: &Ctx<'_>) -> Result<Verdict, Fail> {
        match self.file() {
            Some(f) => f.decide(ctx),
            None => Err(self.broken().unwrap_or_else(|| Fail::shape("no strategy"))),
        }
    }

    fn time_varying(&self) -> bool {
        self.file().is_some_and(|f| f.head.time_varying)
    }
}

/// 读那份策略（策略目录 `dir` 里 id ＝ `uses.id` 的那一份）＋ 生效的设置。
pub(crate) fn load_in(dir: Option<&Path>, uses: &Uses) -> Loaded {
    let shelved = shelf::find(&shelf::all(dir), &uses.id);
    let head = shelved
        .as_ref()
        .and_then(|s| s.file.as_ref().ok())
        .map(|f| &f.head);
    Loaded {
        id: uses.id.clone(),
        settings: uses
            .draft
            .clone()
            .or_else(|| head.map(|h| h.settings.clone()))
            .unwrap_or_else(|| Value::Object(Default::default())),
        tz: crate::accounts::quota::rules::tz_of(head),
        shelved,
    }
}

/// 交给判的号：先照设置里的号列表（`{start: true}` ＝ 开会话用的号），再接其余的号（照账号库的先后）。兜底取的就是这个先后。
pub(crate) fn listed(settings: &Value, start: &str, mut all: Vec<Acct>) -> Vec<Acct> {
    let mut out: Vec<Acct> = Vec::new();
    for r in settings
        .get(crate::accounts::quota::rules::form::ACCOUNTS)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let id = if r.get("start") == Some(&Value::Bool(true)) {
            Some(start)
        } else {
            r.get("id").and_then(Value::as_str)
        };
        if let Some(i) = id.and_then(|id| all.iter().position(|a| a.id == id)) {
            out.push(all.remove(i));
        }
    }
    out.extend(all);
    out
}

fn w_of(w: usize) -> &'static str {
    if w == 0 {
        "5h"
    } else {
        "7d"
    }
}

/// 会话那一条 ⇒ 判的会话状态。
pub(crate) fn sigma_of(s: &Session) -> Sigma {
    Sigma {
        start: s.start.clone(),
        current: s.current.clone(),
        temp: s.temp.clone(),
        wake_when: s
            .wake
            .when
            .iter()
            .map(|w| WakeWhen {
                acct: w.acct.clone(),
                w: usize::from(w.w == "7d"),
                used_at_least: w.used_at_least,
            })
            .collect(),
        memo: s.memo.clone(),
    }
}

/// 一判的结论落进会话那一条（换了号记一条；停发记一条，同一个 `until` 不重记；拖出来的顺序 · memo · 叫醒照新的）。
pub(crate) fn apply(
    s: &mut Session,
    d: &Decision,
    facts: &[Acct],
    now: u64,
    cause: Option<book::Cause>,
) {
    match &d.pick {
        Pick::Use(a) if *a != s.current => {
            let cause = cause.unwrap_or_else(|| {
                if d.strategy_error.is_some() {
                    book::Cause::Error
                } else if d.from_temp {
                    book::Cause::Temp
                } else if facts
                    .iter()
                    .find(|x| x.id == s.current)
                    .is_none_or(|x| !standing::sendable(x, now))
                {
                    book::Cause::Unusable
                } else {
                    book::Cause::Order
                }
            });
            let why = d.why.iter().find(|(x, _)| x == a).map(|(_, w)| w.clone());
            let from = std::mem::replace(&mut s.current, a.clone());
            s.since = now;
            s.push_log(book::Log {
                at: now,
                from,
                to: a.clone(),
                cause,
                why,
            });
        }
        Pick::Hold { until, .. } => {
            let same = s
                .log
                .last()
                .is_some_and(|l| l.cause == book::Cause::Hold { until: *until });
            if !same {
                s.push_log(book::Log {
                    at: now,
                    from: s.current.clone(),
                    to: s.current.clone(),
                    cause: book::Cause::Hold { until: *until },
                    why: None,
                });
            }
        }
        Pick::Use(_) => {}
    }
    s.temp.clone_from(&d.sigma.temp);
    s.memo.clone_from(&d.sigma.memo);
    s.wake = book::Wake {
        at: d.wake_at,
        when: d
            .sigma
            .wake_when
            .iter()
            .map(|w| book::WakeWhen {
                acct: w.acct.clone(),
                w: w_of(w.w).to_string(),
                used_at_least: w.used_at_least,
            })
            .collect(),
    };
}

/// 备料最多试几个号（每试不成一个就记进 `tried` 再判一次）。
const PREPARE_TRIES: usize = 8;

impl Hop {
    /// 会话用的那份策略此刻（这台的策略目录）。
    pub(crate) fn load(&self, uses: &Uses) -> Loaded {
        load_in(self.store.strategy_dir().as_deref(), uses)
    }

    /// ★ 交给判的那一排事实（照 [`listed`] 的先后；开会话用的号不在账号库里也带上）。用速是每个号自己在额度账里量出来的，量不到就空着。
    pub(crate) fn accts(
        &self,
        s: &Session,
        settings: &Value,
        row: &dyn Fn(&str) -> Option<bool>,
        now: u64,
    ) -> Vec<Acct> {
        let lib = self.library();
        let tz = crate::Tz::default();
        let mut all: Vec<Acct> = self
            .facts(&s.agent, row, now, &tz)
            .iter()
            .map(Acct::of_fact)
            .collect();
        if !all.iter().any(|a| a.id == s.start) {
            all.push(Acct::of_fact(
                &self.fact_one(&s.agent, &lib, &s.start, row, now, &tz),
            ));
        }
        // 开会话用的号发的是那个会话自己带的登录（同备料那一处：起会话的号恒接得上），不看账号库里那一份登录在不在。
        if let Some(x) = all.iter_mut().find(|a| a.id == s.start) {
            if x.barred
                .is_some_and(|b| b.why == crate::accounts::quota::facts::FactWhy::Login)
            {
                x.barred = None;
            }
        }
        listed(settings, &s.start, all)
    }

    /// 照会话那一条判一次（纯：不写、不碰缓存）。
    pub(crate) fn judge_session(
        &self,
        s: &Session,
        loaded: &Loaded,
        facts: &[Acct],
        now: u64,
        tried: &[String],
    ) -> Decision {
        decide::turn(
            &sigma_of(s),
            facts,
            &loaded.settings,
            loaded,
            crate::accounts::quota::rules::clock_at(&loaded.tz, now),
            now,
            tried,
        )
    }

    /// 这一发带着会话 id、这一家有可换的登录 ⇒ 这个会话那一条（第一次看见 / 换了开它的号 / 一天头一次看见 ⇒ 记下）。
    fn entry(&self, own: &Own, a: &Turn<'_>) -> Option<Session> {
        let book = self.store.now();
        let need = book.sessions.get(a.sid).is_none_or(|s| {
            s.start != a.start
                || s.agent != a.agent
                || a.now.saturating_sub(s.last_seen()) >= book::SEEN_REFRESH
        });
        if !need {
            return book.sessions.get(a.sid).cloned();
        }
        match rotation::relay_change(&self.store, own, |b| {
            b.saw(a.sid, a.agent, a.start, a.now);
            b.sessions.get(a.sid).cloned()
        }) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("[rotate] {}", e.logged());
                // 写不成（盘上还是旧版 · 读不懂）：照内存里那一份接着判，不记。
                let mut b = book;
                b.saw(a.sid, a.agent, a.start, a.now);
                b.sessions.get(a.sid).cloned()
            }
        }
    }

    /// 结论落账（跟盘上那一条不同才写）。回落账以后的那一条。
    fn settle(
        &self,
        own: &Own,
        sid: &str,
        s: &Session,
        d: &Decision,
        facts: &[Acct],
        now: u64,
    ) -> Session {
        let mut n = s.clone();
        apply(&mut n, d, facts, now, None);
        if n != *s {
            if let Err(e) = rotation::relay_change(&self.store, own, |b| {
                if let Some(x) = b.sessions.get_mut(sid) {
                    apply(x, d, facts, now, None);
                }
            }) {
                tracing::warn!("[rotate] {}", e.logged());
            }
        }
        n
    }

    /// 判一次、落账、记进缓存；回这一判。
    fn run(
        &self,
        own: &Own,
        a: &Turn<'_>,
        s: &Session,
        loaded: &Loaded,
        facts: &[Acct],
        tried: &[String],
    ) -> Decision {
        let d = self.judge_session(s, loaded, facts, a.now, tried);
        if let Some(e) = &d.strategy_error {
            tracing::warn!("[rotate] 策略「{}」这一判走了兜底：{}", loaded.id, e.msg);
        }
        let n = self.settle(own, a.sid, s, &d, facts, a.now);
        let mut c = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        c.runs += 1;
        if tried.is_empty() {
            c.map.insert(
                a.sid.to_string(),
                Cached {
                    snap: snap(&n),
                    rev: loaded.rev().to_string(),
                    pick: d.pick.clone(),
                    tn: d.tn,
                    when: d.sigma.wake_when.clone(),
                    own_reading: false,
                },
            );
        } else {
            c.forget(a.sid);
        }
        d
    }

    /// 缓存里这个会话的结论还作数吗（作数 ⇒ 那个结论）。
    fn cached(&self, a: &Turn<'_>, s: &Session, loaded: &Loaded) -> Option<Pick> {
        let mut c = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        c.drain();
        let hit = c.map.get(a.sid)?;
        let fresh =
            hit.snap == snap(s) && hit.rev == loaded.rev() && hit.tn.is_none_or(|t| a.now < t);
        if !fresh {
            c.forget(a.sid);
            return None;
        }
        if hit.own_reading {
            let when = hit.when.clone();
            let using = match &hit.pick {
                Pick::Use(x) => Some(x.clone()),
                Pick::Hold { .. } => None,
            };
            drop(c);
            let lib = self.library();
            let fact = |acct: &str| {
                Acct::of_fact(&self.fact_one(
                    a.agent,
                    &lib,
                    acct,
                    a.row,
                    a.now,
                    &crate::Tz::default(),
                ))
            };
            // 自己的读数只在越过约的读数时作废；读数让在用的号事实上发不出去了（超额在兜 · 被拒）也作废 —— 那是事实变了。
            let crossed = when
                .iter()
                .any(|w| standing::used_at(&fact(&w.acct), w.w, a.now) >= w.used_at_least)
                || using.is_some_and(|x| !standing::sendable(&fact(&x), a.now));
            let mut c = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if crossed {
                c.forget(a.sid);
                return None;
            }
            let hit = c.map.get_mut(a.sid)?;
            hit.own_reading = false;
            return Some(hit.pick.clone());
        }
        Some(hit.pick.clone())
    }

    /// 照结论备料：停发 ⇒ 这一家的「用满」回包；换到的号接不上 ⇒ 记进 `tried` 再判（同 429），还是没有 ⇒ 照开会话用的号走。
    #[allow(clippy::too_many_arguments)]
    fn go_for(
        &self,
        own: &Own,
        a: &Turn<'_>,
        lib: &Library,
        s: &Session,
        loaded: &Loaded,
        mut pick: Pick,
        mut tried: Vec<String>,
    ) -> Go {
        let mut s = s.clone();
        for _ in 0..PREPARE_TRIES {
            match pick {
                Pick::Hold {
                    until: Some(at),
                    account,
                } => {
                    let Some(reply) = crate::agents::limit_reply_of(a.agent) else {
                        return Go::Start;
                    };
                    // 回包里卡着的那个窗口：在这一刻重置的那一窗（先看到时最早恢复的那个号）。
                    let facts = self.accts(&s, &loaded.settings, a.row, a.now);
                    let win = |f: &Acct| (0..2).find(|w| f.resets[*w] == Some(at));
                    let w = facts
                        .iter()
                        .find(|f| account.as_deref() == Some(f.id.as_str()))
                        .and_then(win)
                        .or_else(|| facts.iter().find_map(win));
                    return Go::Hold {
                        reply: reply(at, a.now, w.map(|w| if w == 0 { "5h" } else { "7d" })),
                    };
                }
                Pick::Hold { until: None, .. } => return Go::Start,
                Pick::Use(x) => match self.prepare(own, a, lib, &x) {
                    Ok(go) => return go,
                    Err(_) => {
                        tried.push(x);
                        let facts = self.accts(&s, &loaded.settings, a.row, a.now);
                        let d = self.run(own, a, &s, loaded, &facts, &tried);
                        apply(&mut s, &d, &facts, a.now, None);
                        pick = d.pick;
                    }
                },
            }
        }
        Go::Start
    }

    /// ★ 发之前：缓存的结论还作数就照它（不叫策略）；不作数 ⇒ 判一次。换到别的号 ⇒ 备那个号的料；停发 ⇒ 回「用满」。
    pub(crate) fn steer(&self, a: &Turn<'_>, own: &Own) -> Go {
        if a.sid.is_empty() || crate::agents::login_of(a.agent).is_none() {
            return Go::Start;
        }
        let lib = self.library();
        if !lib.enabled {
            return Go::Start;
        }
        let Some(s) = self.entry(own, a) else {
            return Go::Start;
        };
        // 这个会话不轮换（没选策略）⇒ 只用开会话的号，不叫策略。
        if s.strategy.id.is_empty() {
            return Go::Start;
        }
        let loaded = self.load(&s.strategy);
        let (pick, s) = match self.cached(a, &s, &loaded) {
            Some(p) => (p, s),
            None => {
                let facts = self.accts(&s, &loaded.settings, a.row, a.now);
                let d = self.run(own, a, &s, &loaded, &facts, &[]);
                let mut n = s.clone();
                apply(&mut n, &d, &facts, a.now, None);
                (d.pick, n)
            }
        };
        self.go_for(own, a, &lib, &s, &loaded, pick, Vec::new())
    }

    /// ★ 回包被拒：在同一发里带着 `tried`（这一发已经发过的号）再判一次（算法 #27）。别的请求刚把这个会话换走了 ⇒ 跟上它。
    /// 一个号事实上都发不出去 ⇒ `None`（上游那份拒绝就是真的「用满」，中转原样交回）。
    pub(crate) fn on_refused(
        &self,
        a: &Turn<'_>,
        heard_from: &str,
        heard: &QuotaReading,
        tried: &[String],
        own: &Own,
    ) -> Option<Go> {
        self.quota.notify(Source::Refused, Some(a.sid));
        if a.sid.is_empty() || crate::agents::login_of(a.agent).is_none() {
            return None;
        }
        let lib = self.library();
        if !lib.enabled {
            return None;
        }
        let s = self.entry(own, a)?;
        if s.strategy.id.is_empty() {
            return None;
        }
        if s.current != heard_from && !tried.contains(&s.current) {
            if let Ok(go) = self.prepare(own, a, &lib, &s.current) {
                return Some(go);
            }
        }
        let loaded = self.load(&s.strategy);
        let mut facts = self.accts(&s, &loaded.settings, a.row, a.now);
        // 刚回来的那一发被拒：额度账未必已经记下，照它把那个号记成被拒。
        if let Some(x) = facts.iter_mut().find(|x| x.id == heard_from) {
            x.refused_until = Some(heard.resets_at.unwrap_or(a.now + 60).max(a.now + 1));
        }
        let mut tried = tried.to_vec();
        if !tried.iter().any(|t| t == heard_from) {
            tried.push(heard_from.to_string());
        }
        let d = self.run(own, a, &s, &loaded, &facts, &tried);
        // 号列表里的号事实上一个都发不出去 ⇒ 上游那份拒绝就是真的「用满」，原样交回（不另造一份）；
        // 有号发得出去、只是策略此刻不用它们 ⇒ 停发，回我们那份「用满」（重置时刻照策略交的）。
        let pool: Vec<&Acct> = {
            let ids: Vec<&str> = loaded
                .settings
                .get(crate::accounts::quota::rules::form::ACCOUNTS)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|r| {
                    if r.get("start") == Some(&Value::Bool(true)) {
                        Some(s.start.as_str())
                    } else {
                        r.get("id").and_then(Value::as_str)
                    }
                })
                .collect();
            facts
                .iter()
                .filter(|x| ids.is_empty() || ids.contains(&x.id.as_str()))
                .collect()
        };
        let all_shut = pool
            .iter()
            .all(|x| tried.contains(&x.id) || !standing::sendable(x, a.now));
        if all_shut && matches!(d.pick, Pick::Hold { .. }) {
            return None;
        }
        let mut n = s.clone();
        apply(&mut n, &d, &facts, a.now, None);
        Some(self.go_for(own, a, &lib, &n, &loaded, d.pick, tried))
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/upstream_select/turn_tests.rs"]
mod tests;

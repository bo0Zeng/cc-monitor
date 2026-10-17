//! ★ 中转发之前那一判（第五版）：叫会话用的那份策略、照它交的先后做动作；只在节点上判，
//! 结论缓存到下一个节点 —— 节点之间一发都不叫策略（算法 #28）。
//!
//! - 判（[`decide::turn`]）是纯的；这里拼它要的：会话那一条（`rotation.json`，带着它载入的那份副本 ⇒ [`Adopted`]）· 每个号的事实。
//! - 缓存作废：过了下一个已知节点 `tn` · 会话那一条的快照变了 · 用的那份（id · 版本 · 设置）变了 · 事实那几种节点（外来的读数 · 被拒 · 续期失败 ·
//!   `quota.json` · 账号库 / 登录文件）与策略目录 ⇒ 全部；这个会话自己的读数 ⇒ 只在越过它记着的 `wake.when` 某一条时。
//! - 结论跟盘上那一条不同才写（先在内存里试，没变不进锁）。

use super::rotate::{Go, Hop, Library, Turn};
use crate::accounts::quota::nodes::{Node, Nodes, Source};
use crate::accounts::quota::rotation;
use crate::accounts::quota::rules::book::{self, Session};
use crate::accounts::quota::rules::decide::{self, Decision, Pick, Sigma, WakeWhen};
use crate::accounts::quota::rules::{listed_ids, standing, Acct, Adopted};
use crate::agents::QuotaReading;
use crate::platform::fs::Own;
use serde_json::Value;
use std::collections::HashMap;

/// 会话那一条里，判要看的那几格（`seen` · 换号记录 · 「从几点起」不算）。
#[derive(Debug, Clone, PartialEq)]
struct Snap {
    agent: String,
    start: String,
    current: String,
    temp: Vec<String>,
    off: Vec<String>,
    memo: Option<Value>,
}

fn snap(s: &Session) -> Snap {
    Snap {
        agent: s.agent.clone(),
        start: s.start.clone(),
        current: s.current.clone(),
        temp: s.temp.clone(),
        off: s.off.clone(),
        memo: s.memo.clone(),
    }
}

/// 用的那份里，判要看的那几格。
type PlanKey = (String, String, Value);

fn key(p: &Adopted) -> PlanKey {
    (p.id.clone(), p.rev.clone(), p.settings.clone())
}

struct Cached {
    snap: Snap,
    plan: PlanKey,
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

/// 交给判的号：先照设置里的号列表（`{start: true}` ＝ 开会话用的号），再接其余的号（照账号库的先后）。兜底取的就是这个先后。
pub(crate) fn listed(settings: &Value, start: &str, mut all: Vec<Acct>) -> Vec<Acct> {
    let mut out: Vec<Acct> = Vec::new();
    for id in listed_ids(settings, start) {
        if let Some(i) = all.iter().position(|a| a.id == id) {
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
        off: s.off.clone(),
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
        me: Default::default(),
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

/// 锁里判的那一下：落账前 · 落账后的会话那一条 · 照它判的那一份 · 这一判 · 它看的事实。
struct InTurn {
    before: Session,
    after: Session,
    plan: Adopted,
    d: Decision,
    facts: Vec<Acct>,
}

/// 备料最多试几个号（每试不成一个就记进 `tried` 再判一次）。
const PREPARE_TRIES: usize = 8;

impl Hop {
    /// ★ 交给判的那一排事实（照 [`listed`] 的先后；开会话用的号不在账号库里也带上）。用速是每个号自己在额度账里量出来的，量不到就空着。
    /// `book` ＝ 判的时候看的那一份账本（别的会话此刻在哪个号上照它：同一把锁里判，前一个会话刚换到哪就算到哪）。
    pub(crate) fn accts(
        &self,
        sid: &str,
        s: &Session,
        book: &book::Book,
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
        for x in &mut all {
            x.others = self.others_on(&s.agent, sid, book, x, now);
        }
        listed(settings, &s.start, all)
    }

    /// ★ 别的会话压在 `x` 5 小时窗上的用速（点 / 分）＝ 号的总用速里不是这台的那一份 ＋ 这台别的会话此刻在它上面的用速。
    /// 这台的会话在哪个号上照 `book`（没记在账本里的照它最近一发答它的号）；用速按 `x` 的「1% ≈ 多少」折成点。
    /// 量不到号的总用速 · 1% ≈ 多少 · 窗口没开 ⇒ 空（不知道别人会不会用掉它）。
    fn others_on(
        &self,
        agent: &str,
        sid: &str,
        book: &book::Book,
        x: &Acct,
        now: u64,
    ) -> Option<f64> {
        let tpp = x.per_pct[0].filter(|t| *t > 0)? as f64;
        let o = self.quota.entry(agent, &x.id)?;
        let name = crate::accounts::quota::facts::slot_window(
            Some(&o.reading),
            "5h",
            now,
            &super::rotate::slot_fn(agent),
        )?;
        let total = o.load(&name, now)?;
        let spend = self.quota.spend();
        let away = (total - spend.here(agent, &x.id, now) / tpp).max(0.0);
        let near: f64 = spend
            .others(agent, sid, now)
            .iter()
            .filter(|(k, at, _)| {
                book.sessions
                    .get(k)
                    .filter(|b| b.agent == agent)
                    .map_or(at.as_str(), |b| b.current.as_str())
                    == x.id
            })
            .map(|(_, _, pace)| pace / tpp)
            .sum();
        Some(away + near)
    }

    /// 判的会话状态：会话那一条 ＋ 本会话此刻量出来的。
    pub(crate) fn sigma(&self, sid: &str, s: &Session, now: u64) -> Sigma {
        Sigma {
            me: self.quota.spend().me(sid, now),
            ..sigma_of(s)
        }
    }

    /// 照会话那一条判一次（纯：不写、不碰缓存）。
    pub(crate) fn judge_session(
        &self,
        sid: &str,
        s: &Session,
        plan: &Adopted,
        facts: &[Acct],
        now: u64,
        tried: &[String],
    ) -> Decision {
        decide::turn(
            &self.sigma(sid, s, now),
            facts,
            &plan.settings,
            plan,
            crate::accounts::quota::rules::clock_at(&plan.tz, now),
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
        // 新会话载入「新会话默认」那一份（读文件在锁外）。
        let copy = if book.sessions.contains_key(a.sid) {
            None
        } else {
            self.adoptions.for_new()
        };
        match rotation::relay_change(&self.store, own, |b| {
            b.saw(a.sid, a.agent, a.start, a.now, || copy.clone());
            b.sessions.get(a.sid).cloned()
        }) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("[rotate] {}", e.logged());
                // 写不成（盘上还是旧版 · 读不懂）：照内存里那一份接着判，不记。
                let mut b = book;
                b.saw(a.sid, a.agent, a.start, a.now, || copy);
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

    /// 判一次（不落账）；回这一判。
    fn judge(
        &self,
        a: &Turn<'_>,
        s: &Session,
        plan: &Adopted,
        facts: &[Acct],
        tried: &[String],
    ) -> Decision {
        let d = self.judge_session(a.sid, s, plan, facts, a.now, tried);
        if let Some(e) = &d.strategy_error {
            tracing::warn!("[rotate] 策略「{}」这一判走了兜底：{}", plan.id, e.msg);
        }
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).runs += 1;
        d
    }

    /// 这一判成了（换到的号备好了料 · 停发）⇒ 落账、记进缓存（带着 `tried` 判的不缓存：下一发照事实重判）。
    #[allow(clippy::too_many_arguments)]
    fn commit(
        &self,
        own: &Own,
        a: &Turn<'_>,
        s: &Session,
        plan: &Adopted,
        d: &Decision,
        facts: &[Acct],
        tried: &[String],
    ) {
        let n = self.settle(own, a.sid, s, d, facts, a.now);
        self.remember(a, &n, plan, d, tried);
    }

    /// 落了账的这一判记进缓存（带着 `tried` 判的不缓存：下一发照事实重判）。
    fn remember(&self, a: &Turn<'_>, n: &Session, plan: &Adopted, d: &Decision, tried: &[String]) {
        let mut c = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if tried.is_empty() {
            c.map.insert(
                a.sid.to_string(),
                Cached {
                    snap: snap(n),
                    plan: key(plan),
                    pick: d.pick.clone(),
                    tn: d.tn,
                    when: d.sigma.wake_when.clone(),
                    own_reading: false,
                },
            );
        } else {
            c.forget(a.sid);
        }
    }

    /// 缓存里这个会话的结论还作数吗（作数 ⇒ 那个结论）。
    fn cached(&self, a: &Turn<'_>, s: &Session, plan: &Adopted) -> Option<Pick> {
        let mut c = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        c.drain();
        let hit = c.map.get(a.sid)?;
        let fresh =
            hit.snap == snap(s) && hit.plan == key(plan) && hit.tn.is_none_or(|t| a.now < t);
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

    /// 照结论备料：停发 ⇒ 这一家的「用满」回包；换到的号接不上 ⇒ 记进 `tried`、照同一份会话与事实再判（同 429），还是没有 ⇒ 照开会话用的号走。
    /// 只有成了的那一判落账（`judged` ＝ 这一判与它看的事实；照缓存来的 ⇒ 空）：接不上的号没换成，不记。
    #[allow(clippy::too_many_arguments)]
    fn go_for(
        &self,
        own: &Own,
        a: &Turn<'_>,
        lib: &Library,
        s: &Session,
        plan: &Adopted,
        mut pick: Pick,
        mut judged: Option<(Decision, Vec<Acct>)>,
        mut tried: Vec<String>,
    ) -> Go {
        let commit = |judged: &Option<(Decision, Vec<Acct>)>, tried: &[String]| {
            if let Some((d, facts)) = judged {
                self.commit(own, a, s, plan, d, facts, tried);
            }
        };
        for _ in 0..PREPARE_TRIES {
            match pick {
                Pick::Hold {
                    until: Some(at),
                    account,
                } => {
                    commit(&judged, &tried);
                    let Some(reply) = crate::agents::limit_reply_of(a.agent) else {
                        return Go::Start;
                    };
                    // 回包里卡着的那个窗口：在这一刻重置的那一窗（先看到时最早恢复的那个号）。
                    let facts =
                        self.accts(a.sid, s, &self.store.now(), &plan.settings, a.row, a.now);
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
                Pick::Hold { until: None, .. } => {
                    commit(&judged, &tried);
                    return Go::Start;
                }
                Pick::Use(x) => match self.prepare(own, a, lib, &x) {
                    Ok(go) => {
                        commit(&judged, &tried);
                        return go;
                    }
                    Err(_) => {
                        tried.push(x);
                        let facts = match judged.take() {
                            Some((_, f)) => f,
                            None => self.accts(
                                a.sid,
                                s,
                                &self.store.now(),
                                &plan.settings,
                                a.row,
                                a.now,
                            ),
                        };
                        let d = self.judge(a, s, plan, &facts, &tried);
                        pick = d.pick.clone();
                        judged = Some((d, facts));
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
        let plan = Adopted::of(s.uses.as_ref());
        // 这个会话不轮换（没选策略）⇒ 只用开会话的号，不叫策略。
        if plan.id.is_empty() {
            return Go::Start;
        }
        if let Some(p) = self.cached(a, &s, &plan) {
            return self.go_for(own, a, &lib, &s, &plan, p, None, Vec::new());
        }
        if let Some(j) = self.judge_in_turn(own, a) {
            let InTurn {
                before,
                after,
                plan,
                d,
                facts,
            } = j;
            let to = match &d.pick {
                Pick::Use(x) if *x != before.current => x.clone(),
                _ => {
                    self.remember(a, &after, &plan, &d, &[]);
                    return self.go_for(own, a, &lib, &after, &plan, d.pick, None, Vec::new());
                }
            };
            if let Ok(go) = self.prepare(own, a, &lib, &to) {
                self.remember(a, &after, &plan, &d, &[]);
                return go;
            }
            // 换到的号接不上：那一笔撤回（没换成的不记），照同一份事实带着它再判（同 429）。
            self.undo(own, a.sid, &after, &before);
            let tried = vec![to];
            let d = self.judge(a, &before, &plan, &facts, &tried);
            let pick = d.pick.clone();
            return self.go_for(own, a, &lib, &before, &plan, pick, Some((d, facts)), tried);
        }
        // 账本写不成：照锁外那一份判，成了再落账（写不进去就只这一发照它走）。
        let facts = self.accts(a.sid, &s, &self.store.now(), &plan.settings, a.row, a.now);
        let d = self.judge(a, &s, &plan, &facts, &[]);
        let pick = d.pick.clone();
        self.go_for(own, a, &lib, &s, &plan, pick, Some((d, facts)), Vec::new())
    }

    /// ★ 各会话的判依次来：在账本那把跨进程的锁里读会话那一条与别的会话此刻在哪个号上 → 判 → 落账，
    /// 后一个会话判时看得见前一个刚换到哪（[`Self::others_on`] 照这一份账本）。账本写不成 · 会话不轮换 ⇒ `None`。
    fn judge_in_turn(&self, own: &Own, a: &Turn<'_>) -> Option<InTurn> {
        let r = rotation::relay_change(&self.store, own, |b| {
            let before = b.sessions.get(a.sid)?.clone();
            let plan = Adopted::of(before.uses.as_ref());
            if plan.id.is_empty() {
                return None;
            }
            let facts = self.accts(a.sid, &before, b, &plan.settings, a.row, a.now);
            let d = self.judge(a, &before, &plan, &facts, &[]);
            let n = b.sessions.get_mut(a.sid)?;
            apply(n, &d, &facts, a.now, None);
            Some(InTurn {
                after: n.clone(),
                before,
                plan,
                d,
                facts,
            })
        });
        r.unwrap_or_else(|e| {
            tracing::warn!("[rotate] {}", e.logged());
            None
        })
    }

    /// 撤回锁里落的那一笔：会话那一条还是落账后的样子 ⇒ 换回落账前的（别人又改过 ⇒ 不动）。
    fn undo(&self, own: &Own, sid: &str, after: &Session, before: &Session) {
        if let Err(e) = rotation::relay_change(&self.store, own, |b| {
            if b.sessions.get(sid) == Some(after) {
                b.sessions.insert(sid.to_string(), before.clone());
            }
        }) {
            tracing::warn!("[rotate] {}", e.logged());
        }
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
        let plan = Adopted::of(s.uses.as_ref());
        if plan.id.is_empty() {
            return None;
        }
        if s.current != heard_from && !tried.contains(&s.current) {
            if let Ok(go) = self.prepare(own, a, &lib, &s.current) {
                return Some(go);
            }
        }
        let mut facts = self.accts(a.sid, &s, &self.store.now(), &plan.settings, a.row, a.now);
        // 刚回来的那一发被拒：额度账未必已经记下，照它把那个号记成被拒。
        if let Some(x) = facts.iter_mut().find(|x| x.id == heard_from) {
            x.refused_until = Some(heard.resets_at.unwrap_or(a.now + 60).max(a.now + 1));
        }
        let mut tried = tried.to_vec();
        if !tried.iter().any(|t| t == heard_from) {
            tried.push(heard_from.to_string());
        }
        let d = self.judge(a, &s, &plan, &facts, &tried);
        // 号列表里的号事实上一个都发不出去 ⇒ 上游那份拒绝就是真的「用满」，原样交回（不另造一份）；
        // 有号发得出去、只是策略此刻不用它们 ⇒ 停发，回我们那份「用满」（重置时刻照策略交的）。
        let pool: Vec<&Acct> = {
            let ids = listed_ids(&plan.settings, &s.start);
            facts
                .iter()
                .filter(|x| ids.is_empty() || ids.contains(&x.id.as_str()))
                .collect()
        };
        let all_shut = pool
            .iter()
            .all(|x| tried.contains(&x.id) || !standing::sendable(x, a.now));
        if all_shut && matches!(d.pick, Pick::Hold { .. }) {
            self.commit(own, a, &s, &plan, &d, &facts, &tried);
            return None;
        }
        let pick = d.pick.clone();
        Some(self.go_for(own, a, &lib, &s, &plan, pick, Some((d, facts)), tried))
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/accounts/upstream_select/turn_tests.rs"]
mod tests;

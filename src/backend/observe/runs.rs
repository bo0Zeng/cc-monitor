//! **运行簿**：一个会话 ＝ 一个主运行 ＋ 零到多个子运行（被委派出去的那几次）。这里只认「运行」，不认任何一家的形状 ——
//! 记录属于哪个运行、哪条记录派出了子运行、子运行的记录住哪、流与记录拿什么对账，都问适配层给的那一组面（[`RunFaces`]）。
//!
//! 两半：
//! - [`RunBook`]：每条流连接一本，watcher 写、流归位（`stream::run_route`）读：每个会话的运行表 ＋ 对账键归哪个运行。
//! - [`RunTrack`]：watcher 那一侧的手 —— 子运行的记录文件在哪（`ChildFace::sources`）、读到哪了；与主记录走同一条文件事件管线，不轮询。

use crate::agents::{ChildLink, RunEnd, RunFaces, RunMark};
use crate::common::cells::Words;
use crate::platform::fs::Ro;
use crate::platform::paths::path_key;
use crate::wire::{Frame, RunDid, RunEnded, RunInfo, RunState, RunWhy};
use cmd_wire::frame::{RunEndLine, RunExplain, RunGroups, RunLine};
use copy_core::copy_text;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::io::{Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// 每个会话最多记几个运行（超了先丢最久没动静的已收场的，再丢最久没动静的）。
pub(crate) const RUNS_KEEP: usize = 64;
/// 每个会话最多记住几个被挤出运行表的已收场运行（先进先出）：挤出只是不再列，「它已经收场了」不许跟着忘。
pub(crate) const ENDED_KEEP: usize = 4096;
/// 每个会话记住几个「这次工具调用派出了子运行、标签是什么」（先进先出）。
pub(crate) const LINKS_KEEP: usize = 256;
/// 每个会话记住几个对账键的归属（先进先出）。
pub(crate) const RIDS_KEEP: usize = 512;
/// 一个子运行没有任何收场信号、它的记录又这么久没再写 ⇒ 「状态不明」，不当它在跑。到点就判（watcher 有在跑的运行时
/// 至多等到最早那个期限，[`next_event`]），读记录 / 出帧时也判。比一次前台工具调用最长的等待（10 分钟）再宽一截。
#[allow(
    clippy::disallowed_methods,
    reason = "子运行「久未再写 ⇒ 状态不明」的阈值（`observe::runs::STALE_AFTER`）"
)]
pub(crate) const STALE_AFTER: Duration = Duration::from_millis(15 * 60 * 1000);
/// 失败收场的报错原话至多留几个字（运行表一变就整份重发）。
pub(crate) const ERROR_CHARS: usize = 2000;
/// 列表里「最近结束」那一组至多几个（在跑的不限：同一时刻在跑的本来就有限）。
pub(crate) const RECENT_ENDED: usize = 5;
/// 监控板 peek 至多列几个子运行（余下的只给个数那一截）。
pub(crate) const PEEK_SHOWN: usize = 8;

/// 自 1970 起的毫秒（线上那三个时刻）。
fn ms(t: SystemTime) -> Option<u64> {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_millis()).ok())
}

fn capped(e: Option<String>) -> Option<String> {
    e.map(|e| e.chars().take(ERROR_CHARS).collect())
}

/// 一次派出（父侧那次工具调用）：标签 · 类别 · 派出它的那个子运行（主运行 ⇒ `None`）· 那条记录的时刻（毫秒）。
#[derive(Clone)]
struct Spawn {
    label: Option<String>,
    kind: Option<String>,
    parent: Option<String>,
    at: Option<u64>,
}

/// 簿里的一个运行：线上那一格 ＋ 它最近一次动静的时刻（子记录的写入时刻；只有父侧说到过 ⇒ 说到它的那份记录的时刻）。
struct Run {
    info: RunInfo,
    seen: SystemTime,
    /// 它自己的记录里最近一次应答的对账键。
    rid: Option<String>,
    /// 收场了 ⇒ 怎么收的（与 `info.state` 是不是三种收场之一同进退）。
    closed: Option<Closed>,
}

/// 收场那一刻：它最近一次应答的对账键 · 收场的时刻 · 是不是它自己的记录写出的终局。
#[derive(Clone)]
struct Closed {
    rid: Option<String>,
    at: SystemTime,
    own: bool,
}

impl Closed {
    /// 收场之后读到它的一条记录（对账键 `rid`、写于 `seen`）：是不是又一次新的应答（被续跑了）。
    /// 收场只粘同一轮 —— 自己写过终局 ⇒ 之后别的应答都是新的一轮；只有派出那一方说过 ⇒ 要比那一刻晚写的才算。
    fn resumed_by(&self, rid: Option<&str>, seen: SystemTime) -> bool {
        rid.is_some() && rid != self.rid.as_deref() && (self.own || seen > self.at)
    }
}

/// 被挤出运行表的已收场运行（[`ENDED_KEEP`]）：线上那一格（派出它的那次工具调用 · 终态 · 几个时刻）· 判续跑要的那一格。
struct Gone {
    info: RunInfo,
    closed: Closed,
}

fn state_of(e: RunEnd) -> RunState {
    match e {
        RunEnd::Done => RunState::Done,
        RunEnd::Failed => RunState::Failed,
        RunEnd::Stopped => RunState::Stopped,
    }
}

#[derive(Default)]
struct Sess {
    runs: Vec<Run>,
    /// 被挤出 `runs` 的已收场运行（`gone_order` 先进先出）。一个运行只在两处之一。
    gone: HashMap<String, Gone>,
    gone_order: VecDeque<String>,
    /// 有运行没留下就被忘了（在跑的被挤出 `runs` · `gone` 超了 [`ENDED_KEEP`]）⇒ 总数只能说「至少」。
    lost: bool,
    /// 父侧工具调用 id ⇒ 那次派出：调用先到、子运行是哪个后到。
    labels: HashMap<String, Spawn>,
    label_order: VecDeque<String>,
    /// 对账键 ⇒ 哪个运行（`None` ＝ 主运行）。
    rids: HashMap<String, Option<String>>,
    rid_order: VecDeque<String>,
    /// 子运行 id ⇒ 它挂着的最早那一条待答请求（待答登记表交来的，[`RunBook::dress_needs`]）。
    asks: HashMap<String, crate::agents::asks::Ask>,
}

impl Sess {
    fn find(&mut self, run: &str) -> Option<&mut Run> {
        self.runs.iter_mut().find(|r| r.info.run == run)
    }

    /// 表里的这个运行；不在 ⇒ 立一个（先查被挤出的那张：收过场的照它的终态立回来，不当新的在跑）。
    fn slot(&mut self, run: &str, seen: SystemTime) -> &mut Run {
        if let Some(i) = self.runs.iter().position(|r| r.info.run == run) {
            return &mut self.runs[i];
        }
        let back = self.gone.remove(run);
        if back.is_some() {
            self.gone_order.retain(|g| g != run);
        }
        if self.runs.len() >= RUNS_KEEP {
            let oldest = |any: bool| {
                self.runs
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| any || r.info.state != RunState::Running)
                    .min_by_key(|(_, r)| r.seen)
                    .map(|(i, _)| i)
            };
            let victim = oldest(false).or_else(|| oldest(true)).unwrap_or(0);
            let v = self.runs.remove(victim);
            match v.closed {
                Some(closed) => self.bury(v.info, closed),
                None => self.lost = true,
            }
        }
        let (info, closed) = match back {
            Some(g) => (g.info, Some(g.closed)),
            None => (
                RunInfo {
                    run: run.to_string(),
                    ..RunInfo::default()
                },
                None,
            ),
        };
        self.runs.push(Run {
            info,
            seen,
            rid: None,
            closed,
        });
        self.runs.last_mut().expect("just pushed")
    }

    fn bury(&mut self, info: RunInfo, closed: Closed) {
        let run = info.run.clone();
        if self
            .gone
            .insert(run.clone(), Gone { info, closed })
            .is_none()
        {
            self.gone_order.push_back(run);
            while self.gone_order.len() > ENDED_KEEP {
                if let Some(old) = self.gone_order.pop_front() {
                    self.gone.remove(&old);
                    self.lost = true;
                }
            }
        }
    }

    fn learn_rid(&mut self, rid: String, owner: Option<String>) {
        if self.rids.insert(rid.clone(), owner).is_none() {
            self.rid_order.push_back(rid);
            while self.rid_order.len() > RIDS_KEEP {
                if let Some(old) = self.rid_order.pop_front() {
                    self.rids.remove(&old);
                }
            }
        }
    }

    /// 子运行自己的一条记录（对账键 `rid`，`seen` ＝ 那份记录的写入时刻，`when` ＝ 记录自己写着的时刻）。先到的收场信号算数；
    /// 收场只粘同一轮（[`Closed::resumed_by`]：被续跑 ⇒ 回到在跑）。
    fn mark(
        &mut self,
        m: RunMark,
        rid: Option<String>,
        seen: SystemTime,
        when: Option<u64>,
    ) -> bool {
        if let Some(g) = self.gone.get_mut(&m.run) {
            if !g.closed.resumed_by(rid.as_deref(), seen) {
                if m.end.is_some() {
                    g.closed = Closed {
                        rid: rid.or(g.closed.rid.take()),
                        at: seen,
                        own: true,
                    };
                }
                return false;
            }
        }
        let r = self.slot(&m.run, seen);
        let before = r.info.clone();
        r.seen = seen;
        let w = when;
        r.info.active_ms = r.info.active_ms.max(w);
        r.info.started_ms = match (r.info.started_ms, w) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        if r.closed
            .as_ref()
            .is_some_and(|c| c.resumed_by(rid.as_deref(), seen))
        {
            r.closed = None;
            r.info.ended_ms = None;
            r.info.why = None;
            r.info.error = None;
        }
        match (&r.closed, m.end) {
            (None, None) => r.info.state = RunState::Running,
            (None, Some(e)) => {
                r.info.state = state_of(e);
                r.info.why = Some(RunWhy::Own);
                r.info.ended_ms = w;
                r.info.error = if e == RunEnd::Failed {
                    capped(m.error.clone())
                } else {
                    None
                };
            }
            (Some(_), _) => {}
        }
        if m.end.is_some() {
            r.closed = Some(Closed {
                rid: rid.clone().or(r.rid.clone()),
                at: seen,
                own: true,
            });
        }
        if rid.is_some() {
            r.rid = rid;
        }
        if r.info.state != RunState::Running {
            r.info.waiting = None;
        } else if let Some(crate::agents::RunDid::Tool { name }) = &m.did {
            r.info.waiting = Some(name.clone());
        } else if m.answered || m.did.is_some() {
            r.info.waiting = None;
        }
        if matches!(m.did, Some(crate::agents::RunDid::Tool { .. })) {
            r.info.calls = r.info.calls.saturating_add(1);
        }
        if m.did.is_some() {
            r.info.last = m.did;
        }
        r.info != before
    }

    /// 派出那一方说它收场了（`at` 判续跑用，`when` ＝ 那条记录自己写着的时刻）。已经收过场的不动。
    fn end(
        r: &mut Run,
        e: Option<RunEnd>,
        error: Option<String>,
        at: SystemTime,
        when: Option<u64>,
    ) {
        if let (Some(e), None) = (e, &r.closed) {
            r.info.state = state_of(e);
            r.info.why = Some(RunWhy::Reported);
            r.info.ended_ms = when;
            r.info.waiting = None;
            r.info.error = if e == RunEnd::Failed {
                capped(error)
            } else {
                None
            };
            r.closed = Some(Closed {
                rid: r.rid.clone(),
                at,
                own: false,
            });
        }
    }

    /// 一次派出记到运行上：标签与类别（派出那一格有才盖）· 派出它的那个子运行 · 开始的时刻（取早的）。
    fn spawned(r: &mut Run, sp: &Spawn) {
        r.info.label = sp.label.clone().or(r.info.label.take());
        r.info.kind = sp.kind.clone().or(r.info.kind.take());
        // 分叉出去的子运行，记录里抄着派它的那次调用、记成它自己的 ⇒ 那一句说的「谁派的」是它自己，不算。
        if sp.parent.as_deref() != Some(r.info.run.as_str()) {
            r.info.parent = sp.parent.clone();
        }
        let at = sp.at;
        r.info.started_ms = match (r.info.started_ms, at) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
    }

    /// 说到子运行的一条（`at` ＝ 那条记录的时刻：来自子记录 ⇒ 那份记录的写入时刻；`when` ＝ 记录自己写着的时刻；
    /// `from` ＝ 这条记录是哪个子运行写的，主运行 ⇒ `None`）。
    fn link(
        &mut self,
        l: ChildLink,
        at: SystemTime,
        when: Option<u64>,
        from: Option<&str>,
    ) -> bool {
        match (l.run, l.tool) {
            (Some(run), Some(tool)) => {
                // 被挤出表的已收场运行：只补上是哪次调用派出的（收场是粘的，不回表）。
                if let Some(g) = self.gone.get_mut(&run) {
                    let changed = g.info.tool.as_deref() != Some(tool.as_str());
                    g.info.tool = Some(tool);
                    return changed;
                }
                // 派出那一次没读到（派它的那份记录没读）⇒ 照对上那一形自己带的（子运行那一侧落盘的那一份）。
                let known = self.labels.get(&tool).cloned().or_else(|| {
                    (l.label.is_some() || l.kind.is_some() || l.from.is_some()).then(|| Spawn {
                        label: l.label.clone(),
                        kind: l.kind.clone(),
                        parent: l.from.clone(),
                        at: None,
                    })
                });
                let r = self.slot(&run, at);
                let before = r.info.clone();
                r.info.tool = Some(tool);
                r.info.background |= l.background;
                if let Some(sp) = &known {
                    Self::spawned(r, sp);
                }
                Self::end(r, l.end, l.error, at, when);
                r.info != before
            }
            // 只说收场：认识的运行才算（同一种通知也说别的后台任务，那些不是子运行）。
            (Some(run), None) => match self.find(&run) {
                Some(r) => {
                    let before = r.info.clone();
                    Self::end(r, l.end, l.error, at, when);
                    r.info != before
                }
                None => false,
            },
            (None, Some(tool)) => {
                let sp = Spawn {
                    label: l.label,
                    kind: l.kind,
                    parent: from.map(str::to_string),
                    at: when,
                };
                if self.labels.insert(tool.clone(), sp.clone()).is_none() {
                    self.label_order.push_back(tool.clone());
                    while self.label_order.len() > LINKS_KEEP {
                        if let Some(old) = self.label_order.pop_front() {
                            self.labels.remove(&old);
                        }
                    }
                }
                let mut changed = false;
                for r in self
                    .runs
                    .iter_mut()
                    .filter(|r| r.info.tool.as_deref() == Some(&tool))
                {
                    let before = r.info.clone();
                    r.info.label = sp.label.clone();
                    r.info.kind = sp.kind.clone();
                    Self::spawned(r, &sp);
                    changed |= r.info != before;
                }
                changed
            }
            (None, None) => false,
        }
    }

    /// 在跑却久未再写的 ⇒ 状态不明（[`STALE_AFTER`]，到点即算）。
    fn settle(&mut self, now: SystemTime) -> bool {
        let mut changed = false;
        for r in &mut self.runs {
            let quiet = now.duration_since(r.seen).unwrap_or_default();
            if r.info.state == RunState::Running && quiet >= STALE_AFTER {
                r.info.state = RunState::Unknown;
                r.info.why = Some(RunWhy::Quiet);
                r.info.waiting = None;
                changed = true;
            }
        }
        changed
    }

    /// 运行表（最早动过的在前），每一项写好字、配上它挂着的待答请求（已等多久照 `now` 算）。
    fn table(&self, now: SystemTime) -> Vec<RunInfo> {
        let mut v: Vec<&Run> = self.runs.iter().collect();
        v.sort_by_key(|r| r.seen);
        let now_ms = ms(now).unwrap_or(0);
        v.iter()
            .map(|r| {
                let kids = v
                    .iter()
                    .filter(|k| k.info.parent.as_deref() == Some(r.info.run.as_str()))
                    .count();
                let mut i = dressed(r.info.clone(), kids);
                i.needs = self.asks.get(&r.info.run).and_then(|a| {
                    crate::observe::recognize::facts::dress_ask_with(None, Some(a.clone()), now_ms)
                });
                i
            })
            .collect()
    }

    /// 这条会话派过几个子运行：表里的 ＋ 被挤出的已收场的；漏过 ⇒ 第二格为真，这个数只是下界（此刻仍记着的都各不相同）。
    fn total(&self) -> Total {
        Total {
            n: self.runs.len() + self.gone.len(),
            at_least: self.lost,
        }
    }

    /// 被挤出表的已收场运行里对上了派出调用的那些（先挤出的在前）。
    fn ended(&self) -> Vec<RunEnded> {
        self.gone_order
            .iter()
            .filter_map(|run| {
                let g = self.gone.get(run)?;
                Some(RunEnded {
                    run: run.clone(),
                    tool: g.info.tool.clone()?,
                    state: g.info.state,
                    status_text: status_text(g.info.state),
                })
            })
            .collect()
    }

    /// 最早那个在跑的运行到「久未动静」的时刻（没有在跑的 ⇒ `None`）。
    fn due(&self) -> Option<SystemTime> {
        self.runs
            .iter()
            .filter(|r| r.info.state == RunState::Running)
            .map(|r| r.seen + STALE_AFTER)
            .min()
    }
}

/// 这台的运行簿（整台一本：watcher 写、各条流连接的流归位读，read8a 起）。
#[derive(Default)]
pub struct RunBook {
    inner: Mutex<HashMap<String, Sess>>,
    /// 谁要知道「记录到了 · 会话走了」（活卡，`stream::tap::adopt` 装一次；没装 ⇒ 不说）。
    watch: std::sync::OnceLock<Arc<dyn BookWatch>>,
}

/// 运行簿往外说的两件事（活卡听）：一条带对账键的记录过了（主或子运行）· 一个会话走了。
/// 在运行簿的锁外调（听的一方会回头问运行簿）。
pub(crate) trait BookWatch: Send + Sync {
    /// `owner`：这个对账键归哪个运行（`Some(None)` ＝ 主运行；`None` ＝ 说不出）。
    fn rid(&self, sid: &str, rid: &str, owner: Option<Option<String>>);
    fn gone(&self, sid: &str);
}

impl RunBook {
    fn with<T>(&self, f: impl FnOnce(&mut HashMap<String, Sess>) -> T) -> T {
        f(&mut self.inner.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// 一条记录（`child` 有 ⇒ 它来自子运行的记录文件，值是那份文件的写入时刻）。回：这个会话的运行表变没变。
    pub(crate) fn record(
        &self,
        faces: &RunFaces,
        sid: &str,
        v: &Value,
        child: Option<SystemTime>,
    ) -> bool {
        let mark = faces.run_of(v);
        let rid = faces.response_id(v);
        let own_rid = rid.clone().filter(|_| mark.is_some());
        let links = faces.child_links(v);
        let now = crate::common::time::now();
        let (changed, heard) = self.with(|m| {
            let s = m.entry(sid.to_string()).or_default();
            let mut heard = None;
            if let Some(rid) = rid {
                let owner = match (&mark, child.is_some()) {
                    (Some(mk), _) => Some(Some(mk.run.clone())),
                    (None, false) => Some(None),
                    (None, true) => None,
                };
                if let Some(o) = &owner {
                    s.learn_rid(rid.clone(), o.clone());
                }
                heard = Some((rid, owner));
            }
            let mut changed = false;
            let at = child.unwrap_or(now);
            let when = faces.written(v).or_else(|| ms(at));
            let from = mark.as_ref().map(|mk| mk.run.clone());
            if let Some(mk) = mark {
                changed |= s.mark(mk, own_rid, at, when);
            }
            for l in links {
                changed |= s.link(l, at, when, from.as_deref());
            }
            changed |= s.settle(now);
            (changed, heard)
        });
        if let (Some((rid, owner)), Some(w)) = (heard, self.watch.get()) {
            w.rid(sid, &rid, owner);
        }
        changed
    }

    /// 父记录里说到子运行的那几行只记派出 / 收场（不学对账键：只读尾巴的流接上会话时补读已有的那一截用，[`RunTrack::prime`]）。
    /// 回：运行表变没变。
    pub(crate) fn said(&self, sid: &str, said: &[RunSaid]) -> bool {
        if said.is_empty() {
            return false;
        }
        let now = crate::common::time::now();
        self.with(|m| {
            let s = m.entry(sid.to_string()).or_default();
            let mut changed = false;
            for r in said {
                let when = r.when.or_else(|| ms(now));
                for l in &r.links {
                    changed |= s.link(l.clone(), now, when, None);
                }
            }
            changed
        })
    }

    /// 子运行记录那一侧说出的对上（派出那一刻落盘的，[`crate::agents::ChildFace::spawned`]；`at` ＝ 那份记录的写入时刻）。回：运行表变没变。
    pub(crate) fn spawned(&self, sid: &str, l: ChildLink, at: SystemTime) -> bool {
        self.with(|m| {
            m.entry(sid.to_string())
                .or_default()
                .link(l, at, ms(at), None)
        })
    }

    /// 这个子运行已经收场了（表里收过场的 · 被挤出的已收场的）⇒ 它最近一次动静记成 `seen`（它的记录最后写的那一刻），回真；
    /// 没收场 · 簿里没有 ⇒ 假。
    pub(crate) fn ended_at(&self, sid: &str, run: &str, seen: SystemTime) -> bool {
        self.with(|m| {
            let Some(s) = m.get_mut(sid) else {
                return false;
            };
            if s.gone.contains_key(run) {
                return true;
            }
            match s.find(run) {
                Some(r) if r.closed.is_some() => {
                    r.seen = seen;
                    true
                }
                _ => false,
            }
        })
    }

    /// 这个会话此刻有几个在跑的子运行。
    pub(crate) fn running(&self, sid: &str) -> usize {
        self.with(|m| {
            m.get(sid).map_or(0, |s| {
                s.runs
                    .iter()
                    .filter(|r| r.info.state == RunState::Running)
                    .count()
            })
        })
    }

    /// 这个对账键归哪个运行：没学到 ⇒ `None`；主运行 ⇒ `Some(None)`。
    pub(crate) fn owner_of(&self, sid: &str, rid: &str) -> Option<Option<String>> {
        self.with(|m| m.get(sid).and_then(|s| s.rids.get(rid).cloned()))
    }

    /// 这个会话的运行表那一帧（出帧前先按 `now` 判一次久未动静）。
    pub(crate) fn frame_at(&self, sid: &str, now: SystemTime) -> Frame {
        let (runs, ended, total) = self.with(|m| match m.get_mut(sid) {
            Some(s) => {
                s.settle(now);
                (s.table(now), s.ended(), s.total())
            }
            None => (Vec::new(), Vec::new(), Total::default()),
        });
        let groups = groups_of(&runs);
        let summary = summary_of(&runs, total);
        Frame::SessionRuns(crate::wire::SessionRuns {
            sid: sid.to_string(),
            runs,
            ended,
            groups,
            summary,
        })
    }

    /// 这个会话此刻挂着的待答请求（挂上的先后）⇒ 配到发它的那个运行上（一个运行只配最早那一条；主会话的不配）。
    /// 没有运行表的会话不记。回：配的变没变。
    pub(crate) fn dress_needs(&self, sid: &str, asks: Vec<crate::agents::asks::Ask>) -> bool {
        let mut by_run: HashMap<String, crate::agents::asks::Ask> = HashMap::new();
        for a in asks {
            if let Some(run) = a.run.as_ref().map(|r| r.id.clone()) {
                by_run.entry(run).or_insert(a);
            }
        }
        self.with(|m| match m.get_mut(sid) {
            Some(s) if s.asks != by_run => {
                s.asks = by_run;
                true
            }
            _ => false,
        })
    }

    /// 有运行表的会话（watcher 听到需手动变了，照它们逐个配 needs）。
    pub(crate) fn sids(&self) -> Vec<String> {
        self.with(|m| {
            let mut v: Vec<String> = m.keys().cloned().collect();
            v.sort();
            v
        })
    }

    /// 全部会话按 `now` 判一次久未动静：回运行表变了的那几个会话（排好序）。
    pub(crate) fn settle_all(&self, now: SystemTime) -> Vec<String> {
        self.with(|m| {
            let mut v: Vec<String> = m
                .iter_mut()
                .filter_map(|(sid, s)| s.settle(now).then(|| sid.clone()))
                .collect();
            v.sort();
            v
        })
    }

    /// 全部会话里最早那个在跑的运行到「久未动静」的时刻（没有在跑的 ⇒ `None`）。
    pub(crate) fn next_due(&self) -> Option<SystemTime> {
        self.with(|m| m.values().filter_map(Sess::due).min())
    }

    /// 这个会话的运行表（最早动过的在前）。
    pub(crate) fn runs(&self, sid: &str) -> Vec<RunInfo> {
        let now = crate::common::time::now();
        self.with(|m| m.get(sid).map(|s| s.table(now)).unwrap_or_default())
    }

    /// 会话退休了（派出它们的那一方没了）：还算在跑的再也等不到收场信号 ⇒ 状态不明。回：运行表变没变。
    pub(crate) fn orphan(&self, sid: &str) -> bool {
        self.with(|m| {
            let mut changed = false;
            for r in m.get_mut(sid).into_iter().flat_map(|s| s.runs.iter_mut()) {
                if r.info.state == RunState::Running {
                    r.info.state = RunState::Unknown;
                    r.info.why = Some(RunWhy::Orphaned);
                    r.info.waiting = None;
                    changed = true;
                }
            }
            changed
        })
    }

    /// 会话走了。
    pub(crate) fn forget(&self, sid: &str) {
        self.with(|m| m.remove(sid));
        if let Some(w) = self.watch.get() {
            w.gone(sid);
        }
    }

    /// 会话退休：还算在跑的子运行改成状态不明，表变了就回最后那一帧（交出去之后各家的跟踪再 `RunTrack::forget`）。
    pub(crate) fn retire(&self, sid: &str) -> Option<Frame> {
        let f = self.orphan(sid).then(|| self.frame(sid));
        if let Some(w) = self.watch.get() {
            w.gone(sid);
        }
        f
    }

    /// 这个会话此刻有没有子运行（宣告那一刻有就发一帧运行表）。
    pub(crate) fn has_runs(&self, sid: &str) -> bool {
        !self.runs(sid).is_empty()
    }

    /// 这个会话此刻的运行表那一帧（出帧前按此刻判一次久未动静）。
    pub(crate) fn frame(&self, sid: &str) -> Frame {
        self.frame_at(sid, crate::common::time::now())
    }

    /// 全部会话按 `now` 判一次久未动静：表变了的那几个各一帧。
    pub(crate) fn due_frames(&self, now: SystemTime) -> Vec<Frame> {
        self.settle_all(now)
            .into_iter()
            .map(|sid| self.frame_at(&sid, now))
            .collect()
    }

    /// 装听的一方（只一次；再装忽略）。
    pub(crate) fn watch_with(&self, w: Arc<dyn BookWatch>) {
        if self.watch.set(w).is_err() {
            tracing::warn!("运行簿的听者装了第二次 —— 忽略（一本运行簿只交给一份活卡）");
        }
    }
}

/// [`RunTrack::main_record`] 的答。
#[derive(Debug, Default)]
pub(crate) struct MainRecord {
    pub(crate) rid: Option<String>,
    /// 这条记录属于某个子运行（不是主运行一轮的收尾，也不该算主运行的事）。
    pub(crate) in_run: bool,
    pub(crate) changed: bool,
}

/// 一份子运行记录读到哪了 · 派出那一刻落盘的对上（`ChildFace::spawned`）收到了没有。
struct ChildCursor {
    sid: String,
    /// 盘上那一份（原样大小写；表里按 [`path_key`] 认）。
    path: PathBuf,
    consumed: u64,
    linked: bool,
}

/// watcher 那一侧的运行跟踪：运行面 ＋ 运行簿 ＋ 各会话的主记录路径 ＋ 子运行记录的游标。
/// 两张表都按 [`path_key`] 认路径（与 watcher 的游标同一种规范化）：主记录有时以折过的键交进来、文件事件总是原样，两边得对得上。
pub(crate) struct RunTrack {
    pub(crate) faces: RunFaces,
    pub(crate) book: Arc<RunBook>,
    parents: HashMap<String, Vec<PathBuf>>,
    children: HashMap<PathBuf, ChildCursor>,
    /// 到此刻从子运行记录读了多少字节（判据数它；收下会话那一趟照它判要不要把放掉的内存还回去）。
    child_read: u64,
}

impl RunTrack {
    pub(crate) fn new(faces: RunFaces, book: Arc<RunBook>) -> Self {
        Self {
            faces,
            book,
            parents: HashMap::new(),
            children: HashMap::new(),
            child_read: 0,
        }
    }

    /// 到此刻从子运行记录读了多少字节。
    pub(crate) fn child_bytes(&self) -> u64 {
        self.child_read
    }

    /// 这一家有没有子运行（没有 ⇒ 下面这些全是空转，watcher 直接跳过）。
    fn has_children(&self) -> bool {
        self.faces.children.is_some()
    }

    /// 一行主记录交出去之前：归属与派出链接记进簿里。回：（对账键, 它其实属于某个子运行, 运行表变没变）。
    pub(crate) fn main_record(&self, sid: &str, line: &crate::agents::Line<'_>) -> MainRecord {
        let none = MainRecord::default();
        if self.faces.response_id.is_none()
            && self.faces.child_link.is_none()
            && self.faces.run_of.is_none()
        {
            return none;
        }
        let Some(v) = line.value() else {
            return none;
        };
        MainRecord {
            changed: self.book.record(&self.faces, sid, v, None),
            rid: self.faces.response_id(v),
            in_run: self.faces.run_of(v).is_some(),
        }
    }

    /// 只读尾巴的流接上一个会话：它的主记录已有的那一截（`chunk`，从头起）里说到子运行的几行补进簿里 ——
    /// 接上之前派出 / 收场的那些，否则只剩子记录自己的那一半（标签没有、收场看不见）。只挑预筛命中的行解析（[`said_of`]）。
    /// 回：运行表变没变。
    pub(crate) fn prime(&self, sid: &str, chunk: &[u8]) -> bool {
        if !self.has_children() {
            return false;
        }
        let Some(last_nl) = chunk.iter().rposition(|&b| b == b'\n') else {
            return false;
        };
        let said: Vec<RunSaid> = chunk[..last_nl]
            .split(|&b| b == b'\n')
            .filter_map(|line| said_of(&self.faces, std::str::from_utf8(line).ok()?))
            .collect();
        self.book.said(sid, &said)
    }

    /// 同 [`Self::prime`]，那一截已经照 [`said_of`] 挑好了（冷开扫描图 · 骨架留的那几条，`record_scan`）。
    pub(crate) fn prime_said(&self, sid: &str, said: &[RunSaid]) -> bool {
        self.has_children() && self.book.said(sid, said)
    }

    /// 一个会话的主记录在这里（宣告 / 每次读到它时都说；只有头一次真去找）：把它此刻已有的子运行记录收进来。
    /// 簿里已经收场（父侧那一截在这之前补进来了，[`Self::prime`]）、记录又 [`STALE_AFTER`] 之内没再写的，只留游标、不读正文 ——
    /// 没读的那几格（工具调用次数 · 最近一件事）只有它自己的记录说得出；它再被写 ⇒ 文件事件照旧从 0 读起。其余从头读一遍。
    /// 之后新长出来的子运行记录靠它自己的文件事件进来（[`Self::on_path`]）。读得多 ⇒ 读完把放掉的内存还给系统。回：运行表变没变。
    pub(crate) fn adopt(&mut self, sid: &str, parent: &Path) -> bool {
        if !self.has_children() {
            return false;
        }
        let key = path_key(parent);
        let ps = self.parents.entry(sid.to_string()).or_default();
        if ps.contains(&key) {
            return false;
        }
        ps.push(key);
        let before = self.child_bytes();
        let changed = self.discover_under(sid, parent);
        crate::platform::proc::return_freed_memory(self.child_bytes() - before);
        changed
    }

    /// 收进 `parent` 底下此刻在盘上的子运行记录（已收进来的不动；收场又久未再写的不读，[`Self::adopt`]）。
    fn discover_under(&mut self, sid: &str, parent: &Path) -> bool {
        let mut changed = false;
        let now = crate::common::time::now();
        for c in self.faces.sources(parent) {
            let key = path_key(&c);
            if self.children.contains_key(&key) {
                continue;
            }
            let mut cur = ChildCursor {
                sid: sid.to_string(),
                path: c,
                consumed: 0,
                linked: false,
            };
            let written = Ro::metadata(&cur.path)
                .ok()
                .and_then(|m| m.modified().ok())
                .filter(|w| now.duration_since(*w).unwrap_or_default() >= STALE_AFTER);
            if let Some(written) = written {
                if let Some(l) = self.faces.spawned(&cur.path) {
                    let run = l.run.clone();
                    cur.linked = true;
                    changed |= self.book.spawned(sid, l, written);
                    if run.is_some_and(|r| self.book.ended_at(sid, &r, written)) {
                        self.children.insert(key, cur);
                        continue;
                    }
                }
            }
            self.children.insert(key.clone(), cur);
            changed |= self.read_child(&key);
        }
        changed
    }

    /// 一个文件事件：是（或刚成为）某个会话的子运行记录 ⇒ 读新行、记进簿里，回那个会话（运行表变没变）。
    /// 不是 ⇒ `None`（调用方照旧当主记录 / 别的东西处理）。
    pub(crate) fn on_path(&mut self, p: &Path) -> Option<(String, bool)> {
        if !self.has_children() {
            return None;
        }
        let key = path_key(p);
        if self.parents.values().any(|ps| ps.contains(&key)) {
            return None;
        }
        if !self.children.contains_key(&key) {
            // 新长出来的子运行记录：只认这一家说得出「属于哪份父记录」的形状，且只在那一份底下找（不是每个事件都把全部会话扫一遍）。
            let parent = self.faces.owner(p)?;
            let parent_key = path_key(&parent);
            let sid = self
                .parents
                .iter()
                .find(|(_, ps)| ps.contains(&parent_key))
                .map(|(sid, _)| sid.clone())?;
            let changed = self.discover_under(&sid, &parent);
            self.children.contains_key(&key).then_some(())?;
            return Some((sid, changed));
        }
        let sid = self.children.get(&key).map(|c| c.sid.clone())?;
        let changed = self.read_child(&key);
        Some((sid, changed))
    }

    /// 读一份子运行记录（表里的键 `key`）从游标到最后一个整行；变短了 ⇒ 从头读。
    fn read_child(&mut self, key: &Path) -> bool {
        let Some(cur) = self.children.get_mut(key) else {
            return false;
        };
        let p = cur.path.clone();
        let Ok(mut f) = Ro::open(&p) else {
            return false;
        };
        let meta = f.metadata().ok();
        let len = meta.as_ref().map_or(0, |m| m.len());
        let written = meta
            .and_then(|m| m.modified().ok())
            .unwrap_or_else(crate::common::time::now);
        // 派出那一刻落盘的对上：卡（父侧那次调用）在父侧结果回来之前就对上这个运行；那一份比子记录晚落 ⇒ 下次读到再问。
        let mut changed = false;
        if !cur.linked {
            if let Some(l) = self.faces.spawned(&p) {
                cur.linked = true;
                changed |= self.book.spawned(&cur.sid, l, written);
            }
        }
        if len < cur.consumed {
            cur.consumed = 0;
        }
        if f.seek(SeekFrom::Start(cur.consumed)).is_err() {
            return changed;
        }
        // 按块读到尾（第一次认领时是整份；不一次读进内存）：每块整行逐行记进簿里、游标跟着块走；末尾没写完的那截等下次。
        let sid = cur.sid.clone();
        let mut blocks =
            crate::common::append_read::Blocks::new(f, crate::common::append_read::READ_BLOCK);
        while let Ok(Some(whole)) = blocks.next() {
            cur.consumed += whole.len() as u64;
            self.child_read += whole.len() as u64;
            for line in whole[..whole.len() - 1].split(|&b| b == b'\n') {
                if let Some(v) = super::lines::parse(line) {
                    changed |= self.book.record(&self.faces, &sid, &v, Some(written));
                }
            }
        }
        changed
    }

    /// 会话走了：它的主记录路径、子运行游标、运行簿那一页一起摘。
    pub(crate) fn forget(&mut self, sid: &str) {
        self.parents.remove(sid);
        self.children.retain(|_, c| c.sid != sid);
        self.book.forget(sid);
    }
}

/// 主记录里说到子运行的一行留下的：它说的那几条派出 / 对上 / 收场 ＋ 它写下的时刻（毫秒；没写 ⇒ 补进簿那一刻算）。
/// 补簿只要这两样：冷开扫描图与骨架留它、不留那一行原文（[`skel`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunSaid {
    pub(crate) when: Option<u64>,
    pub(crate) links: Vec<ChildLink>,
}

impl RunSaid {
    /// 估算的常驻字节（扫描图的字节上限按它算）。
    pub(crate) fn bytes(&self) -> usize {
        let s = |v: &Option<String>| v.as_ref().map_or(0, String::len);
        32 + self
            .links
            .iter()
            .map(|l| {
                96 + s(&l.tool) + s(&l.label) + s(&l.kind) + s(&l.run) + s(&l.from) + s(&l.error)
            })
            .sum::<usize>()
    }
}

/// 一行主记录（原文）说到子运行了没有：过了预筛、整行解得出、说出了派出 / 对上 / 收场 ⇒ 那几条与写下的时刻；其余 ⇒ `None`。
/// 按块补簿（[`RunTrack::prime`]）与冷开扫描图（`record_scan`）同这一处挑。
pub(crate) fn said_of(faces: &RunFaces, text: &str) -> Option<RunSaid> {
    if !faces.hint(text) {
        return None;
    }
    let v = super::lines::parse(text.as_bytes())?;
    let links = faces.child_links(&v);
    (!links.is_empty()).then(|| RunSaid {
        when: faces.written(&v),
        links,
    })
}

/// 骨架里 [`RunSaid`] 那一格的写法（`#[serde(with = …)]`）：适配层的 [`ChildLink`] 不为骨架带序列化，形状只在这里写一份；
/// 改了它骨架版本号加一（`observe::skeleton::VERSION`）。
pub(crate) mod skel {
    use super::{ChildLink, RunEnd, RunSaid};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    struct Run {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at: Option<u64>,
        links: Vec<Link>,
    }

    #[derive(Serialize, Deserialize)]
    struct Link {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end: Option<End>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        background: bool,
    }

    #[derive(Serialize, Deserialize)]
    enum End {
        Done,
        Failed,
        Stopped,
    }

    pub(crate) fn serialize<S: Serializer>(v: &[RunSaid], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(v.iter().map(|r| {
            Run {
                at: r.when,
                links: r
                    .links
                    .iter()
                    .map(|l| Link {
                        tool: l.tool.clone(),
                        label: l.label.clone(),
                        kind: l.kind.clone(),
                        run: l.run.clone(),
                        from: l.from.clone(),
                        end: l.end.map(|e| match e {
                            RunEnd::Done => End::Done,
                            RunEnd::Failed => End::Failed,
                            RunEnd::Stopped => End::Stopped,
                        }),
                        error: l.error.clone(),
                        background: l.background,
                    })
                    .collect(),
            }
        }))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<RunSaid>, D::Error> {
        Ok(Vec::<Run>::deserialize(d)?
            .into_iter()
            .map(|r| RunSaid {
                when: r.at,
                links: r
                    .links
                    .into_iter()
                    .map(|l| ChildLink {
                        tool: l.tool,
                        label: l.label,
                        kind: l.kind,
                        run: l.run,
                        from: l.from,
                        end: l.end.map(|e| match e {
                            End::Done => RunEnd::Done,
                            End::Failed => RunEnd::Failed,
                            End::Stopped => RunEnd::Stopped,
                        }),
                        error: l.error,
                        background: l.background,
                    })
                    .collect(),
            })
            .collect())
    }
}

/// watcher 等下一个事件：`due` 有 ⇒ 至多等到那一刻，到点回 `Ok(None)`（调用方判一次久未动静、重算期限再等）；
/// 没有 ⇒ 无期限地等。只醒在真期限上，不是节拍。
#[allow(
    clippy::disallowed_methods,
    reason = "子运行「久未动静 ⇒ 状态不明」要到点就判：会话空闲时不会再有它的文件事件，只靠事件判的话面板上的「在跑」永远冻住"
)]
pub(crate) fn next_event<T>(
    rx: &std::sync::mpsc::Receiver<T>,
    due: Option<SystemTime>,
    now: SystemTime,
) -> Result<Option<T>, std::sync::mpsc::RecvError> {
    use std::sync::mpsc::{RecvError, RecvTimeoutError};
    let Some(due) = due else {
        return rx.recv().map(Some);
    };
    let wait = due.duration_since(now).unwrap_or_default();
    match rx.recv_timeout(wait) {
        Ok(e) => Ok(Some(e)),
        Err(RecvTimeoutError::Timeout) => Ok(None),
        Err(RecvTimeoutError::Disconnected) => Err(RecvError),
    }
}

/// 状态写好的字（运行表 · 被挤出的那几项同一处）。
pub(crate) fn status_text(s: RunState) -> Words {
    Words(match s {
        RunState::Running => copy_text("history.row.live", &[]),
        RunState::Done => copy_text("runs.state.done", &[]),
        RunState::Failed => copy_text("runs.state.failed", &[]),
        RunState::Stopped => copy_text("runs.state.stopped", &[]),
        RunState::Unknown => copy_text("sessionState.unseen.name", &[]),
    })
}

/// 会走的钟那一截的空位（出口照此刻 − `from` 用共用的时长格式填）。
const DUR_SLOT: &str = "{dur}";

/// 写好的一截：整句。
fn said(text: String) -> RunLine {
    RunLine {
        text: Words(text),
        from: None,
    }
}

/// 会走的钟那一截 `{dur}` 填什么：有起点 ⇒ 空位（出口按此刻填）；没有 ⇒ 按 0 写好。
fn dur_of(from: Option<u64>) -> String {
    match from {
        Some(_) => DUR_SLOT.to_string(),
        None => copy_core::short_duration(0),
    }
}

/// 写好的一截：从 `from` 起算的会走的钟（`text` 里带 [`dur_of`] 填的那一格）。
fn ticking(text: String, from: Option<u64>) -> RunLine {
    RunLine {
        text: Words(text),
        from,
    }
}

/// 收场了用了多久那一句（起止两头都定了，静态的）：派出卡与智能体窗口事实那一行同一句。在跑 · 状态不明 · 缺一头 ⇒ `None`。
fn took_of(i: &RunInfo) -> Option<Words> {
    if matches!(i.state, RunState::Running | RunState::Unknown) {
        return None;
    }
    let (a, b) = (i.started_ms?, i.ended_ms?);
    let took = copy_core::short_duration(b.saturating_sub(a));
    Some(Words(copy_text(
        "agentWindow.facts.took",
        &[("dur", &took)],
    )))
}

/// 事实那一行（「几点起」在 `started_text`，推出去那一下按流的时区写）：已跑 / 用时 · 最近 · 工具 ×n · agent ×n · 后台派出。
fn facts_of(i: &RunInfo, kids: usize) -> Vec<RunLine> {
    let mut out = Vec::new();
    let live = matches!(i.state, RunState::Running | RunState::Unknown);
    if i.state == RunState::Running && i.started_ms.is_some() {
        let dur = dur_of(i.started_ms);
        out.push(ticking(
            copy_text("agentWindow.facts.running", &[("dur", &dur)]),
            i.started_ms,
        ));
    }
    if let Some(took) = &i.took_text {
        out.push(said(took.0.clone()));
    }
    if live && i.active_ms.is_some() {
        let dur = dur_of(i.active_ms);
        out.push(ticking(
            copy_text("agentWindow.facts.active", &[("dur", &dur)]),
            i.active_ms,
        ));
    }
    if i.calls > 0 {
        let n = i.calls.to_string();
        out.push(said(copy_text("rsTurns.proc.tools", &[("n", &n)])));
    }
    if kids > 0 {
        let n = kids.to_string();
        out.push(said(copy_text("rsTurns.proc.agents", &[("n", &n)])));
    }
    if i.background {
        out.push(said(copy_text("agentWindow.facts.background", &[])));
    }
    out
}

/// 说明那一块：在跑且没在等 ⇒ 不上线。
fn explain_of(i: &RunInfo) -> Option<RunExplain> {
    let plain = |line: RunLine| RunExplain {
        line,
        ..RunExplain::default()
    };
    let dur = dur_of(i.active_ms);
    Some(match i.state {
        RunState::Running => {
            let tool = i.waiting.as_deref()?;
            RunExplain {
                line: ticking(
                    copy_text("agentWindow.why.waiting", &[("tool", tool), ("dur", &dur)]),
                    i.active_ms,
                ),
                note: Some(Words(copy_text("agentWindow.why.waitingNote", &[]))),
                ..RunExplain::default()
            }
        }
        RunState::Done => plain(said(copy_text("agentWindow.why.done", &[]))),
        RunState::Failed => plain(said(copy_text("agentWindow.why.failed", &[]))),
        RunState::Stopped => plain(said(copy_text("agentWindow.why.stopped", &[]))),
        RunState::Unknown => RunExplain {
            lead: Some(status_text(i.state)),
            line: if i.why == Some(RunWhy::Orphaned) {
                said(copy_text("agentWindow.why.orphaned", &[]))
            } else {
                ticking(
                    copy_text("agentWindow.why.quiet", &[("dur", &dur)]),
                    i.active_ms,
                )
            },
            note: None,
            refresh: true,
        },
    })
}

/// 尾巴上的结束线：在跑 ⇒ 不上线。
fn end_of(i: &RunInfo) -> Option<RunEndLine> {
    let sub = match i.state {
        RunState::Running => return None,
        RunState::Done => said(copy_text("agentWindow.end.done", &[])),
        RunState::Failed => said(copy_text("agentWindow.end.failed", &[])),
        RunState::Stopped => said(copy_text("agentWindow.end.stopped", &[])),
        RunState::Unknown if i.why == Some(RunWhy::Orphaned) => {
            said(copy_text("agentWindow.end.orphaned", &[]))
        }
        RunState::Unknown => {
            let dur = dur_of(i.active_ms);
            ticking(
                copy_text("agentWindow.end.quiet", &[("dur", &dur)]),
                i.active_ms,
            )
        }
    };
    Some(RunEndLine {
        head: status_text(i.state),
        sub,
    })
}

/// 列表的三组（运行表最早动过的在前 ⇒ 每组最近动过的在前）：在跑的全列 · 最近结束的 [`RECENT_ENDED`] 个 · 其余连状态不明的；
/// 更早那一组的组头字（「更早 ×n」）这里写好，那一组空 ⇒ 不上线。
fn groups_of(runs: &[RunInfo]) -> RunGroups {
    let mut g = RunGroups::default();
    for r in runs.iter().rev() {
        let id = r.run.clone();
        match r.state {
            RunState::Running => g.running.push(id),
            RunState::Done | RunState::Failed | RunState::Stopped
                if g.recent.len() < RECENT_ENDED =>
            {
                g.recent.push(id)
            }
            _ => g.older.push(id),
        }
    }
    g.older_text = (!g.older.is_empty()).then(|| {
        Words(copy_text(
            "agentsPanel.group.older",
            &[("n", &g.older.len().to_string())],
        ))
    });
    g.running_text = (!g.running.is_empty()).then(|| {
        Words(copy_text(
            "runs.badge.running",
            &[("running", &g.running.len().to_string())],
        ))
    });
    let listed = g.running.len() + g.recent.len() + g.older.len();
    let shown: Vec<String> = g
        .running
        .iter()
        .chain(&g.recent)
        .chain(&g.older)
        .take(PEEK_SHOWN)
        .cloned()
        .collect();
    g.peek = (!shown.is_empty()).then_some(shown);
    g.peek_more_text = (listed > PEEK_SHOWN).then(|| {
        Words(copy_text(
            "runs.peek.more",
            &[("more", &(listed - PEEK_SHOWN).to_string())],
        ))
    });
    g
}

/// 一条会话派过几个子运行（[`Sess::total`]）：`at_least` ⇒ 漏过，`n` 只是下界。
#[derive(Default, Clone, Copy)]
struct Total {
    n: usize,
    at_least: bool,
}

/// 这条会话的 agent 那一枚的字：有在跑的 ⇒ 「agent n · m 在跑」，没有 ⇒ 「agent n」；漏过 ⇒ 「agent 至少 n …」；表是空的 ⇒ 不上线。
/// n 是照实数的总数（[`Total`]），不是运行表的长度（表只列 [`RUNS_KEEP`] 个）。
fn summary_of(runs: &[RunInfo], total: Total) -> Option<Words> {
    if runs.is_empty() {
        return None;
    }
    let n = total.n.to_string();
    let running = runs.iter().filter(|r| r.state == RunState::Running).count();
    let r = running.to_string();
    Some(Words(match (running > 0, total.at_least) {
        (true, false) => copy_text("agentsPanel.render.summary", &[("n", &n), ("running", &r)]),
        (true, true) => copy_text(
            "agentsPanel.render.summaryAtLeast",
            &[("n", &n), ("running", &r)],
        ),
        (false, false) => copy_text("agentsPanel.render.summaryDone", &[("n", &n)]),
        (false, true) => copy_text("agentsPanel.render.summaryDoneAtLeast", &[("n", &n)]),
    }))
}

/// 出帧那一下给一项写好字：状态 · 名字（没有 ⇒ 通称）· 最近一件事 · 事实 · 说明 · 结束线（`kids`：它派出的、还在表里的几个）。
/// 簿里记的是事实，字只在这里写。
fn dressed(mut i: RunInfo, kids: usize) -> RunInfo {
    i.took_text = took_of(&i);
    i.facts = facts_of(&i, kids);
    i.explain = explain_of(&i);
    i.end = end_of(&i);
    i.status_text = status_text(i.state);
    i.label_text = Words(
        i.label
            .clone()
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| copy_text("newSession.label.agent", &[])),
    );
    i.last_text = i.last.as_ref().map(|d| {
        Words(match d {
            RunDid::Say => copy_text("runs.last.say", &[]),
            RunDid::Think => copy_text("runs.last.think", &[]),
            RunDid::Tool { name } => copy_text("runs.last.tool", &[("tool", name)]),
        })
    });
    i
}

/// 判据用：往一本运行簿里直接记一个在跑的子运行（不走记录那一条管线）。
#[cfg(test)]
#[path = "../../../tests/backend/observe/runs_testing.rs"]
pub(crate) mod testing;

/// 收下会话时只为没收场、或最近还在写的子运行从头读。
#[cfg(test)]
#[path = "../../../tests/backend/observe/runs_adopt_tests.rs"]
mod adopt_tests;

/// 子运行的个数照实说（A2-11）。
#[cfg(test)]
#[path = "../../../tests/backend/observe/runs_total_tests.rs"]
mod total_tests;

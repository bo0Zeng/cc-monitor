//! **运行簿**：一个会话 ＝ 一个主运行 ＋ 零到多个子运行（被委派出去的那几次）。这里只认「运行」，不认任何一家的形状 ——
//! 记录属于哪个运行、哪条记录派出了子运行、子运行的记录住哪、流与记录拿什么对账，都问适配层给的那一组面（[`RunFaces`]）。
//!
//! 两半：
//! - [`RunBook`]：每条流连接一本，watcher 写、流归位（`stream::run_route`）读：每个会话的运行表 ＋ 对账键归哪个运行。
//! - [`RunTrack`]：watcher 那一侧的手 —— 子运行的记录文件在哪（`ChildFace::sources`）、读到哪了；与主记录走同一条文件事件管线，不轮询。

use crate::agents::{ChildLink, RunEnd, RunFaces, RunMark};
use crate::stream::wire::{Frame, RunEnded, RunInfo, RunState, RunWhy};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Seek, SeekFrom};
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
pub(crate) const STALE_AFTER: Duration = Duration::from_millis(15 * 60 * 1000);
/// 失败收场的报错原话至多留几个字（运行表一变就整份重发）。
pub(crate) const ERROR_CHARS: usize = 2000;

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
    /// 父侧工具调用 id ⇒ 那次派出：调用先到、子运行是哪个后到。
    labels: HashMap<String, Spawn>,
    label_order: VecDeque<String>,
    /// 对账键 ⇒ 哪个运行（`None` ＝ 主运行）。
    rids: HashMap<String, Option<String>>,
    rid_order: VecDeque<String>,
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
            if let Some(closed) = v.closed {
                self.bury(v.info, closed);
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
        r.info.parent = sp.parent.clone();
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
                let known = self.labels.get(&tool).cloned();
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

    /// 运行表（最早动过的在前）。
    fn table(&self) -> Vec<RunInfo> {
        let mut v: Vec<&Run> = self.runs.iter().collect();
        v.sort_by_key(|r| r.seen);
        v.into_iter().map(|r| r.info.clone()).collect()
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

/// 一条流连接的运行簿（watcher 与流归位共用一本）。
#[derive(Default)]
pub struct RunBook {
    inner: Mutex<HashMap<String, Sess>>,
    /// 新学到对账键的归属 ⇒ 叫醒流归位（挂起的那几段可能对上了）。
    wake: tokio::sync::Notify,
}

/// 这个进程里活着的运行簿（每条流连接一本，见 [`RunBook::shared`]）。只读查询（`session-interrupts`）在这里看全部。
static LIVE_BOOKS: Mutex<Vec<std::sync::Weak<RunBook>>> = Mutex::new(Vec::new());

/// `sid` 此刻在跑的子运行（全部活着的运行簿合起来，按运行号去重）：显示名（标签 ＞ 种类 ＞ 运行号）。
pub(crate) fn running_names(sid: &str) -> Vec<String> {
    let books: Vec<Arc<RunBook>> = {
        let mut g = LIVE_BOOKS.lock().unwrap_or_else(|e| e.into_inner());
        g.retain(|w| w.strong_count() > 0);
        g.iter().filter_map(std::sync::Weak::upgrade).collect()
    };
    let mut seen = std::collections::BTreeMap::<String, String>::new();
    for b in books {
        for r in b.runs(sid) {
            if r.state == RunState::Running {
                let name = r
                    .label
                    .clone()
                    .or_else(|| r.kind.clone())
                    .unwrap_or_else(|| r.run.clone());
                seen.entry(r.run.clone()).or_insert(name);
            }
        }
    }
    seen.into_values().collect()
}

impl RunBook {
    /// 一本新的运行簿，登记进本进程的活簿表（连接断了它随 `Arc` 一起走，表里那一格自然失效）。
    pub fn shared() -> Arc<Self> {
        let b = Arc::new(Self::default());
        LIVE_BOOKS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Arc::downgrade(&b));
        b
    }

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
        let (changed, learned) = self.with(|m| {
            let s = m.entry(sid.to_string()).or_default();
            let mut learned = false;
            if let Some(rid) = rid {
                let owner = match (&mark, child.is_some()) {
                    (Some(mk), _) => Some(Some(mk.run.clone())),
                    (None, false) => Some(None),
                    (None, true) => None,
                };
                if let Some(o) = owner {
                    s.learn_rid(rid, o);
                    learned = true;
                }
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
            (changed, learned)
        });
        if learned {
            self.wake.notify_one();
        }
        changed
    }

    /// 父记录里只读派出 / 收场那几格（不学对账键：只读尾巴的流接上会话时补读已有的那一截用）。回：运行表变没变。
    pub(crate) fn links(&self, faces: &RunFaces, sid: &str, v: &Value) -> bool {
        let links = faces.child_links(v);
        if links.is_empty() {
            return false;
        }
        let now = crate::common::time::now();
        let when = faces.written(v).or_else(|| ms(now));
        self.with(|m| {
            let s = m.entry(sid.to_string()).or_default();
            let mut changed = false;
            for l in links {
                changed |= s.link(l, now, when, None);
            }
            changed
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
        let (runs, ended) = self.with(|m| match m.get_mut(sid) {
            Some(s) => {
                s.settle(now);
                (s.table(), s.ended())
            }
            None => (Vec::new(), Vec::new()),
        });
        Frame::SessionRuns {
            sid: sid.to_string(),
            runs,
            ended,
        }
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
        self.with(|m| m.get(sid).map(Sess::table).unwrap_or_default())
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
    }

    /// 会话退休：还算在跑的子运行改成状态不明，表变了就回最后那一帧（交出去之后各家的跟踪再 `RunTrack::forget`）。
    pub(crate) fn retire(&self, sid: &str) -> Option<Frame> {
        self.orphan(sid).then(|| self.frame(sid))
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

    /// 等下一次「学到了对账键的归属」（之前已学到而没人在等 ⇒ 立刻返回一次）。
    pub(crate) async fn learned(&self) {
        self.wake.notified().await;
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

/// 一份子运行记录读到哪了。
struct ChildCursor {
    sid: String,
    consumed: u64,
}

/// watcher 那一侧的运行跟踪：运行面 ＋ 运行簿 ＋ 各会话的主记录路径 ＋ 子运行记录的游标。
pub(crate) struct RunTrack {
    pub(crate) faces: RunFaces,
    pub(crate) book: Arc<RunBook>,
    parents: HashMap<String, Vec<PathBuf>>,
    children: HashMap<PathBuf, ChildCursor>,
}

impl RunTrack {
    pub(crate) fn new(faces: RunFaces, book: Arc<RunBook>) -> Self {
        Self {
            faces,
            book,
            parents: HashMap::new(),
            children: HashMap::new(),
        }
    }

    /// 这一家有没有子运行（没有 ⇒ 下面这些全是空转，watcher 直接跳过）。
    fn has_children(&self) -> bool {
        self.faces.children.is_some()
    }

    /// 一行主记录交出去之前：归属与派出链接记进簿里。回：（对账键, 它其实属于某个子运行, 运行表变没变）。
    pub(crate) fn main_record(&self, sid: &str, raw: &str) -> MainRecord {
        let none = MainRecord::default();
        if self.faces.response_id.is_none()
            && self.faces.child_link.is_none()
            && self.faces.run_of.is_none()
        {
            return none;
        }
        let Ok(v) = serde_json::from_str::<Value>(raw.trim_start_matches('\u{feff}').trim()) else {
            return none;
        };
        MainRecord {
            changed: self.book.record(&self.faces, sid, &v, None),
            rid: self.faces.response_id(&v),
            in_run: self.faces.run_of(&v).is_some(),
        }
    }

    /// 只读尾巴的流接上一个会话：它的主记录已有的那一截（`chunk`，从头起）里说到子运行的几行补进簿里 ——
    /// 接上之前派出 / 收场的那些，否则只剩子记录自己的那一半（标签没有、收场看不见）。只挑预筛命中的行解析。回：运行表变没变。
    pub(crate) fn prime(&self, sid: &str, chunk: &[u8]) -> bool {
        if !self.has_children() {
            return false;
        }
        let Some(last_nl) = chunk.iter().rposition(|&b| b == b'\n') else {
            return false;
        };
        let mut changed = false;
        for line in chunk[..last_nl].split(|&b| b == b'\n') {
            let Ok(text) = std::str::from_utf8(line) else {
                continue;
            };
            if !self.faces.hint(text) {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}').trim())
            {
                changed |= self.book.links(&self.faces, sid, &v);
            }
        }
        changed
    }

    /// 一个会话的主记录在这里（宣告 / 每次读到它时都说；只有头一次真去找）：把它此刻已有的子运行记录收进来、从头读一遍。
    /// 之后新长出来的子运行记录靠它自己的文件事件进来（[`Self::on_path`]）。回：运行表变没变。
    pub(crate) fn adopt(&mut self, sid: &str, parent: &Path) -> bool {
        if !self.has_children() {
            return false;
        }
        let ps = self.parents.entry(sid.to_string()).or_default();
        if ps.iter().any(|p| p == parent) {
            return false;
        }
        ps.push(parent.to_path_buf());
        self.discover_under(sid, parent)
    }

    fn discover_under(&mut self, sid: &str, parent: &Path) -> bool {
        let mut changed = false;
        for c in self.faces.sources(parent) {
            if !self.children.contains_key(&c) {
                self.children.insert(
                    c.clone(),
                    ChildCursor {
                        sid: sid.to_string(),
                        consumed: 0,
                    },
                );
                changed |= self.read_child(&c);
            }
        }
        changed
    }

    /// 一个文件事件：是（或刚成为）某个会话的子运行记录 ⇒ 读新行、记进簿里，回那个会话（运行表变没变）。
    /// 不是 ⇒ `None`（调用方照旧当主记录 / 别的东西处理）。
    pub(crate) fn on_path(&mut self, p: &Path) -> Option<(String, bool)> {
        if !self.has_children() {
            return None;
        }
        if self.parents.values().any(|ps| ps.iter().any(|x| x == p)) {
            return None;
        }
        if !self.children.contains_key(p) {
            // 新长出来的子运行记录：只认这一家说得出「属于哪份父记录」的形状，且只在那一份底下找（不是每个事件都把全部会话扫一遍）。
            let parent = self.faces.owner(p)?;
            let sid = self
                .parents
                .iter()
                .find(|(_, ps)| ps.contains(&parent))
                .map(|(sid, _)| sid.clone())?;
            let changed = self.discover_under(&sid, &parent);
            self.children.contains_key(p).then_some(())?;
            return Some((sid, changed));
        }
        let sid = self.children.get(p).map(|c| c.sid.clone())?;
        let changed = self.read_child(p);
        Some((sid, changed))
    }

    /// 读一份子运行记录从游标到最后一个整行；变短了 ⇒ 从头读。
    fn read_child(&mut self, p: &Path) -> bool {
        let Some(cur) = self.children.get_mut(p) else {
            return false;
        };
        let Ok(mut f) = std::fs::File::open(p) else {
            return false;
        };
        let meta = f.metadata().ok();
        let len = meta.as_ref().map_or(0, |m| m.len());
        let written = meta
            .and_then(|m| m.modified().ok())
            .unwrap_or_else(crate::common::time::now);
        if len < cur.consumed {
            cur.consumed = 0;
        }
        if f.seek(SeekFrom::Start(cur.consumed)).is_err() {
            return false;
        }
        let mut buf = Vec::new();
        if f.read_to_end(&mut buf).is_err() {
            return false;
        }
        let Some(last_nl) = buf.iter().rposition(|&b| b == b'\n') else {
            return false;
        };
        cur.consumed += last_nl as u64 + 1;
        let sid = cur.sid.clone();
        let mut changed = false;
        for line in buf[..last_nl].split(|&b| b == b'\n') {
            let text = String::from_utf8_lossy(line);
            let t = text.trim_start_matches('\u{feff}').trim();
            if t.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(t) {
                changed |= self.book.record(&self.faces, &sid, &v, Some(written));
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

/// watcher 等下一个事件：`due` 有 ⇒ 至多等到那一刻，到点回 `Ok(None)`（调用方判一次久未动静、重算期限再等）；
/// 没有 ⇒ 无期限地等。只醒在真期限上，不是节拍。
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

/// 判据用：往一本运行簿里直接记一个在跑的子运行（不走记录那一条管线）。
#[cfg(test)]
#[path = "../../../tests/backend/observe/runs_testing.rs"]
pub(crate) mod testing;

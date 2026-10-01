//! **运行簿**：一个会话 ＝ 一个主运行 ＋ 零到多个子运行（被委派出去的那几次）。这里只认「运行」，不认任何一家的形状 ——
//! 记录属于哪个运行、哪条记录派出了子运行、子运行的记录住哪、流与记录拿什么对账，都问适配层给的那一组面（[`RunFaces`]）。
//!
//! 两半：
//! - [`RunBook`]：每条流连接一本，watcher 写、流归位（`stream::run_route`）读：每个会话的运行表 ＋ 对账键归哪个运行。
//! - [`RunTrack`]：watcher 那一侧的手 —— 子运行的记录文件在哪（`ChildFace::sources`）、读到哪了；与主记录走同一条文件事件管线，不轮询。

use crate::agents::{ChildLink, RunEnd, RunFaces, RunMark};
use crate::stream::wire::{Frame, RunInfo, RunState};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// 每个会话最多记几个运行（超了先丢最久没动静的已收场的，再丢最久没动静的）。
pub(crate) const RUNS_KEEP: usize = 64;
/// 每个会话记住几个「这次工具调用派出了子运行、标签是什么」（先进先出）。
pub(crate) const LINKS_KEEP: usize = 256;
/// 每个会话记住几个对账键的归属（先进先出）。
pub(crate) const RIDS_KEEP: usize = 512;
/// 一个子运行没有任何收场信号、它的记录又这么久没再写 ⇒ 「状态不明」，不当它在跑。只在读记录 / 收到文件事件时算，不轮询。
/// 比一次前台工具调用最长的等待（10 分钟）再宽一截：在等一个长命令的子运行不会被误判。
pub(crate) const STALE_AFTER: Duration = Duration::from_millis(15 * 60 * 1000);

/// 簿里的一个运行：线上那一格 ＋ 它最近一次动静的时刻（子记录的写入时刻；只有父侧说到过 ⇒ 头一次听说它的时刻）。
struct Run {
    info: RunInfo,
    seen: SystemTime,
}

fn ended(s: RunState) -> bool {
    matches!(s, RunState::Done | RunState::Failed | RunState::Stopped)
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
    /// 父侧工具调用 id ⇒（标签, 类别）：调用先到、子运行是哪个后到。
    labels: HashMap<String, (Option<String>, Option<String>)>,
    label_order: VecDeque<String>,
    /// 对账键 ⇒ 哪个运行（`None` ＝ 主运行）。
    rids: HashMap<String, Option<String>>,
    rid_order: VecDeque<String>,
}

impl Sess {
    fn find(&mut self, run: &str) -> Option<&mut Run> {
        self.runs.iter_mut().find(|r| r.info.run == run)
    }

    fn slot(&mut self, run: &str, now: SystemTime) -> &mut Run {
        if let Some(i) = self.runs.iter().position(|r| r.info.run == run) {
            return &mut self.runs[i];
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
            self.runs.remove(victim);
        }
        self.runs.push(Run {
            info: RunInfo {
                run: run.to_string(),
                label: None,
                kind: None,
                tool: None,
                state: RunState::Running,
                last: None,
            },
            seen: now,
        });
        self.runs.last_mut().expect("just pushed")
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

    /// 子运行自己的一条记录（`seen` ＝ 那份记录的写入时刻）。收场是粘的：先到的收场信号算数，之后的记录不把它翻回在跑。
    fn mark(&mut self, m: RunMark, seen: SystemTime) -> bool {
        let r = self.slot(&m.run, seen);
        let before = r.info.clone();
        r.seen = seen;
        if !ended(r.info.state) {
            r.info.state = m.end.map_or(RunState::Running, state_of);
        }
        if m.did.is_some() {
            r.info.last = m.did;
        }
        r.info != before
    }

    fn end(r: &mut Run, e: Option<RunEnd>) {
        if let Some(e) = e {
            if !ended(r.info.state) {
                r.info.state = state_of(e);
            }
        }
    }

    fn link(&mut self, l: ChildLink, now: SystemTime) -> bool {
        match (l.run, l.tool) {
            (Some(run), Some(tool)) => {
                let known = self.labels.get(&tool).cloned();
                let r = self.slot(&run, now);
                let before = r.info.clone();
                r.info.tool = Some(tool);
                if let Some((label, kind)) = known {
                    r.info.label = label.or(r.info.label.take());
                    r.info.kind = kind.or(r.info.kind.take());
                }
                Self::end(r, l.end);
                r.info != before
            }
            // 只说收场：认识的运行才算（同一种通知也说别的后台任务，那些不是子运行）。
            (Some(run), None) => match self.find(&run) {
                Some(r) => {
                    let before = r.info.state;
                    Self::end(r, l.end);
                    r.info.state != before
                }
                None => false,
            },
            (None, Some(tool)) => {
                if self
                    .labels
                    .insert(tool.clone(), (l.label.clone(), l.kind.clone()))
                    .is_none()
                {
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
                    if r.info.label != l.label || r.info.kind != l.kind {
                        r.info.label = l.label.clone();
                        r.info.kind = l.kind.clone();
                        changed = true;
                    }
                }
                changed
            }
            (None, None) => false,
        }
    }

    /// 在跑却久未再写的 ⇒ 状态不明（[`STALE_AFTER`]）。
    fn settle(&mut self, now: SystemTime) -> bool {
        let mut changed = false;
        for r in &mut self.runs {
            let quiet = now.duration_since(r.seen).unwrap_or_default();
            if r.info.state == RunState::Running && quiet > STALE_AFTER {
                r.info.state = RunState::Unknown;
                changed = true;
            }
        }
        changed
    }
}

/// 一条流连接的运行簿（watcher 与流归位共用一本）。
#[derive(Default)]
pub struct RunBook {
    inner: Mutex<HashMap<String, Sess>>,
    /// 新学到对账键的归属 ⇒ 叫醒流归位（挂起的那几段可能对上了）。
    wake: tokio::sync::Notify,
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
        let links = faces.child_links(v);
        let now = SystemTime::now();
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
            if let Some(mk) = mark {
                changed |= s.mark(mk, child.unwrap_or(now));
            }
            for l in links {
                changed |= s.link(l, now);
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
        let now = SystemTime::now();
        self.with(|m| {
            let s = m.entry(sid.to_string()).or_default();
            let mut changed = false;
            for l in links {
                changed |= s.link(l, now);
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

    /// 这个会话的运行表（最早动过的在前）。
    pub(crate) fn runs(&self, sid: &str) -> Vec<RunInfo> {
        self.with(|m| {
            let Some(s) = m.get(sid) else {
                return Vec::new();
            };
            let mut v: Vec<&Run> = s.runs.iter().collect();
            v.sort_by_key(|r| r.seen);
            v.into_iter().map(|r| r.info.clone()).collect()
        })
    }

    /// 会话退休了（派出它们的那一方没了）：还算在跑的再也等不到收场信号 ⇒ 状态不明。回：运行表变没变。
    pub(crate) fn orphan(&self, sid: &str) -> bool {
        self.with(|m| {
            let mut changed = false;
            for r in m.get_mut(sid).into_iter().flat_map(|s| s.runs.iter_mut()) {
                if r.info.state == RunState::Running {
                    r.info.state = RunState::Unknown;
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
            let parents: Vec<(String, PathBuf)> = self
                .parents
                .iter()
                .flat_map(|(sid, ps)| ps.iter().map(move |p| (sid.clone(), p.clone())))
                .collect();
            let mut changed: Vec<String> = Vec::new();
            for (sid, parent) in parents {
                if self.discover_under(&sid, &parent) {
                    changed.push(sid);
                }
            }
            let sid = self.children.get(p).map(|c| c.sid.clone())?;
            return Some((sid.clone(), changed.contains(&sid)));
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
            .unwrap_or_else(SystemTime::now);
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

    /// 会话退休：还算在跑的子运行改成状态不明，表变了就回最后那一帧（交出去之后再 [`Self::forget`]）。
    pub(crate) fn retire(&self, sid: &str) -> Option<Frame> {
        self.book.orphan(sid).then(|| self.frame(sid))
    }

    /// 会话走了：它的主记录路径、子运行游标、运行簿那一页一起摘。
    pub(crate) fn forget(&mut self, sid: &str) {
        self.parents.remove(sid);
        self.children.retain(|_, c| c.sid != sid);
        self.book.forget(sid);
    }

    /// 这个会话此刻有没有子运行（宣告那一刻有就发一帧运行表）。
    pub(crate) fn has_runs(&self, sid: &str) -> bool {
        !self.book.runs(sid).is_empty()
    }

    /// 这个会话此刻的运行表那一帧。
    pub(crate) fn frame(&self, sid: &str) -> Frame {
        Frame::SessionRuns {
            sid: sid.to_string(),
            runs: self.book.runs(sid),
        }
    }
}

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

/// 每个会话最多记几个运行（超了先丢最老的已收场的，再丢最老的）。
pub(crate) const RUNS_KEEP: usize = 64;
/// 每个会话记住几个「这次工具调用派出了子运行、标签是什么」（先进先出）。
pub(crate) const LINKS_KEEP: usize = 256;
/// 每个会话记住几个对账键的归属（先进先出）。
pub(crate) const RIDS_KEEP: usize = 512;

#[derive(Default)]
struct Sess {
    runs: Vec<RunInfo>,
    /// 父侧工具调用 id ⇒（标签, 类别）：调用先到、子运行是哪个后到。
    labels: HashMap<String, (Option<String>, Option<String>)>,
    label_order: VecDeque<String>,
    /// 对账键 ⇒ 哪个运行（`None` ＝ 主运行）。
    rids: HashMap<String, Option<String>>,
    rid_order: VecDeque<String>,
}

impl Sess {
    fn slot(&mut self, run: &str) -> &mut RunInfo {
        if let Some(i) = self.runs.iter().position(|r| r.run == run) {
            return &mut self.runs[i];
        }
        if self.runs.len() >= RUNS_KEEP {
            let victim = self
                .runs
                .iter()
                .position(|r| r.state != RunState::Running)
                .unwrap_or(0);
            self.runs.remove(victim);
        }
        self.runs.push(RunInfo {
            run: run.to_string(),
            label: None,
            kind: None,
            tool: None,
            state: RunState::Running,
            last: None,
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

    fn mark(&mut self, m: RunMark) -> bool {
        let r = self.slot(&m.run);
        let before = r.clone();
        r.state = match m.end {
            Some(RunEnd::Done) => RunState::Done,
            Some(RunEnd::Failed) => RunState::Failed,
            None => RunState::Running,
        };
        if m.did.is_some() {
            r.last = m.did;
        }
        *r != before
    }

    fn link(&mut self, l: ChildLink) -> bool {
        match l.run {
            Some(run) => {
                let known = self.labels.get(&l.tool).cloned();
                let r = self.slot(&run);
                let before = r.clone();
                r.tool = Some(l.tool);
                if let Some((label, kind)) = known {
                    r.label = label.or(r.label.take());
                    r.kind = kind.or(r.kind.take());
                }
                match l.end {
                    Some(RunEnd::Done) => r.state = RunState::Done,
                    Some(RunEnd::Failed) => r.state = RunState::Failed,
                    None => {}
                }
                *r != before
            }
            None => {
                if self
                    .labels
                    .insert(l.tool.clone(), (l.label.clone(), l.kind.clone()))
                    .is_none()
                {
                    self.label_order.push_back(l.tool.clone());
                    while self.label_order.len() > LINKS_KEEP {
                        if let Some(old) = self.label_order.pop_front() {
                            self.labels.remove(&old);
                        }
                    }
                }
                let mut changed = false;
                for r in self.runs.iter_mut().filter(|r| r.tool.as_deref() == Some(&l.tool)) {
                    if r.label != l.label || r.kind != l.kind {
                        r.label = l.label.clone();
                        r.kind = l.kind.clone();
                        changed = true;
                    }
                }
                changed
            }
        }
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

    /// 一条记录（`in_child` ＝ 它来自子运行的记录文件）。回：这个会话的运行表变没变。
    pub(crate) fn record(&self, faces: &RunFaces, sid: &str, v: &Value, in_child: bool) -> bool {
        let mark = faces.run_of(v);
        let rid = faces.response_id(v);
        let links = faces.child_links(v);
        let (changed, learned) = self.with(|m| {
            let s = m.entry(sid.to_string()).or_default();
            let mut learned = false;
            if let Some(rid) = rid {
                let owner = match (&mark, in_child) {
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
                changed |= s.mark(mk);
            }
            for l in links {
                changed |= s.link(l);
            }
            (changed, learned)
        });
        if learned {
            self.wake.notify_one();
        }
        changed
    }

    /// 这个会话此刻有几个在跑的子运行。
    pub(crate) fn running(&self, sid: &str) -> usize {
        self.with(|m| {
            m.get(sid).map_or(0, |s| {
                s.runs
                    .iter()
                    .filter(|r| r.state == RunState::Running)
                    .count()
            })
        })
    }

    /// 这个对账键归哪个运行：没学到 ⇒ `None`；主运行 ⇒ `Some(None)`。
    pub(crate) fn owner_of(&self, sid: &str, rid: &str) -> Option<Option<String>> {
        self.with(|m| m.get(sid).and_then(|s| s.rids.get(rid).cloned()))
    }

    /// 这个会话的运行表（先来的在前）。
    pub(crate) fn runs(&self, sid: &str) -> Vec<RunInfo> {
        self.with(|m| m.get(sid).map(|s| s.runs.clone()).unwrap_or_default())
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

    /// 一行主记录交出去之前：归属与派出链接记进簿里。回：（对账键, 运行表变没变）。
    pub(crate) fn main_record(&self, sid: &str, raw: &str) -> (Option<String>, bool) {
        if self.faces.response_id.is_none() && self.faces.child_link.is_none() {
            return (None, false);
        }
        let Ok(v) = serde_json::from_str::<Value>(raw.trim_start_matches('\u{feff}').trim())
        else {
            return (None, false);
        };
        let changed = self.book.record(&self.faces, sid, &v, false);
        (self.faces.response_id(&v), changed)
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
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
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
                changed |= self.book.record(&self.faces, &sid, &v, true);
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

    /// 这个会话此刻的运行表那一帧。
    pub(crate) fn frame(&self, sid: &str) -> Frame {
        Frame::SessionRuns {
            sid: sid.to_string(),
            runs: self.book.runs(sid),
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/runs_tests.rs"]
mod tests;

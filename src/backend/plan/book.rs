//! 每个工作区留上一次读好的那一份：某一片这一刻读不成（agent 正写到一半 · 写坏了）⇒ 那一片给上一次好的 ＋ 原因 ＋ 那一份读到的时刻；
//! 整次 dump 失败（超时 · 起不来）⇒ 整份给上一次好的 ＋ 原因。全在进程内存里，一个字节都不落盘。
//!
//! 另记「哪个目录属于哪个工作区」（`plan-list` 从会话的工作目录来，一个工作区底下常有好几个会话），
//! 认过的目录下一次不再起 pb。

use super::dump::{self, Ran};
use super::needs;
use super::product;
use super::WhoPort;
use cmd_wire::plan::{PlanRead, PlanSlice, PlanStale};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 一个工作区上一次读好的那一份。
struct Seen {
    /// 成品（已经按「留上一次好的」处置过）。
    doc: PlanRead,
    /// 每一片上一次读好的那一份 ＋ 读到的时刻（epoch ms）。
    good: BTreeMap<String, (PlanSlice, u64)>,
}

/// 读不成的几种（帧面翻成码）。
#[derive(Debug, Clone, PartialEq)]
pub enum Miss {
    /// 那个目录不在 pb 工作区里。
    NotWorkspace(String),
    /// pb 没有 `dump` / 形状不认得。
    Unsupported(String),
    /// 别的失败，也没有上一次好的可给。
    Failed { said: String, raw: Option<String> },
}

/// 进程里那一本。
#[derive(Default)]
pub struct Book {
    seen: Mutex<BTreeMap<String, Seen>>,
    /// 目录 ⇒ 它所在的工作区（`None` ＝ pb 说它不在工作区里）。
    dirs: Mutex<BTreeMap<PathBuf, Option<String>>>,
    /// 起 pb 读不成、又没有上一次好的可给的目录 ⇒ 那一次的结局（不重问时照它答，不再起 pb；读成了 · 认出不是工作区就摘掉）。
    unread: Mutex<BTreeMap<PathBuf, Miss>>,
    /// `(工作区, 片)` ⇒ 顶层全签收那一次的 rev（又有没签收的就删；「要你看」top 那一条的键用它）。
    entered: Mutex<BTreeMap<(String, String), String>>,
}

/// 本子里没记过「哪次进的那一步」时先问的口：`(工作区, 片)` ⇒ 认可里记着的那次 rev（进程刚起时用）。
pub(crate) type Prior<'a> = &'a dyn Fn(&str, &str) -> Option<String>;

/// 原始输出的摘要：只用来分辨「变没变」（不防人故意撞），64 位、16 位十六进制。
pub(crate) fn rev_of(raw: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    raw.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Book {
    /// 在 `dir` 里跑一次 dump、并进本子，回成品（顶上带 `rev` · `readAt` · `stale`）。
    /// `waiting(sid)` ＝ 活会话表说那条会话在等人（[`needs::of_slice`] 判 `ask` 只问它）。
    pub fn read(
        &self,
        entry: &Path,
        dir: &Path,
        who: WhoPort,
        waiting: &dyn Fn(&str) -> bool,
        now_ms: u64,
        prior: Prior,
    ) -> Result<PlanRead, Miss> {
        self.ingest_with(dump::run(entry, dir), dir, who, waiting, now_ms, prior)
    }

    /// [`Self::read`] 的后一半：一次 dump 的结局进本子（判据直接喂结局）；每片加 `needs`（[`needs::of_slice`]）。
    pub(crate) fn ingest_with(
        &self,
        ran: Ran,
        dir: &Path,
        who: WhoPort,
        waiting: &dyn Fn(&str) -> bool,
        now_ms: u64,
        prior: Prior,
    ) -> Result<PlanRead, Miss> {
        match ran {
            Ran::Dump { doc, raw } => {
                let rev = rev_of(&raw);
                let mut out = product::make(&doc, who, rev.clone(), now_ms);
                let ws = out.workspace.clone();
                lock(&self.dirs).insert(dir.to_path_buf(), Some(ws.clone()));
                lock(&self.unread).remove(dir);
                let mut seen = lock(&self.seen);
                let prev = seen.remove(&ws);
                let mut good = prev.map(|s| s.good).unwrap_or_default();
                for sl in &mut out.slices {
                    match sl.error.clone() {
                        None => {
                            let at =
                                self.entered_at(&ws, &sl.name, needs::top_done(sl), &rev, prior);
                            sl.needs = needs::of_slice(sl, at.as_deref(), waiting);
                            good.insert(sl.name.clone(), (sl.clone(), now_ms));
                        }
                        Some(error) => {
                            if let Some((prev, at)) = good.get(&sl.name) {
                                let mut kept = prev.clone();
                                kept.stale = Some(PlanStale {
                                    said: Some(error),
                                    raw: None,
                                    since: *at,
                                    since_text: None,
                                });
                                *sl = kept;
                            }
                        }
                    }
                }
                seen.insert(
                    ws,
                    Seen {
                        doc: out.clone(),
                        good,
                    },
                );
                Ok(out)
            }
            Ran::NotWorkspace(said) => {
                lock(&self.dirs).insert(dir.to_path_buf(), None);
                lock(&self.unread).remove(dir);
                Err(Miss::NotWorkspace(said))
            }
            Ran::Unsupported(said) => self.miss_at(dir, Miss::Unsupported(said)),
            Ran::Failed { said, raw } => {
                let ws = self.workspace_of(dir);
                let seen = lock(&self.seen);
                match ws.and_then(|w| seen.get(&w)) {
                    Some(prev) => {
                        let mut out = prev.doc.clone();
                        out.stale = Some(PlanStale {
                            said: Some(said),
                            raw,
                            since: prev.doc.read_at,
                            since_text: None,
                        });
                        Ok(out)
                    }
                    None => {
                        drop(seen);
                        self.miss_at(dir, Miss::Failed { said, raw })
                    }
                }
            }
        }
    }

    /// 顶层此刻全签收没有：是 ⇒ 记着的那次 rev（没记过 ⇒ 先问 `prior`，再没有就是这一次）；不是 ⇒ 忘掉、`None`。
    fn entered_at(
        &self,
        ws: &str,
        slice: &str,
        at_step: bool,
        rev: &str,
        prior: Prior,
    ) -> Option<String> {
        let mut g = lock(&self.entered);
        let k = (ws.to_string(), slice.to_string());
        if !at_step {
            g.remove(&k);
            return None;
        }
        Some(
            g.entry(k)
                .or_insert_with(|| prior(ws, slice).unwrap_or_else(|| rev.to_string()))
                .clone(),
        )
    }

    /// 这个目录认过、在哪个工作区里（没认过 ⇒ `None`；认过不在工作区里 ⇒ `Some(None)`）。
    /// 记下这个目录读不成的结局、原样回它。
    fn miss_at(&self, dir: &Path, m: Miss) -> Result<PlanRead, Miss> {
        lock(&self.unread).insert(dir.to_path_buf(), m.clone());
        Err(m)
    }

    /// 这个目录上一次起 pb 读不成、又没有好的可给 ⇒ 那一次的结局；没记过 ⇒ `None`。
    pub fn unread(&self, dir: &Path) -> Option<Miss> {
        lock(&self.unread).get(dir).cloned()
    }

    pub fn known_dir(&self, dir: &Path) -> Option<Option<String>> {
        lock(&self.dirs).get(dir).cloned()
    }

    fn workspace_of(&self, dir: &Path) -> Option<String> {
        if let Some(Some(w)) = lock(&self.dirs).get(dir) {
            return Some(w.clone());
        }
        let d = dir.to_string_lossy();
        lock(&self.seen)
            .contains_key(d.as_ref())
            .then(|| d.to_string())
    }

    /// 读好过的工作区（文件窗口反查只看这几个：不为每个目录起一次 pb）。
    pub fn workspaces(&self) -> Vec<String> {
        lock(&self.seen).keys().cloned().collect()
    }

    /// 这个工作区上一次读好的成品。
    pub fn last(&self, workspace: &str) -> Option<PlanRead> {
        lock(&self.seen).get(workspace).map(|s| s.doc.clone())
    }
}

/// 进程里那一本。
pub fn book() -> &'static Book {
    static B: std::sync::OnceLock<Book> = std::sync::OnceLock::new();
    B.get_or_init(Book::default)
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/book_tests.rs"]
mod tests;

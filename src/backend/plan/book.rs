//! 每个工作区留上一次读好的那一份：某一片这一刻读不成（agent 正写到一半 · 写坏了）⇒ 那一片给上一次好的 ＋ 原因 ＋ 那一份读到的时刻；
//! 整次 dump 失败（超时 · 起不来）⇒ 整份给上一次好的 ＋ 原因。全在进程内存里，一个字节都不落盘。
//!
//! 另记「哪个目录属于哪个工作区」（`plan-list` 从会话的工作目录来，一个工作区底下常有好几个会话），
//! 认过的目录下一次不再起 pb。

use super::dump::{self, Ran};
use super::product::{self, Views};
use super::WhoPort;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 一个工作区上一次读好的那一份。
struct Seen {
    /// 成品（已经按「留上一次好的」处置过）。
    doc: Value,
    views: Views,
    /// 每一片上一次读好的那一份 ＋ 读到的时刻（epoch ms）。
    good: BTreeMap<String, (Value, u64)>,
    rev: String,
    read_at: u64,
}

/// 读不成的几种（帧面翻成码）。
#[derive(Debug, PartialEq)]
pub(crate) enum Miss {
    /// 那个目录不在 pb 工作区里。
    NotWorkspace(String),
    /// pb 没有 `dump` / 形状不认得。
    Unsupported(String),
    /// 别的失败，也没有上一次好的可给。
    Failed { said: String, raw: Option<String> },
}

/// 进程里那一本。
#[derive(Default)]
pub(crate) struct Book {
    seen: Mutex<BTreeMap<String, Seen>>,
    /// 目录 ⇒ 它所在的工作区（`None` ＝ pb 说它不在工作区里）。
    dirs: Mutex<BTreeMap<PathBuf, Option<String>>>,
}

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
    pub(crate) fn read(
        &self,
        entry: &Path,
        dir: &Path,
        who: WhoPort,
        now_ms: u64,
    ) -> Result<Value, Miss> {
        self.take(dump::run(entry, dir), dir, who, now_ms)
    }

    /// [`Self::read`] 的后一半：一次 dump 的结局进本子（判据直接喂结局）。
    pub(crate) fn take(
        &self,
        ran: Ran,
        dir: &Path,
        who: WhoPort,
        now_ms: u64,
    ) -> Result<Value, Miss> {
        match ran {
            Ran::Dump { doc, raw } => {
                let made = product::make(&doc, who);
                let ws = made
                    .doc
                    .get("workspace")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                lock(&self.dirs).insert(dir.to_path_buf(), Some(ws.clone()));
                let rev = rev_of(&raw);
                let mut seen = lock(&self.seen);
                let prev = seen.remove(&ws);
                let (mut good, prev_views) = prev.map(|s| (s.good, s.views)).unwrap_or_default();
                let mut out = made.doc;
                if let Some(slices) = out.get_mut("slices").and_then(Value::as_array_mut) {
                    for sl in slices.iter_mut() {
                        let name = sl
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let error = sl.get("error").cloned().unwrap_or(Value::Null);
                        if error.is_null() {
                            good.insert(name, (sl.clone(), now_ms));
                        } else if let Some((prev, at)) = good.get(&name) {
                            let mut kept = prev.clone();
                            kept["stale"] = json!({"said": error, "since": at});
                            *sl = kept;
                        }
                    }
                }
                out["rev"] = json!(rev);
                out["readAt"] = json!(now_ms);
                out["stale"] = Value::Null;
                // 读不成的那一片留着上一次的 `agent_view`（成品给的是上一次那一份）。
                let mut views = made.views;
                {
                    for ((sl, id), v) in &prev_views {
                        let kept = out["slices"].as_array().is_some_and(|a| {
                            a.iter()
                                .any(|s| s["name"] == json!(sl) && !s["stale"].is_null())
                        });
                        if kept {
                            views
                                .entry((sl.clone(), id.clone()))
                                .or_insert_with(|| v.clone());
                        }
                    }
                }
                seen.insert(
                    ws,
                    Seen {
                        doc: out.clone(),
                        views,
                        good,
                        rev,
                        read_at: now_ms,
                    },
                );
                Ok(out)
            }
            Ran::NotWorkspace(said) => {
                lock(&self.dirs).insert(dir.to_path_buf(), None);
                Err(Miss::NotWorkspace(said))
            }
            Ran::Unsupported(said) => Err(Miss::Unsupported(said)),
            Ran::Failed { said, raw } => {
                let ws = self.workspace_of(dir);
                let seen = lock(&self.seen);
                match ws.and_then(|w| seen.get(&w)) {
                    Some(prev) => {
                        let mut out = prev.doc.clone();
                        out["stale"] = json!({"said": said, "raw": raw, "since": prev.read_at});
                        Ok(out)
                    }
                    None => Err(Miss::Failed { said, raw }),
                }
            }
        }
    }

    /// 这个目录认过、在哪个工作区里（没认过 ⇒ `None`；认过不在工作区里 ⇒ `Some(None)`）。
    pub(crate) fn known_dir(&self, dir: &Path) -> Option<Option<String>> {
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

    /// 这个工作区上一次读好的成品。
    pub(crate) fn last(&self, workspace: &str) -> Option<Value> {
        lock(&self.seen).get(workspace).map(|s| s.doc.clone())
    }

    /// 上一次读好的那一份的摘要。
    pub(crate) fn rev(&self, workspace: &str) -> Option<String> {
        lock(&self.seen).get(workspace).map(|s| s.rev.clone())
    }

    /// 一格的 `agent_view`（上一次读好的那一份里的）。
    pub(crate) fn view(&self, workspace: &str, slice: &str, id: &str) -> Option<String> {
        lock(&self.seen)
            .get(workspace)
            .and_then(|s| s.views.get(&(slice.to_string(), id.to_string())).cloned())
    }

    /// 认过的工作区（`plan_changed` 盯的就是它们）。
    pub(crate) fn workspaces(&self) -> Vec<String> {
        lock(&self.seen).keys().cloned().collect()
    }
}

/// 进程里那一本。
pub(crate) fn book() -> &'static Book {
    static B: std::sync::OnceLock<Book> = std::sync::OnceLock::new();
    B.get_or_init(Book::default)
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/book_tests.rs"]
mod tests;

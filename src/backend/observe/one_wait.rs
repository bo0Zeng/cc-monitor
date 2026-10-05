//! **一次性等待**：盯一个目录的文件事件，每来一批问一次「到了没有」；到了立刻回，到期限回「没等到」。
//!
//! 两种用法（换号重启那条命令用）：等某条会话的记录里长出满足判定的一条（压缩摘要）· 等某条会话由一个新进程报出（pidfile）。
//! 事件由内核推（`notify`，不传时间窗），检查跑在 `notify` 自己的线程上；等的那一侧只有**一次**有界等待，期限由发起方给。
//! 先装耳朵、再看一眼：装好之前已经到了的也认得出。

use crate::observe::watcher::{running_sessions, Follow};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// 装好了的一次等待。丢掉它 ⇒ 耳朵一起摘。
pub(crate) struct Armed {
    _ears: notify::RecommendedWatcher,
    hit: tokio::sync::oneshot::Receiver<()>,
}

impl Armed {
    /// 等到 ⇒ `true`；`ms` 毫秒内没等到 ⇒ `false`。
    pub(crate) async fn within(self, ms: u64) -> bool {
        let wait = std::time::Duration::from_millis(ms);
        matches!(tokio::time::timeout(wait, self.hit).await, Ok(Ok(())))
    }
}

/// 一次等待的状态：还没交出去的那一声 ＋ 「到了没有」。
struct Pending<F> {
    tell: Option<tokio::sync::oneshot::Sender<()>>,
    seen: F,
}

impl<F: FnMut() -> bool> Pending<F> {
    fn check(&mut self) {
        if self.tell.is_some() && (self.seen)() {
            if let Some(t) = self.tell.take() {
                let _ = t.send(());
            }
        }
    }
}

/// 盯 `dir`（不递归），每批事件问一次 `seen`。
fn arm(dir: &Path, seen: impl FnMut() -> bool + Send + 'static) -> Result<Armed, String> {
    use notify::Watcher;
    let (tell, hit) = tokio::sync::oneshot::channel::<()>();
    let state = Arc::new(Mutex::new(Pending {
        tell: Some(tell),
        seen,
    }));
    let on_event = state.clone();
    let mut ears = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            on_event.lock().unwrap_or_else(|e| e.into_inner()).check();
        }
    })
    .map_err(|e| e.to_string())?;
    ears.watch(dir, notify::RecursiveMode::NonRecursive)
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    state.lock().unwrap_or_else(|e| e.into_inner()).check();
    Ok(Armed { _ears: ears, hit })
}

/// 等 `record` 里**此刻之后**长出满足 `hit` 的一条（已有的行不算）。读法同流式 watcher 的续读（[`Follow`]）。
pub(crate) fn record_line(
    record: &Path,
    hit: impl Fn(&serde_json::Value) -> bool + Send + 'static,
) -> Result<Armed, String> {
    let dir = record
        .parent()
        .ok_or_else(|| format!("{}: no parent directory", record.display()))?;
    let mut follow = Follow::from_end(record);
    arm(dir, move || {
        follow
            .more()
            .iter()
            .any(|raw| serde_json::from_str::<serde_json::Value>(raw).is_ok_and(|v| hit(&v)))
    })
}

/// 等 `sid` 那份会话记录里此刻之后出现一条压缩摘要（记录按 sid 在这台的记录树里找，同分叉那一找）。
pub(crate) fn compact_summary(agent_home: &Path, sid: &str) -> Result<Armed, String> {
    let record = crate::observe::history_query::session_record(agent_home, sid)?;
    let kind = crate::agents::record_kind_of(&record).unwrap_or_default();
    record_line(&record, move |v| crate::agents::is_compact_summary(kind, v))
}

/// 等 `sid` 由一个**此刻还没在跑**的进程报出（pidfile 目录里一份活的、认得是它的 pidfile）。
pub(crate) fn session_arrival(agent_home: &Path, sid: &str) -> Result<Armed, String> {
    let dir = crate::observe::watcher::pidfile_dir(agent_home);
    let home = agent_home.to_path_buf();
    let sid = sid.to_string();
    let now = |home: &Path, sid: &str| -> Vec<u32> {
        running_sessions(home)
            .into_iter()
            .filter(|(s, _)| s == sid)
            .map(|(_, p)| p)
            .collect()
    };
    let before = now(&home, &sid);
    arm(&dir, move || {
        now(&home, &sid).iter().any(|p| !before.contains(p))
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/one_wait_tests.rs"]
mod tests;

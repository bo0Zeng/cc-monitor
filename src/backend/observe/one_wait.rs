//! **一次性等待**：盯一个目录的文件事件，每来一批问一次「到了没有」；到了立刻回，到期限回「没等到」。
//!
//! 三种用法：换号重启等某条会话的记录里长出满足判定的一条（压缩摘要）· 等某条会话由一个新进程报出（pidfile）；起会话交第一句之前等它就绪（pidfile 说闲着）。
//! 事件由内核推（经盯盘原语 [`crate::platform::watch_file`]，不传时间窗），检查跑在它收的那条线程上；等的那一侧只有**一次**有界等待，期限由发起方给。
//! 先装耳朵、再看一眼：装好之前已经到了的也认得出。

use crate::observe::watcher::{running_sessions, Follow};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// 装好了的一次等待。丢掉它 ⇒ 耳朵一起摘。
pub(crate) struct Armed {
    _ears: crate::platform::watch_file::Watching,
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

/// 发起方那一头：持着的那一端（[`tokio::sync::oneshot::Sender`]）一丢（撤单 · 断线：外层的 future 被丢）⇒ 等的那一侧当场醒。
pub(crate) type Gone = tokio::sync::oneshot::Receiver<()>;

impl Armed {
    /// 同 [`Armed::within`]，在阻塞线程上等（起会话那条命令起好之后在 runtime 的阻塞线程上等）：期限是发起方的截止时刻 `until`，
    /// `gone` 先醒（发起方放手）⇒ 当场 `false`。不在 runtime 里 ⇒ 等不了，回 `false`（说出来）。
    pub(crate) fn within_blocking(self, until: crate::platform::child::Until, gone: Gone) -> bool {
        match tokio::runtime::Handle::try_current() {
            Ok(rt) => rt.block_on(async move {
                tokio::select! {
                    biased;
                    _ = gone => false,
                    hit = self.within(until.left_ms()) => hit,
                }
            }),
            Err(e) => {
                tracing::warn!("one-wait: no runtime to wait on ({e})");
                false
            }
        }
    }
}

/// 盯 `dir`（不递归），每批事件问一次 `seen`。
fn arm(dir: &Path, seen: impl FnMut() -> bool + Send + 'static) -> Result<Armed, String> {
    let (tell, hit) = tokio::sync::oneshot::channel::<()>();
    let state = Arc::new(Mutex::new(Pending {
        tell: Some(tell),
        seen,
    }));
    let on_event = state.clone();
    let ears = crate::platform::watch_file::watch(
        &[(dir.to_path_buf(), false)],
        |_| true,
        "one-wait",
        move |_| on_event.lock().unwrap_or_else(|e| e.into_inner()).check(),
    )?;
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

/// 等 `sid` 就绪、等着下一句输入（[`crate::observe::accounts_query::session_idle`]）：盯这台的 pidfile 目录，装好之前已经闲着的也认得出。
pub(crate) fn session_ready(agent_home: &Path, sid: &str) -> Result<Armed, String> {
    let dir = crate::observe::watcher::pidfile_dir(agent_home);
    let home = agent_home.to_path_buf();
    let sid = sid.to_string();
    arm(&dir, move || {
        crate::observe::accounts_query::session_idle(&home, &sid)
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/observe/one_wait_tests.rs"]
mod tests;

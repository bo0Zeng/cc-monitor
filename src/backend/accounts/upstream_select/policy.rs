//! 上游选择的**热重载**（`accounts/policy.rs  热重载（Reload）`）。
//!
//! 这一整块是从 `server.rs`（中转）搬过来的：「那张表什么时候重读」是**上游选择的私事**，
//! 中转认识「凭据文件」这个东西本身就是越界。行为一个字节没动。

/// 那份凭据文件**这一刻**的印记：`(mtime, 字节数)`。读不到就是 `None`。
///
/// ⚠ 两样一起取是有意的，理由见 `Accounts::refresh_if_changed` 的「它买不到什么」。
pub(super) fn stamp_of(path: &std::path::Path) -> Option<(std::time::SystemTime, u64)> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

/// 那张表从哪儿重读。默认上游那张表（`super::Upstreams`，每 agent 一行）归 `Accounts` 自己持有，重载与首次装表读同一份；
/// 本结构体只有「从哪儿读、上次读到什么样」两样。
pub(crate) struct Reload {
    path: std::path::PathBuf,
    /// 上次读到的 mtime。`None` = 那时读不到（文件不在 / stat 失败）。
    seen: std::sync::Mutex<Option<(std::time::SystemTime, u64)>>,
}

impl Reload {
    pub(crate) fn new(
        path: std::path::PathBuf,
        seen: Option<(std::time::SystemTime, u64)>,
    ) -> Self {
        Self {
            path,
            seen: std::sync::Mutex::new(seen),
        }
    }

    pub(super) fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// 这一刻的印记与上次读到的**一样吗**。一样 ⇒ 那份文件没动过，不必重读。
    pub(super) fn seen_is(&self, now: &Option<(std::time::SystemTime, u64)>) -> bool {
        *self.seen.lock().expect("lock") == *now
    }

    /// 记下这一刻的印记。⚠ 只在**真的换过表**之后才叫它：解析坏了那一支**不更新**，
    /// 下次请求进来还会再试一次，人把文件改回来就自动恢复。
    pub(super) fn remember(&self, now: Option<(std::time::SystemTime, u64)>) {
        *self.seen.lock().expect("lock") = now;
    }
}

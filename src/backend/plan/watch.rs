//! `plan_changed`：盯认过的工作区（计划仓 `.planned-build/` 整棵 ＋ 工作区根上那份 `.env`），
//! 有动静就重跑一次 dump，**输出摘要变了**才推一帧 `{workspace, rev}`；界面收到再来 `plan-read`。
//!
//! 合并不靠定时器（后端零定时器）：一次重读期间来的事件全攒着，重读完一并吞掉、再读一次。
//! 重读本身要 0.1–1 秒，正好把 agent 一口气写的几份并成一两次；写到一半读到的那一刻，
//! 那一片 pb 回 `error` ⇒ 本子给上一次好的、摘要多半不变 ⇒ 不推。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 同时盯着的工作区上限（一个工作区一棵小目录树；再多说明认错了目录）。
pub(crate) const MAX_WATCHED: usize = 32;

/// 计划仓的目录名（工作区根底下）。
pub(crate) const PLAN_DIR: &str = ".planned-build";
/// 工作区根上 pb 记当前片与 auto 的那份文件。
pub(crate) const WORKSPACE_ENV: &str = ".env";

/// 重读一次、回新的摘要（读不成 ⇒ `None`，不推）。生产由帧面宿主给（真 pb ＋ 会话表），判据喂假的。
pub(crate) type Reread = Arc<dyn Fn(&Path) -> Option<String> + Send + Sync>;

/// 进程里那条「某个工作区的计划变了」的通道（流连接订它推 `plan_changed`）：`(工作区, 摘要)`。
pub(crate) fn changes() -> &'static tokio::sync::broadcast::Sender<(String, String)> {
    static TX: std::sync::OnceLock<tokio::sync::broadcast::Sender<(String, String)>> =
        std::sync::OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<(String, String)>(64).0)
}

/// 盯着的那几个：工作区 ⇒ 它的监听器（活着就一直盯）。
static HELD: Mutex<Vec<(PathBuf, notify::RecommendedWatcher)>> = Mutex::new(Vec::new());

/// 这个事件要不要重读：计划仓里的任何东西 · 工作区根上那份 `.env`。
pub(crate) fn concerns(ws: &Path, p: &Path) -> bool {
    p.starts_with(ws.join(PLAN_DIR)) || p == ws.join(WORKSPACE_ENV)
}

/// 开始盯一个工作区（已在盯 ⇒ 什么都不做；满了 ⇒ 只出声）。`last` 是此刻本子里的摘要（之后变了才推）。
pub(crate) fn arm(ws: &Path, last: Option<String>, reread: Reread) {
    let mut g = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if g.iter().any(|(p, _)| p == ws) {
        return;
    }
    if g.len() >= MAX_WATCHED {
        tracing::warn!("[plan] 盯着的工作区已满 {MAX_WATCHED} 个，{} 的变化要重开计划页才看得见", ws.display());
        return;
    }
    match watch(ws, last, reread) {
        Ok(w) => g.push((ws.to_path_buf(), w)),
        Err(e) => tracing::warn!("[plan] 盯不上 {}：{e}（计划变了要重开计划页才看得见）", ws.display()),
    }
}

/// 挂监听、起那条重读线程。返回的那一份活着就一直盯。
pub(crate) fn watch(ws: &Path, last: Option<String>, reread: Reread) -> Result<notify::RecommendedWatcher, String> {
    use notify::Watcher;
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let root = ws.to_path_buf();
    let mut w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(ev) = res else { return };
        if matches!(ev.kind, notify::EventKind::Access(_)) {
            return;
        }
        if ev.paths.iter().any(|p| concerns(&root, p)) {
            let _ = tx.send(());
        }
    })
    .map_err(|e| e.to_string())?;
    w.watch(ws, notify::RecursiveMode::NonRecursive).map_err(|e| e.to_string())?;
    let plan = ws.join(PLAN_DIR);
    if plan.is_dir() {
        w.watch(&plan, notify::RecursiveMode::Recursive).map_err(|e| e.to_string())?;
    }
    let target = ws.to_path_buf();
    std::thread::Builder::new()
        .name("plan-watch".to_string())
        .spawn(move || {
            let mut last = last;
            // 监听器一丢（发端随它走）⇒ 收不到 ⇒ 线程退出。
            while rx.recv().is_ok() {
                while rx.try_recv().is_ok() {}
                let Some(rev) = reread(&target) else { continue };
                if last.as_deref() != Some(rev.as_str()) {
                    last = Some(rev.clone());
                    let _ = changes().send((target.to_string_lossy().to_string(), rev));
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(w)
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/watch_tests.rs"]
mod tests;

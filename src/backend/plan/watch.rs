//! `plan_changed`：盯认过的工作区（计划仓 `.planned-build/` 整棵 ＋ 工作区根上那份 `.env`），
//! 有动静就重跑一次 dump，**输出摘要或要你看的数变了**才推一帧 `{workspace, rev, needs}`；界面收到再来 `plan-read`。
//! 认可 / 撤销认可不动盘上的计划，由那条命令自己经 [`changes`] 推一帧新的数。
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

/// 一个工作区此刻的样子：输出摘要 · 要你看的数（不含问人那一种）。
pub(crate) type Seen = (String, u64);

/// 重读一次、回新的样子（读不成 ⇒ `None`，不推）。生产由帧面宿主给（真 pb ＋ 会话表 ＋ 认可记录），判据喂假的。
pub(crate) type Reread = Arc<dyn Fn(&Path) -> Option<Seen> + Send + Sync>;

/// 一帧 `plan_changed`：`(工作区, 摘要, 要你看的数)`。
pub(crate) type Change = (String, String, u64);

/// 进程里那条「某个工作区的计划变了」的通道（流连接订它推 `plan_changed`）。
pub(crate) fn changes() -> &'static tokio::sync::broadcast::Sender<Change> {
    static TX: std::sync::OnceLock<tokio::sync::broadcast::Sender<Change>> =
        std::sync::OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel::<Change>(64).0)
}

/// 盯着的那几个：工作区 ⇒ 它的监听器（活着就一直盯）。
static HELD: Mutex<Vec<(PathBuf, crate::platform::watch_file::Watching)>> = Mutex::new(Vec::new());

/// 这个事件要不要重读：计划仓里的任何东西 · 工作区根上那份 `.env`。
pub(crate) fn concerns(ws: &Path, p: &Path) -> bool {
    p.starts_with(ws.join(PLAN_DIR)) || p == ws.join(WORKSPACE_ENV)
}

/// 开始盯一个工作区（已在盯 ⇒ 什么都不做；满了 ⇒ 只出声）。`last` 是此刻的样子（之后变了才推）。
pub(crate) fn arm(ws: &Path, last: Option<Seen>, reread: Reread) {
    let mut g = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if g.iter().any(|(p, _)| p == ws) {
        return;
    }
    if g.len() >= MAX_WATCHED {
        tracing::warn!(
            "[plan] 盯着的工作区已满 {MAX_WATCHED} 个，{} 的变化要重开计划页才看得见",
            ws.display()
        );
        return;
    }
    match watch(ws, last, reread) {
        Ok(w) => g.push((ws.to_path_buf(), w)),
        Err(e) => tracing::warn!(
            "[plan] 盯不上 {}：{e}（计划变了要重开计划页才看得见）",
            ws.display()
        ),
    }
}

/// 挂监听（工作区根本层 ＋ 计划仓整棵，经 [`crate::platform::watch_file`]）、起那条重读线程。返回的那一份活着就一直盯。
pub(crate) fn watch(
    ws: &Path,
    last: Option<Seen>,
    reread: Reread,
) -> Result<crate::platform::watch_file::Watching, String> {
    let mut dirs = vec![(ws.to_path_buf(), false)];
    let plan = ws.join(PLAN_DIR);
    if plan.is_dir() {
        dirs.push((plan, true));
    }
    let root = ws.to_path_buf();
    let target = ws.to_path_buf();
    let mut last = last;
    crate::platform::watch_file::watch(
        &dirs,
        move |p| concerns(&root, p),
        "plan-watch",
        move |_| {
            let Some(now) = reread(&target) else { return };
            if last.as_ref() != Some(&now) {
                last = Some(now.clone());
                let _ = changes().send((target.to_string_lossy().to_string(), now.0, now.1));
            }
        },
    )
}

#[cfg(test)]
#[path = "../../../tests/backend/plan/watch_tests.rs"]
mod tests;

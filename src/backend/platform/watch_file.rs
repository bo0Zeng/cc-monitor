//! 盯盘的唯一原语：在几个目录上挂监听（`notify`），滤掉只读访问，路径过了调用方给的过滤才算一次「有动静」；
//! 另起一条线程收，一阵动静（一次处理期间又来的）并成一次回调。返回的监听器活着就一直盯，一丢 ⇒ 线程跟着退出。
//!
//! 合并不靠定时器（后端零定时器）：回调本身的耗时就是合并窗口。调用方只给「盯哪几个目录 · 哪些路径算数 · 有动静做什么」。

use std::path::{Path, PathBuf};

/// 盯一个目录：要不要连子目录一起。
pub(crate) type Dir = (PathBuf, bool);

/// 挂监听、起那条收的线程（名字 `thread`）。`concerns` 认一条路径算不算数；`on_change` 每阵动静调一次。
pub(crate) fn watch(
    dirs: &[Dir],
    concerns: impl Fn(&Path) -> bool + Send + 'static,
    thread: &str,
    mut on_change: impl FnMut() + Send + 'static,
) -> Result<notify::RecommendedWatcher, String> {
    use notify::Watcher;
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let mut w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(ev) = res else { return };
        if matches!(ev.kind, notify::EventKind::Access(_)) {
            return;
        }
        if ev.paths.iter().any(|p| concerns(p)) {
            let _ = tx.send(());
        }
    })
    .map_err(|e| e.to_string())?;
    for (d, deep) in dirs {
        let mode = if *deep {
            notify::RecursiveMode::Recursive
        } else {
            notify::RecursiveMode::NonRecursive
        };
        w.watch(d, mode).map_err(|e| e.to_string())?;
    }
    std::thread::Builder::new()
        .name(thread.to_string())
        .spawn(move || {
            // 监听器一丢（发端随它走）⇒ 收不到 ⇒ 线程退出。
            while rx.recv().is_ok() {
                while rx.try_recv().is_ok() {}
                on_change();
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(w)
}

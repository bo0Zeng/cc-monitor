//! 盯盘的唯一原语：在几个目录上挂监听（`notify`），滤掉只读访问，路径过了调用方给的过滤才算一次「有动静」；
//! 另起一条线程收，一阵动静（一次处理期间又来的）并成一次回调、交出这一阵动过的路径。返回的那一份活着就一直盯，一丢 ⇒ 线程跟着退出；
//! 名单换得动（盯的目录跟着账号清单 / 浏览名单走的那几处）、踢得醒（自己做完一件事要同步一趟的那一处）。
//!
//! 合并不靠定时器（后端零定时器）：回调本身的耗时就是合并窗口。调用方只给「盯哪几个目录 · 哪些路径算数 · 有动静做什么」。
//! 用它的：轮换 · 计划 · 一次性等待 · 各号 MCP 与信任同步 · 浏览目录保鲜。生产段里挂 `notify` 监听只此一处
//! （会话流那一份 `observe/watcher.rs` 除外：它是流本身，盯整棵记录树、按批去抖、自己分派）——判据 `only_this_primitive_hangs_a_notify_watcher`。

use std::path::{Path, PathBuf};

/// 盯一个目录：要不要连子目录一起。
pub(crate) type Dir = (PathBuf, bool);

/// 一份活着的监听：名单换得动（[`Watching::rearm`]）、踢得醒（[`Watching::kick`]）。一丢 ⇒ 监听卸掉、收的线程跟着退出。
pub(crate) struct Watching {
    inner: notify::RecommendedWatcher,
    /// 此刻真挂着的那几个目录。
    armed: Vec<Dir>,
    /// 通往收的线程那一头：`Some(路径)` ＝ 一条过了过滤的动静 · `None` ＝ 踢一下。
    tx: std::sync::mpsc::Sender<Option<PathBuf>>,
}

impl Watching {
    /// 名单跟上 `dirs`：新来的挂上、离开的卸掉（留下的不重挂）。回这一趟挂不上的逐条原因（「目录：原话」）。
    pub(crate) fn rearm(&mut self, dirs: &[Dir]) -> Vec<String> {
        use notify::Watcher;
        for gone in self.armed.iter().filter(|d| !dirs.contains(d)) {
            // 卸不掉多半是那个目录已经没了（watch 跟着 inode 一起走了）：名单照样摘。
            let _ = self.inner.unwatch(&gone.0);
        }
        self.armed.retain(|d| dirs.contains(d));
        let mut failed = Vec::new();
        for d in dirs {
            if self.armed.contains(d) {
                continue;
            }
            match self.inner.watch(&d.0, mode(d.1)) {
                Ok(()) => self.armed.push(d.clone()),
                Err(e) => failed.push(format!("{}: {e}", d.0.display())),
            }
        }
        failed
    }

    /// 此刻真挂着的目录数。
    pub(crate) fn armed(&self) -> usize {
        self.armed.len()
    }

    /// 踢一下：没有文件动静也回调一次（路径为空）—— 调用方自己做完了一件事、要同步一趟时用。
    pub(crate) fn kick(&self) {
        let _ = self.tx.send(None);
    }
}

fn mode(deep: bool) -> notify::RecursiveMode {
    if deep {
        notify::RecursiveMode::Recursive
    } else {
        notify::RecursiveMode::NonRecursive
    }
}

/// 挂监听、起那条收的线程（名字 `thread`）。`concerns` 认一条路径算不算数；`on_change` 每阵动静调一次，
/// 交出这一阵里过了过滤的路径（去重；踢一下那一次为空）。`dirs` 里任何一个挂不上 ⇒ `Err`（要容忍的调用方传空名单、再 [`Watching::rearm`]）。
pub(crate) fn watch(
    dirs: &[Dir],
    concerns: impl Fn(&Path) -> bool + Send + 'static,
    thread: &str,
    mut on_change: impl FnMut(&[PathBuf]) + Send + 'static,
) -> Result<Watching, String> {
    use notify::Watcher;
    let (tx, rx) = std::sync::mpsc::channel::<Option<PathBuf>>();
    let events = tx.clone();
    let mut inner = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(ev) = res else { return };
        if matches!(ev.kind, notify::EventKind::Access(_)) {
            return;
        }
        for p in ev.paths.into_iter().filter(|p| concerns(p)) {
            let _ = events.send(Some(p));
        }
    })
    .map_err(|e| e.to_string())?;
    for (d, deep) in dirs {
        inner
            .watch(d, mode(*deep))
            .map_err(|e| format!("{}: {e}", d.display()))?;
    }
    std::thread::Builder::new()
        .name(thread.to_string())
        .spawn(move || {
            // 监听器一丢（两个发端随它走）⇒ 收不到 ⇒ 线程退出。
            while let Ok(first) = rx.recv() {
                let mut batch: Vec<PathBuf> = Vec::new();
                for p in std::iter::once(first)
                    .chain(std::iter::from_fn(|| rx.try_recv().ok()))
                    .flatten()
                {
                    if !batch.contains(&p) {
                        batch.push(p);
                    }
                }
                on_change(&batch);
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(Watching {
        inner,
        armed: dirs.to_vec(),
        tx,
    })
}

#[cfg(test)]
#[path = "../../../tests/backend/platform/watch_file_tests.rs"]
mod tests;

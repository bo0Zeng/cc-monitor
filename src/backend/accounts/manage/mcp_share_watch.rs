//! **什么时候同步**：常驻后端里一个监听器盯账号库目录与每个号的目录（只盯直接子项），只认两个名字 ——
//! 各号的配置文件（Claude 改了它、用户在那个号里 `claude mcp add` 了）与账号清单（加号 / 删号 ⇒ 盯的名单跟着换）。
//!
//! 不轮询、不传时间窗：事件进一条无超时的通道，工作线程收到一条就把攒着的排空、同步一趟（一阵连着的改动并成一趟）。
//! 同步自己写回去的那一下也会来一个事件，下一趟算出来没有要写的 ⇒ 停在那里。
//! 加号 · 建库 · 删号是这台后端自己做的：做完之后帧面宿主 [`kick`] 一下（新建的目录此前没被盯着）。
//!
//! ⚠ 买不到的（同 `files/browse_watch.rs`）：watch 绑 inode 不绑路径 · 内核队列溢出会丢事件 —— 丢了的那一下等下一个事件补上。

use crate::assets::door::Door;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;

/// 进程里那一个监听器与通往工作线程的那一头（第一次 [`start`] 时起，此后一直持有）。
struct Live {
    watcher: notify::RecommendedWatcher,
    armed: Vec<PathBuf>,
    kick: Sender<()>,
}

static LIVE: Mutex<Option<Live>> = Mutex::new(None);

/// 这个路径的最后一段是不是值得同步一趟的那两个名字之一。
fn relevant(p: &std::path::Path) -> bool {
    p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
        n == super::layout::identity_config_file() || n == super::scan::MANIFEST_FILE
    })
}

/// 此刻该盯的目录：账号库目录 ＋ 清单里每个号的目录（在的那几个）。
fn wanted(home: &str) -> Vec<PathBuf> {
    let accts = super::scan::accts_root(home);
    let mut out = vec![PathBuf::from(&accts)];
    if let Ok(Some(list)) = super::mcp_share_exec::accounts_in(home) {
        out.extend(list.into_iter().map(|(_, dir)| PathBuf::from(dir)));
    }
    out.retain(|p| p.is_dir());
    out
}

/// 名单跟上此刻（新来的挂上、离开的卸掉）。挂不上的只出声。
fn rearm(home: &str) {
    use notify::Watcher;
    let want = wanted(home);
    let Ok(mut g) = LIVE.lock() else { return };
    let Some(live) = g.as_mut() else { return };
    for gone in live.armed.iter().filter(|p| !want.contains(p)) {
        let _ = live.watcher.unwatch(gone);
    }
    live.armed.retain(|p| want.contains(p));
    for p in want {
        if live.armed.contains(&p) {
            continue;
        }
        match live.watcher.watch(&p, notify::RecursiveMode::NonRecursive) {
            Ok(()) => live.armed.push(p),
            Err(e) => tracing::warn!("账号之间同步 MCP：盯不上 {}：{e}", p.display()),
        }
    }
}

/// 工作线程：收一条 ⇒ 排空 ⇒ 名单跟上 ⇒ 同步一趟。只出声，不拖垮后端。
fn work(rx: Receiver<()>, d: &'static (dyn Door + Sync)) {
    while rx.recv().is_ok() {
        while rx.try_recv().is_ok() {}
        let home = match crate::assets::door::home(d) {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!("账号之间同步 MCP：家目录说不出来：{e}");
                continue;
            }
        };
        rearm(&home);
        match super::mcp_share_exec::sync(d) {
            Ok(v) => {
                if !v.changed.is_empty() {
                    tracing::info!("账号之间同步 MCP：改写了 {}", v.changed.join(" · "));
                }
                for n in v.notes {
                    tracing::warn!("账号之间同步 MCP：{n}");
                }
                for c in v.conflicts {
                    tracing::warn!("账号之间同步 MCP：{} 两边都改了，等你在账号页里挑", c.name);
                }
            }
            Err((_, why)) => tracing::warn!("账号之间同步 MCP：这一趟没做成：{why}"),
        }
    }
}

/// 起监听器与工作线程（进程里只起一次），并立刻同步一趟（后端刚起来时各号可能已经不一样了）。
/// `d` = 这台的文件管理面（写各号配置文件与共享集合都经它）。
pub(crate) fn start(d: &'static (dyn Door + Sync)) {
    let Ok(mut g) = LIVE.lock() else { return };
    if g.is_some() {
        return;
    }
    let (tx, rx) = mpsc::channel::<()>();
    let to_worker = tx.clone();
    let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(ev) = res else { return };
        if matches!(
            ev.kind,
            notify::EventKind::Access(
                notify::event::AccessKind::Read | notify::event::AccessKind::Open(_)
            )
        ) {
            return;
        }
        if ev.paths.iter().any(|p| relevant(p)) {
            let _ = to_worker.send(());
        }
    });
    let watcher = match watcher {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(
                "账号之间同步 MCP：监听器起不来，这一次后端运行期间只在加号 / 建库时同步：{e}"
            );
            return;
        }
    };
    let spawned = std::thread::Builder::new()
        .name("acct-mcp-share".to_string())
        .spawn(move || work(rx, d));
    if let Err(e) = spawned {
        tracing::warn!("账号之间同步 MCP：工作线程起不来：{e}");
        return;
    }
    *g = Some(Live {
        watcher,
        armed: Vec::new(),
        kick: tx,
    });
    drop(g);
    kick();
}

/// 踢工作线程一下（名单跟上 ＋ 同步一趟）。监听器没起 ⇒ 什么都不做。
pub(crate) fn kick() {
    if let Ok(g) = LIVE.lock() {
        if let Some(live) = g.as_ref() {
            let _ = live.kick.send(());
        }
    }
}

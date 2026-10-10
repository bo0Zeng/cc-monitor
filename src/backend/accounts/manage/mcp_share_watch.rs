//! **什么时候同步**：常驻后端里一个监听器盯账号库目录 · 每个号的目录 · 家目录（都只盯直接子项），只认两个名字 ——
//! 各号的配置文件（Claude 改了它、用户在那个号里 `claude mcp add` 了、在那个号里信任了一个目录）· 家目录下账号 0 那一份
//! （信任只读它，一个字节不写）与账号清单（加号 / 删号 ⇒ 盯的名单跟着换）。每一趟先同步用户级 MCP（[`super::mcp_share_exec`]），
//! 再同步信任（[`super::trust_share_exec`]）。
//!
//! 不轮询、不传时间窗：经盯盘原语 [`crate::platform::watch_file`]，一阵连着的改动并成一趟回调、同步一趟。
//! 同步自己写回去的那一下也会来一个事件，下一趟算出来没有要写的 ⇒ 停在那里。
//! 加号 · 建库 · 删号是这台后端自己做的：做完之后帧面宿主 [`kick`] 一下（新建的目录此前没被盯着）。
//!
//! ⚠ 买不到的（同 `files/browse_watch.rs`）：watch 绑 inode 不绑路径 · 内核队列溢出会丢事件 —— 丢了的那一下等下一个事件补上。

use crate::assets::door::Door;
use crate::platform::watch_file::Watching;
use std::path::PathBuf;
use std::sync::Mutex;

/// 进程里那一份监听（第一次 [`start`] 时起，此后一直持有；名单跟着账号清单换）。
static LIVE: Mutex<Option<Watching>> = Mutex::new(None);

/// 这个路径的最后一段是不是值得同步一趟的那两个名字之一。
fn relevant(p: &std::path::Path) -> bool {
    p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
        n == super::layout::identity_config_file() || n == super::scan::MANIFEST_FILE
    })
}

/// 此刻该盯的目录：账号库目录 ＋ 清单里每个号的目录（在的那几个）＋ 有账号库时家目录（账号 0 那一份是信任的来源之一）。
fn wanted(home: &str) -> Vec<PathBuf> {
    let accts = super::scan::accts_root(home);
    let mut out = vec![PathBuf::from(&accts)];
    if let Ok(Some(list)) = super::mcp_share_exec::accounts_in(home) {
        out.extend(list.into_iter().map(|(_, dir)| PathBuf::from(dir)));
        out.push(PathBuf::from(home));
    }
    out.retain(|p| p.is_dir());
    out
}

/// 名单跟上此刻（新来的挂上、离开的卸掉）。挂不上的只出声。
fn rearm(home: &str) {
    let want: Vec<(PathBuf, bool)> = wanted(home).into_iter().map(|p| (p, false)).collect();
    let Ok(mut g) = LIVE.lock() else { return };
    let Some(live) = g.as_mut() else { return };
    for e in live.rearm(&want) {
        tracing::warn!("账号之间同步 MCP：盯不上 {e}");
    }
}

/// 一趟：名单跟上 ⇒ 同步一趟。只出声，不拖垮后端。
fn sync_once(d: &'static (dyn Door + Sync)) {
    let home = match crate::assets::door::home(d) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!("账号之间同步 MCP：家目录说不出来：{e}");
            return;
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
        Err(f) => tracing::warn!(
            "账号之间同步 MCP：这一趟没做成：{}",
            crate::common::said::Said::from(f).logged()
        ),
    }
    match super::trust_share_exec::sync(d) {
        Ok(changed) if !changed.is_empty() => {
            tracing::info!("账号之间同步信任：改写了 {}", changed.join(" · "))
        }
        Ok(_) => {}
        Err(f) => tracing::warn!(
            "账号之间同步信任：这一趟没做成：{}",
            crate::common::said::Said::from(f).logged()
        ),
    }
}

/// 起监听（进程里只起一次），并立刻同步一趟（后端刚起来时各号可能已经不一样了）。
/// `d` = 这台的文件管理面（写各号配置文件与共享集合都经它）。
pub(crate) fn start(d: &'static (dyn Door + Sync)) {
    let Ok(mut g) = LIVE.lock() else { return };
    if g.is_some() {
        return;
    }
    match crate::platform::watch_file::watch(&[], relevant, "acct-mcp-share", move |_| sync_once(d))
    {
        Ok(w) => *g = Some(w),
        Err(e) => {
            tracing::warn!(
                "账号之间同步 MCP：监听器起不来，这一次后端运行期间只在加号 / 建库时同步：{e}"
            );
            return;
        }
    }
    drop(g);
    kick();
}

/// 踢工作线程一下（名单跟上 ＋ 同步一趟）。监听器没起 ⇒ 什么都不做。
pub(crate) fn kick() {
    if let Ok(g) = LIVE.lock() {
        if let Some(live) = g.as_ref() {
            live.kick();
        }
    }
}

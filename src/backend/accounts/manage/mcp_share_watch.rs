//! **什么时候同步**：常驻后端里一个监听器盯账号库目录 · 每个号的目录 · 有信任格的每一家账号 0 那份配置所在的目录（都只盯直接子项），
//! 只认这几个名字（取自各家）——各号的 MCP 配置与配置文件（那一家改了它、用户在那个号里加了一个 MCP、在那个号里信任了一个目录）·
//! 每一家账号 0 那一份配置（信任只读它，一个字节不写）· 各号的 MCP 登录文件（在那个号里登录了一个 MCP、那一家续了一条 MCP 登录）与账号清单
//! （加号 / 删号 ⇒ 盯的名单跟着换）。每一趟先同步用户级 MCP（[`super::mcp_share_exec`]），再同步 MCP 登录
//! （[`super::mcp_logins_exec`]），再同步信任（[`super::trust_share_exec`]）。
//!
//! 不轮询、不传时间窗：经盯盘原语 [`crate::platform::watch_file`]，一阵连着的改动并成一趟回调、同步一趟。
//! 同步自己写回去的那一下也会来一个事件，下一趟算出来没有要写的 ⇒ 停在那里。
//! 加号 · 建库 · 删号是这台后端自己做的：做完之后帧面宿主 [`kick`] 一下（新建的目录此前没被盯着）。
//!
//! ⚠ 买不到的（同 `files/browse_watch.rs`）：watch 绑 inode 不绑路径 · 内核队列溢出会丢事件 —— 丢了的那一下等下一个事件补上。

use crate::common::door::Door;
use crate::platform::watch_file::Watching;
use std::path::PathBuf;
use std::sync::Mutex;

/// 进程里那一份监听（第一次 [`start`] 时起，此后一直持有；名单跟着账号清单换）。
static LIVE: Mutex<Option<Watching>> = Mutex::new(None);

/// 这个路径的最后一段是不是值得同步一趟的那几个名字之一。
/// 文件名取自各家，认哪几家与那几趟同步挑家同一个判法：同步服务器的那几家（[`super::mcp_share_exec::families`]）各号的 MCP 配置
/// （[`crate::agents::McpCells::file`]）· 同步登录的那几家（[`super::mcp_logins_exec::login_families`]）各号的 MCP 登录文件
/// （[`crate::agents::McpLogins::file`]）· 声明了信任格的每一家的配置文件（[`trust_families`]，信任读它）· 账号清单。
fn relevant(p: &std::path::Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| relevant_among(crate::agents::REGISTRY, n))
}

/// [`relevant`] 的可喂夹具那一半（只看最后一段）。
pub(crate) fn relevant_among(reg: &[crate::agents::Adapter], n: &str) -> bool {
    super::mcp_share_exec::families(reg)
        .iter()
        .any(|f| f.cells.file == n)
        || super::mcp_logins_exec::login_families(reg).any(|(_, _, l)| l.file == n)
        || trust_families(reg).any(|(_, f)| f.config_file == n)
        || n == super::scan::MANIFEST_FILE
}

/// 注册表里声明了信任格的每一家（家名 ＋ 账号面），与信任那一趟（[`super::trust_share_exec::sync_among`]）挑家同一个判法。
fn trust_families(
    reg: &[crate::agents::Adapter],
) -> impl Iterator<Item = (&'static str, crate::agents::AccountsFace)> + '_ {
    reg.iter().filter_map(|a| {
        a.accounts
            .filter(|f| f.trust.is_some())
            .map(|f| (a.kind, f))
    })
}

/// 此刻该盯的目录：账号库目录 ＋ 清单里每个号的目录（在的那几个）＋ 有账号库时，清单里有号、声明了信任格的每一家
/// 账号 0 那份配置的父目录（[`super::trust_share_exec::trust_zero_dirs`]，账号 0 那一份是信任的来源之一）。
fn wanted(home: &str) -> Vec<PathBuf> {
    let m = super::mcp_share_exec::manifest_in(home).ok().flatten();
    wanted_among(crate::agents::REGISTRY, home, m.as_ref())
}

/// [`wanted`] 的可喂夹具那一半（注册表 · 清单由调用方给）。
pub(crate) fn wanted_among(
    reg: &[crate::agents::Adapter],
    home: &str,
    m: Option<&super::model::Manifest>,
) -> Vec<PathBuf> {
    let accts = super::scan::accts_root(home);
    let mut out = vec![PathBuf::from(&accts)];
    if let Some(m) = m {
        out.extend(m.managed().map(|a| PathBuf::from(&a.config_dir)));
        for dir in super::trust_share_exec::trust_zero_dirs(home, reg, m) {
            if !out.iter().any(|o| *o == dir) {
                out.push(dir);
            }
        }
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
fn sync_once(d: &'static (dyn Door + Sync), own: &crate::platform::fs::Own) {
    let home = match super::scan::home(d) {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!("账号之间同步 MCP：家目录说不出来：{e}");
            return;
        }
    };
    rearm(&home);
    share_once(d, crate::agents::REGISTRY);
    logins_once(d, own, crate::agents::REGISTRY);
    match super::trust_share_exec::sync(d) {
        Ok(changed) if !changed.is_empty() => {
            tracing::info!("账号之间同步信任：改写了 {}", changed.join(" · "))
        }
        Ok(_) => {}
        Err(f) => tracing::warn!("账号之间同步信任：这一趟没做成：{}", f.logged()),
    }
}

/// 一趟里同步用户级 MCP 的那一下与它出的声（注册表可喂夹具）：逐家说，每一行带是哪一家的（缺省那一家也写）。
pub fn share_once(d: &dyn Door, reg: &[crate::agents::Adapter]) {
    match super::mcp_share_exec::sync_each(d, reg) {
        Ok(each) => {
            for (kind, v) in each {
                if !v.changed.is_empty() {
                    tracing::info!(
                        "账号之间同步 MCP（{kind}）：改写了 {}",
                        v.changed.join(" · ")
                    );
                }
                for n in v.notes {
                    tracing::warn!("账号之间同步 MCP（{kind}）：{n}");
                }
                for c in v.conflicts {
                    tracing::warn!(
                        "账号之间同步 MCP（{kind}）：{} 两边都改了，等账号页里挑",
                        c.name
                    );
                }
            }
        }
        Err(f) => tracing::warn!("账号之间同步 MCP：这一趟没做成：{}", f.logged()),
    }
}

/// 一趟里同步 MCP 登录的那一下与它出的声（注册表可喂夹具）：改写了谁记一行，没落下的那几句各一行。
pub fn logins_once(d: &dyn Door, own: &crate::platform::fs::Own, reg: &[crate::agents::Adapter]) {
    match super::mcp_logins_exec::sync_among(d, own, reg) {
        Ok(v) => {
            if !v.changed.is_empty() {
                tracing::info!("账号之间同步 MCP 登录：改写了 {}", v.changed.join(" · "));
            }
            for n in v.notes {
                tracing::warn!("账号之间同步 MCP 登录：{n}");
            }
        }
        Err(f) => tracing::warn!("账号之间同步 MCP 登录：这一趟没做成：{}", f.logged()),
    }
}

/// 起监听（进程里只起一次），并立刻同步一趟（后端刚起来时各号可能已经不一样了）。
/// `d` = 这台的文件管理面（写各号配置文件与共享集合都经它）；`own` = 写自家状态的句柄（各号凭据文件经它铸的窄句柄写）。
pub fn start(d: &'static (dyn Door + Sync), own: crate::platform::fs::Own) {
    let Ok(mut g) = LIVE.lock() else { return };
    if g.is_some() {
        return;
    }
    match crate::platform::watch_file::watch(&[], relevant, "acct-mcp-share", move |_| {
        sync_once(d, &own)
    }) {
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
pub fn kick() {
    if let Ok(g) = LIVE.lock() {
        if let Some(live) = g.as_ref() {
            live.kick();
        }
    }
}

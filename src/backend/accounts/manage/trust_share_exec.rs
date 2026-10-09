//! **信任在各号之间同步的那一半 IO** ＋ **cc-monitor 起会话前预标信任**。
//!
//! - 同步一趟：读家目录下账号 0 那一份（**只读**，一个字节不写）与清单里每个号的那一份 → 信任过的并集 →
//!   每个号缺的那几格标上（[`super::trust_share::mark`]：只换那几格）→ 经文件管理面 `files-put` 写，`expect` = 读到的那一份（CAS）。
//!   被抢先改了 ⇒ 这个号这一趟不写；抢先的那一下自己会再触发一次同步。同步自己写回去触发的那一趟算出来没有要写的 ⇒ 停。
//! - 预标：cc-monitor 自己起 / 重启会话之前，把工作目录要标的那几个键（适配层说，[`TrustCells::dir_keys`]）标进要用的那个号（只许清单里的号目录），随后由同步传到全部号。
//! - 整趟持账号库那把锁（与 MCP 同步 · 改账号库那几条命令同一把）。写不成只出声，不挡起会话。

use super::layout;
use super::mcp_share_exec::{accounts_in, lock, put_private, read_text, MAX_CONFIG_BYTES};
use super::scan::join;
use super::trust_share;
use crate::agents::TrustCells;
use crate::assets::door::{self, Door, Refused};
use std::collections::BTreeSet;

type Refusal = crate::stream::inbound::spec::Fail;

/// 预标那一下的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Marked {
    /// 标上了。
    Wrote,
    /// 本来就信任过。
    Already,
    /// 没写：这台没有账号库 / 那一家没有「信任」这件事。
    Nothing,
    /// 没写：那个目录不在清单里（或写法不安全）。
    NotListed,
    /// 没写：两次都被抢先改了（下一次文件事件由同步补上）。
    Stale,
}

/// 同步一趟（文件事件 · 加号 · 建库之后）。回改写了的号名。这台没有账号库 / 那一家没有「信任」这件事 ⇒ 什么都不做。
pub(crate) fn sync(d: &dyn Door) -> Result<Vec<String>, Refusal> {
    let Some(cells) = layout::face().and_then(|f| f.trust) else {
        return Ok(Vec::new());
    };
    let home = door::home(d).map_err(|e| Refusal::new("io_failed", e))?;
    let _held = lock(&home)?;
    let Some(list) = accounts_in(&home)? else {
        return Ok(Vec::new());
    };
    Ok(sync_with(
        d,
        &home,
        &cells,
        layout::identity_config_file(),
        &list,
    ))
}

/// 一份配置：原文（不在 ⇒ `None`）＋ 信任过的目录。读不出来 / 解不开 ⇒ `Err`。
fn read_conf(
    cells: &TrustCells,
    path: &str,
) -> Result<(Option<String>, BTreeSet<String>), crate::common::said::Said> {
    let Some(raw) = read_text(path, MAX_CONFIG_BYTES)? else {
        return Ok((None, BTreeSet::new()));
    };
    let v: serde_json::Value = serde_json::from_str(raw.trim_start_matches('\u{feff}'))
        .map_err(|e| crate::common::said::Said::from(e.to_string()))?;
    let set = trust_share::trusted(cells, &v);
    Ok((Some(raw), set))
}

/// 号目录过得了那两道关：写法安全（全仓那一条规矩）＋ 在清单里。过得了 ⇒ 清单里那一条的写法。
fn listed<'a>(list: &'a [(String, String)], dir: &str) -> Option<&'a (String, String)> {
    let norm = |p: &str| p.trim_end_matches('/').to_string();
    if !acct_core::config_dir_ok(dir) {
        return None;
    }
    list.iter()
        .find(|(_, d)| acct_core::config_dir_ok(d) && norm(d) == norm(dir))
}

/// [`sync`] 的本体：信任那几格 · 配置文件名 · 清单（`(号名, 号目录)`）是参数（判据拿假适配层的格式喂它）。
/// 家目录下那一份（账号 0）只读。读不出来的号这一趟不参与（不读它、也不写它）。
pub(crate) fn sync_with(
    d: &dyn Door,
    home: &str,
    cells: &TrustCells,
    file: &str,
    list: &[(String, String)],
) -> Vec<String> {
    let mut union = BTreeSet::new();
    let zero = join(home, file);
    match read_conf(cells, &zero) {
        Ok((_, set)) => union.extend(set),
        Err(e) => tracing::warn!(
            "账号之间同步信任：{zero} 读不出来，这一次不并它：{}",
            e.logged()
        ),
    }
    let mut seen = Vec::new();
    for (name, dir) in list {
        if listed(list, dir).is_none() {
            tracing::warn!("账号之间同步信任：{name} 号的目录写法不安全，跳过它");
            continue;
        }
        let path = join(dir, file);
        match read_conf(cells, &path) {
            Ok((raw, set)) => {
                union.extend(set);
                seen.push((name, path, raw));
            }
            Err(e) => tracing::warn!(
                "账号之间同步信任：{name} 号的配置 {path} 读不出来，这一次跳过它：{}",
                e.logged()
            ),
        }
    }
    let mut changed = Vec::new();
    for (name, path, raw) in seen {
        match write_marks(d, home, cells, &path, raw.as_deref(), &union) {
            Ok(Marked::Wrote) => changed.push(name.clone()),
            Ok(Marked::Stale) => {
                tracing::info!(
                    "账号之间同步信任：{name} 号的配置刚被改过，这一次没写（它一改完会再同步一次）"
                )
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("账号之间同步信任：{name} 号这一次没写上：{}", e.logged()),
        }
    }
    changed
}

/// 一份配置里标上 `dirs`：缺的那几格换上，CAS 写回（`expect` = 读到的那一份）。
fn write_marks(
    d: &dyn Door,
    home: &str,
    cells: &TrustCells,
    path: &str,
    raw: Option<&str>,
    dirs: &BTreeSet<String>,
) -> Result<Marked, crate::common::said::Said> {
    let Some(text) = trust_share::mark(cells, raw.unwrap_or("{}\n"), dirs)? else {
        return Ok(Marked::Already);
    };
    let rel = door::rel_under(home, path)?;
    match put_private(d, home, &rel, &text, raw) {
        Ok(()) => Ok(Marked::Wrote),
        Err(Refused::Stale(_)) => Ok(Marked::Stale),
        Err(e) => Err(crate::common::said::Said::from(e.said())),
    }
}

/// 起会话之前：`cwd` 标进 `config_dir` 那个号（必须在清单里）。这台没有账号库 / 那一家没有「信任」这件事 ⇒ [`Marked::Nothing`]。
pub(crate) fn pretrust(
    d: &dyn Door,
    config_dir: &str,
    cwd: &str,
) -> Result<Marked, crate::common::said::Said> {
    let Some(cells) = layout::face().and_then(|f| f.trust) else {
        return Ok(Marked::Nothing);
    };
    let home = door::home(d)?;
    let _held = lock(&home).map_err(crate::common::said::Said::from)?;
    let Some(list) = accounts_in(&home).map_err(crate::common::said::Said::from)? else {
        return Ok(Marked::Nothing);
    };
    pretrust_with(
        d,
        &home,
        &cells,
        layout::identity_config_file(),
        &list,
        config_dir,
        cwd,
    )
}

/// [`pretrust`] 的本体（同 [`sync_with`]，几格是参数）。被抢先改了 ⇒ 重读再试一次；还不成 ⇒ [`Marked::Stale`]。
pub(crate) fn pretrust_with(
    d: &dyn Door,
    home: &str,
    cells: &TrustCells,
    file: &str,
    list: &[(String, String)],
    config_dir: &str,
    cwd: &str,
) -> Result<Marked, crate::common::said::Said> {
    let Some((_, dir)) = listed(list, config_dir) else {
        return Ok(Marked::NotListed);
    };
    let dirs: BTreeSet<String> = (cells.dir_keys)(cwd).into_iter().collect();
    let path = join(dir, file);
    for _ in 0..2 {
        let raw = read_text(&path, MAX_CONFIG_BYTES)?;
        match write_marks(d, home, cells, &path, raw.as_deref(), &dirs)? {
            Marked::Stale => continue,
            done => return Ok(done),
        }
    }
    Ok(Marked::Stale)
}

#[cfg(all(test, unix))]
#[path = "../../../../tests/backend/accounts/manage/trust_share_exec_tests.rs"]
mod tests;

//! 后端原子写落在原名旁边的旁名：写到一半的件、换名件、留下的备份，各一个形状。
//! 造名的一侧（文件管理面的写与备份 · 账号库执行 · 自家状态的原子写）只经这里拼。
//! 账号库的共享集只认已知共享表里的名字，旁名本来就不在里面，不另认。

use std::ffi::{OsStr, OsString};

/// 文件管理面写入的换名件带的记号：`.<名>.ccm-swap-<pid>-<序>.part`。
const SWAP_MARK: &str = ".ccm-swap-";
/// 文件管理面覆盖之前另存的原文带的记号：`<名>.ccm-backup-<毫秒>-<序>`（留下不删）。
const BACKUP_MARK: &str = ".ccm-backup-";
/// 账号库执行复制 · 隔离时的换名件带的记号：`<路径>.ccm-tmp-<pid>`。
const TMP_MARK: &str = ".ccm-tmp-";

/// 旁名里带的记号（判据核「记号的字面只在这一份里」用）。
#[cfg(test)]
const MARKS: &[&str] = &[SWAP_MARK, BACKUP_MARK, TMP_MARK];

/// 自家状态原子写的临时件的尾巴：`<名>.<pid>.<线程>.<序>.tmp`（凭据续期也走这一形）。
const OWN_TMP_SUFFIX: &str = ".tmp";

/// 换名件的尾巴（认名只看 [`SWAP_MARK`]，尾巴不参与）。
const SWAP_SUFFIX: &str = ".part";

/// 文件管理面写入 `name` 时的换名件名（隐藏件，名字可以不是 UTF-8）。
pub(crate) fn swap(name: &OsStr, pid: u32, seq: u64) -> OsString {
    let mut s = OsString::from(".");
    s.push(name);
    s.push(format!("{SWAP_MARK}{pid}-{seq}{SWAP_SUFFIX}"));
    s
}

/// 文件管理面覆盖 `name` 之前另存原文的备份件名。
pub(crate) fn backup(name: &OsStr, ms: u128, seq: u64) -> OsString {
    let mut s = name.to_os_string();
    s.push(format!("{BACKUP_MARK}{ms}-{seq}"));
    s
}

/// 账号库执行落到 `at` 之前的换名件（`at` 是整条路径）。
pub(crate) fn exec_tmp(at: &str, pid: u32) -> String {
    format!("{at}{TMP_MARK}{pid}")
}

/// 自家状态原子写 `name` 时的临时件名。
pub(crate) fn own_tmp(name: &OsStr, pid: u32, thread: std::thread::ThreadId, seq: u64) -> OsString {
    let mut s = name.to_os_string();
    s.push(format!(".{pid}.{thread:?}.{seq}{OWN_TMP_SUFFIX}"));
    s
}

#[cfg(test)]
#[path = "../../../../tests/backend/platform/fs/side_name_tests.rs"]
mod tests;

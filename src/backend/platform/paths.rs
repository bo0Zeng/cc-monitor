//! U2（2026-08-01）：**路径的平台语义**。
//!
//! 与 [`super::proc`] 分开成两个文件，不是为了整齐：`path_key` 处理的是
//! **NTFS 的大小写不敏感**，既不是 `/proc` 也不是进程身份，塞进那个自称
//! 「`/proc` 与进程身份这一族」的模块名不副实。
//!
//! 更实际的理由是 **U4**：给 Windows 补第二套实现时，路径语义与进程语义会各自长出一批分支，
//! 现在分开是零成本，到时候再搬就是第二次搬同一段代码 —— 那正是账本要防的「补丁叠补丁」。
//! （功能计划步骤 1 本来就写了要建本文件，实现时漏了，Phase D 审计逮出来补上。）

use std::path::{Path, PathBuf};

/// Case-fold the path on Windows so notify's NTFS case variance does not double
/// emit; on other platforms keep the path verbatim.
///
/// 〔TL1 · 4C〕从前这里写「与 monitor 侧那份 jsonl 读者的同名两分支同规则」—— 那份读者 CF1 删了，
/// 今天这两个分支只有这一份（调用方是本 crate 的 `observe/watcher.rs`）。
#[cfg(windows)]
pub(crate) fn path_key(p: &Path) -> PathBuf {
    PathBuf::from(p.to_string_lossy().to_ascii_lowercase())
}

#[cfg(not(windows))]
pub(crate) fn path_key(p: &Path) -> PathBuf {
    p.to_path_buf()
}

/// `K-R55`（2026-09-11）：**本平台放「每用户临时文件」的那个根目录**。
///
/// # 它从哪来
///
/// `K-R52` 的 A2 堆里挂着 `observe/watcher.rs::tmux_socket_dir` 那句
/// `PathBuf::from("/tmp")`，签字栏逐字：「同一行上方的 `uid` 那半**已经**有两条
/// `#[cfg]` 臂了，而这一半没有 —— 典型的『只修一半』。该和 `platform/paths.rs` 住一起。」
/// 这就是它搬过来之后的样子，连同它那另一半（[`current_uid`]）。
///
/// ⚠ **搬的是平台原语，不是那条组合**：`tmux_socket_dir` 自己（读 `TMUX_TMPDIR`、
/// 拼 `tmux-<uid>`）是 **tmux 的约定**，不是平台语义 ⇒ 它留在 `observe/`。
/// 分界线就是 `K33` 裁定二那一句：住进适配层的是「这台机器上这件事怎么做」，
/// 不是「这件事是什么」。
///
/// ⚠ 非 unix 那一臂给的是 `std::env::temp_dir()`，**不是**一个编出来的答案：
/// 那是标准库对同一个概念（本平台的临时目录）在那个平台上的取值口。
/// 而 tmux 在那里本来就不存在 ⇒ 这条路走不到，给它一个真实的根目录只为**别撒谎**。
#[cfg(unix)]
pub(crate) fn temp_root() -> PathBuf {
    PathBuf::from("/tmp")
}

#[cfg(not(unix))]
pub(crate) fn temp_root() -> PathBuf {
    std::env::temp_dir()
}

/// 跑着的这个进程的 uid。**非 unix 上没有这个概念 ⇒ 给 `0`**。
///
/// ⚠⚠ 这一半先前就住在 `observe/watcher.rs` 里、**并且已经有两条 cfg 臂**
/// （那份注释还记着「首版直接 `unsafe { libc::getuid() }`，本机 cargo test 全绿、
/// Windows 侧编不过」）。搬过来不是因为它坏，是因为**它和它的另一半被拆在两处**：
/// 一半有门、一半没门，而两半合起来才是一条路径。
#[cfg(unix)]
pub(crate) fn current_uid() -> u32 {
    // SAFETY: `getuid` 无副作用、不会失败。
    unsafe { libc::getuid() }
}

#[cfg(not(unix))]
pub(crate) fn current_uid() -> u32 {
    0
}

/// 〔W5-FILES · 第五波 · `设计/60 §3.7`「不跨文件系统边界那一档没做（设备号要走 `platform/`）」〕
/// 一个路径（**跟链接**）所在文件系统的设备号 —— 走一棵树时「这一层是不是挂着另一个文件系统」那一判用它
/// （`files::index::build` · `files::size`）。调用方只把它用在「不跟链接地看过、确是目录」的条目上，跟不跟链接在那里没有差别。
///
/// 非 unix ⇒ `None`：那一判在那个平台上**不开口**（全当同一个文件系统），如实登记，不编一个数。
#[cfg(unix)]
pub(crate) fn device_of(p: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt as _;
    std::fs::metadata(p).ok().map(|m| m.dev())
}

#[cfg(not(unix))]
pub(crate) fn device_of(_p: &Path) -> Option<u64> {
    None
}

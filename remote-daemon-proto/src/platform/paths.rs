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
/// 与 monitor 侧 `src-tauri/src/watcher.rs` 的同名两分支同规则。
/// **原注释写的是「Mirrors `watcher.rs`」** —— U2 把本函数搬进 daemon 的 `platform/` 之后，
/// 读者会去看**本 crate** 的 `watcher.rs`，而那里已经没有 `path_key` 了。写全路径，别留悬空指向。
#[cfg(windows)]
pub(crate) fn path_key(p: &Path) -> PathBuf {
    PathBuf::from(p.to_string_lossy().to_ascii_lowercase())
}

#[cfg(not(windows))]
pub(crate) fn path_key(p: &Path) -> PathBuf {
    p.to_path_buf()
}

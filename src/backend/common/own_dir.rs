//! 后端建自家目录的那一个函数（`~/.cc-monitor` 与它底下后端自己的几层）：0700、已存在不动 —— 按 umask 建会是 0775。
//! 第四层各模块（`exit_policy` · `asset_catalog` · `relay/door` · `skill_ledger`）与第三层的暂存区（`files_commit`）都调 [`ensure_private_dir`]；
//! 远端那一份（SFTP 部署建目录）不在这台机器的文件系统上，用同一个 [`PRIVATE_DIR_MODE`]（`dial/sftp.rs::make_dir`）。
//!
//! # 它在 `readonly_guard` 第四层上（后端自己的状态，不是用户数据）
//!
//! 写口 [`ensure_private_dir`]：第四层别的模块调它不算越门；第四层之外只有一扇门 —— `control/files_commit.rs` 建暂存区那两层。
//! 动词只有「建目录」这一件（带着权限位一次建成，`DirBuilder`，只许住本模块）。

use std::path::Path;

/// 后端在每台机器上的家目录名（相对用户家目录）。
pub const DIR_NAME: &str = ".cc-monitor";

/// 后端自家目录**建的那一下**给的权限位：只给本人。
pub const PRIVATE_DIR_MODE: u32 = crate::platform::fs::PRIVATE_DIR_MODE;

/// 建**一层**自家目录，建的那一下就是 [`PRIVATE_DIR_MODE`]（unix：权限位随创建一起生效，没有「先按 umask 建出来再收窄」的那一段）。
/// **已在的不动**（那可能是用户自己设的）；父目录不在 ⇒ 照实报错（调用方都只建「家目录下那一层」，父目录本来就在）。
/// 非 unix：照常建（那边不是 unix 权限位这一问；目录的 ACL 按父目录继承）。
pub fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    crate::platform::fs::create_dir_private(dir)
}

/// 家（[`DIR_NAME`]）里那个**不出会话**的工作目录名（`quota-probe` 起官方客户端之前经 `Own` 建）：在那里跑的官方客户端（替号开额度窗口的那一句 · `quota-probe` 报用量的那一次）
/// 历史页与会话列表都不出（观测侧按它藏）。
pub const HIDDEN_WORK_DIR: &str = "autostart";

#[cfg(test)]
#[path = "../../../tests/backend/common/own_dir_tests.rs"]
mod tests;

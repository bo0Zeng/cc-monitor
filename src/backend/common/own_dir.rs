//! 后端自家目录的几个名字与权限位（`~/.cc-monitor` · 它底下那个不出会话的工作目录 · 建目录给的那个权限位）。
//! 只是常量：建目录经 `platform::fs::Own::ensure_dir`（本机）；远端 SFTP 部署建目录用同一个 [`PRIVATE_DIR_MODE`]（`ssh-dial/sftp.rs::make_dir`）。

/// 后端在每台机器上的家目录名（相对用户家目录）。
pub const DIR_NAME: &str = ".cc-monitor";

/// 后端自家目录**建的那一下**给的权限位：只给本人。
pub const PRIVATE_DIR_MODE: u32 = crate::platform::fs::PRIVATE_DIR_MODE;

/// 家（[`DIR_NAME`]）里那个**不出会话**的工作目录名（`quota-probe` 起官方客户端之前经 `Own` 建）：在那里跑的官方客户端（替号开额度窗口的那一句 · `quota-probe` 报用量的那一次）
/// 历史页与会话列表都不出（观测侧按它藏）。
pub const HIDDEN_WORK_DIR: &str = "autostart";

#[cfg(test)]
#[path = "../../../tests/backend/common/own_dir_tests.rs"]
mod tests;

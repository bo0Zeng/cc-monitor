//! 「Tauri 命令只剩 monitor 自己的事（逐行理由）；命令实现经 `inbound_client` / `BackendDoor` 碰到后端的不许留在 monitor。」
//!
//! 表与判据都在测试文件里（整体 `cfg(test)`，非测试构建为空）。

#[cfg(test)]
#[path = "command_home_registry_tests.rs"]
mod tests;

//! `设计/99 §2.1 ⑬`：「Tauri 命令分两张封闭表「monitor 自己的事」（逐行理由）与「待迁」（逐行卡在哪），
//! 命令实现经 `inbound_client` / `BackendDoor` 碰到后端的必须在「待迁」；MIG 各路合完「待迁」为空。」
//!
//! 两张表与判据都在测试文件里（整体 `cfg(test)`，非测试构建为空）。MIG 各路迁走一条命令 ⇒ 从「待迁」删那一行；
//! 命令留下但判定进了后端（只剩开窗 / 开终端 / 放字节）⇒ 挪到「monitor 自己的事」并写理由。

#[cfg(test)]
#[path = "../../../../tests/frontend/shell/command_home_registry_tests.rs"]
mod tests;

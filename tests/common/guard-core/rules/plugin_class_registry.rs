//! `ccm` 是一套骨架：按哪一家（Claude Code · Codex …）起会有什么不同，全在适配层的起会话事实
//! （`agents::LaunchFace`）上，ccm 按注册表那一格问。这里钉一件：`src/backend/control/ccm/mod.rs`
//! 生产段里没有一个吃 agent 名、回那一家那一份的函数（带正控）。
//!
//! `cc-bus` 住后端、起进程口只有一处由后端 `readonly_guard` 的起进程登记表管；`--ccm-probe` 的能力表
//! 覆盖消费者要的那几个由 `ccm_tests.rs` 钉，都不在这里。

#[cfg(test)]
#[path = "plugin_class_registry_tests.rs"]
mod tests;

//! 终端那几条有 Rust 客户端的命令：给一台远端开终端（`terminal-ssh`，本机后端答；主界面 · 壳问同一条）·
//! 退订终端画面（`terminal-unfollow`，壳替界面发）。

use serde::{Deserialize, Serialize};

/// `terminal-ssh` 的入参：哪一台（名字；怎么拨照本机后端的机器表组）＋ 要在那台跑什么。
/// `command`（主界面交成品命令）与 `cwd`（文件窗口「在此打开终端」只交意图，命令由后端拼）**至多给一个**；都不给 ⇒ 那台的登录 shell（命令面板「打开终端」）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct TerminalSshReq {
    /// 那台的名字。
    pub machine: crate::Machine,
    /// 要在那台跑的命令。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub command: Option<String>,
    /// 在那台的这个目录开登录 shell（串，或非 UTF-8 的 `{"b16": "<hex>"}`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(optional, type = "string | { b16: string }")
    )]
    pub cwd: Option<serde_json::Value>,
}

/// `terminal-unfollow` 的入参：退订那张终端画面票（monitor 替界面向那台退订）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct UnfollowReq {
    /// 退不在册的也回 `ok`（记下它：订阅那一问若还在路上，到了也不起）。
    pub ticket: String,
}

/// `terminal-ssh` 的回包：那一整行命令。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct SshCommand {
    /// 这台电脑终端里跑的那一行（Windows：PowerShell `& '<ssh 全路径>' -t … -- 'bash -lic …'`；别处：同一组参数的 POSIX shell 一行）。
    pub command: String,
}

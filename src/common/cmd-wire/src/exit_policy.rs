//! 「退出行为」那个值（值住后端所在那台）的两条：`exit-policy-read`（monitor 退出臂在决定那一刻现问）· `exit-policy-set`（设置页经通道写）。
//! 两条回包同一个形状。

use serde::{Deserialize, Serialize};

/// `exit-policy-read` 的入参：不收参数（缺 · `null` · `{}` 都认，多一个键 ⇒ `bad_args`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ExitPolicyReadReq {}

/// `exit-policy-set` 的入参：要写的值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ExitPolicySetReq {
    pub kill_on_exit: bool,
}

/// 一次读数的线上形状（`exit-policy-read` 与 `exit-policy-set` 共用）。`said` 是成品，界面原样摆。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(rename_all = "camelCase")]
pub struct ExitPolicy {
    /// `absent`（没设过）· `chosen` · `unreadable`（读不出：也照答，不是失败）—— 后端 `exit_policy::Read::state` 那三个词。
    #[cfg_attr(
        any(test, feature = "ts"),
        ts(type = "\"absent\" | \"chosen\" | \"unreadable\"")
    )]
    pub state: String,
    /// 生效值：monitor 退出时结束本机常驻后端。
    pub kill_on_exit: bool,
    /// `unreadable` 时的原因（那一句不带原话）。
    pub reason: Option<String>,
    /// `unreadable` 时的复制详情（时刻 · 机器 · 命令 · 码 · 原话；排法同失败应答）。
    pub detail: Option<String>,
    /// 那份文件的路径（在 `~/.cc-monitor/` 下；家目录说不出 ⇒ `null`）。
    pub path: Option<String>,
    /// 这个值意味着什么的一句话。
    pub said: String,
}

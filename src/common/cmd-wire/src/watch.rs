//! 「这条流在看哪几个会话」那一问（`stream-watch`）的入参与应答；attach 行的 `watch` 一格是同一份入参（接上即生效）。
//! 后端 · 桌面 · 手机同一份。

use serde::{Deserialize, Serialize};

/// `stream-watch` 应答里的一格：这个会话刚进了这条流的名单，它的这份记录从第 `seq` 行起上流
/// （`seq` 与行帧同一个行号空间）；`[0, seq)` 那段客户端按骨架补（`history-index` 从上次的末端续）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct WatchFrom {
    /// 会话 id。
    pub sid: String,
    /// 这份会话记录在那台机器上的绝对路径。
    pub path: String,
    /// 这条流从这一行起发它。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub seq: u64,
}

/// `stream-watch` 的入参：在看哪几个（整份换）＋ 要哪几个的主线外清单、手上各有什么（`branch`；不报 ＝ 一份都不要）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct WatchAsk {
    /// 在看的会话 id（空 ＝ 一个都不看）。
    pub sids: Vec<String>,
    /// 要哪几个会话的主线外清单、手上各有什么（可缺 ＝ 一份都不要）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
    pub branch: Vec<BranchHeld>,
}

/// `stream-watch.branch` 的一格：这条连接要这个会话的主线外清单；手上已有那份记录（`path`）的前 `at` 条、摘要 `sum`（上一帧 `session_branch` 给的）。
/// 手上什么都没有 ⇒ `at: 0`、不带 `path` / `sum`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct BranchHeld {
    pub sid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
    pub path: Option<String>,
    /// 手上那份的前几条（照手上最后一帧 `session_branch` 的 `at + off.len()`；手上没有 ⇒ `0`）。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
    pub sum: Option<String>,
}

/// `stream-watch` 的应答。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct WatchReply {
    /// 刚进名单的会话每份记录一格（先前就在名单里的不回）。
    pub from: Vec<WatchFrom>,
}

//! `resync`（手动对齐）的入参与回包：后端 `stream/resync_face.rs` 照它答，壳的 `comms/resident-client` 照它读回那台当下的能力事实。

use crate::frame::Unavailable;
use serde::{Deserialize, Serialize};

/// `resync` 的入参。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
#[serde(deny_unknown_fields)]
pub struct ResyncReq {
    /// 可缺：给了 ⇒ 只对那一个会话（关卡 2「对齐后重试」，不许空串）；不给 / `null` ⇒ 整机。
    #[serde(default)]
    #[cfg_attr(any(test, feature = "ts"), ts(optional = nullable))]
    pub sid: Option<String>,
}

/// `resync` 的回包：这一趟的差异 ＋ 这台**当下**的能力事实（与 hello 那两格同一个函数、同形）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(
    any(test, feature = "ts"),
    ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/"))
)]
pub struct Resynced {
    /// 这次补宣告的会话数。
    pub added: usize,
    /// 补移除的会话数（各份 watcher 取最大：看的是同一台机器）。
    pub removed: usize,
    /// 这次真写了几处 `@ccm_sid`（值一样的不写）。
    pub retagged: usize,
    /// 在跟的会话这次从游标补读出几行（带 `sid` 只数那一个；各份 watcher 相加）。
    pub caught_up: usize,
    /// 几份 watcher 做完了对齐（常驻后端每条连接一份 ＋ 空转那一份）。
    pub watchers: usize,
    /// 这台当下接得下但做不到的命令（同 hello 那一格）：握手之后才装上 tmux 的机器，靠「重新对齐」让客户端认出来。
    pub unavailable: Vec<Unavailable>,
}

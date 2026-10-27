//! 会话输入队那三条（`session-queue-put` · `-cancel` · `-read`）的线上形：往一条会话说的每一句都排进那条会话的队，
//! 队住会话所在那台（主人），序号由主人发。判定（去重 · 端内保序 · 发序号 · 撤 · 未送达）只住 [`super::session_queue`]。

use cmd_wire::{Machine, Sid};
use serde::{Deserialize, Serialize};

/// 队里一条此刻在哪一态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "snake_case")]
pub(crate) enum QueueState {
    /// 排着（有序号，等投递）。
    Queued,
    /// 收着：同一端前面那一号还没到，先不排、不发序号（等到了或超时再排）。
    Held,
    /// 会话结束时还没送：留在名单里给发件人看，撤掉才消失。
    Undelivered,
}

/// 名单里的一条。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct QueueItem {
    /// 主人发的序号（收着的那几条还没有）。先后只看它。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub(crate) seq: Option<u64>,
    /// 发件端给的唯一 id（重发去重）。
    pub(crate) id: String,
    /// 发自哪一端。
    pub(crate) end: String,
    /// 发件端说的几点发的（unix 毫秒；只给人看，不参与排序）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) at_ms: i64,
    /// 那一句。
    pub(crate) text: String,
    #[cfg_attr(test, ts(inline))]
    pub(crate) state: QueueState,
}

/// 一条会话此刻的名单：排着的照序号在前，收着的照到达先后在后，未送达的照序号在最后。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionQueue {
    pub(crate) items: Vec<QueueItem>,
}

/// `session-queue-put` 的入参：往 `machine` 那台的 `sid` 那条会话投一句。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SessionQueuePutReq {
    /// 会话所在那台（本机 ⇒ `<local>`）。
    pub(crate) machine: Machine,
    pub(crate) sid: Sid,
    /// 发件端给的唯一 id：同 id 再投只入队一次。
    pub(crate) id: String,
    /// 发自哪一端。
    pub(crate) end: String,
    /// 这一端自己的递增序号（从 1 数）：给了 ⇒ 同一端先后两条保序（前一号没到先收着）；不给 ⇒ 照到达先后。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub(crate) n: Option<u64>,
    /// 几点发的（发件端的 unix 毫秒，只给人看）。
    #[cfg_attr(test, ts(type = "number"))]
    pub(crate) at_ms: i64,
    pub(crate) text: String,
}

/// `session-queue-put` 这一下的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "snake_case")]
pub(crate) enum QueuePutOutcome {
    /// 排进去了（有序号）。
    Queued,
    /// 收着（同一端前一号还没到）。
    Held,
    /// 这个 id 收过：没再入队（那一条此刻的样子见 `seq` · `place`；已经送掉或撤掉 ⇒ 两格都不在）。
    Seen,
}

/// `session-queue-put` 的回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct QueuePut {
    #[cfg_attr(test, ts(inline))]
    pub(crate) outcome: QueuePutOutcome,
    /// 那一条的序号（收着的还没有）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub(crate) seq: Option<u64>,
    /// 排第几（排着的那几条里，从 1 数；不是排着 ⇒ 不在）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub(crate) place: Option<u32>,
}

/// `session-queue-cancel` 的入参：撤 `machine` 那台 `sid` 那条会话队里的 `id`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionQueueCancelReq {
    pub(crate) machine: Machine,
    pub(crate) sid: Sid,
    pub(crate) id: String,
}

/// `session-queue-cancel` 的回包：撤没撤成 ＋ 撤完的整份名单。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct QueueCancelled {
    /// `false` ⇒ 那一条已不在队里（送过 · 撤过 · 没有这一条），什么都没动。
    pub(crate) cancelled: bool,
    pub(crate) queue: SessionQueue,
}

/// `session-queue-read` 的入参。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionQueueReadReq {
    pub(crate) machine: Machine,
    pub(crate) sid: Sid,
}

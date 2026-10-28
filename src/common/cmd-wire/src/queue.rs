//! 会话输入队那三条（`session-queue-put` · `-cancel` · `-read`）的线上形；名单（[`QueueList`]）也是 `changed {session_queue}` 的体。
//! 往一条会话说的每一句都排进那条会话的队，队住会话所在那台（主人），序号由主人发。判定（去重 · 端内保序 · 发序号 · 撤 · 未送达）只住后端
//! `history/session_queue.rs`；这里只放形状。

use crate::{Machine, Sid};
use serde::{Deserialize, Serialize};

/// 队里一条此刻在哪一态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "snake_case")]
pub enum QueueState {
    /// 排着（有序号，等投递）。
    Queued,
    /// 收着：同一端前面那一号还没到，先不排、不发序号（等到了或超时再排）。
    Held,
    /// 会话结束时还没送 · 送不进去：留在名单里给发件人看，撤掉才消失。
    Undelivered,
}

/// 名单里的一条。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueueItem {
    /// 主人发的序号（收着的那几条还没有）。先后只看它。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub seq: Option<u64>,
    /// 发件端给的唯一 id（重发去重）。
    pub id: String,
    /// 发自哪一端。
    pub end: String,
    /// 发件端说的几点发的（unix 毫秒；只给人看，不参与排序）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub at_ms: i64,
    /// 那一句。
    pub text: String,
    /// 随这一句发的附件编号（`session-attachment-commit` 回的，按先后）；没有 ⇒ 不在。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<Vec<String>>"))]
    pub attachments: Vec<String>,
    /// 这一句是编辑已发的那一句（发送那一下选的怎么退）：投递时先退到那一句之前再说它；普通的一句 ⇒ 不在。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub rewind: Option<crate::rewind::Rewind>,
    #[cfg_attr(any(test, feature = "ts"), ts(inline))]
    pub state: QueueState,
    /// 那一条此刻的样子，会话那台写好（排队 · 第 N · 排队 · 失败 · 未送进会话；同回执那一句）：各端照摆，不按 `state` 自己挑字。
    pub said: String,
    /// 发自哪一端，会话那台写好的那个词（认得的端写名字，别的照 `end` 原样）。
    pub from: String,
}

/// 一条会话此刻的名单：排着的照序号在前，收着的照到达先后在后，未送达的照序号在最后。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct QueueList {
    pub items: Vec<QueueItem>,
    /// 最近送进 agent 的那几条的 id（先送的在前，只留最近几条）：发件端据它把那一句标「已送进」；不在名单也不在这里 ⇒ 撤了。
    pub sent: Vec<String>,
}

/// `session-queue-put` 的入参：往 `machine` 那台的 `sid` 那条会话投一句。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionQueuePutReq {
    /// 会话所在那台（本机 ⇒ `<local>`）。
    pub machine: Machine,
    pub sid: Sid,
    /// 发件端给的唯一 id（这一端待发箱里那一句的编号）：同 id 再投只入队一次。
    pub id: String,
    /// 发自哪一端。
    pub end: String,
    /// 这一端自己的递增序号（从 1 数）：给了 ⇒ 同一端先后两条保序（前一号没到先收着）；不给 ⇒ 照到达先后。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub n: Option<u64>,
    /// 几点发的（发件端的 unix 毫秒，只给人看）。
    #[cfg_attr(any(test, feature = "ts"), ts(type = "number"))]
    pub at_ms: i64,
    pub text: String,
    /// 随这一句发的附件编号（按先后）。会话那台当场按会话工作目录找：有一份不在 ⇒ 拒（`attachment_gone`），一个字节不写。缺 ⇒ 不带。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, as = "Option<Vec<String>>"))]
    pub attachments: Vec<String>,
    /// 编辑已发的那一句（确认那一下才带）：投递时先在会话里退到 `rewind.to` 那一句之前，再说 `text`。普通的一句 ⇒ 不带。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub rewind: Option<crate::rewind::Rewind>,
}

/// `session-queue-put` 这一下的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "snake_case")]
pub enum QueuePutOutcome {
    /// 排进去了（有序号）。
    Queued,
    /// 收着（同一端前一号还没到）。
    Held,
    /// 这个 id 收过：没再入队（那一条此刻的样子见 `seq` · `place`；已经送掉或撤掉 ⇒ 两格都不在）。
    Seen,
}

/// `session-queue-put` 的回执：会话那台已落盘。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueuePut {
    #[cfg_attr(any(test, feature = "ts"), ts(inline))]
    pub outcome: QueuePutOutcome,
    /// 那一条的序号（收着的还没有）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub seq: Option<u64>,
    /// 排第几（排着的那几条里，从 1 数；不是排着 ⇒ 不在）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional, type = "number"))]
    pub place: Option<u32>,
    /// 那一条此刻的样子，会话那台写好（排队 · 第几 · 已送进 · 已撤），发件端贴在那一句下面。
    pub said: String,
}

/// `session-queue-cancel` 的入参：撤 `machine` 那台 `sid` 那条会话队里的 `id`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct SessionQueueCancelReq {
    pub machine: Machine,
    pub sid: Sid,
    pub id: String,
}

/// `session-queue-cancel` 的回包：撤没撤成 ＋ 撤完的整份名单。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct QueueCancelled {
    /// `false` ⇒ 那一条已不在队里（送过 · 撤过 · 没有这一条），什么都没动。
    pub cancelled: bool,
    pub queue: QueueList,
}

/// `session-queue-read` 的入参。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct SessionQueueReadReq {
    pub machine: Machine,
    pub sid: Sid,
}

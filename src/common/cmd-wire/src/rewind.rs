//! 编辑已发的一句（长按改 · 发送时选怎么退，同在终端里双击 Esc 回退）的线上形。
//!
//! 改动全攒在发件那一端：长按 · 改字 · 选怎么退都不碰会话，只有确认那一下随改好的那句带一份 [`Rewind`]
//! 进那条会话的输入队（待发 · 回执才算出去 · 同编号重发，同普通那一句）；之后调出回退菜单 · 核对 · 按键 · 送字 · 读回全在会话那台。
//! 发送那一下的选项由会话那台照那一家申报的交出（[`RewindOptions`]），界面不写死。判定只住后端（`control/rewind.rs`）。

use crate::{Machine, Sid};
use serde::{Deserialize, Serialize};

/// 怎么退（通用的几样；各家确认页上那一项叫什么由适配层认）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "snake_case")]
pub enum RewindHow {
    /// 只退对话（代码不动），再说改好的那句。
    Conversation,
    /// 对话和代码一起退到那一句之前，再说改好的那句。
    CodeAndConversation,
}

impl RewindHow {
    /// 通用的几种（发送那一下照这个先后都列出，那一家没申报的灰着）。
    pub const ALL: [RewindHow; 2] = [RewindHow::Conversation, RewindHow::CodeAndConversation];
}

/// 主 agent 正在干活时什么时候退。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "snake_case")]
pub enum RewindWhen {
    /// 等这一轮跑完再退（缺省；排在队里，可撤）。
    AfterTurn,
    /// 现在停下这一轮再退（会打断）。
    Now,
}

/// 确认那一下随改好的那句带上的：退到哪一句之前 · 怎么退 · 什么时候退。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rewind {
    /// 被编辑的那一句在记录里的 id（`history-user-inputs` 那一行的 `uuid`）：退到它之前，改好的那句接在它原来的位置上。
    pub to: String,
    #[cfg_attr(any(test, feature = "ts"), ts(inline))]
    pub how: RewindHow,
    #[cfg_attr(any(test, feature = "ts"), ts(inline))]
    pub when: RewindWhen,
}

/// `session-rewind-options` 的入参：`machine` 那台 `sid` 那条会话、退到 `to` 那一句之前，此刻发送那一下给哪几项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(deny_unknown_fields)]
pub struct SessionRewindOptionsReq {
    pub machine: Machine,
    pub sid: Sid,
    /// 被编辑的那一句在记录里的 id（同 [`Rewind::to`]）：哪一项能选按那一句之后的事判。
    pub to: String,
}

/// 「怎么退」的一项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RewindChoiceRow {
    #[cfg_attr(any(test, feature = "ts"), ts(inline))]
    pub how: RewindHow,
    /// 那一项的字（会话那台写好）。
    pub text: String,
    /// 缺省选它（只落在能选的那一项上）。
    pub default: bool,
    /// 这一刻能不能选；不能 ⇒ 界面灰着摆、写 [`Self::why`]。
    pub enabled: bool,
    /// 不能选的那一句（会话那台写好）；能选 ⇒ 不在。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub why: Option<String>,
}

/// 「什么时候退」的一项（只在主 agent 正在干活时给）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RewindWhenRow {
    #[cfg_attr(any(test, feature = "ts"), ts(inline))]
    pub when: RewindWhen,
    pub text: String,
    pub default: bool,
}

/// `session-rewind-options` 的回包：照那一家申报的交出，界面照摆（「取消」是界面自己的骨架字，不在这里）。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RewindOptions {
    /// 「怎么退」那几项（通用的几种都列，不能选的灰着带原因）；空 ⇒ 这一家不支持从这里回退（为什么见 `said`）。
    pub how: Vec<RewindChoiceRow>,
    /// 「什么时候退」那几项；空 ⇒ 主 agent 此刻空着（只有后台子 agent 在跑也算空着），不给这一行。
    pub when: Vec<RewindWhenRow>,
    /// 给人看的那一句（会话那台写好）：`how` 空 ⇒ 为什么退不了；有项 ⇒ 退之前要知道的（这句之后派出的子 agent 还在跑）；都没有 ⇒ 不在。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub said: Option<String>,
}

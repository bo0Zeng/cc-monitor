//! **通用记录**：线上「这一行在界面里是什么」的那一形（两个前端共吃的契约面）。各家的盘上格式只住它自己的适配层，
//! 适配层把一行翻成这里的 [`Record`]；通用层与界面只认它（字段名不带任何一家的写法）。
//!
//! 公共四格 ＋ 时刻字：`agent`（哪一家）· `id`（会话内非空、唯一）· `at`（记录时刻）· `timeText`（这台本地钟的 `HH:MM`，界面照抄）· `t`（哪一类）。
//! `t` 的闭集：`said`（人那一侧说的）· `reply`（代理的回复）· `retry`（一次要重试的上游失败）· `title`（会话标题）· `queued`（插进正在跑那一轮的一句）。
//! 内容块 [`Block`] 是两家共有的词：正文 · 推理 · 工具调用 · 工具结果 · 图片。
//!
//! 不在这一形里的：链（上一条是谁 —— 主线外清单另给，`mainline`）· 会话事实（分叉血缘、用量、花费 —— `history-facts`）· 原文（`--with-raw` 的 `raw`）。

use crate::agents::{ApiReason, ChildRunTag, StepResult, ToolCard, ToolStep, UserText};
use serde::Serialize;
use std::collections::BTreeMap;

/// 一条通用记录。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        rename = "LineRecord",
        export_to = "../../frontend/ui/generated/"
    )
)]
pub struct Record {
    /// 哪一家（注册表里那一家的 `kind`）。
    pub agent: String,
    /// 这条记录在本会话里的身份：非空、会话内唯一（主线外清单、分叉、跳转都按它认）。
    pub id: String,
    /// 记录时刻（ISO-8601 原样）；记录里没有 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub at: Option<String>,
    /// `at` 在这台本地钟上的钟面 `HH:MM`（界面照抄、不换算）；没有时刻 / 解不出 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub time_text: Option<String>,
    /// 哪一类与它自己的格。
    #[serde(flatten)]
    pub body: Body,
}

/// [`Record`] 的类别（`t`）与各自的格。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "t", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(
    test,
    ts(
        export,
        rename = "LineRecordBody",
        export_to = "../../frontend/ui/generated/"
    )
)]
pub enum Body {
    /// 人那一侧说的一条（人打的字 · 斜杠命令 · 工具结果 · 系统注入 … 谁说的由 `who` 说）。
    #[serde(rename_all = "camelCase")]
    Said {
        /// 谁说的、要显示的正文（判定只在适配层）。
        who: UserText,
        blocks: Vec<Block>,
        /// 这条里每个工具结果的一句（工具调用 id ⇒ [`StepResult`]）。没有 ⇒ 缺。
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        #[cfg_attr(test, ts(optional, as = "Option<BTreeMap<String, StepResult>>"))]
        results: BTreeMap<String, StepResult>,
        /// 这条记录自己的工作目录。
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        cwd: Option<String>,
    },
    /// 代理的回复。
    #[serde(rename_all = "camelCase")]
    Reply {
        blocks: Vec<Block>,
        /// 型号名原样。
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        model: Option<String>,
        /// 代理那一侧自动写的应答（不是模型说的，不建卡）。
        auto_reply: bool,
        /// 这条是一轮的结束（与帧 `turn_end` 同一个判定）。
        ends_turn: bool,
        /// 工具调用 id ⇒ 卡型（普通工具卡不列）。
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        #[cfg_attr(test, ts(optional, as = "Option<BTreeMap<String, ToolCard>>"))]
        cards: BTreeMap<String, ToolCard>,
        /// 工具调用 id ⇒ 一行人话。
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        #[cfg_attr(test, ts(optional, as = "Option<BTreeMap<String, ToolStep>>"))]
        steps: BTreeMap<String, ToolStep>,
        /// 派出子运行的那几次工具调用 ⇒ 标签与类别。
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        #[cfg_attr(test, ts(optional, as = "Option<BTreeMap<String, ChildRunTag>>"))]
        runs: BTreeMap<String, ChildRunTag>,
        /// 这条是上游最终失败写的报错（报错正文在 `blocks` 的正文里）。不是 ⇒ 缺。
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        error: Option<ReplyError>,
    },
    /// 一次上游失败、将重试（「重试中 N/M」那条细条）。
    Retry {
        reason: ApiReason,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        attempt: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(test, ts(optional))]
        max: Option<u32>,
    },
    /// 会话标题（代理起的 · 人改的）。
    Title { text: String, by: TitleBy },
    /// 人在一轮跑着时插进去的一句（`at` 是打字的时刻）。
    Queued { who: UserText },
}

/// 报错回复的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub struct ReplyError {
    pub reason: ApiReason,
    /// HTTP 状态码（只有 HTTP 类错误有）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub status: Option<u32>,
}

/// 标题是谁起的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum TitleBy {
    Agent,
    User,
}

/// 内容块（两家共有的词）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum Block {
    Text {
        text: String,
    },
    Thinking {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        #[cfg_attr(test, ts(type = "unknown"))]
        input: serde_json::Value,
    },
    ToolResult {
        /// 对的是哪次工具调用。
        #[serde(rename = "for")]
        of: String,
        /// 结果内容（正文 / 图片块；原文是一个字符串时已归成一个正文块）。
        content: Vec<Block>,
        #[serde(rename = "isError")]
        is_error: bool,
    },
    Image {
        /// 图片来源原样（形状照那一家的写法透传）。
        #[cfg_attr(test, ts(type = "unknown"))]
        source: serde_json::Value,
    },
}

/// 排队消息的打字时刻表（一份会话一张；上界固定，挤掉最老的）。通用：只认 [`super::QueueMark`]，不认哪一家的字段。
#[derive(Debug, Default, Clone)]
pub(crate) struct TypedTimes {
    /// (原句, 打字时刻)，越往后越新。
    seen: std::collections::VecDeque<(String, String)>,
}

impl TypedTimes {
    /// 表的上界（条数）。
    pub(crate) const CAP: usize = 200;

    /// 这一行过一遍表：打字那一刻 ⇒ 记下；被插进那一轮的那一条 ⇒ `at` / `timeText` 换成打字时刻（配不上 ⇒ 原样）。
    pub(crate) fn pass(&mut self, t: &mut super::Translated) {
        match &t.queue {
            Some(super::QueueMark::Typed { text, at }) => {
                self.seen.retain(|(k, _)| k != text);
                self.seen.push_back((text.clone(), at.clone()));
                while self.seen.len() > Self::CAP {
                    self.seen.pop_front();
                }
            }
            Some(super::QueueMark::Taken { text }) => {
                let typed = self.seen.iter().rev().find(|(k, _)| k == text);
                if let (Some((_, at)), Some(r)) = (typed, t.record.as_mut()) {
                    r.time_text = crate::common::time::iso_hm_here(at);
                    r.at = Some(at.clone());
                }
            }
            None => {}
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/backend/agents/record_tests.rs"]
mod tests;

//! **通用记录**：线上「这一行在界面里是什么」的那一形（两个前端共吃的契约面）。各家的盘上格式只住它自己的适配层，
//! 适配层把一行翻成这里的 [`Record`]；通用层与界面只认它（字段名不带任何一家的写法）。
//!
//! 公共四格 ＋ 时刻字：`agent`（哪一家）· `id`（会话内非空、唯一）· `at`（记录时刻）· `timeText`（看的那一台钟上的 `HH:MM`，出口那一下写，界面照抄）· `t`（哪一类）。
//! `t` 的闭集：`said`（人那一侧说的）· `reply`（代理的回复）· `retry`（一次要重试的上游失败）· `title`（会话标题）· `queued`（插进正在跑那一轮的一句）·
//! `unread`（这一家的这一行适配层认不出：写好的一句 ＋ 原文摘录）。
//! 内容块 [`Block`] 是两家共有的词：正文 · 推理 · 工具调用 · 工具结果 · 图片。
//!
//! 不在这一形里的：链（上一条是谁 —— 主线外清单另给，`mainline`）· 会话事实（分叉血缘、用量、花费 —— `history-facts`）· 原文（线上不带）。

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
    /// `at` 在看的那一台钟上的钟面 `HH:MM`（出口那一下按请求 / 流带来的时区写，[`Record::stamp`]；界面照抄、不换算）；没有时刻 / 解不出 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, as = "Option<String>"))]
    pub time_text: Option<crate::common::cells::Words>,
    /// `at` 的毫秒（自 1970；出口那一下随钟面一起写）：界面算「两条记录之间多久」只用这一格（交那一个读口），不自己解析 `at`；解不出 ⇒ 缺。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional, type = "number"))]
    pub at_ms: Option<i64>,
    /// 哪一类与它自己的格。
    #[serde(flatten)]
    pub body: Body,
}

impl Record {
    /// 出口那一下（推 `line` 帧 · 回按页读的那几条）按看的那一台的时区写钟面：`timeText` ＝ `at` 的 `HH:MM`。
    /// 解析那一层不写它：同一份记录可能推给几个时区不同的看的人。
    pub(crate) fn stamp(&mut self, tz: &crate::common::time::Tz) {
        self.at_ms = self
            .at
            .as_deref()
            .and_then(crate::common::time::parse_iso8601_ms);
        self.time_text = self
            .at
            .as_deref()
            .and_then(|a| crate::common::time::iso_hm(a, tz))
            .map(crate::common::cells::Words);
    }
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
    /// 这一家的这一行，适配层认不出（没见过的类型 · 见过的类型、形状变了）。出口照 `text` 画一行，不按 `type` 分支。
    /// 相邻同类由出记录页那一遍并成一条（[`unread_text`] 重写 `text`、`count` 记几条）。
    Unread {
        why: UnreadWhy,
        /// 那一行自己说的类型原样（标签，出口不许按它取字）；没有 ⇒ `null`。
        #[serde(rename = "type")]
        kind: Option<String>,
        /// 写好的一句（[`unread_text`]）。
        #[cfg_attr(test, ts(as = "String"))]
        text: crate::common::cells::Words,
        /// 恒 `warn`。
        #[cfg_attr(test, ts(type = "\"warn\""))]
        tone: crate::common::cells::Tone,
        /// 原文截到 [`EXCERPT_BYTES`]（按字符截，不截半个字）：给［复制详情］，出口照抄、不解析。
        #[cfg_attr(test, ts(as = "String"))]
        excerpt: crate::common::cells::Words,
        /// 并了几条（适配层每行给 1）。
        count: u32,
    },
}

/// 认不出那一行的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
pub enum UnreadWhy {
    /// 这一家没见过的类型。
    UnknownType,
    /// 见过的类型、形状变了（更值得警惕）。
    ParseFailed,
}

/// 原文摘录的上界（字节）：与漂移账的样例同一个上界、同一个截断（[`excerpt_of`]）。
pub const EXCERPT_BYTES: usize = 400;

/// 按字符边界把原文截到 [`EXCERPT_BYTES`] 以内（漂移账的样例也走它）。
pub fn excerpt_of(raw: &str) -> String {
    if raw.len() <= EXCERPT_BYTES {
        return raw.to_string();
    }
    let mut end = EXCERPT_BYTES;
    while !raw.is_char_boundary(end) {
        end -= 1;
    }
    raw[..end].to_string()
}

/// 认不出那一条的一句：「认不出的记录 · {type}」/「记录形状变了 · {type}」，并了几条 ⇒ 后面带「 · {n} 条」。
pub fn unread_text(why: UnreadWhy, kind: Option<&str>, count: u32) -> crate::common::cells::Words {
    use copy_core::copy_text;
    let kind = kind
        .filter(|k| !k.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| copy_text("record.unread.noType", &[]));
    let n = count.to_string();
    let text = match (why, count > 1) {
        (UnreadWhy::UnknownType, false) => {
            copy_text("record.unread.unknownType", &[("type", &kind)])
        }
        (UnreadWhy::UnknownType, true) => copy_text(
            "record.unread.unknownTypeMany",
            &[("type", &kind), ("n", &n)],
        ),
        (UnreadWhy::ParseFailed, false) => {
            copy_text("record.unread.parseFailed", &[("type", &kind)])
        }
        (UnreadWhy::ParseFailed, true) => copy_text(
            "record.unread.parseFailedMany",
            &[("type", &kind), ("n", &n)],
        ),
    };
    crate::common::cells::Words(text)
}

impl Body {
    /// 适配层认不出的一行（每行一条，`count` 1）。
    pub fn unread(why: UnreadWhy, kind: Option<String>, raw: &str) -> Self {
        Self::Unread {
            why,
            text: unread_text(why, kind.as_deref(), 1),
            kind,
            tone: crate::common::cells::Tone::Warn,
            excerpt: crate::common::cells::Words(excerpt_of(raw)),
            count: 1,
        }
    }
}

/// 通用记录的类（记录上 `t` 那一格的六个词；骨架行 `t` 也是它，`unread` 除外：骨架不出认不出的行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordClass {
    Said,
    Reply,
    Retry,
    Title,
    Queued,
    Unread,
}

impl Body {
    /// 这条记录的类。
    pub fn class(&self) -> RecordClass {
        match self {
            Self::Said { .. } => RecordClass::Said,
            Self::Reply { .. } => RecordClass::Reply,
            Self::Retry { .. } => RecordClass::Retry,
            Self::Title { .. } => RecordClass::Title,
            Self::Queued { .. } => RecordClass::Queued,
            Self::Unread { .. } => RecordClass::Unread,
        }
    }
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

    /// 这一行过一遍表：打字那一刻 ⇒ 记下；被插进那一轮的那一条 ⇒ `at` 换成打字时刻（钟面随它，出口那一下写）（配不上 ⇒ 原样）。
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

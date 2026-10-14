//! **通用记录**：线上「这一行在界面里是什么」的那一形（两个前端共吃的契约面）。各家的盘上格式只住它自己的适配层，
//! 适配层把一行翻成这里的 [`Record`]；通用层与界面只认它（字段名不带任何一家的写法）。
//!
//! 公共四格 ＋ 时刻字：`agent`（哪一家）· `id`（会话内非空、唯一）· `at`（记录时刻）· `timeText`（看的那一台钟上的 `HH:MM`，出口那一下写，界面照抄）· `t`（哪一类）。
//! `t` 的闭集：`said`（人那一侧说的）· `reply`（代理的回复）· `retry`（一次要重试的上游失败）· `title`（会话标题）· `queued`（插进正在跑那一轮的一句）·
//! `unread`（这一家的这一行适配层认不出：写好的一句 ＋ 原文摘录）。
//! 内容块 [`Block`] 是两家共有的词：正文 · 推理 · 工具调用 · 工具结果 · 图片。
//!
//! 不在这一形里的：链（上一条是谁 —— 主线外清单另给，`mainline`）· 会话事实（分叉血缘、用量、花费 —— `history-facts`）· 原文（线上不带）。

//!
//! 形状住 `cmd-wire`（[`cmd_wire::record`]，手机按同一份解）；这里是后端在它上面做的事（写钟面 · 认不出那一行的一句 · 类别）。

pub(crate) use cmd_wire::record::{Block, Body, Record, ReplyError, TitleBy, UnreadWhy};

/// 后端写记录那一侧（形状在 `cmd-wire`，孤儿规则不许在那边的类型上加固有方法）。
pub(crate) trait RecordExt {
    /// 出口那一下（推 `line` 帧 · 回按页读的那几条）按看的那一台的时区写钟面：`timeText` ＝ `at` 的 `HH:MM`。
    /// 解析那一层不写它：同一份记录可能推给几个时区不同的看的人。
    fn stamp(&mut self, tz: &crate::common::time::Tz);
}

impl RecordExt for Record {
    fn stamp(&mut self, tz: &crate::common::time::Tz) {
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

/// 后端造 / 认记录类别那一侧（形状在 `cmd-wire`）。
pub(crate) trait BodyExt {
    /// 适配层认不出的一行（每行一条，`count` 1）。
    fn unread(why: UnreadWhy, kind: Option<String>, raw: &str) -> Self;
    /// 这条记录的类。
    fn class(&self) -> RecordClass;
}

impl BodyExt for Body {
    fn unread(why: UnreadWhy, kind: Option<String>, raw: &str) -> Self {
        Self::Unread {
            why,
            text: unread_text(why, kind.as_deref(), 1),
            kind,
            tone: crate::common::cells::Tone::Warn,
            excerpt: crate::common::cells::Words(excerpt_of(raw)),
            count: 1,
        }
    }

    fn class(&self) -> RecordClass {
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

/// 通用记录的类（记录上 `t` 那一格的六个词；骨架行 `t` 也是它，`unread` 除外：骨架不出认不出的行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../frontend/ui/generated/"))]
#[serde(rename_all = "camelCase")]
pub enum RecordClass {
    Said,
    Reply,
    Retry,
    Title,
    Queued,
    Unread,
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

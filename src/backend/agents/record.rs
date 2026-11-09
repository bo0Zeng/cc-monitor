//! **通用记录**：线上「这一行在界面里是什么」的那一形（两个前端共吃的契约面）。各家的盘上格式只住它自己的适配层，
//! 适配层把一行翻成这里的 [`Record`]；通用层与界面只认它（字段名不带任何一家的写法）。
//!
//! 公共四格 ＋ 时刻字：`agent`（哪一家）· `id`（会话内非空、唯一）· `at`（记录时刻）· `timeText`（看的那一台钟上的 `HH:MM`，出口那一下写，界面照抄）· `t`（哪一类）。
//! `t` 的闭集：`said`（人那一侧说的；一轮中途插进来的那一句也是它）· `reply`（代理的回复）· `retry`（一次要重试的上游失败）· `title`（会话标题）·
//! `unread`（这一家的这一行适配层认不出：写好的一句 ＋ 原文摘录）。
//! 内容块 [`Block`] 是两家共有的词：正文（助手那一句是解析好的块树，[`reply_blocks`]）· 推理 · 工具调用 · 工具结果 · 图片。
//!
//! 不在这一形里的：链（上一条是谁 —— 主线外清单另给，`mainline`）· 会话事实（分叉血缘、用量、花费 —— `history-facts`）· 原文（线上不带）。

//!
//! 形状住 `cmd-wire`（[`cmd_wire::record`]，手机按同一份解）；这里是后端在它上面做的事（造记录 · 类别；钟面由出口那一层写，`wire::stamp`）；认不出那一行的一句住 `face_core::record`（手机解不开的那一条也写这一句），摘录的截法跟形状一起住 `cmd-wire`。

pub use cmd_wire::record::excerpt_of;
pub use cmd_wire::record::{Block, Body, Record, ReplyError, TitleBy, UnreadWhy};
pub use face_core::record::unread_body;

pub use cmd_wire::record::Draw;

/// 助手那一行的内容块：正文块解析成块树（[`Block::Prose`]，两家适配层都经它，解析只有 `md_core::parse` 一处）；别的块照原样。
pub(crate) fn reply_blocks(blocks: Vec<Block>) -> Vec<Block> {
    blocks
        .into_iter()
        .map(|b| match b {
            Block::Text { text, .. } => prose(&text),
            b => b,
        })
        .collect()
}

/// 一段助手正文 ⇒ [`Block::Prose`]：树（`md_core::parse`）＋ 照原文字节写好的「展开 {大小}」（两端照放进按钮）。
pub fn prose(text: &str) -> Block {
    let size = copy_core::size_text(text.len() as u64);
    Block::prose(
        md_core::parse(text),
        cmd_wire::cells::Words(copy_core::copy_text(
            "cards.text.showAll",
            &[("size", &size)],
        )),
    )
}

/// 造一条通用记录（两家适配层都经它）：画成哪一形（[`Draw`]）· 人说没说，从这条的类与格算一次，界面照摆。
/// 钟面那两格留给出口（`wire::stamp::RecordExt::stamp`，帧面那一层）。
pub fn record(agent: cmd_wire::AgentKind, id: String, at: Option<String>, body: Body) -> Record {
    let body = said_doc(body);
    Record {
        agent,
        id,
        at,
        time_text: None,
        at_ms: None,
        draw: draw_of(&body),
        human: human_of(&body),
        body,
    }
}

/// 报错 / 重试的原因一词（服务器过载 · 额度满 · 网络中断 · 需登录 · 上下文超长 · 原因不明）：两端照摆的那几句都经它，只住这一处。
pub fn reason_word(reason: crate::agents::ApiReason) -> String {
    use crate::agents::ApiReason;
    match reason {
        ApiReason::Overloaded => copy_core::copy_text("apiError.reason.overloaded", &[]),
        ApiReason::Quota => copy_core::copy_text("apiError.reason.quota", &[]),
        ApiReason::Network => copy_core::copy_text("apiError.reason.network", &[]),
        ApiReason::Auth => copy_core::copy_text("acct.tag.login", &[]),
        ApiReason::Context => copy_core::copy_text("apiError.reason.context", &[]),
        ApiReason::Unknown => copy_core::copy_text("reason.io.unknown", &[]),
    }
}

/// 报错回复那一格（两家适配层都经它）：状态码 ＋ 照原因写好的卡头那一句（原因本身不上线）。
pub fn reply_error(reason: crate::agents::ApiReason, status: Option<u32>) -> ReplyError {
    ReplyError {
        status,
        title: crate::common::cells::Words(copy_core::copy_text(
            "apiError.card.title",
            &[("reason", &reason_word(reason))],
        )),
    }
}

/// 来话（agent 交回 · 途中来话 · 另一个会话 · 主会话后来的话）的正文解析成块树，两家适配层都经它（解析只有 `md_core::parse` 一处）；
/// 另一个会话的来话顺手写好抬头那一句。
fn said_doc(mut body: Body) -> Body {
    use cmd_wire::record::Speaker;
    if let Body::Said { who, .. } = &mut body {
        if let Speaker::PeerSession { from, title, .. } = &mut who.speaker {
            *title = Some(crate::common::cells::Words(copy_core::copy_text(
                "speaker.peer.title",
                &[("from", from.as_deref().unwrap_or("?"))],
            )));
        }
        if let Speaker::AgentMessage {
            body: Some(text),
            doc,
            ..
        }
        | Speaker::PeerSession {
            body: Some(text),
            doc,
            ..
        }
        | Speaker::Coordinator {
            body: Some(text),
            doc,
        } = &mut who.speaker
        {
            *doc = Some(md_core::parse(text));
        }
    }
    body
}

/// 人在这条里说了话：人打的字 · 斜杠命令 · `!` 命令。
fn human_of(body: &Body) -> bool {
    use cmd_wire::record::Speaker;
    matches!(
        body,
        Body::Said { who, .. }
            if matches!(who.speaker, Speaker::Human | Speaker::SlashCommand { .. } | Speaker::BashInput { .. })
    )
}

/// 画成哪一形。思考的空串是「没写进记录」（线上约定），照画成一步；空的正文不算。
fn draw_of(body: &Body) -> Draw {
    use cmd_wire::record::{Speaker, ToolCard};
    let some = |b: bool| if b { Draw::Card } else { Draw::None };
    match body {
        Body::Said {
            who,
            blocks,
            attachments,
            ..
        } => match &who.speaker {
            Speaker::System { body } => {
                if body.as_deref().is_some_and(|b| !b.is_empty()) {
                    Draw::Strip
                } else {
                    Draw::None
                }
            }
            Speaker::CommandOutput => Draw::None,
            Speaker::Human => some(
                !who.text.is_empty()
                    || !attachments.is_empty()
                    || blocks.iter().any(|b| matches!(b, Block::Image { .. })),
            ),
            Speaker::CompactSummary { .. } | Speaker::AgentTask => some(!who.text.is_empty()),
            Speaker::ToolResult => {
                if blocks.iter().any(|b| matches!(b, Block::ToolResult { .. })) {
                    Draw::Steps
                } else {
                    Draw::None
                }
            }
            Speaker::SlashCommand { .. }
            | Speaker::BashInput { .. }
            | Speaker::BashOutput { .. }
            | Speaker::TaskNotification { .. }
            | Speaker::AgentMessage { .. }
            | Speaker::PeerSession { .. }
            | Speaker::Coordinator { .. }
            | Speaker::Interrupt => Draw::Card,
        },
        Body::Reply {
            blocks,
            auto_reply,
            cards,
            error,
            ..
        } => {
            if error.is_some() {
                return Draw::Card;
            }
            if *auto_reply {
                return Draw::None;
            }
            // 助手的正文都是树（[`reply_blocks`]）：解析出来一块都没有（空 · 只剩空白）⇒ 不算。
            let shown =
                |b: &&Block| !matches!(b, Block::Prose { doc, .. } if doc.blocks.is_empty());
            let mut any = false;
            for b in blocks.iter().filter(shown) {
                any = true;
                match b {
                    Block::Prose { .. } => return Draw::Card,
                    Block::ToolUse { id, .. } if cards.get(id) == Some(&ToolCard::Interactive) => {
                        return Draw::Card
                    }
                    _ => {}
                }
            }
            if any {
                Draw::Steps
            } else {
                Draw::None
            }
        }
        Body::Retry { attempt, .. } => face_core::record::retry_draw(*attempt),
        Body::Unread { .. } => Draw::Card,
        Body::Title { .. } => Draw::None,
    }
}

/// 后端造 / 认记录类别那一侧（形状在 `cmd-wire`）。
pub trait BodyExt {
    /// 这条记录的类。
    fn class(&self) -> RecordClass;
}

impl BodyExt for Body {
    fn class(&self) -> RecordClass {
        match self {
            Self::Said { .. } => RecordClass::Said,
            Self::Reply { .. } => RecordClass::Reply,
            Self::Retry { .. } => RecordClass::Retry,
            Self::Title { .. } => RecordClass::Title,
            Self::Unread { .. } => RecordClass::Unread,
        }
    }
}

/// 通用记录的类（记录上 `t` 那一格的五个词；骨架行 `t` 也是它，`unread` 除外：骨架不出认不出的行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(export, export_to = concat!(env!("CARGO_MANIFEST_DIR"), "/../../frontend/ui/generated/")))]
#[serde(rename_all = "camelCase")]
pub enum RecordClass {
    Said,
    Reply,
    Retry,
    Title,
    Unread,
}

#[cfg(test)]
#[path = "../../../tests/backend/agents/record_tests.rs"]
mod tests;

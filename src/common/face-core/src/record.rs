//! **认不出那一条**的句子：后端适配层认不出的那一行（[`unread_body`]）· 出记录页并条时重写的那一句（[`unread_text`]）·
//! 手机核心解不开的那一条（手机核心 `record::OrUnread` 调 [`unread_body`]）同这一份。
//! 线上形（`Body::Unread` · `UnreadWhy` · `t` 的闭集 · 摘录的截法）住 `cmd_wire::record`。

use cmd_wire::cells::{Tone, Words};
use cmd_wire::record::{excerpt_of, Body, Draw, SaidPart, TextFold, UnreadWhy};
use copy_core::copy_text;

/// 认不出那一条的一句：「认不出的记录 · {type}」/「记录形状变了 · {type}」，并了几条 ⇒ 后面带「 · {n} 条」。
pub fn unread_text(why: UnreadWhy, kind: Option<&str>, count: u32) -> Words {
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
    Words(text)
}

/// 认不出的一行（每行一条，`count` 1）：一句 ＋ 原文摘录。
pub fn unread_body(why: UnreadWhy, kind: Option<String>, raw: &str) -> Body {
    Body::Unread {
        why,
        text: unread_text(why, kind.as_deref(), 1),
        kind,
        tone: Tone::Warn,
        excerpt: Words(excerpt_of(raw)),
        count: 1,
    }
}

// ── 记录 → 卡片切好的段：折不折 · 露几行 · 展开那一句只在这里定（两端照 `TextFold` 露，不自己数行、不留阈值）。

/// 粘贴块的正文超过这么多行 ⇒ 整块收成一行。
const PASTE_FOLD_LINES: u32 = 12;
/// 派活那段超过这么多行 ⇒ 先露这么多行。
const BRIEF_LINES: u32 = 4;
/// 命令输出超过这么多行 ⇒ 先露 [`OUTPUT_HEAD_LINES`] 行。
const OUTPUT_FOLD_LINES: u32 = 30;
const OUTPUT_HEAD_LINES: u32 = 20;

fn line_count(s: &str) -> u32 {
    u32::try_from(s.split('\n').count()).unwrap_or(u32::MAX)
}

fn plain(text: String) -> SaidPart {
    SaidPart { text, fold: None }
}

/// 人粘贴进来的一块在正文里的界（字节下标）：整块 `[start, end)` 含两头的标记，正文那一截 `[body_start, body_end)`。
/// 标记长什么样是那一家的格式知识（适配层认），这里只吃界。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasteSpan {
    pub start: usize,
    pub end: usize,
    pub body_start: usize,
    pub body_end: usize,
}

/// 照粘贴块的界把人说的话切成段（[`cmd_wire::record::UserText::parts`]）：标记不露、块正文两头的换行去掉；块超过 [`PASTE_FOLD_LINES`] 行
/// ⇒ 那一段整块收成一行（「粘贴的内容 · N 行」）、紧挨着它的那一个换行不另起空行；照排的相邻几段并成一段。没有块 ⇒ 空（整段照排）。
pub fn said_parts(text: &str, spans: &[PasteSpan]) -> Vec<SaidPart> {
    let mut spans: Vec<PasteSpan> = spans
        .iter()
        .copied()
        .filter(|s| {
            s.start <= s.body_start
                && s.body_start <= s.body_end
                && s.body_end <= s.end
                && s.end <= text.len()
        })
        .collect();
    spans.sort_by_key(|s| s.start);
    if spans.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<SaidPart> = Vec::new();
    let mut push = |p: SaidPart| {
        if p.fold.is_none() {
            if p.text.is_empty() {
                return;
            }
            if let Some(last) = out.last_mut().filter(|l| l.fold.is_none()) {
                last.text.push_str(&p.text);
                return;
            }
        }
        out.push(p);
    };
    let strip_head = |s: &str| {
        s.strip_prefix("\r\n")
            .or_else(|| s.strip_prefix('\n'))
            .unwrap_or(s)
            .to_string()
    };
    let mut at = 0;
    let mut after_fold = false;
    for s in spans {
        if s.start < at {
            continue;
        }
        let inner = text[s.body_start..s.body_end].trim_matches(['\n', '\r']);
        let total = line_count(inner);
        let fold = total > PASTE_FOLD_LINES;
        let mut before = text[at..s.start].to_string();
        if after_fold {
            before = strip_head(&before);
        }
        if fold {
            before = before
                .strip_suffix("\r\n")
                .or_else(|| before.strip_suffix('\n'))
                .unwrap_or(&before)
                .to_string();
        }
        push(plain(before));
        push(SaidPart {
            text: inner.to_string(),
            fold: fold.then(|| TextFold {
                shown: 0,
                total,
                more: Words(copy_text(
                    "speaker.paste.fold",
                    &[("n", &total.to_string())],
                )),
            }),
        });
        after_fold = fold;
        at = s.end;
    }
    let tail = &text[at..];
    push(plain(if after_fold {
        strip_head(tail)
    } else {
        tail.to_string()
    }));
    out
}

/// 派活那段（[`cmd_wire::record::Speaker::AgentTask`]）：超过 [`BRIEF_LINES`] 行 ⇒ 一段、先露那么多行；否则空（整段照排）。
pub fn brief_parts(text: &str) -> Vec<SaidPart> {
    let total = line_count(text);
    if total <= BRIEF_LINES {
        return Vec::new();
    }
    vec![SaidPart {
        text: text.to_string(),
        fold: Some(TextFold {
            shown: BRIEF_LINES,
            total,
            more: Words(copy_text("agentWindow.brief.more", &[])),
        }),
    }]
}

/// 一份命令输出（去掉两头空白之后数行）超过 [`OUTPUT_FOLD_LINES`] 行 ⇒ 先露 [`OUTPUT_HEAD_LINES`] 行；否则 `None`。
pub fn output_fold(text: &str) -> Option<TextFold> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let total = line_count(t);
    (total > OUTPUT_FOLD_LINES).then(|| TextFold {
        shown: OUTPUT_HEAD_LINES,
        total,
        more: Words(copy_text(
            "bash.output.showAll",
            &[("n", &total.to_string())],
        )),
    })
}

/// 一次要重试的上游失败画不画：接在一串后面的那条（第 2 次起）不另起一条，那一串的整句住会话事实 `retries[]`；
/// 第一次 · 没写第几次的 ⇒ 一条细条。
pub fn retry_draw(attempt: Option<u32>) -> Draw {
    match attempt {
        Some(a) if a > 1 => Draw::None,
        _ => Draw::Card,
    }
}

#[cfg(test)]
#[path = "../../../../tests/common/face-core/record_tests.rs"]
mod tests;

//! 认不出那一条（后端适配层认不出的那一行 · 手机核心解不开的那一条）的句子的判据（判法那一半在手机核心 `record.rs`）。
//!
//! 1. 句子：没见过的类 · 形状变了两种，并了几条 ⇒ 带条数那一形；没有类名 ⇒ 「无类型」那一词。

use super::*;
use cmd_wire::record::{Body, UnreadWhy, EXCERPT_BYTES};
use copy_core::copy_text;

#[test]
fn the_sentence_names_the_kind_and_the_count_when_merged() {
    assert_eq!(
        unread_text(UnreadWhy::UnknownType, Some("nope"), 1).0,
        copy_text("record.unread.unknownType", &[("type", "nope")])
    );
    assert_eq!(
        unread_text(UnreadWhy::UnknownType, Some("nope"), 2).0,
        copy_text(
            "record.unread.unknownTypeMany",
            &[("type", "nope"), ("n", "2")]
        )
    );
    assert_eq!(
        unread_text(UnreadWhy::ParseFailed, Some("said"), 1).0,
        copy_text("record.unread.parseFailed", &[("type", "said")])
    );
    assert_eq!(
        unread_text(UnreadWhy::ParseFailed, Some("said"), 3).0,
        copy_text(
            "record.unread.parseFailedMany",
            &[("type", "said"), ("n", "3")]
        )
    );
    let no_type = copy_text("record.unread.noType", &[]);
    assert_eq!(
        unread_text(UnreadWhy::UnknownType, None, 1).0,
        copy_text("record.unread.unknownType", &[("type", &no_type)])
    );
    assert_eq!(
        unread_text(UnreadWhy::UnknownType, Some(""), 1).0,
        unread_text(UnreadWhy::UnknownType, None, 1).0,
        "空类名同没有"
    );
}

#[test]
fn the_unread_body_is_one_warn_line_with_the_excerpt_cut_at_a_char_boundary() {
    let raw = "认".repeat(EXCERPT_BYTES);
    let Body::Unread {
        why,
        kind,
        text,
        tone,
        excerpt,
        count,
    } = unread_body(UnreadWhy::ParseFailed, Some("said".into()), &raw)
    else {
        panic!("不是认不出那一支");
    };
    assert_eq!(why, UnreadWhy::ParseFailed);
    assert_eq!(kind.as_deref(), Some("said"));
    assert_eq!(text, unread_text(UnreadWhy::ParseFailed, Some("said"), 1));
    assert_eq!(tone, cmd_wire::cells::Tone::Warn);
    assert!(excerpt.0.len() <= EXCERPT_BYTES && raw.starts_with(&excerpt.0));
    assert_eq!(count, 1);
}

// ── 记录 → 卡片切好的段：粘贴块 · 派活那段 · 命令输出怎么折 · 重试那一串画几条（阈值只在这一层）。

fn lines_of(n: usize) -> String {
    (1..=n)
        .map(|i| format!("l{i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 照标记在 `text` 里的字节位置给一块的界（适配层认标记，这里只吃界）。
fn span(text: &str, open: &str, close: &str) -> PasteSpan {
    let start = text.find(open).unwrap();
    let body_start = start + open.len();
    let body_end = body_start + text[body_start..].find(close).unwrap();
    PasteSpan {
        start,
        end: body_end + close.len(),
        body_start,
        body_end,
    }
}

#[test]
fn a_said_text_with_pasted_blocks_comes_cut_into_parts() {
    let long = lines_of(13);
    let text = format!("看这段：\n<A>\n{long}\n</A>\n再看<B>甲\n乙</B>完");
    let parts = said_parts(
        &text,
        &[span(&text, "<A>", "</A>"), span(&text, "<B>", "</B>")],
    );
    let shown: Vec<(&str, Option<(u32, u32)>)> = parts
        .iter()
        .map(|p| (p.text.as_str(), p.fold.as_ref().map(|f| (f.shown, f.total))))
        .collect();
    assert_eq!(
        shown,
        vec![
            ("看这段：", None),
            (long.as_str(), Some((0, 13))),
            ("再看甲\n乙完", None),
        ],
        "标记不露、折起那块两头的换行不另起空行、照排的几段并成一段"
    );
    assert_eq!(
        parts[1].fold.as_ref().unwrap().more.0,
        copy_text("speaker.paste.fold", &[("n", "13")])
    );
    assert!(said_parts("没有粘贴", &[]).is_empty(), "没有粘贴块 ⇒ 不切");
}

#[test]
fn a_long_command_output_folds_to_its_head() {
    let f = output_fold(&format!("\n{}\n", lines_of(35))).expect("35 行要折");
    assert_eq!((f.shown, f.total), (20, 35));
    assert_eq!(f.more.0, copy_text("bash.output.showAll", &[("n", "35")]));
    assert_eq!(output_fold(&lines_of(5)), None, "5 行不折");
    assert_eq!(output_fold("  "), None);
}

#[test]
fn a_long_brief_comes_as_one_folded_part() {
    let parts = brief_parts(&lines_of(9));
    assert_eq!(parts.len(), 1);
    let f = parts[0].fold.as_ref().unwrap();
    assert_eq!((f.shown, f.total), (4, 9));
    assert_eq!(f.more.0, copy_text("agentWindow.brief.more", &[]));
    assert!(brief_parts(&lines_of(2)).is_empty(), "短的不切");
}

#[test]
fn two_retries_in_a_row_draw_only_one_bar() {
    let draws: Vec<Draw> = [Some(1), Some(2), Some(3), None]
        .into_iter()
        .map(retry_draw)
        .collect();
    assert_eq!(draws, vec![Draw::Card, Draw::None, Draw::None, Draw::Card]);
}

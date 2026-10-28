//! 托管终端的屏（`host/managed/screen.rs`）：整屏字节还原 · 回滚抓屏 · 清回滚 · 终端询问自己答。

use super::*;

fn cells(s: &vt100::Screen) -> Vec<(String, vt100::Color, vt100::Color, bool)> {
    let (rows, cols) = s.size();
    let mut v = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let cell = s.cell(r, c).expect("格在");
            v.push((
                cell.contents().to_string(),
                cell.fgcolor(),
                cell.bgcolor(),
                cell.bold(),
            ));
        }
    }
    v
}

#[test]
fn the_full_screen_bytes_rebuild_the_same_screen_cell_by_cell() {
    let mut s = Screen::new(10, 40);
    s.feed(b"plain \x1b[1;31mbold red\x1b[0m \x1b[38;2;10;20;30mtruecolor\x1b[0m\r\n");
    s.feed(b"\x1b[?2004h\x1b[?1h\x1b[5;7Hhere\x1b[?25l");
    let mut fresh = vt100::Parser::new(10, 40, 0);
    fresh.process(&s.full());
    let (a, b) = (s.screen(), fresh.screen());
    assert_eq!(cells(a), cells(b), "逐格相等");
    assert_eq!(a.cursor_position(), b.cursor_position(), "光标相等");
    assert_eq!(a.hide_cursor(), b.hide_cursor());
    assert_eq!(a.bracketed_paste(), b.bracketed_paste());
    assert_eq!(a.application_cursor(), b.application_cursor());
}

#[test]
fn capture_back_returns_the_last_lines_printed_and_says_capped() {
    let mut s = Screen::new(24, 80);
    for i in 1..=3000 {
        s.feed(format!("line {i}\r\n").as_bytes());
    }
    let got = s.capture(2000, false);
    let mut lines: Vec<&str> = got.lines.iter().map(String::as_str).collect();
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    let want: Vec<String> = (1001..=3000).map(|i| format!("line {i}")).collect();
    assert!(lines.len() >= 2000, "至少 2000 行：{}", lines.len());
    let tail: Vec<&str> = lines[lines.len() - 2000..].to_vec();
    assert_eq!(tail, want.iter().map(String::as_str).collect::<Vec<_>>());
    assert!(got.capped, "回滚比要的多 ⇒ capped");
    // 抓屏不挪看的位置：之后当屏照旧是最新一屏。
    assert_eq!(s.screen().scrollback(), 0);
}

#[test]
fn capture_of_a_short_history_is_not_capped() {
    let mut s = Screen::new(5, 20);
    for i in 1..=8 {
        s.feed(format!("l{i}\r\n").as_bytes());
    }
    let got = s.capture(100, false);
    assert!(!got.capped);
    assert_eq!(got.lines.first().map(String::as_str), Some("l1"));
}

#[test]
fn erase_scrollback_clears_history_like_a_real_terminal() {
    let mut s = Screen::new(5, 20);
    for i in 1..=50 {
        s.feed(format!("l{i}\r\n").as_bytes());
    }
    assert!(s.capture(100, false).lines.len() > 5);
    s.feed(b"\x1b[3Jkept");
    let got = s.capture(100, false);
    assert_eq!(got.lines.len(), 5, "只剩当屏：{:?}", got.lines);
    assert!(
        got.lines.iter().any(|l| l.contains("kept")),
        "当屏还在：{:?}",
        got.lines
    );
}

#[test]
fn device_attributes_are_answered_once_even_when_split_across_chunks() {
    let mut s = Screen::new(5, 20);
    let mut out = s.feed(b"ab\x1b[");
    out.extend(s.feed(b"c"));
    assert_eq!(out, DA1_REPLY.to_vec());
    assert_eq!(s.feed(b"\x1b[>c"), DA2_REPLY.to_vec());
}

#[test]
fn cursor_position_report_answers_where_the_cursor_is_at_that_moment() {
    let mut s = Screen::new(5, 20);
    let out = s.feed(b"\x1b[3;7H\x1b[6nmore text");
    assert_eq!(out, b"\x1b[3;7R".to_vec());
}

#[test]
fn xtversion_names_ccm() {
    let mut s = Screen::new(5, 20);
    let out = s.feed(b"\x1b[>q");
    let text = String::from_utf8(out).expect("utf8");
    assert!(text.starts_with("\x1bP>|ccm "), "{text:?}");
    assert!(text.ends_with("\x1b\\"), "{text:?}");
}

#[test]
fn kitty_keyboard_query_is_not_answered() {
    let mut s = Screen::new(5, 20);
    assert!(s.feed(b"\x1b[?u").is_empty());
}

#[test]
fn the_title_is_kept_for_new_clients() {
    let mut s = Screen::new(5, 20);
    s.feed(b"\x1b]0;hello\x07");
    assert_eq!(s.parser.callbacks().title, b"hello");
    // 新接上的一端只拿整屏字节：标题也在里面。
    struct T(Vec<u8>);
    impl vt100::Callbacks for T {
        fn set_window_title(&mut self, _: &mut vt100::Screen, t: &[u8]) {
            self.0 = t.to_vec();
        }
    }
    let mut fresh = vt100::Parser::new_with_callbacks(5, 20, 0, T(Vec::new()));
    fresh.process(&s.full());
    assert_eq!(fresh.callbacks().0, b"hello");
}

#[test]
fn the_scrollback_constant_matches_the_terminal_page_limit() {
    assert_eq!(
        SCROLLBACK,
        crate::control::terminals::MAX_SCROLLBACK as usize
    );
}

#[test]
fn resize_keeps_content_and_reports_the_new_size() {
    let mut s = Screen::new(5, 20);
    s.feed(b"abc");
    s.resize(10, 30);
    assert_eq!(s.size(), (10, 30));
    assert!(s.screen().contents().starts_with("abc"));
}

/// `vt100` 不记的那几样（焦点 · 光标形状 · 同步输出）由这里记；新接上的一端拿整屏字节也拿到它们。
#[test]
fn focus_cursor_shape_and_sync_are_kept_for_new_clients() {
    let mut s = Screen::new(5, 20);
    s.feed(b"\x1b[?1004h\x1b[5 q");
    let mut fresh = Extra::default();
    let mut p = vt100::Parser::new_with_callbacks(5, 20, 0, ExtraOnly(Extra::default()));
    p.process(&s.full());
    fresh.clone_from(&p.callbacks().0);
    assert!(fresh.focus);
    assert_eq!(fresh.shape, 5);
    assert!(!fresh.sync);
    // 清回滚换 parser 那一下也不丢。
    s.feed(b"\x1b[3J");
    let mut p = vt100::Parser::new_with_callbacks(5, 20, 0, ExtraOnly(Extra::default()));
    p.process(&s.full());
    assert!(p.callbacks().0.focus);
    // 关掉 ⇒ 整屏字节里不再开；一条里几项一起开关也认（`?1004;2026h`）。
    s.feed(b"\x1b[?1004l\x1b[0 q\x1b[?25;2026h");
    let mut p = vt100::Parser::new_with_callbacks(5, 20, 0, ExtraOnly(Extra::default()));
    p.process(&s.full());
    assert!(!p.callbacks().0.focus);
    assert_eq!(p.callbacks().0.shape, 0);
    assert!(p.callbacks().0.sync, "开着同步输出时接上的一端也先不画");
}

struct ExtraOnly(Extra);
impl vt100::Callbacks for ExtraOnly {
    fn unhandled_csi(
        &mut self,
        _: &mut vt100::Screen,
        i1: Option<u8>,
        _: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        self.0.csi(i1, params, c);
    }
}

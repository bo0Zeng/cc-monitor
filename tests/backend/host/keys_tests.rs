//! 键表：照 tmux 的缺省键（A3-3-3-1）。

use super::*;

fn feed(k: &mut Keys, b: &[u8]) -> Vec<Act> {
    k.feed(b)
}

#[test]
fn prefix_d_detaches_and_sends_nothing() {
    let mut k = Keys::new(DEFAULT_PREFIX);
    assert_eq!(feed(&mut k, b"\x02d"), vec![Act::Detach]);
}

#[test]
fn prefix_prefix_sends_exactly_one_prefix_byte() {
    let mut k = Keys::new(DEFAULT_PREFIX);
    assert_eq!(feed(&mut k, b"\x02\x02"), vec![Act::Send(vec![0x02])]);
}

#[test]
fn ordinary_bytes_pass_through_and_split_around_the_prefix() {
    let mut k = Keys::new(DEFAULT_PREFIX);
    assert_eq!(
        feed(&mut k, b"ab\x02rcd"),
        vec![
            Act::Send(b"ab".to_vec()),
            Act::Redraw,
            Act::Send(b"cd".to_vec())
        ]
    );
}

#[test]
fn the_prefix_can_arrive_in_one_chunk_and_the_key_in_the_next() {
    let mut k = Keys::new(DEFAULT_PREFIX);
    assert!(feed(&mut k, b"\x02").is_empty());
    assert_eq!(feed(&mut k, b"?"), vec![Act::ListKeys]);
}

#[test]
fn a_key_not_in_the_table_after_the_prefix_does_nothing() {
    let mut k = Keys::new(DEFAULT_PREFIX);
    assert!(feed(&mut k, b"\x02z").is_empty());
    assert_eq!(
        feed(&mut k, b"z"),
        vec![Act::Send(b"z".to_vec())],
        "之后回到平常"
    );
}

#[test]
fn the_rest_of_the_table() {
    let mut k = Keys::new(DEFAULT_PREFIX);
    assert_eq!(feed(&mut k, b"\x02D"), vec![Act::ListClients]);
    assert_eq!(feed(&mut k, b"\x02["), vec![Act::CopyMode]);
    assert_eq!(feed(&mut k, b"\x02\x1b[5~"), vec![Act::CopyModePageUp]);
    assert_eq!(feed(&mut k, b"\x02s"), vec![Act::ChooseTerminal]);
}

#[test]
fn prefix_x_asks_first_and_only_y_ends() {
    let mut k = Keys::new(DEFAULT_PREFIX);
    assert_eq!(feed(&mut k, b"\x02x"), vec![Act::AskEnd]);
    assert_eq!(feed(&mut k, b"n"), vec![Act::CancelEnd], "别的键 ⇒ 不结束");
    assert_eq!(feed(&mut k, b"\x02x"), vec![Act::AskEnd]);
    assert_eq!(feed(&mut k, b"y"), vec![Act::End]);
}

#[test]
fn another_prefix_key_is_honoured() {
    let mut k = Keys::new(0x01);
    assert_eq!(
        feed(&mut k, b"\x02"),
        vec![Act::Send(vec![0x02])],
        "Ctrl+b 成了普通键"
    );
    assert_eq!(feed(&mut k, b"\x01d"), vec![Act::Detach]);
}

#[test]
fn the_key_list_names_every_binding_like_tmux() {
    let text = list_keys(DEFAULT_PREFIX);
    for cmd in [
        "send-prefix",
        "detach-client",
        "choose-client",
        "refresh-client",
        "list-keys",
        "copy-mode",
        "kill-pane",
        "choose-tree",
    ] {
        assert!(text.contains(cmd), "{cmd} 不在：{text}");
    }
    assert!(text.contains("C-b d"), "{text}");
}

#[test]
fn copy_mode_emacs_table() {
    let mut c = CopyKeys::new(false);
    assert_eq!(c.feed(b"\x1b[A"), vec![Copy::Up]);
    assert_eq!(c.feed(b"\x1b[6~"), vec![Copy::PageDown]);
    assert_eq!(c.feed(b"\x1b<"), vec![Copy::Top]);
    assert_eq!(c.feed(b"\x1b>"), vec![Copy::Bottom]);
    assert_eq!(c.feed(b"q"), vec![Copy::Quit]);
    assert!(c.feed(b"g").is_empty(), "g 是 vi 表的");
}

#[test]
fn copy_mode_vi_table() {
    let mut c = CopyKeys::new(true);
    assert_eq!(c.feed(b"k"), vec![Copy::Up]);
    assert_eq!(c.feed(b"j"), vec![Copy::Down]);
    assert_eq!(c.feed(b"g"), vec![Copy::Top]);
    assert_eq!(c.feed(b"G"), vec![Copy::Bottom]);
    assert_eq!(c.feed(b"\x1b"), vec![Copy::Quit]);
}

#[test]
fn copy_mode_search_collects_text_until_enter() {
    let mut c = CopyKeys::new(false);
    assert!(c.feed(b"/").is_empty());
    assert!(c.feed(b"abc").is_empty());
    assert_eq!(c.prompt(), Some("/abc"));
    assert_eq!(
        c.feed(b"\r"),
        vec![Copy::Search {
            text: "abc".into(),
            forward: true
        }]
    );
    assert_eq!(c.feed(b"n"), vec![Copy::Next]);
    assert_eq!(c.feed(b"N"), vec![Copy::Prev]);
    assert!(c.feed(b"?x\x7f").is_empty(), "退格删掉一个字");
    assert_eq!(c.prompt(), Some("?"));
}

#[test]
fn vi_is_chosen_from_editor_like_tmux() {
    assert!(vi_from(Some("vim"), None));
    assert!(vi_from(None, Some("/usr/bin/nvim")));
    assert!(!vi_from(Some("emacs"), None));
    assert!(!vi_from(None, None));
}

#[test]
fn a_bracketed_paste_passes_through_whole_even_with_the_prefix_inside() {
    let mut k = Keys::new(DEFAULT_PREFIX);
    let paste = b"\x1b[200~a\x02db\x1b[201~";
    assert_eq!(feed(&mut k, paste), vec![Act::Send(paste.to_vec())]);
    assert_eq!(
        feed(&mut k, b"\x02d"),
        vec![Act::Detach],
        "粘完之后前缀照旧"
    );
}

#[test]
fn a_bracketed_paste_split_across_chunks_is_still_one_paste() {
    let mut k = Keys::new(DEFAULT_PREFIX);
    let mut sent = Vec::new();
    for chunk in [&b"x\x1b[20"[..], b"0~a\x02", b"d\x1b[2", b"01~"] {
        for a in feed(&mut k, chunk) {
            match a {
                Act::Send(b) => sent.extend(b),
                other => panic!("粘贴里不该有动作：{other:?}"),
            }
        }
    }
    assert_eq!(sent, b"x\x1b[200~a\x02d\x1b[201~".to_vec());
    assert_eq!(feed(&mut k, b"\x02d"), vec![Act::Detach]);
}

// ── 鼠标（A3-3-33）：外层开了 SGR 1006 上报，送来的 `ESC[<b;x;yM/m` 认出来，其余照原样 ──

fn m(code: u16, col: u16, row: u16, press: bool) -> Mouse {
    Mouse {
        code,
        col,
        row,
        press,
    }
}

#[test]
fn a_mouse_report_is_picked_out_of_the_bytes_around_it() {
    let mut s = Mice::default();
    assert_eq!(
        s.split(b"ab\x1b[<64;10;5Mcd"),
        vec![
            Piece::Bytes(b"ab".to_vec()),
            Piece::Mouse(m(64, 10, 5, true)),
            Piece::Bytes(b"cd".to_vec())
        ]
    );
}

#[test]
fn a_mouse_report_split_over_two_chunks_is_still_one_event() {
    let mut s = Mice::default();
    assert!(s.split(b"\x1b[<0;3").is_empty());
    assert_eq!(s.split(b";4m"), vec![Piece::Mouse(m(0, 3, 4, false))]);
}

/// 块尾单独一个 Esc（Esc 键）不拖到下一块；别的转义序列照原样。
#[test]
fn a_lone_escape_and_other_sequences_pass_through_untouched() {
    let mut s = Mice::default();
    assert_eq!(s.split(b"\x1b"), vec![Piece::Bytes(b"\x1b".to_vec())]);
    assert_eq!(s.split(b"\x1b[A"), vec![Piece::Bytes(b"\x1b[A".to_vec())]);
    assert_eq!(s.split(b"x\x1b["), vec![Piece::Bytes(b"x\x1b[".to_vec())]);
    assert_eq!(
        s.split(b"\x1b[<1;2X"),
        vec![Piece::Bytes(b"\x1b[<1;2X".to_vec())],
        "不是鼠标上报的照原样"
    );
}

#[test]
fn the_wheel_is_told_apart_from_clicks_and_ignores_modifiers() {
    assert_eq!(m(64, 1, 1, true).wheel(), Some(Wheel::Up));
    assert_eq!(m(65, 1, 1, true).wheel(), Some(Wheel::Down));
    assert_eq!(m(64 | 4, 1, 1, true).wheel(), Some(Wheel::Up), "Shift");
    assert_eq!(m(65 | 16, 1, 1, true).wheel(), Some(Wheel::Down), "Ctrl");
    assert_eq!(m(0, 1, 1, true).wheel(), None);
    assert_eq!(m(64, 1, 1, false).wheel(), None, "松开不算一格");
}

/// 交给里面：照里面开的模式筛、照里面的编码重写；坐标在里面的屏外 ⇒ 不交。
#[test]
fn a_mouse_event_is_rewritten_for_what_the_inside_asked_for() {
    use vt100::{MouseProtocolEncoding as E, MouseProtocolMode as M};
    let inner = (24, 80);
    let wheel = m(64, 10, 5, true);
    assert_eq!(
        wheel.encode(M::PressRelease, E::Sgr, inner),
        Some(b"\x1b[<64;10;5M".to_vec())
    );
    assert_eq!(
        wheel.encode(M::PressRelease, E::Default, inner),
        Some(vec![0x1b, b'[', b'M', 64 + 32, 10 + 32, 5 + 32])
    );
    assert_eq!(
        m(0, 3, 4, false).encode(M::PressRelease, E::Default, inner),
        Some(vec![0x1b, b'[', b'M', 3 + 32, 3 + 32, 4 + 32]),
        "老编码的松开是 3"
    );
    assert_eq!(wheel.encode(M::None, E::Sgr, inner), None);
    assert_eq!(m(0, 3, 4, false).encode(M::Press, E::Sgr, inner), None);
    assert_eq!(
        m(32, 3, 4, true).encode(M::PressRelease, E::Sgr, inner),
        None
    );
    assert_eq!(
        m(32, 3, 4, true).encode(M::ButtonMotion, E::Sgr, inner),
        Some(b"\x1b[<32;3;4M".to_vec()),
        "按着拖"
    );
    assert_eq!(
        m(35, 3, 4, true).encode(M::ButtonMotion, E::Sgr, inner),
        None
    );
    assert_eq!(
        m(35, 3, 4, true).encode(M::AnyMotion, E::Sgr, inner),
        Some(b"\x1b[<35;3;4M".to_vec())
    );
    assert_eq!(
        m(0, 81, 4, true).encode(M::PressRelease, E::Sgr, inner),
        None,
        "屏外"
    );
    assert_eq!(
        m(0, 230, 4, true).encode(M::PressRelease, E::Default, (24, 300)),
        None,
        "老编码装不下"
    );
    assert_eq!(
        m(0, 230, 4, true).encode(M::PressRelease, E::Utf8, (24, 300)),
        Some([b"\x1b[M ".as_slice(), "\u{106}".as_bytes(), b"$"].concat())
    );
}

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

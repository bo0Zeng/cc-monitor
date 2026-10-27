//! 单独终端窗口（`open_terminal_in_new_window`）：一个终端一扇（窗口名按「那台 ＋ 句柄」定，已开着就拉前），
//! 落在查看窗那一组权限里（页的 URL 指向哪份 html 由 `tests/frontend/ui/entry-graphs.vitest.ts` 对账）。

use super::{terminal_window_label, TERMINAL_WINDOW_PREFIX};

#[test]
fn a_terminal_window_is_named_by_machine_and_handle() {
    let a = terminal_window_label("<local>", "tmux-%7");
    assert!(a.starts_with(TERMINAL_WINDOW_PREFIX));
    assert!(a.starts_with("viewer-"), "落在 `viewer-*` 那一组权限里");
    assert_eq!(a, terminal_window_label("<local>", "tmux-%7"), "同一个终端 ⇒ 同一扇窗");
    assert_ne!(a, terminal_window_label("<local>", "tmux-%8"), "别的终端 ⇒ 别的窗");
    assert_ne!(a, terminal_window_label("devbox", "tmux-%7"), "别的台 ⇒ 别的窗");
    assert_ne!(
        terminal_window_label("a", "bc"),
        terminal_window_label("ab", "c"),
        "两段之间不认错"
    );
    assert!(
        a.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "窗口名只用字母数字与 - _：{a}"
    );
}

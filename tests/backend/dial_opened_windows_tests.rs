//! ↗ 的本机记录（`dial/opened_windows.rs`）：一份两行怎么读 · 前奏在两种方言里长什么样。标签的形状在 `WindowLabel` 那一处判（`observe/session_terminals_tests.rs`）。

use super::*;

#[test]
fn a_record_is_the_label_then_the_machine() {
    let want = Some(Opened {
        label: WindowLabel { pid: 600, start: 20 },
        machine: "devbox".into(),
    });
    assert_eq!(parse("600-20\ndevbox\n"), want);
    assert_eq!(parse("600-20\r\ndevbox\r\n"), want, "CRLF 也认");
    assert_eq!(parse("600-20\n"), None, "缺机器名 ⇒ 当没有");
    assert_eq!(parse("\ndevbox\n"), None, "缺标签 ⇒ 当没有");
    assert_eq!(parse(""), None);
}

#[test]
fn a_record_whose_first_line_is_not_a_label_is_not_a_record() {
    assert_eq!(parse("123-abc\ndevbox\n"), None);
    assert_eq!(parse("123\ndevbox\n"), None);
}

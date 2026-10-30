//! ↗ 的本机记录（`dial/opened_windows.rs`）：一份两行怎么读 · 标签怎么拆 · 前奏在两种方言里长什么样。

use super::*;

#[test]
fn a_record_is_the_label_then_the_machine() {
    let want = Some(Opened {
        label: "600-20".into(),
        machine: "devbox".into(),
    });
    assert_eq!(parse("600-20\ndevbox\n"), want);
    assert_eq!(parse("600-20\r\ndevbox\r\n"), want, "CRLF 也认");
    assert_eq!(parse("600-20\n"), None, "缺机器名 ⇒ 当没有");
    assert_eq!(parse("\ndevbox\n"), None, "缺标签 ⇒ 当没有");
    assert_eq!(parse(""), None);
}

#[test]
fn a_label_is_a_pid_and_a_start() {
    assert_eq!(label_parts("600-20"), Some((600, 20)));
    assert_eq!(label_parts("600-"), None);
    assert_eq!(label_parts("x-20"), None);
    assert_eq!(label_parts("600"), None);
}

//! 路径的显示形：家目录那一截写成 `~`（那台按自己的家目录写，两端照抄）。期望手写。

use super::*;

/// 两种分隔符都认；不在家下面 · 家说不出 ⇒ 原样。
#[test]
fn a_path_under_the_home_is_shown_with_a_tilde() {
    let x = Some(Path::new("/home/x"));
    assert_eq!(home_short("/home/x/a", x), "~/a");
    assert_eq!(home_short("/home/x", x), "~");
    assert_eq!(home_short("/home/x/a", Some(Path::new("/home/x/"))), "~/a");
    // 前缀相同、不在分隔处断开 ⇒ 不是家底下（`/home/user` 与 `/home/user2`）。
    assert_eq!(
        home_short("/home/user2/a", Some(Path::new("/home/user"))),
        "/home/user2/a"
    );
    assert_eq!(home_short("/etc/a", x), "/etc/a");
    assert_eq!(home_short("~/a", x), "~/a");
    assert_eq!(home_short("/home/x/a", None), "/home/x/a");
    assert_eq!(home_short("/x", Some(Path::new("/"))), "/x");
    assert_eq!(
        home_short(r"C:\Users\u\.cc-monitor", Some(Path::new(r"C:\Users\u"))),
        r"~\.cc-monitor"
    );
}

/// 字节版：非 UTF-8 的名字一个字节不改，只换家那一截。
#[test]
fn the_byte_form_keeps_names_that_are_not_utf8() {
    let x = Some(Path::new("/home/x"));
    assert_eq!(home_short_bytes(b"/home/x/\xff", x), b"~/\xff".to_vec());
    assert_eq!(home_short_bytes(b"/other/\xff", x), b"/other/\xff".to_vec());
}

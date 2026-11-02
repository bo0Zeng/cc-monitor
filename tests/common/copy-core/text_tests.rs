//! 截断 · 首个非空行 · 压空白：口径一种（按字符留 N 个，截了接一个「…」，省略号不占那 N 个）。期望手写。

use super::*;

/// 按字符数，不按字节：中文 · emoji 不截出半个字；正好 N 个不加省略号。
#[test]
fn clip_keeps_n_chars_and_marks_the_cut() {
    assert_eq!(clip("abcdef", 3), "abc…");
    assert_eq!(clip("abc", 3), "abc");
    assert_eq!(clip("ab", 3), "ab");
    assert_eq!(clip("", 3), "");
    assert_eq!(clip("你好世界", 2), "你好…");
    assert_eq!(clip("😀😀😀", 2), "😀😀…");
    assert_eq!(clip("abc", 0), "…");
    // 不动正文里的空白与换行（要压先压）。
    assert_eq!(clip("a\n b", 10), "a\n b");
}

/// 头一个非空行，去两头空白；全是空白 ⇒ 没有。
#[test]
fn first_line_is_the_first_non_blank_line_trimmed() {
    assert_eq!(first_line("\n  \n  第一行  \n第二行"), Some("第一行"));
    assert_eq!(first_line("one"), Some("one"));
    assert_eq!(first_line("\r\n\t x \r\n"), Some("x"));
    assert_eq!(first_line(""), None);
    assert_eq!(first_line(" \n\t\n"), None);
}

/// 连串空白（空格 · 制表 · 换行 · 全角空格）压成一个空格，去两头。
#[test]
fn collapse_ws_squeezes_runs_into_one_space() {
    assert_eq!(collapse_ws("  a \n\t b  c "), "a b c");
    assert_eq!(collapse_ws("a\u{3000}\u{3000}b"), "a b");
    assert_eq!(collapse_ws(" \n "), "");
    assert_eq!(collapse_ws("ab"), "ab");
}

/// `clip_flat` 与「先压再截」逐例同值（对拍），只是不整条压。
#[test]
fn clip_flat_equals_clip_of_collapsed() {
    let cases = [
        "",
        "   ",
        "abc",
        "a  b   c",
        "  a  b  ",
        "abc def",
        "abc ",
        "abc  def ghi",
        "你好  世界\n再见",
        "😀 😀\t😀",
    ];
    for s in cases {
        for n in 0..12 {
            assert_eq!(clip_flat(s, n), clip(&collapse_ws(s), n), "{s:?} / {n}");
        }
    }
    // 前缀几乎全是空白：压后照样够数（不因只走前缀而少字）。
    let s = format!("{}正文{}", " ".repeat(640), "字".repeat(100));
    assert_eq!(clip_flat(&s, 80), clip(&collapse_ws(&s), 80));
}

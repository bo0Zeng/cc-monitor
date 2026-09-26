use super::*;

#[test]
fn posix_quote_breaks_single_quotes_the_posix_way() {
    assert_eq!(posix_quote("/p"), "'/p'");
    assert_eq!(posix_quote("a'b"), "'a'\\''b'");
    assert_eq!(posix_quote(""), "''");
}

/// 〔TL3 · `INVARIANTS §47` ②〕自由文本的拒绝集：恰好 NUL / CR / LF（正反各一格 —— §47「拒过头也算违反」）。
/// 要求住址：`INVARIANTS §47` ②「走唯一的 quote ＋ 形式判定 ＋ 拒绝集」；主会话 09-26 按 V131 裁「自由文本路径的拒绝集只收控制字符（NUL / CR / LF）……不拒 shell 元字符」。
#[test]
fn free_text_refuses_exactly_nul_cr_lf_and_lets_real_names_through() {
    for good in [
        "/home/u/Bob's notes",
        "/data/照片 (2019)",
        "a & b; c | d $x `y` *?!<>\"",
        "tab\tinside",
        "",
    ] {
        assert!(free_text_ok(good), "真实好值被拒了：{good:?}");
    }
    for bad in ["a\0b", "a\rb", "a\nb", "\n"] {
        assert!(!free_text_ok(bad), "坏值放行了：{bad:?}");
    }
    assert_eq!(FREE_TEXT_REFUSED, ['\0', '\r', '\n']);
}

/// 〔TL3 · `INVARIANTS §47` ②〕POSIX 自由文本路径：形式（绝对 · 无 `..` 段）＋ 拒绝集，**正反各一格**。
#[test]
fn a_posix_free_path_is_absolute_without_parent_segments_and_nothing_quote_cannot_hold() {
    for good in [
        "/",
        "/home/u/Bob's notes",
        "/data/照片 (2019)",
        "/a..b/c...",
        "/srv/a&b;c",
    ] {
        assert!(posix_free_path_ok(good), "真实好值被拒了：{good:?}");
    }
    for bad in [
        "", "rel", "~/x", "/a/../b", "/a/..", "../a", "/a\nb", "/a\rb", "/a\0b",
    ] {
        assert!(!posix_free_path_ok(bad), "坏值放行了：{bad:?}");
    }
}

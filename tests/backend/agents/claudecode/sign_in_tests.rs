//! Claude 登录那一形里纯的几样：认链接（只认那一家的登录页、要带 state）· 粘回来的 `码#state` 要对得上这一趟 · 认「码不对」那一行。
//! 链接 · 码都是写死的假串。
use super::*;

const LINK: &str = "https://claude.com/cai/oauth/authorize?code=true&client_id=fake&state=st8";

#[test]
fn only_the_sign_in_page_counts_as_the_link() {
    assert_eq!(
        link_in(&format!("If the browser didn't open, visit: {LINK}")).as_deref(),
        Some(LINK)
    );
    assert_eq!(link_in(LINK).as_deref(), Some(LINK));
    for not in [
        "Opening browser to sign in…",
        "visit: https://evil.example/cai/oauth/authorize?state=st8",
        "visit: https://claude.com.evil.example/oauth/authorize?state=st8",
        "visit: https://claude.com/settings?state=st8",
        "visit: https://claude.com/cai/oauth/authorize?code=true",
        "visit: http://claude.com/cai/oauth/authorize?state=st8",
    ] {
        assert_eq!(link_in(not), None, "{not}");
    }
}

#[test]
fn a_paste_must_carry_this_rounds_state() {
    assert!(paste_fits("abc-123_x#st8", LINK));
    for bad in [
        "abc#other",
        "abc",
        "#st8",
        "abc#",
        "a b#st8",
        "abc#st8#x",
        "abc;rm#st8",
    ] {
        assert!(!paste_fits(bad, LINK), "{bad}");
    }
}

#[test]
fn the_bad_code_line_is_recognised() {
    assert!(said_bad_paste(
        "Invalid code. Please make sure the full code was copied"
    ));
    assert!(!said_bad_paste("Login successful."));
}

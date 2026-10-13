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

// ── MCP 那一形（`claude mcp login <名> --no-browser`）：链接是那台 MCP 服务器自己的授权页，粘回来的是回跳到本机那一整条 ──

const MCP_LINK: &str = "https://auth.example.test/authorize?response_type=code&client_id=c1&redirect_uri=http%3A%2F%2Flocalhost%3A43117%2Fcallback&state=st9&code_challenge=x";

#[test]
fn the_mcp_link_is_the_servers_authorize_page_with_a_state_and_a_way_back() {
    assert_eq!(
        mcp_link_in(&format!("  {MCP_LINK}")).as_deref(),
        Some(MCP_LINK)
    );
    let local =
        "http://127.0.0.1:9/authorize?redirect_uri=http%3A%2F%2Flocalhost%3A1%2Fcallback&state=s";
    assert_eq!(
        mcp_link_in(local).as_deref(),
        Some(local),
        "本机开发用的服务器（http 只认本机）"
    );
    let connector = "https://claude.ai/settings/connectors?server=x";
    assert_eq!(
        mcp_link_in(connector).as_deref(),
        Some(connector),
        "claude.ai 连接器那条"
    );
    for not in [
        "Starting authentication for \"notes\"…",
        "https://auth.example.test/authorize?redirect_uri=http%3A%2F%2Flocalhost%3A1%2Fcallback",
        "https://auth.example.test/authorize?state=st9",
        "http://auth.example.test/authorize?redirect_uri=http%3A%2F%2Flocalhost%3A1%2Fcallback&state=s",
        "http://localhost.evil.test/authorize?redirect_uri=http%3A%2F%2Flocalhost%3A1%2Fcallback&state=s",
    ] {
        assert_eq!(mcp_link_in(not), None, "{not}");
    }
}

#[test]
fn a_mcp_paste_is_this_rounds_way_back_with_a_code() {
    assert!(mcp_paste_fits(
        "http://localhost:43117/callback?code=abc&state=st9",
        MCP_LINK
    ));
    assert!(mcp_paste_fits(
        "http://localhost:43117/callback?state=st9&code=a-b_c.d~e%2F",
        MCP_LINK
    ));
    for bad in [
        "http://localhost:43117/callback?code=abc&state=other",
        "http://localhost:43117/callback?state=st9",
        "http://localhost:43117/callback?code=&state=st9",
        "http://localhost:9999/callback?code=abc&state=st9",
        "http://localhost:43117/callbackx?code=abc&state=st9",
        "http://localhost:43117/callback?code=a b&state=st9",
        "http://localhost:43117/callback?code=a\u{1b}b&state=st9",
        "abc#st9",
    ] {
        assert!(!mcp_paste_fits(bad, MCP_LINK), "{bad}");
    }
    assert!(!mcp_paste_fits(
        "http://localhost:43117/callback?code=a&state=st9",
        "https://claude.ai/x"
    ));
}

#[test]
fn the_mcp_bad_paste_line_is_recognised_and_the_face_wants_a_terminal() {
    assert!(mcp_said_bad_paste(
        "That doesn't look like a redirect URL for this sign-in — paste the full address from the browser page this sign-in opened."
    ));
    assert!(!mcp_said_bad_paste(
        "Authenticated with \"notes\". Its tools are now available in Claude Code."
    ));
    assert!(MCP_FACE.terminal && !FACE.terminal);
    assert_eq!(MCP_FACE.args, &["mcp", "login", "--no-browser"]);
    assert_eq!(MCP_FACE.paste, crate::agents::PasteShape::RedirectUrl);
    assert_eq!(FACE.paste, crate::agents::PasteShape::Code);
}

//! Codex 登录那一形：`codex login` 的输出认链接、不收粘回 · `--device-auth` 认链接与码 · 成败与状态那几行（状态以退出码为准）。
//! 样例照 codex-rs rust-v0.167.0 打印的原样手写（各段出处写在用到它的那一处），不起真 codex。

use super::*;

const LINK: &str = "https://auth.openai.com/oauth/authorize?response_type=code&client_id=app_EMoamEEZ73f0CkXaXp7hrann&redirect_uri=http%3A%2F%2F127.0.0.1%3A1455%2Fauth%2Fcallback&scope=openid+profile+email+offline_access&code_challenge=c1&code_challenge_method=S256&id_token_add_organizations=true&codex_cli_simplified_flow=true&state=S7ate_x&originator=codex_cli_rs";

/// `codex login` 打在 stderr 上的那一段（`codex_cli::login::print_login_server_start`）。
fn browser_output() -> String {
    format!(
        "Starting local login server on http://localhost:1455.\nIf your browser did not open, navigate to this URL to authenticate:\n\n{LINK}\n\nOn a remote or headless machine? Use `codex login --device-auth` instead.\n"
    )
}

#[test]
fn the_browser_login_prints_the_authorize_link() {
    let found: Vec<String> = browser_output().lines().filter_map(link_in).collect();
    assert_eq!(found, vec![LINK.to_string()]);
    // 本机回调那一行、别家的授权页都不算。
    assert_eq!(
        link_in("Starting local login server on http://localhost:1455."),
        None
    );
    assert_eq!(
        link_in("https://evil.example/oauth/authorize?state=a&redirect_uri=x"),
        None
    );
    assert_eq!(
        link_in("https://auth.openai.com/oauth/authorize?client_id=x"),
        None
    );
}

#[test]
fn nothing_pasted_is_ever_taken() {
    // 不收粘回：这一形没有判粘的那一手（核心不往它 stdin 写）。
    assert!(FACE.paste.pasted().is_none());
}

/// `--device-auth` 打在 stdout 上的那一段（`codex_login::device_code_auth::device_code_prompt`，带它自己的 ANSI 颜色）。
const DEVICE: &str = "\nWelcome to Codex [v\x1b[90m0.167.0\x1b[0m]\n\x1b[90mOpenAI's command-line coding agent\x1b[0m\n\nFollow these steps to sign in with ChatGPT using device code authorization:\n\n1. Open this link in your browser and sign in to your account\n   \x1b[94mhttps://auth.openai.com/codex/device\x1b[0m\n\n2. Enter this one-time code \x1b[90m(expires in 15 minutes)\x1b[0m\n   \x1b[94mQ7KD-9PXM\x1b[0m\n\n\x1b[90mContinue only if you started this login in Codex. If a website or another person gave you this code, cancel.\x1b[0m\n";

#[test]
fn the_device_login_prints_a_link_and_a_code() {
    let lines: Vec<&str> = DEVICE.lines().collect();
    assert_eq!(
        device_prompt(&lines),
        Some(DeviceCode {
            link: "https://auth.openai.com/codex/device".into(),
            code: "Q7KD-9PXM".into(),
        })
    );
    // 少了码那一行 · 别的输出 ⇒ 认不出。
    assert_eq!(device_prompt(&lines[..10]), None);
    let other: Vec<String> = browser_output().lines().map(str::to_string).collect();
    let other: Vec<&str> = other.iter().map(String::as_str).collect();
    assert_eq!(device_prompt(&other), None);
}

#[test]
fn the_outcome_lines_are_recognised_and_anything_else_is_not() {
    assert_eq!(said("Successfully logged in"), Some(Said::SignedIn));
    assert_eq!(
        said("Error logging in: login was not completed"),
        Some(Said::SignInFailed)
    );
    assert_eq!(
        said("Error logging in with device code: device code expired"),
        Some(Said::SignInFailed)
    );
    assert_eq!(said("Successfully logged out"), Some(Said::SignedOut));
    assert_eq!(said("Not logged in"), Some(Said::NotSignedIn));
    assert_eq!(said("Logged in using ChatGPT"), Some(Said::SignedInChatgpt));
    assert_eq!(
        said("API key configured - sk-***"),
        Some(Said::SignedInOther)
    );
    assert_eq!(
        said("Logged in using personal access token"),
        Some(Said::SignedInOther)
    );
    assert_eq!(
        said("Starting local login server on http://localhost:1455."),
        None
    );
    assert_eq!(said(""), None);
}

#[test]
fn the_commands_run_under_the_account_codex_home() {
    assert_eq!((FACE.program, FACE.args), ("codex", &["login"][..]));
    assert_eq!(FACE.dir_env, "CODEX_HOME");
    assert_eq!(FACE.paste, crate::agents::PasteShape::NoPaste);
    assert_eq!((FACE.opens_browser, FACE.terminal), (true, false));
    assert_eq!(FACE.sign_out, Some(LOGOUT_ARGS));
    let status = FACE.status.expect("声明了状态那一条");
    assert_eq!(status.args, STATUS_ARGS);
    // codex-rs 0.167 的 `codex login status`：登着 ⇒ 那一行 ＋ 退 0；没登 ⇒ `Not logged in` ＋ 退 1；读坏 ⇒ `Error checking login status: …` ＋ 退 1。
    let read = |code: Option<i32>, l: &[&str]| {
        (status.read)(code, &l.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    };
    assert_eq!(read(Some(0), &["Logged in using ChatGPT"]), Some(true));
    assert_eq!(read(Some(0), &["API key configured - sk-…"]), Some(true));
    assert_eq!(read(Some(1), &["Not logged in"]), Some(false));
    // 退出码与那一行对不上 · 没退出码 · 认不出 ⇒ 说不清。
    assert_eq!(read(Some(1), &["Logged in using ChatGPT"]), None);
    assert_eq!(read(Some(0), &["Not logged in"]), None);
    assert_eq!(read(None, &["Logged in using ChatGPT"]), None);
    assert_eq!(read(Some(1), &["Error checking login status: boom"]), None);
    assert_eq!(read(Some(0), &["huh"]), None);
    assert_eq!(DEVICE_ARGS, &["login", "--device-auth"]);
    assert_eq!(LOGOUT_ARGS, &["logout"]);
    assert_eq!(STATUS_ARGS, &["login", "status"]);
    // 会盖掉那个号自己登录的那几格起它时摘掉。
    for k in ["CODEX_API_KEY", "OPENAI_API_KEY", "CODEX_ACCESS_TOKEN"] {
        assert!(FACE.env_remove.contains(&k), "{k}");
    }
}

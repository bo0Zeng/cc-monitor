//! Claude 登录一个号的那一形（`claude auth login --claudeai`，官方文档 cli-reference 有这条子命令，不要终端）。
//! 实测（2.1.296，假家目录里跑、拿到链接就杀）：stdout 依次打印 `Opening browser to sign in…` /
//! `If the browser didn't open, visit: <链接>` / `Paste code here if prompted > `；stdin 逐行读 `码#state`；
//! 码不对 ⇒ stderr `Invalid code…`、接着等；成 ⇒ `Login successful.`、退出码 0；不成 ⇒ stderr `Login failed: …`、退出码 1；
//! 关 stdin 它也不退（作业收的时候杀组）。这几行字不算约定：链接按白名单宽松地认，成败只看退出码。

use crate::agents::SignInFace;

/// 认得的登录页：订阅号那一家的几个域名 ＋ 路径里有 `/oauth/authorize`。
const HOSTS: &[&str] = &["claude.com", "claude.ai", "platform.claude.com"];
const AUTHORIZE: &str = "/oauth/authorize";

pub(crate) const FACE: SignInFace = SignInFace {
    program: "claude",
    args: &["auth", "login", "--claudeai"],
    dir_env: super::paths::CONFIG_DIR_ENV,
    // 会盖掉那个号自己的登录的那几格（同查用量那一形）。
    env_remove: &[
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "CLAUDE_CODE_OAUTH_TOKEN",
    ],
    link_in,
    paste_fits,
    said_bad_paste,
};

/// 一行里那条登录链接（`https://` 起、到空白为止）；不是认得的登录页 ⇒ `None`。
fn link_in(line: &str) -> Option<String> {
    let at = line.find("https://")?;
    let url: &str = line[at..].split_whitespace().next()?;
    let rest = &url["https://".len()..];
    let (host, path) = rest.split_at(rest.find('/')?);
    (HOSTS.contains(&host) && path.contains(AUTHORIZE) && state_of(url).is_some())
        .then(|| url.to_string())
}

/// 链接里 `state=` 那一格。
fn state_of(url: &str) -> Option<&str> {
    let q = url.split_once('?')?.1;
    q.split('&')
        .find_map(|kv| kv.strip_prefix("state="))
        .filter(|s| !s.is_empty())
}

/// 粘回来的是 `码#state`，`state` 要等于这一趟链接里那个；码只许 URL 安全的字。
fn paste_fits(pasted: &str, link: &str) -> bool {
    let safe = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~' | '%'))
    };
    match (pasted.split_once('#'), state_of(link)) {
        (Some((code, state)), Some(want)) => safe(code) && safe(state) && state == want,
        _ => false,
    }
}

/// 它说码不对的那一行。
fn said_bad_paste(line: &str) -> bool {
    line.contains("Invalid code")
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/sign_in_tests.rs"]
mod tests;

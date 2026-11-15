//! Codex 登录一个号的那几形：`codex login`（浏览器）· `codex login --device-auth`（显示码）· `codex logout` · `codex login status`。
//! 起它们时 `CODEX_HOME` 指那个号的目录（[`FACE`] 的 `dir_env`）；令牌由 Codex 自己写进那个号的 `auth.json`，后端不写。
//!
//! 依据（codex-rs，核到 tag `rust-v0.167.0`，只读源码、没起真 codex）：
//! - `codex login`：本机起回调服务（默认口 1455），stderr 打 `Starting local login server on http://localhost:1455.` /
//!   `If your browser did not open, navigate to this URL to authenticate:` / 空行 / 授权链接（`https://auth.openai.com/oauth/authorize?…`，
//!   带 `state` 与 `redirect_uri=http://127.0.0.1:1455/auth/callback`）/ 空行 / 指向 `--device-auth` 的那一句（`codex_cli::login::print_login_server_start` ·
//!   `codex_login::server`）。它自己开浏览器；浏览器回跳到本机那个口就完成，**不读 stdin**。
//! - `--device-auth`：stdout 打一段带 ANSI 颜色的提示，`1.` 下一行是链接（`<issuer>/codex/device`），`2. Enter this one-time code` 下一行是码
//!   （`codex_login::device_code_auth::device_code_prompt`）；不用粘回，人在浏览器里填码它就完成。
//! - 成 ⇒ `Successfully logged in`、退 0；不成 ⇒ `Error logging in…`（device 那一形 `Error logging in with device code: …`）、退 1。
//!   `logout`：`Successfully logged out` / `Not logged in`；`login status`：`Logged in using ChatGPT` · `API key configured - …` ·
//!   `Logged in using …` / `Not logged in`。这几行字不算约定：成败以退出码为准，这里只认来给人看。

use crate::agents::{PasteShape, SignInFace, SignInStatus};

/// 认得的登录页（默认签发方）。
const HOSTS: &[&str] = &["auth.openai.com"];
const AUTHORIZE: &str = "/oauth/authorize";
const DEVICE_PATH: &str = "/codex/device";

/// 登录一个号那一形（浏览器回调）：链接照 [`link_in`] 认；浏览器回跳到本机那个口由它自己收，不收粘回。
pub const FACE: SignInFace = SignInFace {
    program: "codex",
    args: &["login"],
    dir_env: super::parse::HOME_ENV,
    // 会盖掉那个号自己的登录、或把登录指到别处的那几格。
    env_remove: &[
        "CODEX_API_KEY",
        "OPENAI_API_KEY",
        "CODEX_ACCESS_TOKEN",
        "CODEX_REFRESH_TOKEN_URL_OVERRIDE",
        "CODEX_APP_SERVER_LOGIN_CLIENT_ID",
    ],
    link_in,
    terminal: false,
    paste: PasteShape::NoPaste,
    opens_browser: true,
    sign_out: Some(LOGOUT_ARGS),
    status: Some(SignInStatus {
        args: STATUS_ARGS,
        read: status_of,
    }),
};

/// `login status` 的退出码与它说的那一行 ⇒ 登没登：以退出码为准、那一行作印证（登着 ⇒ 退 0；没登 · 读坏 ⇒ 退 1）。
/// 两样对不上 · 没退出码 · 没说认得的那一行 ⇒ 读不出。
fn status_of(code: Option<i32>, lines: &[String]) -> Option<bool> {
    let says = lines.iter().find_map(|l| match said(l)? {
        Said::SignedInChatgpt | Said::SignedInOther => Some(true),
        Said::NotSignedIn => Some(false),
        _ => None,
    })?;
    (code? == if says { 0 } else { 1 }).then_some(says)
}

/// 显示码那一形的参数（链接与码照 [`device_prompt`] 认）。
pub const DEVICE_ARGS: &[&str] = &["login", "--device-auth"];
/// 退出登录。
pub const LOGOUT_ARGS: &[&str] = &["logout"];
/// 登录状态。
pub const STATUS_ARGS: &[&str] = &["login", "status"];

/// 去掉 ANSI 的 CSI 颜色序列（`ESC [ … 字母`）。
fn plain(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut it = line.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{1b}' && it.next_if_eq(&'[').is_some() {
            for d in it.by_ref() {
                if d.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// 一行里那条 `https://` 链接（到空白为止）与它的主机 · 路径。
fn url_in(line: &str) -> Option<(String, String, String)> {
    let line = plain(line);
    let at = line.find("https://")?;
    let url = line[at..].split_whitespace().next()?.to_string();
    let rest = &url["https://".len()..];
    let cut = rest.find(['/', '?']).unwrap_or(rest.len());
    let host = rest[..cut].to_string();
    let path = rest[cut..].split('?').next().unwrap_or("").to_string();
    Some((url, host, path))
}

/// 一行里那条授权链接：认得的登录页、路径是 `/oauth/authorize`、带 `state` 与 `redirect_uri`；别的 ⇒ `None`。
pub fn link_in(line: &str) -> Option<String> {
    let (url, host, path) = url_in(line)?;
    (HOSTS.contains(&host.as_str())
        && path == AUTHORIZE
        && query_of(&url, "state=").is_some()
        && query_of(&url, "redirect_uri=").is_some())
    .then_some(url)
}

/// 链接里 `键=` 那一格的值（原样，没解码）。
fn query_of<'a>(url: &'a str, key: &str) -> Option<&'a str> {
    let q = url.split_once('?')?.1;
    let q = q.split('#').next().unwrap_or(q);
    q.split('&')
        .find_map(|kv| kv.strip_prefix(key))
        .filter(|s| !s.is_empty())
}

/// 显示码那一形给出的链接与码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCode {
    pub link: String,
    pub code: String,
}

/// `--device-auth` 的输出（一行一项）⇒ 链接与码：`1.` 那一句之后头一条认得的 `/codex/device` 链接，
/// `2. Enter this one-time code` 之后头一个非空行就是码（只许字母数字与 `-`）。缺一样 ⇒ `None`。
pub fn device_prompt(lines: &[&str]) -> Option<DeviceCode> {
    let mut link = None;
    let mut code = None;
    let mut want_code = false;
    for l in lines {
        let p = plain(l);
        let t = p.trim();
        if want_code && !t.is_empty() {
            want_code = false;
            if t.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
                code = Some(t.to_string());
            }
            continue;
        }
        if t.starts_with("2. Enter this one-time code") {
            want_code = true;
            continue;
        }
        if link.is_none() {
            if let Some((url, host, path)) = url_in(t) {
                if HOSTS.contains(&host.as_str()) && path == DEVICE_PATH {
                    link = Some(url);
                }
            }
        }
    }
    Some(DeviceCode {
        link: link?,
        code: code?,
    })
}

/// 输出里认得的那几行说了什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Said {
    /// 登录成了。
    SignedIn,
    /// 登录没成。
    SignInFailed,
    /// 退出成了。
    SignedOut,
    /// 本来就没登录（`logout` · `login status` 都这么说）。
    NotSignedIn,
    /// `login status`：订阅号登录着。
    SignedInChatgpt,
    /// `login status`：别的登录形（API key · 访问令牌 …）。
    SignedInOther,
}

/// 一行输出 ⇒ 它说了什么；别的行 ⇒ `None`。
pub fn said(line: &str) -> Option<Said> {
    let p = plain(line);
    let t = p.trim();
    Some(match t {
        "Successfully logged in" => Said::SignedIn,
        "Successfully logged out" => Said::SignedOut,
        "Not logged in" => Said::NotSignedIn,
        "Logged in using ChatGPT" => Said::SignedInChatgpt,
        _ if t.starts_with("Error logging in") => Said::SignInFailed,
        _ if t.starts_with("Logged in using ") || t.starts_with("API key configured") => {
            Said::SignedInOther
        }
        _ => return None,
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/sign_in_tests.rs"]
mod tests;

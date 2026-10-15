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
    terminal: false,
    paste: crate::agents::PasteShape::Code,
    // 实测先打印 `Opening browser to sign in…`。
    opens_browser: true,
};

/// 登录一个 MCP 服务器那一形（`claude mcp login <名> --no-browser`，cli-reference 有这一行；核心在最后接上服务器名）。
/// 2.1.296 内嵌 JS（`mcpLoginHandler`）：stdin 不是终端就在要粘那一步放弃 ⇒ 起在伪终端里。stdout 依次
/// `Starting authentication for "<名>"…` / `Visit this URL to authorize:` / 两格缩进的链接（终端里包成 OSC 8 超链接）/
/// `Waiting for authorization… (^C to cancel)` / 提示符 `Or paste the redirect URL here: `；粘的不对 ⇒
/// `That doesn't look like a redirect URL for this sign-in — …`、接着等；成 ⇒ `Authenticated with "<名>". …`、退 0、
/// 自己删掉需登录缓存里那一条；不成 ⇒ `Couldn't complete authentication for "<名>": …`、退非 0。本机回调口照开
/// （浏览器在同一台 ⇒ 不用粘就完成）。claude.ai 连接器只打印 claude.ai 的链接、退 0（不用粘）。
pub(crate) const MCP_FACE: SignInFace = SignInFace {
    program: "claude",
    args: &["mcp", "login", "--no-browser"],
    dir_env: super::paths::CONFIG_DIR_ENV,
    env_remove: FACE.env_remove,
    link_in: mcp_link_in,
    paste_fits: mcp_paste_fits,
    said_bad_paste: mcp_said_bad_paste,
    terminal: true,
    paste: crate::agents::PasteShape::RedirectUrl,
    opens_browser: false,
};

/// 一行里那条 MCP 授权链接：那台服务器自己的授权页（`https://`；`http://` 只认本机），带 `state` 与 `redirect_uri`；
/// 或 claude.ai 连接器那条（订阅号那几个域名）。
fn mcp_link_in(line: &str) -> Option<String> {
    let at = line.find("https://").or_else(|| line.find("http://"))?;
    let url: &str = line[at..].split_whitespace().next()?;
    let (scheme, rest) = url.split_once("://")?;
    let host = rest.split(['/', '?']).next()?;
    let name = host.rsplit_once(':').map_or(host, |(h, _)| h);
    if scheme == "https" && HOSTS.contains(&name) {
        return Some(url.to_string());
    }
    let local = matches!(name, "localhost" | "127.0.0.1" | "[::1]");
    if scheme == "http" && !local {
        return None;
    }
    (query_of(url, "state=").is_some() && query_of(url, "redirect_uri=").is_some())
        .then(|| url.to_string())
}

/// 链接里 `键=` 那一格的值（`key` 连等号一起给；原样，没解码）。
fn query_of<'a>(url: &'a str, key: &str) -> Option<&'a str> {
    let q = url.split_once('?')?.1;
    let q = q.split('#').next().unwrap_or(q);
    q.split('&')
        .find_map(|kv| kv.strip_prefix(key))
        .filter(|s| !s.is_empty())
}

/// `%XX` 解码（只给 `redirect_uri` 那一格用；坏的 ⇒ `None`）。
fn percent_decoded(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// 粘回来的是这一趟的回跳地址：以链接里的 `redirect_uri` 开头、紧跟 `?`，`state` 对得上、带 `code`；只有可见 ASCII、没有空白。
fn mcp_paste_fits(pasted: &str, link: &str) -> bool {
    if pasted.is_empty() || !pasted.bytes().all(|c| c.is_ascii_graphic()) {
        return false;
    }
    let (Some(back), Some(state)) = (
        query_of(link, "redirect_uri=").and_then(percent_decoded),
        query_of(link, "state="),
    ) else {
        return false;
    };
    let Some(rest) = pasted.strip_prefix(back.as_str()) else {
        return false;
    };
    rest.starts_with('?')
        && query_of(pasted, "state=") == Some(state)
        && query_of(pasted, "code=").is_some()
}

/// 它说粘的不是这一趟的回跳地址的那一行。
fn mcp_said_bad_paste(line: &str) -> bool {
    line.contains("doesn't look like a redirect URL")
}

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

//! Codex 经中转那一份：默认上游 · 会话头 · 怎么把它指到中转 · 直接敲的 Codex 那份配置里的地址。
//!
//! - Codex（0.159）改上游地址只认配置键（`openai_base_url` · 自定义 provider 的 `base_url`），不认环境变量 ⇒
//!   ccm 起它时用 `-c` 定义一个自家 provider：`base_url` 指中转（**不带**钥匙段）、照旧用 ChatGPT / API key 登录、
//!   钥匙经 `env_http_headers` 从环境变量 `relay_route_core::KEY_ENV` 带进钥匙头（钥匙不进 argv）。
//!   自家 provider 直接发 HTTP SSE（不先试 WebSocket）。
//! - 同一个地址两种登录都用 ⇒ 真上游按这一发带没带 `ChatGPT-Account-ID`（只有 ChatGPT 登录带）二选一。
//! - 直接敲的 Codex（用户自己往配置里贴 `openai_base_url`）走内置 provider：先试 WebSocket，中转回 426 后改发 HTTP SSE。

use crate::agents::{SettingsBaseUrl, SettingsEnvFace, SettingsUnreadable};
use std::path::{Path, PathBuf};

/// 这一家的默认上游那一格（注册表 `Adapter.upstream`）。
pub(crate) const UPSTREAM: crate::agents::DefaultUpstream = crate::agents::DefaultUpstream {
    route_id: super::AGENT_KIND,
    env: "CCM_AGENT_UPSTREAM_CODEX",
    fallback: crate::agents::Fallback::ByHeader {
        header: "ChatGPT-Account-ID",
        present: "https://chatgpt.com/backend-api/codex",
        absent: "https://api.openai.com/v1",
    },
    // 每一发都带：值 ＝ 根会话 id（＝ rollout 文件名里那个 id）；子 agent 的请求也是根的。
    session_header: Some("session-id"),
    stream: Some(crate::agents::sse_openai_responses::FACE),
    // 本线程 id：子 agent ＝ 它自己的；主运行也带、值等于 `session-id`（通用层据此归主运行）。
    owner_header: Some("thread-id"),
    // API key 凭据文件没有 agent 这一维，行都归 claude-code ⇒ `/s/` 代入对这一家照旧拒。
    owns_credentials_file: false,
    settings_env: Some(SETTINGS_ENV),
    context_mark: None,
    // 回包里没有这一家的额度头 ⇒ 不记。
    quota: None,
    window_slot: None,
    window_key: None,
    usage: None,
    login: None,
    // 没有这一家认得的「用满」回包 ⇒ 硬上限对它不成立。
    limit_reply: None,
    inject: crate::agents::Inject::Args(launch_args),
};

/// ccm 起 Codex 时定义的那个自家 provider 的 id（会话记录的 `model_provider` 会记成它）。
const PROVIDER: &str = "ccm";

/// 不带钥匙的中转地址 ⇒ 垫在透传之前的 `-c` 那几组（值是 TOML：串带引号、布尔不带）。
pub(crate) fn launch_args(url: &str) -> Vec<String> {
    let p = format!("model_providers.{PROVIDER}");
    [
        format!("model_provider=\"{PROVIDER}\""),
        format!("{p}.name=\"cc-monitor\""),
        format!("{p}.base_url=\"{url}\""),
        format!("{p}.wire_api=\"responses\""),
        format!("{p}.requires_openai_auth=true"),
        format!(
            "{p}.env_http_headers.{}=\"{}\"",
            relay_route_core::KEY_HEADER,
            relay_route_core::KEY_ENV
        ),
    ]
    .into_iter()
    .flat_map(|kv| ["-c".to_string(), kv])
    .collect()
}

/// 直接敲的 Codex 也走中转：`~/.codex/config.toml` 顶层的 `openai_base_url`（只读）。
pub(crate) const SETTINGS_ENV: SettingsEnvFace = SettingsEnvFace {
    read: read_config_base_url,
    snippet: config_snippet,
};

/// 读那份配置的上限（几 KB；超了按「读不了」说，不当没写）。
const CONFIG_CAP_BYTES: u64 = 1 << 20;

/// 顶层那个键。
const KEY: &str = "openai_base_url";

/// `$HOME/.codex/config.toml`（缺省的家；另设 `CODEX_HOME` 的号各有各的那一份，归多账号那一维）＋ 顶层那一格。**只读**。
fn read_config_base_url(home: &Path) -> (PathBuf, SettingsBaseUrl) {
    let file = home.join(".codex").join("config.toml");
    let raw = match std::fs::metadata(&file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (file, SettingsBaseUrl::Unset)
        }
        Err(e) => Err(SettingsUnreadable::Io(e.to_string())),
        Ok(m) if !m.is_file() => Err(SettingsUnreadable::NotFile),
        Ok(m) if m.len() > CONFIG_CAP_BYTES => {
            let error = SettingsUnreadable::TooLarge(CONFIG_CAP_BYTES);
            Err(error)
        }
        Ok(_) => std::fs::read_to_string(&file).map_err(|e| SettingsUnreadable::Io(e.to_string())),
    };
    let found = match raw {
        Ok(text) => config_base_url(&text),
        Err(why) => SettingsBaseUrl::Unreadable(why),
    };
    (file, found)
}

/// 原文 → 顶层（第一个表头之前）`openai_base_url` 的值。只认单行的基本串 / 字面串；空串当没写；
/// 值不是串、串没收尾、同一个键写了两次 ⇒ 读不懂（不猜成没写）。
pub(crate) fn config_base_url(raw: &str) -> SettingsBaseUrl {
    let bad = SettingsBaseUrl::Unreadable(SettingsUnreadable::BadShape);
    let mut found: Option<String> = None;
    for line in raw.trim_start_matches('\u{feff}').lines() {
        let t = line.trim();
        if t.starts_with('[') {
            break;
        }
        let Some((k, v)) = t.split_once('=') else {
            continue;
        };
        let k = k.trim();
        let quoted = |q: char| k.strip_prefix(q).and_then(|r| r.strip_suffix(q)) == Some(KEY);
        if k != KEY && !quoted('"') && !quoted(SQ) {
            continue;
        }
        let Some(value) = toml_line_string(v.trim()) else {
            return bad;
        };
        if found.replace(value).is_some() {
            return bad;
        }
    }
    match found {
        Some(u) if !u.is_empty() => SettingsBaseUrl::Set(u),
        _ => SettingsBaseUrl::Unset,
    }
}

/// TOML 字面串的引号。
const SQ: char = '\'';

/// 一行里 `=` 右边那一截 ⇒ 单行串的值（后面只许空白或注释）。认不出 ⇒ `None`。
fn toml_line_string(v: &str) -> Option<String> {
    let rest_ok = |rest: &str| {
        let r = rest.trim();
        r.is_empty() || r.starts_with('#')
    };
    if let Some(body) = v.strip_prefix(SQ) {
        if body.starts_with(SQ) {
            return None;
        }
        let end = body.find(SQ)?;
        return rest_ok(&body[end + 1..]).then(|| body[..end].to_string());
    }
    let body = v.strip_prefix('"')?;
    if body.starts_with("\"\"") {
        return None;
    }
    let mut out = String::new();
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return rest_ok(&body[i + 1..]).then_some(out),
            '\\' => match chars.next()?.1 {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                't' => out.push('\t'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                _ => return None,
            },
            c => out.push(c),
        }
    }
    None
}

/// 要合并进那份配置**顶层**（第一个表头之前）的那一行。
fn config_snippet(url: &str) -> String {
    format!("{KEY} = \"{url}\"")
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/relay_tests.rs"]
mod tests;

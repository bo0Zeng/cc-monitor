//! Codex 经中转那一份：默认上游 · 会话头 · 怎么把它指到中转 · 直接敲的 Codex 那份配置里的地址。
//!
//! - Codex（0.159）改上游地址只认配置键 `openai_base_url`，不认环境变量 ⇒ ccm 起它时垫 `-c openai_base_url="<地址>"`，
//!   与直接敲的贴进 `config.toml` 顶层那一行同一个键、同一条地址。地址里插的是**只许直通那一把**钥匙
//!   （进 argv、也会被 Codex 写进它自己的日志；那一把开不了 `/s/`，见 `comms_outward::door`）。
//! - 不另定义 provider：自家 provider 会让会话记录里的 provider 换成它，而 Codex 的选会话列表 · `--last` ·
//!   app-server 的会话清单都按「当前 provider」过滤（真 codex 实测：经不经中转起的会话分成两拨）。
//! - 内置 provider 先试 WebSocket，中转回 426 后同一轮改发 HTTP SSE。
//! - 同一个地址两种登录都用 ⇒ 真上游按这一发带没带 `ChatGPT-Account-ID`（只有 ChatGPT 登录带）二选一。

use crate::agents::{SettingsBaseUrl, SettingsEnvFace, SettingsUnreadable};
use crate::platform::fs::Ro;
use cap_core::{Cap, Over};
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
    // 请求说不出这一发是不是对话（每一发都当对话）、也说不出扩展上下文 ⇒ 只这一个。
    relay_heads: &[crate::agents::RelayHead::Run("thread-id")],
    relay_env: &[],
    // API key 凭据文件没有 agent 这一维，行都归 claude-code ⇒ `/s/` 代入对这一家照旧拒。
    owns_credentials_file: false,
    settings_env: Some(SETTINGS_ENV),
    // 回包头 `x-codex-primary-*` · `x-codex-secondary-*` 那一族（两窗按时长起名，`300m` ⇒ 5h · `10080m` ⇒ 7d）。
    quota: Some(super::parse::quota_of_headers),
    window_slot: Some(super::parse::slot_of),
    window_key: Some(super::parse::key_of),
    usage: Some(super::usage::FACE),
    warm: Some(super::usage::WARM),
    // 换号时读的那份登录 ＝ 账号面那一份（同一个凭据文件 · 同一段；账号 0 不续不写）。
    login: Some(super::accounts::LOGIN),
    // 换到别的号：`Authorization: Bearer <那个号的访问令牌>` 与 `ChatGPT-Account-ID: <那个号的 account_id>`（codex-rs `model-provider/src/auth.rs`）。
    swap_heads: &[
        crate::agents::SwapHead {
            name: "Authorization",
            prefix: "Bearer ",
            cell: crate::agents::CredCell::Access,
        },
        crate::agents::SwapHead {
            name: "ChatGPT-Account-ID",
            prefix: "",
            cell: crate::agents::CredCell::Field("account_id"),
        },
    ],
    // 不能预定会话号（没有那个旗标）⇒ 中转按第一发带的来处绑号。
    bind: crate::agents::Bind::Origin,
    // 用满那一发照 Codex 认的样子回（[`limit_reply`]）。
    limit_reply: Some(limit_reply),
    resumes_within: None,
    inject: crate::agents::Inject::Args(launch_args),
    // 照常见 HTTP 客户端的读法：另认 `ALL_PROXY`。
    proxy: crate::agents::ProxyFace {
        via: &[
            "https_proxy",
            "HTTPS_PROXY",
            "http_proxy",
            "HTTP_PROXY",
            "all_proxy",
            "ALL_PROXY",
        ],
        skip: &["no_proxy", "NO_PROXY"],
    },
};

/// 中转地址 ⇒ 垫在透传之前的那一组 `-c`（值是 TOML 串）。计划里拿到的是不带钥匙的地址，`ccm` exec 那一刻插上钥匙。
/// 说不出卡在哪个窗口时，重置时刻离此刻多远以内标 5 小时那一窗（更远标一周那一窗）。
const FIVE_HOURS_SECS: u64 = 5 * 3600;
/// 两窗的分钟数（`limit_window_minutes` 那一格；同 Codex 报的时长）。
const FIVE_HOURS_MIN: u64 = 300;
const WEEK_MIN: u64 = 7 * 24 * 60;

/// 「用满」回包（注册表 `DefaultUpstream.limit_reply`）：照 Codex 认的那一形 —— codex-rs `rust-v0.167.0`
/// `codex_api::api_bridge`：429、体 `{"error":{"type":"usage_limit_reached","resets_at":<unix 秒>,"limit_window_minutes":<分>}}`
/// ⇒ `UsageLimitReached`，Codex 照它原生的样子说「用满 · 几点重置」。体里不放 `code`（带了会先被别的分支截走）；
/// 卡着的窗口（`key`：`5h` · `7d`）照它的分钟数，说不出 ⇒ 按离重置远近标（只关显示用词，重置时刻照真的）。
fn limit_reply(reset_at: u64, now: u64, key: Option<&str>) -> crate::agents::LimitReply {
    let near = reset_at.saturating_sub(now) <= FIVE_HOURS_SECS;
    let minutes = match key {
        Some(k) if k == cmd_wire::quota::SLOT_5H => FIVE_HOURS_MIN,
        Some(k) if k == cmd_wire::quota::SLOT_7D => WEEK_MIN,
        _ if near => FIVE_HOURS_MIN,
        _ => WEEK_MIN,
    };
    let body = serde_json::json!({
        "error": {
            "type": "usage_limit_reached",
            "message": "usage limit reached",
            "resets_at": reset_at,
            "limit_window_minutes": minutes,
        }
    });
    crate::agents::LimitReply {
        status: "429 Too Many Requests",
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: body.to_string().into_bytes(),
    }
}

pub(crate) fn launch_args(url: &str) -> Vec<String> {
    vec!["-c".to_string(), format!("{KEY}=\"{url}\"")]
}

/// 直接敲的 Codex 也走中转：`~/.codex/config.toml` 顶层的 `openai_base_url`（只读）。
pub const SETTINGS_ENV: SettingsEnvFace = SettingsEnvFace {
    read: read_config_base_url,
    merge: config_merge,
    slot: KEY,
    // 项目级那一层不看（Codex 的项目配置这里没接）。
    covered_in: |_| None,
};

/// 读那份配置的上限（几 KB）：超了按「读不了」说（结果里带着为什么），不当没写。
const CONFIG_CAP_BYTES: Cap = Cap {
    bytes: 1 << 20,
    over: Over::DegradeAndSay,
};

/// 顶层那个键。
const KEY: &str = "openai_base_url";

/// `$HOME/.codex/config.toml`（缺省的家；另设 `CODEX_HOME` 的号各有各的那一份，归多账号那一维）＋ 顶层那一格。**只读**。
fn read_config_base_url(home: &Path) -> (PathBuf, SettingsBaseUrl) {
    let file = home.join(".codex").join("config.toml");
    let raw = match Ro::metadata(&file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (file, SettingsBaseUrl::Unset)
        }
        Err(e) => Err(SettingsUnreadable::Io(e.to_string())),
        Ok(m) if !m.is_file() => Err(SettingsUnreadable::NotFile),
        Ok(m) if m.len() > CONFIG_CAP_BYTES.bytes => {
            let error = SettingsUnreadable::TooLarge(CONFIG_CAP_BYTES.bytes);
            Err(error)
        }
        Ok(_) => Ro::read_string_capped(&file, CONFIG_CAP_BYTES)
            .map_err(|e| SettingsUnreadable::Io(e.raw.unwrap_or(e.said))),
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

/// 那份配置现在的内容 ＋ 地址 ⇒ 合好的整份（只算不写）：顶层已有那一行 ⇒ 就地换掉那一行；没有 ⇒ 放到最前面
/// （表头之前才是顶层）。别的行原样。现在的内容读不懂 ⇒ `None`；合好的那份再读一遍不是这条地址 ⇒ `None`。
pub(crate) fn config_merge(now: &str, url: &str) -> Option<String> {
    if matches!(config_base_url(now), SettingsBaseUrl::Unreadable(_)) {
        return None;
    }
    let line = config_snippet(url);
    let mut hit = false;
    let mut out: Vec<String> = Vec::new();
    let mut top = true;
    for l in now.lines() {
        let t = l.trim().trim_start_matches('\u{feff}');
        top &= !t.starts_with('[');
        let key = t
            .split_once('=')
            .map(|(k, _)| k.trim().trim_matches(['"', SQ]));
        if top && key == Some(KEY) {
            out.push(line.clone());
            hit = true;
        } else {
            out.push(l.to_string());
        }
    }
    if !hit {
        out.insert(0, line);
    }
    let whole = out.join("\n") + "\n";
    (config_base_url(&whole) == SettingsBaseUrl::Set(url.to_string())).then_some(whole)
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/relay_tests.rs"]
mod tests;

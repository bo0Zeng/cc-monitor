//! Claude 的**账号文件**：`.claude.json` 的信任判定 · 登录邮箱 · 「一个身份由哪几份文件组成」那张表。
//!
//! ⚠ 只装 Claude Code 自己的布局与格式。**账号清单（manifest）与配置目录白名单不在这里** ——
//! 那是账号库的格式（`accounts/manage/model.rs` 读写、`observe/accounts_query.rs` 只读）。

use crate::agents::{AccountsFace, IdentityClass, IdentityRoot};
use crate::common::fs::read_regular_capped;
use std::path::Path;

/// 账号级配置文件的文件名（住在配置根下）。
pub(crate) const CONFIG_FILE_NAME: &str = ".claude.json";

/// Claude Code 把「你是谁」与「你的本机状态」放在哪几份文件里。账号库里每个号各有一份的就是这几项，
/// 其余顶层项都链回共享的配置根。Claude Code 换了文件名或位置 ⇒ 只改这张表。
///
/// 收哪些：Claude Code 在每个配置根里各写一份、共享会串号或互相覆盖的 —— 登录身份、按这个号从服务端取回的
/// 设置与限额（连同记着「是给哪个身份取的」那份戳）、这个号的 MCP 授权缓存与用量、各自的清理 / 更新记录、
/// 状态目录（同意记录之类）与反馈草稿。判据拿一份号目录的结构（名字与类型）与本表两向相等。
pub(crate) const NATIVE_IDENTITY: &[(&str, IdentityRoot, IdentityClass)] = &[
    (
        acct_core::CREDENTIALS_NAME,
        IdentityRoot::ConfigDir,
        IdentityClass::Secret,
    ),
    (CONFIG_FILE_NAME, IdentityRoot::Home, IdentityClass::Secret),
    ("backups", IdentityRoot::ConfigDir, IdentityClass::Derived),
    (
        "policy-limits.json",
        IdentityRoot::ConfigDir,
        IdentityClass::State,
    ),
    (
        "policy-limits.json.stamp.json",
        IdentityRoot::ConfigDir,
        IdentityClass::Derived,
    ),
    (
        "remote-settings.json",
        IdentityRoot::ConfigDir,
        IdentityClass::State,
    ),
    (
        "mcp-needs-auth-cache.json",
        IdentityRoot::ConfigDir,
        IdentityClass::State,
    ),
    (
        "stats-cache.json",
        IdentityRoot::ConfigDir,
        IdentityClass::State,
    ),
    (
        ".last-cleanup",
        IdentityRoot::ConfigDir,
        IdentityClass::State,
    ),
    (
        ".last-update-result.json",
        IdentityRoot::ConfigDir,
        IdentityClass::State,
    ),
    ("state", IdentityRoot::ConfigDir, IdentityClass::State),
    ("feedback", IdentityRoot::ConfigDir, IdentityClass::State),
];

/// 注册表里 Claude 那一行的账号库布局（`agents::Adapter::accounts`）。
pub(crate) const FACE: AccountsFace = AccountsFace {
    identity: NATIVE_IDENTITY,
    config_file: CONFIG_FILE_NAME,
    user_mcp_key: super::assets::SERVERS_KEY,
    shared_root: shared_root_in,
    email_in: |root| oauth_email_in(&config_path_in(root)),
};

/// 没设 `CLAUDE_CONFIG_DIR` 时的配置根（家目录下），也就是各号链回去的那个共享库。
pub(crate) fn shared_root_in(home: &Path) -> std::path::PathBuf {
    home.join(super::paths::HOME_DIR_NAME)
}

/// 一份 `.claude.json` 里登录的是哪个邮箱（`oauthAccount.emailAddress`）。只取这一格；读不了 / 没有 ⇒ `None`。
pub(crate) fn oauth_email_in(p: &Path) -> Option<String> {
    let bytes = read_regular_capped(p, MAX_CONFIG_BYTES).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("oauthAccount")?
        .get("emailAddress")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 读取上限。这份文件会被 MCP 配置撑大，给 32MB。
/// 安全：`read_regular_capped` 的 `is_file` 挡 FIFO/设备（审计实测 symlink→`/dev/zero`
/// 6 秒涨 11GB），`take` 限量。
pub(crate) const MAX_CONFIG_BYTES: u64 = 32 * 1024 * 1024;

/// 某个根目录下的配置文件路径。
///
/// ⚠ 有它才能让「账号 0 的配置必须来自 `$HOME`」那条自省判据继续钉得住：
/// 原来那条断言比的是源码里出不出现 `home.join(".claude.json")` 这个**字面量**，
/// 而字面量随本件搬进了适配层。⇒ 判据的比对对象换成这个函数名，性质不变。
pub(crate) fn config_path_in(root: &std::path::Path) -> std::path::PathBuf {
    root.join(CONFIG_FILE_NAME)
}

/// 某个配置根下、对某个 cwd 的**信任状态** → 一行 JSON（`{trusted, known, error}`）。
///
/// `S3` 从 `accounts_query::trust_of_claude_json` 原样搬来（逻辑一字未改）。
///
/// ⚠ **只出这三个字段**：`.claude.json` 里有 `mcpServers` 的环境变量（可能含 API key），
/// 整份吐出去就是把密钥送上 wire。
pub(crate) fn trust_of_config(p: &Path, cwd: &str) -> Result<String, (String, String)> {
    if !p.exists() {
        // 该账号还没有配置文件（全新账号）→ 肯定没信任过，不是错误
        return Ok(
            serde_json::json!({"trusted": false, "known": false, "error": serde_json::Value::Null})
                .to_string(),
        );
    }
    let bytes = read_regular_capped(p, MAX_CONFIG_BYTES)
        .map_err(|e| ("claude_json_unreadable".to_string(), e))?;
    let v: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| ("claude_json_invalid".to_string(), e.to_string()))?;
    let entry = v.get("projects").and_then(|p| p.get(cwd));
    let known = entry.is_some();
    let trusted = entry
        .and_then(|e| e.get("hasTrustDialogAccepted"))
        .and_then(|b| b.as_bool())
        .unwrap_or(false);
    Ok(
        serde_json::json!({"trusted": trusted, "known": known, "error": serde_json::Value::Null})
            .to_string(),
    )
}

//! Claude 的**账号文件**：`.claude.json` 的信任判定 · 登录邮箱 · 「一个身份由哪几份文件组成」那张表。
//!
//! ⚠ 只装 Claude Code 自己的布局与格式。**账号清单（manifest）与配置目录白名单不在这里** ——
//! 那是账号库的格式（`accounts/manage/model.rs` 读写、`observe/accounts_query.rs` 只读）。

use crate::agents::{AccountsFace, IdentityCell, IdentityClass, IdentityRoot, TrustCells};
use crate::common::fs::read_regular_capped;
use crate::common::said::IntoNote as _;
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
        super::mcp::NEEDS_AUTH_CACHE,
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
    watched: &[super::paths::SESSIONS_DIR, super::paths::PROJECTS_DIR],
    session_env: super::paths::SESSION_ENV_KEYS,
    trust_in: |root, cwd| trust_of_config(&config_path_in(root), cwd),
    trust: Some(TRUST_CELLS),
};

/// `.claude.json` 里信任记在 `projects[<目录>].hasTrustDialogAccepted`。
pub(crate) const TRUST_CELLS: TrustCells = TrustCells {
    table: "projects",
    flag: Some("hasTrustDialogAccepted"),
    dir_keys: trust_dir_keys,
};

/// Claude 查 / 存信任用的项目键：工作目录所在 git 仓的根（往上最近一个含 `.git` 的祖先，含自己；`.git` 是目录或文件
/// —— worktree —— 都算）；不在仓里 ⇒ 工作目录本身。预标两格都标（工作目录本身 ＋ 仓的根），不判它先查哪一格。
/// 每一格都是 [`trust_dir_key`] 那一形。
pub(crate) fn trust_dir_keys(cwd: &str) -> Vec<String> {
    let here = trust_dir_key(cwd);
    let mut keys = vec![here.clone()];
    let start = if cfg!(windows) { cwd.to_string() } else { here };
    if let Some(root) = std::path::Path::new(&start)
        .ancestors()
        .find(|p| p.join(".git").exists())
        .and_then(|p| p.to_str())
    {
        let root = trust_dir_key(root);
        if !keys.contains(&root) {
            keys.push(root);
        }
    }
    keys
}

/// 一个目录在 `.claude.json` 里的那一形：POSIX 上是解开符号链接之后的那一形；Windows 上分隔符换成 `/`。
/// 目录此刻解不开（不在）⇒ 原样。
pub(crate) fn trust_dir_key(cwd: &str) -> String {
    if cfg!(windows) {
        return cwd.replace('\\', "/");
    }
    std::fs::canonicalize(cwd)
        .ok()
        .and_then(|p| p.to_str().map(str::to_string))
        .unwrap_or_else(|| cwd.to_string())
}

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

/// 一个号的账号身份住哪：配置目录里那份 `.claude.json`；账号 0 的在家目录下（给了 `base_home`）。
pub(crate) fn identity_file(config_dir: &Path, base_home: Option<&Path>) -> std::path::PathBuf {
    config_path_in(base_home.unwrap_or(config_dir))
}

/// 那份文件里的账号身份：`oauthAccount.accountUuid`（请求体 `metadata.user_id` 里 `account_uuid` 那一格的值）。
/// 读不了 / 没有 / 不像一个 id ⇒ `None`。
pub(crate) fn identity_in(p: &Path) -> Option<String> {
    let bytes = read_regular_capped(p, MAX_CONFIG_BYTES).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("oauthAccount")?
        .get("accountUuid")?
        .as_str()
        .filter(|s| !s.is_empty() && identity_chars_ok(s))
        .map(str::to_string)
}

fn identity_chars_ok(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn user_id_identity(body: &serde_json::Value) -> Option<Option<String>> {
    let Some(raw) = body.get("metadata").and_then(|m| m.get("user_id")) else {
        return Some(None);
    };
    let inner: serde_json::Value = serde_json::from_str(raw.as_str()?).ok()?;
    match inner.get("account_uuid") {
        None => Some(None),
        Some(v) => v.as_str().map(|s| Some(s.to_string())),
    }
}

/// 请求体里账号身份那一格（`metadata.user_id` 这段 JSON 串里的 `account_uuid`）换成 `uuid`：只换那几个字节，别的一个字节不动。
/// 没有这一格 ⇒ [`IdentityCell::Absent`]；那几个字节在整份里不是恰好一处、或换完读回来对不上 ⇒ [`IdentityCell::Unsure`]。
pub(crate) fn rewrite_identity(body: &[u8], uuid: &str) -> IdentityCell {
    if !identity_chars_ok(uuid) {
        return IdentityCell::Unsure;
    }
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return IdentityCell::Absent;
    };
    let old = match user_id_identity(&v) {
        Some(None) => return IdentityCell::Absent,
        Some(Some(old)) => old,
        None => return IdentityCell::Unsure,
    };
    let cell = |u: &str| format!("\\\"account_uuid\\\":\\\"{u}\\\"");
    let Ok(text) = std::str::from_utf8(body) else {
        return IdentityCell::Unsure;
    };
    let needle = cell(&old);
    if text.matches(&needle).count() != 1 {
        return IdentityCell::Unsure;
    }
    let out = text.replacen(&needle, &cell(uuid), 1).into_bytes();
    let back = serde_json::from_slice::<serde_json::Value>(&out)
        .ok()
        .and_then(|v| user_id_identity(&v));
    if back != Some(Some(uuid.to_string())) {
        return IdentityCell::Unsure;
    }
    IdentityCell::Rewritten(out)
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
        .map_err(|e| ("claude_json_unreadable".to_string(), e.into_note()))?;
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

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/accounts_tests.rs"]
mod tests;

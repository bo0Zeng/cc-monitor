//! Claude 的**账号文件**：`.claude.json` 里信任记在哪一格 · 登录邮箱 · 「一个身份由哪几份文件组成」那张表。
//!
//! ⚠ 只装 Claude Code 自己的布局与格式。**账号清单（manifest）与配置目录白名单不在这里** ——
//! 那是账号库的格式（`accounts/manage/model.rs` 读写、`accounts/manage/library.rs` 只读）。

use crate::agents::{AccountsFace, IdentityCell, IdentityClass, IdentityRoot, TrustCells};
use crate::platform::fs::Ro;
use cap_core::{Cap, Over};
use std::path::Path;

/// 账号级配置文件的文件名（住在配置根下）。
pub(crate) const CONFIG_FILE_NAME: &str = ".claude.json";

/// Claude Code 把「你是谁」与「你的本机状态」放在哪几份文件里。账号库里每个号各有一份的就是这几项；
/// 链回共享配置根的是 [`NATIVE_SHARED`] 与 [`SHARED_ON_DEMAND`] 两张表里的。Claude Code 换了文件名或位置 ⇒ 只改这张表。
///
/// 收哪些：Claude Code 在每个配置根里各写一份、共享会串号或互相覆盖的 —— 登录身份、按这个号从服务端取回的
/// 设置与限额（连同记着「是给哪个身份取的」那份戳、远端设置上次同步的时刻）、这个号的 MCP 授权缓存与用量、各自的清理 / 更新记录、
/// 状态目录（同意记录之类）与反馈草稿、还没发出去的遥测事件（`telemetry/`：发不出去先落盘、下回起来用这个号的登录补发，
/// 共用就会拿别的号补发）。判据拿一份号目录的结构（名字与类型）与本表两向相等。
pub(crate) const NATIVE_IDENTITY: &[(&str, IdentityRoot, IdentityClass)] = &[
    (
        super::CREDENTIALS_NAME,
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
        "remote-settings.json.synced-at",
        IdentityRoot::ConfigDir,
        IdentityClass::Derived,
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
    ("telemetry", IdentityRoot::ConfigDir, IdentityClass::State),
];

/// Claude Code 按需才在配置根下建、各号该共用的目录：用户自建的子代理（`agents/`，同 `skills/` 是用户自己的配置）·
/// 计划模式写的计划（`plans/`：会话记录共用，换个号接着那条会话也要找得到它的计划）·
/// 会话记录（[`super::paths::PROJECTS_DIR`]）与在跑会话的登记（[`super::paths::SESSIONS_DIR`]）：后端只在共享库里找这两个。
/// 共享库里还没有也照样链（先在共享库里建）—— 不链的话头一回用就在号里落一份实体，各号就分开了。
pub(crate) const SHARED_ON_DEMAND: &[&str] = &[
    "agents",
    "plans",
    super::paths::PROJECTS_DIR,
    super::paths::SESSIONS_DIR,
];

/// Claude Code 在配置根下建、各号共用的其余顶层项（共享库里有才链；按需才建的那几个在 [`SHARED_ON_DEMAND`]，这里不重）：
/// 用户自己的指令与设置（`CLAUDE.md` · `settings.json`）· 技能与插件 · 输入历史 · 各类缓存 · 文件改动历史 · 后台任务 ·
/// 会话环境与命令行快照。不在这张表也不在身份表的名字（Claude Code 升级后新冒出来的 · 锁 · 原子写的旁名）不链，各号各用各的。
/// Claude Code 换了名字或加了一样共用的 ⇒ 只改这张表。
pub(crate) const NATIVE_SHARED: &[&str] = &[
    "CLAUDE.md",
    "cache",
    "downloads",
    "file-history",
    "history.jsonl",
    "jobs",
    "paste-cache",
    "plugins",
    "session-env",
    "settings.json",
    "shell-snapshots",
    "skills",
];

/// 注册表里 Claude 那一行的账号库布局（`agents::Adapter::accounts`）。
pub(crate) const FACE: AccountsFace = AccountsFace {
    identity: NATIVE_IDENTITY,
    shared: NATIVE_SHARED,
    shared_on_demand: SHARED_ON_DEMAND,
    config_file: CONFIG_FILE_NAME,
    mcp: Some(MCP_CELLS),
    mcp_logins: Some(super::mcp::LOGINS),
    shared_root: shared_root_in,
    email_in: |root| oauth_email_in(&config_path_in(root)),
    login: Some(super::LOGIN),
    sign_in: Some(super::sign_in::FACE),
    session_env: super::paths::SESSION_ENV_KEYS,
    trust: Some(TRUST_CELLS),
    // 有会话在用也续：Claude Code 自己续期拿的是同一把锁（`LoginFace.lock_inside`），两边排队。
    renew: crate::agents::Renew::Anytime,
};

/// 各号的用户级 MCP：`.claude.json` 顶层 `mcpServers` 那一个键（JSON 对象，名 → 定义）。改一个服务器只换那一个键的值，
/// 别的键（登录 · 项目历史 · 本机状态）一个字节不动。
pub(crate) const MCP_CELLS: crate::agents::McpCells = crate::agents::McpCells {
    file: CONFIG_FILE_NAME,
    read: mcp_servers_in,
    edit: mcp_server_set,
};

/// `.claude.json` 原文 ⇒ 用户级 MCP 服务器表（那一键不在 ⇒ 空表）。
fn mcp_servers_in(text: &str) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    use copy_core::copy_text;
    use serde_json::Value;
    let v: Value = serde_json::from_str(text.strip_prefix(BOM).unwrap_or(text)).map_err(|e| {
        copy_text(
            "beAcctMcpShare.read.unreadable",
            &[("line", &e.line().to_string())],
        )
    })?;
    match v.get(super::assets::SERVERS_KEY) {
        _ if !v.is_object() => Err(copy_text("reason.content.unparsable", &[])),
        None => Ok(serde_json::Map::new()),
        Some(Value::Object(m)) => Ok(m.clone()),
        Some(_) => Err(copy_text("beAcctMcpShare.read.serversNotObject", &[])),
    }
}

/// 换一个服务器（`None` = 删）：在原文上就地换、末了核一次只换了这一格（[`crate::common::json_patch::set_checked`]，
/// 别的键 · 键序 · 缩进 · BOM 原样；核不上就不交出去）—— 加 / 改只动 `mcpServers.<名>` 那一格；删换掉 `mcpServers`
/// 那一格的值（那一格里别的服务器照新写法重排，值不变）；文件还不在 ⇒ 只有这一键的新文件（两格缩进、末尾换行）。
fn mcp_server_set(
    text: &str,
    name: &str,
    def: Option<&serde_json::Value>,
) -> Result<String, String> {
    let key = super::assets::SERVERS_KEY;
    // 先照这一家的读法判一遍形状（读不懂的那几句与读时同一种）。
    let mut servers = if text.trim().is_empty() {
        serde_json::Map::new()
    } else {
        mcp_servers_in(text)?
    };
    let (path, v) = match def {
        Some(v) => (vec![key, name], v.clone()),
        None => {
            servers.remove(name);
            (vec![key], serde_json::Value::Object(servers))
        }
    };
    crate::common::json_patch::set_checked(text, &[(&path, v)])
        .ok_or_else(|| copy_core::copy_text("reason.content.unparsable", &[]))
}

/// 文件开头可能带的那个字节序标记（读时跳过、改时原样留着）。
const BOM: char = '\u{feff}';

/// `.claude.json` 里信任记在 `projects[<目录>].hasTrustDialogAccepted`。
pub const TRUST_CELLS: TrustCells = TrustCells {
    table: "projects",
    flag: Some("hasTrustDialogAccepted"),
    dir_keys: trust_dir_keys,
    edit: None,
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
    Ro::canonicalize(cwd)
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
    let v = Ro::read_json(p, CONFIG_CAP).ok()?;
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
    let v = Ro::read_json(p, CONFIG_CAP).ok()?;
    v.get("oauthAccount")?
        .get("accountUuid")?
        .as_str()
        .filter(|s| !s.is_empty() && identity_chars_ok(s))
        .map(str::to_string)
}

fn identity_chars_ok(s: &str) -> bool {
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// 请求体里账号身份那一格（`metadata.user_id` 这段 JSON 串里的 `account_uuid`）读出来是什么。
#[derive(Debug, PartialEq, Eq)]
enum BodyIdentity {
    /// 整份不是一个 JSON 对象。
    NotJson,
    /// 没有这一格（没有 `metadata` · 它不是对象 · 里面没有 `user_id` · 那段串里没有 `account_uuid`）。
    Absent,
    /// 有这一格，但读不出（`user_id` 不是串 · 那段串不是 JSON · `account_uuid` 不是串）。
    Odd,
    Has(String),
}

/// 请求体的外形只取 `metadata` 这一格；别的格（几 MB 的 `messages` 之类）由 serde 原地跳过，不建树、不分配。
#[derive(serde::Deserialize)]
struct BodyCells {
    #[serde(default)]
    metadata: MetaCells,
}

/// `metadata` 里只留 `user_id`（一段几百字节的 JSON 串）；`metadata` 不是对象 ⇒ 当没有这一格。
#[derive(Default)]
struct MetaCells {
    user_id: Option<serde_json::Value>,
}

impl<'de> serde::Deserialize<'de> for MetaCells {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(MetaVisitor)
    }
}

struct MetaVisitor;

impl<'de> serde::de::Visitor<'de> for MetaVisitor {
    type Value = MetaCells;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("metadata")
    }

    fn visit_map<M: serde::de::MapAccess<'de>>(self, mut m: M) -> Result<MetaCells, M::Error> {
        let mut user_id = None;
        while let Some(k) = m.next_key::<std::borrow::Cow<'de, str>>()? {
            if k == "user_id" {
                user_id = Some(m.next_value::<serde_json::Value>()?);
            } else {
                m.next_value::<serde::de::IgnoredAny>()?;
            }
        }
        Ok(MetaCells { user_id })
    }

    fn visit_seq<S: serde::de::SeqAccess<'de>>(self, mut s: S) -> Result<MetaCells, S::Error> {
        while s.next_element::<serde::de::IgnoredAny>()?.is_some() {}
        Ok(MetaCells::default())
    }

    fn visit_unit<E>(self) -> Result<MetaCells, E> {
        Ok(MetaCells::default())
    }

    fn visit_bool<E>(self, _: bool) -> Result<MetaCells, E> {
        Ok(MetaCells::default())
    }

    fn visit_i64<E>(self, _: i64) -> Result<MetaCells, E> {
        Ok(MetaCells::default())
    }

    fn visit_u64<E>(self, _: u64) -> Result<MetaCells, E> {
        Ok(MetaCells::default())
    }

    fn visit_f64<E>(self, _: f64) -> Result<MetaCells, E> {
        Ok(MetaCells::default())
    }

    fn visit_str<E>(self, _: &str) -> Result<MetaCells, E> {
        Ok(MetaCells::default())
    }
}

fn body_identity(body: &[u8]) -> BodyIdentity {
    let Ok(cells) = serde_json::from_slice::<BodyCells>(body) else {
        return BodyIdentity::NotJson;
    };
    let Some(raw) = cells.metadata.user_id else {
        return BodyIdentity::Absent;
    };
    let Some(Ok(inner)) = raw.as_str().map(serde_json::from_str::<serde_json::Value>) else {
        return BodyIdentity::Odd;
    };
    match inner.get("account_uuid") {
        None => BodyIdentity::Absent,
        Some(v) => v
            .as_str()
            .map_or(BodyIdentity::Odd, |s| BodyIdentity::Has(s.to_string())),
    }
}

/// 请求体里账号身份那一格的值（`metadata.user_id` 这段 JSON 串里的 `account_uuid`）：没有 / 读不出 / 空串 / 不像一个 id ⇒ `None`。
pub(crate) fn identity_of_body(body: &[u8]) -> Option<String> {
    match body_identity(body) {
        BodyIdentity::Has(s) if !s.is_empty() && identity_chars_ok(&s) => Some(s),
        _ => None,
    }
}

/// 请求体里账号身份那一格（`metadata.user_id` 这段 JSON 串里的 `account_uuid`）换成 `uuid`：只换那几个字节，别的一个字节不动。
/// 没有这一格 ⇒ [`IdentityCell::Absent`]；那几个字节在整份里不是恰好一处、或换完读回来对不上 ⇒ [`IdentityCell::Unsure`]。
/// 认旧值与读回比对都只取那一格（[`body_identity`]）；手里多出来的只有换好的那一份。
pub(crate) fn rewrite_identity(body: &[u8], uuid: &str) -> IdentityCell {
    if !identity_chars_ok(uuid) {
        return IdentityCell::Unsure;
    }
    let old = match body_identity(body) {
        BodyIdentity::NotJson | BodyIdentity::Absent => return IdentityCell::Absent,
        BodyIdentity::Odd => return IdentityCell::Unsure,
        BodyIdentity::Has(old) => old,
    };
    let cell = |u: &str| format!("\\\"account_uuid\\\":\\\"{u}\\\"");
    let Ok(text) = std::str::from_utf8(body) else {
        return IdentityCell::Unsure;
    };
    let needle = cell(&old);
    let mut hits = text.match_indices(&needle);
    let (Some((at, _)), None) = (hits.next(), hits.next()) else {
        return IdentityCell::Unsure;
    };
    // 换好的那一份按最终长度一次分配（不按 `push` 翻倍长，免得换的那一刻手里有一份半）。
    let with = cell(uuid);
    let mut out = Vec::with_capacity(body.len() - needle.len() + with.len());
    out.extend_from_slice(&body[..at]);
    out.extend_from_slice(with.as_bytes());
    out.extend_from_slice(&body[at + needle.len()..]);
    if body_identity(&out) != BodyIdentity::Has(uuid.to_string()) {
        return IdentityCell::Unsure;
    }
    IdentityCell::Rewritten(out)
}

/// 读这份配置的上限。这份文件会被 MCP 配置撑大，给 32MB。
/// 安全：`Ro::read_capped` 的 `is_file` 挡 FIFO/设备（审计实测 symlink→`/dev/zero`
/// 6 秒涨 11GB），`take` 限量。超了那一份跳过（读的那一口 `warn!` 带路径）：身份 · 邮箱当读不出，资产目录记进 `problems`。
pub const CONFIG_CAP: Cap = Cap {
    bytes: 32 * 1024 * 1024,
    over: Over::SkipAndSay,
};

/// 某个根目录下的配置文件路径。
pub(crate) fn config_path_in(root: &std::path::Path) -> std::path::PathBuf {
    root.join(CONFIG_FILE_NAME)
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/accounts_tests.rs"]
mod tests;

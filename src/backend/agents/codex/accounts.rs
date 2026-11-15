//! Codex 的**账号面**：号的载体（一号一个 `CODEX_HOME` 目录）· `auth.json` 里的凭据怎么读 · 交给核心续期的那一形
//! （[`LOGIN`]）· 目录信任怎么写进那个号的 `config.toml`（[`TRUST_CELLS`]）。
//!
//! 只有格式知识与判法，一个字节不写：续期 · 写回由核心照 [`LOGIN`] 那几格做；信任只算出改好的整份（[`trust_edit`]），写不写由核心定。
//! 写不写哪个号目录不在这里判：默认家（`~/.codex`，账号 0）该只读，由核心那一侧挡。
//!
//! 依据（codex-rs，核到 tag `rust-v0.167.0`；下面的名字都是那边的全路径）：
//! - `auth.json` 的形：`codex_login::auth::storage::AuthDotJson`（`tokens` · `last_refresh` · `OPENAI_API_KEY` …）与
//!   `codex_login::token_data::TokenData`（`id_token` 存原 JWT 串 · `access_token` · `refresh_token` · `account_id` ·
//!   `oauth_provenance{issuer, client_id}` · `login_owner_id`）。邮箱：`id_token` 的 `email`，没有再看
//!   `https://api.openai.com/profile` 里的 `email`（`codex_login::token_data::parse_chatgpt_jwt_claims`）。
//! - 续不续：访问令牌的 `exp` 落在此刻往后 5 分钟之内 ⇒ 续；读不出 `exp` ⇒ `last_refresh` 早过 8 天才续
//!   （`codex_login::auth::manager` 里的刷新窗口与刷新间隔两个常量）。
//! - 往哪续：`<issuer>/oauth/token` 与 provenance 的 client id，没有 provenance ⇒ `https://auth.openai.com/oauth/token` 与
//!   默认 client id（`codex_login::auth::manager::refresh_token_endpoint` · `codex_login::auth::manager::CLIENT_ID`）。
//! - 写回：`auth.lock` 上的排他文件锁里重读、换 `id_token` · `access_token` · `refresh_token` 与 `last_refresh`、整份写
//!   （`codex_login::auth::manager::persist_tokens`）；核心照 [`LOGIN`] 同样换这四格（`id_token` 核过 JWT 形状才落盘）。锁不罩网络请求 ⇒ 只续此刻没有 Codex 会话在用的号（[`Renew::Idle`]）。
//! - 信任：`[projects."<键>"] trust_level = "trusted"`，键是解开链接后的工作目录（Windows 上转小写，
//!   `codex_config::loader::project_trust_key`）；工作目录那一项压过仓根那一项 ⇒ 只标工作目录。

use crate::agents::{
    AccountsFace, BaseLogin, Expiry, IdentityCell, IdentityClass, IdentityRoot, LoginFace,
    LoginOrigin, Renew, SessionEnvKeys, TrustCells, WriteLock,
};
use crate::platform::fs::Ro;
use cap_core::{Cap, Over};
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::path::Path;

/// 号目录里的凭据文件。
pub const AUTH_FILE: &str = "auth.json";
/// 号目录里 Codex 写凭据时拿排他文件锁的那一份（`File::lock`，不是 `mkdir` 锁）。
pub const AUTH_LOCK: &str = "auth.lock";
/// 号目录里的配置（目录信任 · 钩子信任住这里）。
pub const CONFIG_FILE: &str = "config.toml";
/// 凭据里放令牌的那一节。
const TOKENS: &str = "tokens";
/// 凭据顶层「上次续的时刻」那一格（RFC 3339）。
const LAST_REFRESH: &str = "last_refresh";
/// 没有 provenance 时的令牌端点与 client id。
const DEFAULT_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const DEFAULT_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
/// 令牌端点在签发方底下的路径。
const TOKEN_PATH: &str = "/oauth/token";
/// 读不出 `exp` 时，`last_refresh` 往后这么多毫秒算过期。
const REFRESH_INTERVAL_MS: u64 = 8 * 86_400_000;

/// 读 `auth.json` 的上限（几 KB 的东西）：超了当读不了。
const AUTH_CAP: Cap = Cap {
    bytes: 1 << 20,
    over: Over::SkipAndSay,
};

/// 每个号各有一份的那几项：凭据 · 它的锁 · 那个号自己的配置（信任记在这里）。别家的号不链共享库，表外的名字各号各用各的。
const IDENTITY: &[(&str, IdentityRoot, IdentityClass)] = &[
    (AUTH_FILE, IdentityRoot::ConfigDir, IdentityClass::Secret),
    (AUTH_LOCK, IdentityRoot::ConfigDir, IdentityClass::Derived),
    (CONFIG_FILE, IdentityRoot::ConfigDir, IdentityClass::State),
];

/// 会话进程环境里认号的那一格（`CODEX_HOME`）。上游地址：Codex 0.167.0 不从环境读（中转地址经 `-c openai_base_url=…` 交，
/// `relay.rs`）⇒ 那一格照实没有（`None`），设置文件那一问恒答「说不清」。
const SESSION_ENV: SessionEnvKeys = SessionEnvKeys {
    config_dir: super::parse::HOME_ENV,
    base_url: None,
    settings_may_set_base_url: |_, _, _| true,
};

/// 账号 0 的配置目录（没设 `CODEX_HOME` 时 Codex 用的那一个）。
fn default_home(home: &Path) -> std::path::PathBuf {
    home.join(".codex")
}

/// 注册表里 Codex 那一行的账号面（`agents::Adapter::accounts`）。
pub const FACE: AccountsFace = AccountsFace {
    identity: IDENTITY,
    shared: &[],
    shared_on_demand: &[],
    config_file: CONFIG_FILE,
    // 各号之间同步的 MCP：那个号 `config.toml` 的 `[mcp_servers.<名>]`（TOML 读改住 `mcp.rs`）。
    mcp: Some(MCP_CELLS),
    // MCP 登录存成文件的那一份（`.credentials.json`，读改与两把锁在 `mcp.rs`）。
    mcp_logins: Some(super::mcp::LOGINS),
    shared_root: default_home,
    email_in,
    login: Some(LOGIN),
    sign_in: Some(super::sign_in::FACE),
    session_env: SESSION_ENV,
    trust: Some(TRUST_CELLS),
    // 锁不罩网络请求：Codex 会话自己也会拿同一枚刷新令牌去续 ⇒ 只续此刻没人在用的号。
    renew: Renew::Idle,
};

/// 各号的用户级 MCP（账号面 `mcp` 那一格）：`config.toml` 的 `mcp_servers` 那一节，改一个服务器只换它那一节。
pub(crate) const MCP_CELLS: crate::agents::McpCells = crate::agents::McpCells {
    file: CONFIG_FILE,
    read: super::mcp::servers_in,
    edit: super::mcp::server_set,
};

/// 订阅号登录交给核心续期的那一形：`auth.json` 的 `tokens` 那一节；过期解访问令牌（读不出由 `last_refresh` 推）；
/// 出处读 `tokens.oauth_provenance`；整份写在 `auth.lock` 的文件锁里；续完记 `last_refresh`。
pub(crate) const LOGIN: LoginFace = LoginFace {
    creds_file: AUTH_FILE,
    section: TOKENS,
    access: "access_token",
    refresh: "refresh_token",
    expiry: Expiry::Read(expires_ms),
    // Codex 的 `tokens` 不记 scope（续期不带、回包的不写）、也不记 client id（client id 在出处里，盘上恒缺，核心照缺省）。
    scopes: None,
    // 续成另换 `tokens.id_token`（回包 `id_token`，存原串）：先核 Codex 自己读得了它（[`codex_reads_id_token`]：外形 ＋
    // 认得的几格类型）；核不过 ⇒ 整次不落盘。
    renewed_cells: &[crate::agents::RenewedCell {
        answer: "id_token",
        cell: "id_token",
        check: Some(codex_reads_id_token),
    }],
    client_field: "client_id",
    token_url: DEFAULT_TOKEN_URL,
    client_id: DEFAULT_CLIENT_ID,
    origin: Some(origin),
    renewed_at: Some((LAST_REFRESH, |ms| Value::from(rfc3339_ms(ms)))),
    margin_ms: 5 * 60 * 1000,
    // Codex 续期不另拿锁（只有写回那一把 `auth.lock`）：我方也不拿（自己的几个续期者经配置目录的跨进程锁排队）。
    lock_inside: None,
    lock_beside: None,
    lock_stale_ms: 60_000,
    write_lock: Some(WriteLock::File(AUTH_LOCK)),
    // 文件锁持有方一不在系统就放了，这一格不看。
    write_lock_stale_ms: 15_000,
    write_wait_ms: 20_000,
    identity_file: |dir, _| dir.join(AUTH_FILE),
    identity_in: account_id_in,
    // 账号身份走请求头（`chatgpt-account-id`），请求体里没有这一格。
    identity_of_body: |_| None,
    rewrite_identity: |_, _| IdentityCell::Absent,
    // 账号 0（`~/.codex`）是用户自己的那份：只读，令牌没过期照用，不续不写。
    base_login: BaseLogin::Untouched,
};

/// 目录信任那一格：`config.toml` 是 TOML ⇒ 读写都经这一家自己的两手（[`trusted_dirs`] · [`mark_trusted`]），
/// `table` · `flag` 只记那两格在 TOML 里叫什么。
pub(crate) const TRUST_CELLS: TrustCells = TrustCells {
    table: "projects",
    flag: Some("trust_level"),
    dir_keys: trust_dir_keys,
    edit: Some(crate::agents::TrustEdit {
        trusted: trusted_dirs,
        mark: mark_trusted,
    }),
};

// ═══ 凭据 ═══

/// 订阅号登录着的那一份读出来的样子（不带令牌本身）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed {
    pub email: Option<String>,
    pub account_id: Option<String>,
}

/// 一个号的 `auth.json` 是什么样。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Creds {
    /// 没有这份文件。
    Absent,
    /// 读不了 / 不是 JSON 对象。
    Unreadable,
    /// 是 JSON 对象，但没有订阅号的 `tokens`（API key 号 · 别的登录形）。
    NoTokens,
    Signed(Signed),
}

/// `tokens` 那一节里一格非空的串。
fn token<'a>(t: &'a Map<String, Value>, k: &str) -> Option<&'a str> {
    t.get(k).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// `auth.json` 的原文（没有 ⇒ `None`）⇒ 它是什么样。
pub fn creds_of(raw: Option<&[u8]>) -> Creds {
    let Some(raw) = raw else {
        return Creds::Absent;
    };
    let Ok(Value::Object(doc)) = serde_json::from_slice::<Value>(raw) else {
        return Creds::Unreadable;
    };
    let Some(t) = doc.get(TOKENS).and_then(Value::as_object) else {
        return Creds::NoTokens;
    };
    let email = token(t, "id_token").and_then(jwt_payload).and_then(|c| {
        let at = |v: &Value| v.get("email").and_then(Value::as_str).map(str::to_string);
        at(&c).or_else(|| c.get("https://api.openai.com/profile").and_then(at))
    });
    Creds::Signed(Signed {
        email: email.filter(|e| !e.is_empty()),
        account_id: token(t, "account_id").map(str::to_string),
    })
}

/// 那份 `auth.json` 是什么样（只读）。
fn creds_at(p: &Path) -> Creds {
    match Ro::metadata(p) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Creds::Absent,
        _ => Ro::read_capped(p, AUTH_CAP).map_or(Creds::Unreadable, |b| creds_of(Some(&b))),
    }
}

/// 那个号登录的邮箱；读不到 ⇒ `None`。
pub fn email_in(dir: &Path) -> Option<String> {
    match creds_at(&dir.join(AUTH_FILE)) {
        Creds::Signed(s) => s.email,
        _ => None,
    }
}

/// 那份 `auth.json` 里的账号 id（核心认「这一发是哪个号」用）；读不到 ⇒ `None`。
fn account_id_in(file: &Path) -> Option<String> {
    match creds_at(file) {
        Creds::Signed(s) => s.account_id,
        _ => None,
    }
}

/// 整份凭据 ⇒ 访问令牌几点过期（毫秒）：解 `tokens.access_token` 的 `exp`；读不出 ⇒ `last_refresh` ＋ 8 天；都没有 ⇒ `None`。
fn expires_ms(doc: &Map<String, Value>) -> Option<u64> {
    let exp = doc
        .get(TOKENS)
        .and_then(Value::as_object)
        .and_then(|t| token(t, "access_token"))
        .and_then(jwt_payload)
        .and_then(|c| c.get("exp")?.as_u64());
    if let Some(exp) = exp {
        return Some(exp.saturating_mul(1000));
    }
    let last = doc
        .get(LAST_REFRESH)
        .and_then(Value::as_str)
        .and_then(crate::common::time::parse_iso8601_ms)?;
    u64::try_from(last)
        .ok()
        .map(|ms| ms.saturating_add(REFRESH_INTERVAL_MS))
}

/// 整份凭据 ⇒ 这个号登录时的出处：`tokens.oauth_provenance` 两格都在 ⇒ `<issuer>/oauth/token` 与它的 client id；否则 `None`。
fn origin(doc: &Map<String, Value>) -> Option<LoginOrigin> {
    let p = doc.get(TOKENS)?.get("oauth_provenance")?.as_object()?;
    let issuer = token(p, "issuer")?;
    let client = token(p, "client_id")?;
    Some(LoginOrigin {
        token_url: format!("{}{TOKEN_PATH}", issuer.trim_end_matches('/')),
        client_id: Some(client.to_string()),
    })
}

/// 一段 JWT 的载荷（第二段 base64url 解开的 JSON 对象）；不像 JWT ⇒ `None`。不验签（只读自己号里的那份）。
/// 外形照 Codex 自己的解法（codex-rs `rust-v0.167.0` `codex_login::token_data::decode_jwt_payload`）：只看前三段、都不空
/// （多出来的段不管）；中段是不带 `=` 的 URL-safe base64（`URL_SAFE_NO_PAD`：只许 `A–Z a–z 0–9 - _`）。
fn jwt_payload(jwt: &str) -> Option<Value> {
    let mut parts = jwt.split('.');
    let (Some(h), Some(p), Some(s)) = (parts.next(), parts.next(), parts.next()) else {
        return None;
    };
    if h.is_empty() || p.is_empty() || s.is_empty() {
        return None;
    }
    if !p
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return None;
    }
    let mut std = p.replace('-', "+").replace('_', "/");
    while !std.len().is_multiple_of(4) {
        std.push('=');
    }
    let bytes = cmd_wire::b64::b64_decode(&std).ok()?;
    serde_json::from_slice::<Value>(&bytes)
        .ok()
        .filter(Value::is_object)
}

/// 续成写回 `tokens.id_token` 之前那一核：Codex 自己读得了它（[`jwt_payload`] 那一形 ＋ 认得的几格类型都对）。
/// Codex 读它是反序列化成 `IdClaims`（codex-rs `rust-v0.167.0` `codex_login::token_data`：`email: Option<String>` ·
/// `"https://api.openai.com/profile": Option<ProfileClaims{email: Option<String>}>` ·
/// `"https://api.openai.com/auth": Option<AuthClaims{chatgpt_plan_type: Option<PlanType>（untagged，任何串都收）,
/// chatgpt_user_id · user_id · chatgpt_account_id: Option<String>, chatgpt_account_is_fedramp: bool（可缺，null 不收）}>`），
/// 哪一格类型不对整个解不开；它加载 `auth.json` 也走这一解 ⇒ 写进一个它解不开的，那个号就登不上了。这一核不比它宽。
fn codex_reads_id_token(jwt: &str) -> bool {
    let Some(claims) = jwt_payload(jwt) else {
        return false;
    };
    let str_or_null = |v: Option<&Value>| v.is_none_or(|v| v.is_null() || v.is_string());
    let obj_or_null =
        |v: Option<&Value>, inner: &dyn Fn(&serde_json::Map<String, Value>) -> bool| match v {
            None | Some(Value::Null) => true,
            Some(Value::Object(m)) => inner(m),
            Some(_) => false,
        };
    str_or_null(claims.get("email"))
        && obj_or_null(claims.get("https://api.openai.com/profile"), &|m| {
            str_or_null(m.get("email"))
        })
        && obj_or_null(claims.get("https://api.openai.com/auth"), &|m| {
            [
                "chatgpt_plan_type",
                "chatgpt_user_id",
                "user_id",
                "chatgpt_account_id",
            ]
            .iter()
            .all(|k| str_or_null(m.get(*k)))
                && m.get("chatgpt_account_is_fedramp")
                    .is_none_or(Value::is_boolean)
        })
}

/// 毫秒时刻 ⇒ `YYYY-MM-DDTHH:MM:SS.mmmZ`（Codex 读 RFC 3339）。
fn rfc3339_ms(ms: u64) -> String {
    let secs = i64::try_from(ms / 1000).unwrap_or(i64::MAX);
    let (y, mo, d) = crate::common::time::civil_from_days(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        s / 3600,
        s % 3600 / 60,
        s % 60,
        ms % 1000
    )
}

// ═══ 目录信任 ═══

/// 一个工作目录在 `config.toml` 的 `projects` 里记成哪个键：解开链接后的那一形（Windows 上去掉 `\\?\`、转小写）；解不开 ⇒ 原样。
pub fn trust_dir_keys(cwd: &str) -> Vec<String> {
    let resolved = Ro::canonicalize(cwd)
        .ok()
        .and_then(|p| p.to_str().map(str::to_string))
        .unwrap_or_else(|| cwd.to_string());
    let key = if cfg!(windows) {
        resolved.trim_start_matches(r"\\?\").to_ascii_lowercase()
    } else {
        resolved
    };
    vec![key]
}

/// 目录信任要不要写、写成什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustEdit {
    /// 已经是 `trusted`，不重写。
    Already,
    /// 改好的整份（原文只动那一节）。
    Write(String),
    /// 原文读不懂（[`config_doc`]），或那条路上有一格不是表 ⇒ 不写。
    Unreadable,
}

/// 那份 `config.toml` 的原文 ＋ 目录键 ⇒ 把 `projects."<键>".trust_level = "trusted"` 写进去（[`set_string_at`]：
/// 已有那一格只换值；那一项是表头 · 点号键 · 行内表照它原来的写法补；都没有 ⇒ 末尾加一节）。别的节与键一个字不动。
pub fn trust_edit(config: &str, key: &str) -> TrustEdit {
    let Ok(doc) = config_doc(config) else {
        return TrustEdit::Unreadable;
    };
    let want = ["projects", key, "trust_level"];
    if string_at(&doc, &want) == Some("trusted") {
        return TrustEdit::Already;
    }
    set_string_at(config, &want, "trusted").map_or(TrustEdit::Unreadable, TrustEdit::Write)
}

/// 信任那一格的读法（[`TRUST_CELLS`]）：原文 ⇒ `projects` 里 `trust_level = "trusted"` 的那几个目录；读不懂 ⇒ `Err`。
fn trusted_dirs(config: &str) -> Result<BTreeSet<String>, String> {
    let doc = config_doc(config).map_err(|_| unreadable())?;
    let Some(projects) = doc.get("projects") else {
        return Ok(BTreeSet::new());
    };
    let projects = projects.as_table_like().ok_or_else(unreadable)?;
    Ok(projects
        .iter()
        .filter(|(_, v)| {
            v.as_table_like()
                .and_then(|t| t.get("trust_level"))
                .and_then(toml_edit::Item::as_str)
                == Some("trusted")
        })
        .map(|(k, _)| k.to_string())
        .collect())
}

/// 信任那一格的改法（[`TRUST_CELLS`]）：几个目录键挨个照 [`trust_edit`] 标；都已经是 ⇒ `Ok(None)`；
/// 哪一个读不懂 · 改不安全 ⇒ `Err`、不出新原文。
fn mark_trusted(config: &str, keys: &BTreeSet<String>) -> Result<Option<String>, String> {
    let mut out: Option<String> = None;
    for key in keys {
        match trust_edit(out.as_deref().unwrap_or(config), key) {
            TrustEdit::Already => {}
            TrustEdit::Write(s) => out = Some(s),
            TrustEdit::Unreadable => return Err(unreadable()),
        }
    }
    Ok(out)
}

/// 读不懂那份 `config.toml` 时的那一句（不带原文）。
fn unreadable() -> String {
    copy_core::copy_text("reason.content.unparsable", &[])
}

// ═══ config.toml：读与改只经这一个读法（`toml_edit`，跟 Codex 自己读它同一个规范） ═══

/// `config.toml` 原文 ⇒ 文档；不是合法 TOML（串没收尾 · 重复键 · 重复表头 · 不认的转义 …）⇒ `Err(第几行)`。
/// 「读不读得懂」只在这里判：目录信任 · 钩子信任 · 插件开关 · MCP 都经它。
pub(crate) fn config_doc(text: &str) -> Result<toml_edit::DocumentMut, usize> {
    text.parse::<toml_edit::DocumentMut>().map_err(|e| {
        e.span().map_or(1, |s| {
            text[..s.start.min(text.len())].matches('\n').count() + 1
        })
    })
}

/// 文档里那一路键底下的那一项（表头 · 点号键 · 行内表三种写法都认）；没有 ⇒ `None`。
pub(crate) fn item_at<'a>(
    doc: &'a toml_edit::DocumentMut,
    path: &[&str],
) -> Option<&'a toml_edit::Item> {
    let (last, tables) = path.split_last()?;
    let mut t: &dyn toml_edit::TableLike = doc.as_table();
    for seg in tables {
        t = t.get(seg)?.as_table_like()?;
    }
    t.get(last)
}

/// 那一路键底下的串值（不是串 · 没有 ⇒ `None`）。
pub(crate) fn string_at<'a>(doc: &'a toml_edit::DocumentMut, path: &[&str]) -> Option<&'a str> {
    item_at(doc, path)?.as_str()
}

/// 把那一路键底下写成这个串（`path` 末一段是键，前面几段是表）：缺的表补上（中间那几层是隐式的、最里面那一层是一节表头；
/// 那一层原本是行内表就在行内表里补），已有那一格只换值；别的节 · 键 · 注释 · 写法一个字不动。
/// 原文读不懂 · 半路那一格不是表 ⇒ `None`。改完再读一遍，读回来不是这个值 ⇒ `None`。
pub(crate) fn set_string_at(text: &str, path: &[&str], value: &str) -> Option<String> {
    let mut doc = config_doc(text).ok()?;
    put(doc.as_table_mut(), false, path, value)?;
    let out = doc.to_string();
    let back = config_doc(&out).ok()?;
    (string_at(&back, path) == Some(value)).then_some(out)
}

fn put(t: &mut dyn toml_edit::TableLike, inline: bool, path: &[&str], value: &str) -> Option<()> {
    match path {
        [] => None,
        [k] => {
            t.insert(k, toml_edit::value(value));
            Some(())
        }
        [k, rest @ ..] => {
            if t.get(k).is_none() {
                let fresh = if inline {
                    toml_edit::Item::Value(toml_edit::Value::InlineTable(Default::default()))
                } else {
                    let mut nt = toml_edit::Table::new();
                    nt.set_implicit(rest.len() > 1);
                    toml_edit::Item::Table(nt)
                };
                t.insert(k, fresh);
            }
            let child = t.get_mut(k)?;
            let inline = child.is_inline_table();
            put(child.as_table_like_mut()?, inline, rest, value)
        }
    }
}

/// 一个键写成 TOML 的基本串。
pub(crate) fn toml_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/accounts_tests.rs"]
mod tests;

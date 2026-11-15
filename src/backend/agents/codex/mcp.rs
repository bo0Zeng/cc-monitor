//! Codex 的 **MCP 布局**：`config.toml` 的 `[mcp_servers.<名>]`（用户级在 `$CODEX_HOME/config.toml`，项目级在项目配置层
//! `<目录>/.codex/config.toml`，项目根到工作目录之间每一层都算）· MCP 登录交 `codex mcp login <名>` · 登录凭据存成文件时
//! 住 `$CODEX_HOME/.credentials.json`。
//!
//! 形照 codex-rs rust-v0.167.0：`codex_config::mcp_types::McpServerConfig`（`enabled = false` 停用）·
//! `codex_core::config::edit::document_helpers`（每个服务器一节，`env` · `http_headers` 是它底下的子表）·
//! `codex_rmcp_client::oauth`（配置里 MCP 登录存哪缺省 `auto`，`codex_config::types::OAuthCredentialsStoreMode`：先钥匙串、用不了才落 `.credentials.json`）。
//! 状态：配置里写着的停用；存成文件的那一份令牌在、Codex 自己判它用不了 ⇒ 要登录（[`read_at`]）；别的都是 `Unknown`（钥匙串里那一份 · 没存过的要联网才知道）。
//!
//! 定义在两边都是 JSON 值（名 → 定义）：读把那一节的 TOML 照值翻成 JSON，改把 JSON 照 Codex 自己写的样子翻回一节 TOML，
//! 只换那一个服务器那一节（`toml_edit` 就地改：别的节 · 键 · 注释 · 写法原样）。

use crate::platform::fs::Ro;
use cap_core::{Cap, Over};
use copy_core::copy_text;
use serde_json::{Map, Number, Value};
use std::path::Path;
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table};

use crate::agents::{McpEntry, McpLook, McpRead, McpStatus};
use crate::common::said::IntoNote as _;

/// `config.toml` 里装服务器的那一节。
pub(crate) const SERVERS_KEY: &str = "mcp_servers";
/// MCP 登录存成文件时那一份（相对那个号的 `CODEX_HOME`）：各号之间能同步的只会是它（钥匙串里那一份同步不了）。
const CREDENTIALS_FILE: &str = ".credentials.json";
/// 那几把锁住的子目录（codex-rs `rmcp-client` 的 `oauth/refresh_lock.rs` · `oauth/store_lock.rs`）。
const LOCK_DIR: &str = "mcp-oauth-locks";

/// 存成文件的那一份 MCP 登录（注册表 `AccountsFace.mcp_logins`），形照 codex-rs `rust-v0.167.0` `rmcp-client/src/oauth.rs`：
/// 顶层就是「键 → 一条」（键是 Codex 自己算的 `<服务器名>|<哈希前缀>`，这里原样读写）；一条是 `FallbackTokenEntry`，
/// 令牌三格在它顶上（`access_token` · `refresh_token` · `expires_at`，毫秒）；Codex 整份紧凑写回。
/// Codex 写一条时先拿那一条的排他文件锁 `mcp-oauth-locks/<键的 sha256 十六进制>.lock`，再拿整份那一把
/// `mcp-oauth-locks/file-store.lock`（读时它拿同一份上的共享锁）。只有存法是文件（或钥匙串用不了退回文件）时才有这份。
pub(crate) const LOGINS: crate::agents::McpLogins = crate::agents::McpLogins {
    file: CREDENTIALS_FILE,
    read: logins_in,
    edit: login_set,
    lock: &[
        crate::agents::McpLoginLock::EntryFile { name: entry_lock },
        crate::agents::McpLoginLock::WholeFile {
            name: "mcp-oauth-locks/file-store.lock",
        },
    ],
    access: "access_token",
    refresh: "refresh_token",
    expires_ms: "expires_at",
};

/// 那一条的锁文件：`mcp-oauth-locks/<键的 sha256，64 位小写十六进制>.lock`。
fn entry_lock(key: &str) -> String {
    format!(
        "{LOCK_DIR}/{}.lock",
        crate::common::digest::sha256_hex(key.as_bytes())
    )
}

/// 整份原文 ⇒ 各条（顶层对象原样）；不是 JSON 对象 ⇒ 读不懂（不带原文片段）。
fn logins_in(text: &str) -> Result<Map<String, Value>, String> {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(m)) => Ok(m),
        _ => Err(copy_text("beOauthStore.read.notJson", &[])),
    }
}

/// 换那一条（`None` ＝ 删），别的条原样，照 Codex 自己的写法紧凑写回。
fn login_set(text: &str, key: &str, to: Option<&Value>) -> Result<String, String> {
    let mut m = logins_in(text)?;
    match to {
        Some(v) => {
            m.insert(key.to_string(), v.clone());
        }
        None => {
            m.remove(key);
        }
    }
    serde_json::to_string(&Value::Object(m))
        .map_err(|_| copy_text("reason.content.unparsable", &[]))
}
/// 读一份 `config.toml` 的上限（手写的配置，远不到这个量级）：资产面读插件开关也用它。
pub(crate) const CONFIG_CAP: Cap = Cap {
    bytes: 4 * 1024 * 1024,
    over: Over::SkipAndSay,
};

/// `codex mcp login <名>`（argv，程序名在头；注册表 `McpFace.login`）。
pub(crate) fn login_argv(name: &str) -> Vec<String> {
    [super::resume::DEFAULT_COMMAND, "mcp", "login", name]
        .map(str::to_string)
        .to_vec()
}

// ───────────────────────── 读面 ─────────────────────────

/// 注册表 `McpFace.read`：没设账号那一份 `$CODEX_HOME/config.toml` ＋ 各号的家各一份 ＋ 项目里那几层（[`read_at`]）。
pub(crate) fn read(project_dir: Option<&Path>, look: &McpLook) -> McpRead {
    let user = super::home().map(|h| h.join(super::accounts::CONFIG_FILE));
    read_at(user.as_deref(), project_dir, look)
}

/// [`read`] 的本体，逐号读：没设账号那一份（位置是参数，判据拿夹具喂它）在前，再 `look.homes` 里设了账号的每个家各一份
/// `<家>/config.toml`；与某号那一份是同一个文件 ⇒ 只读一次。每一份出它的用户级条目（`source` 是那一份），几个号里的同一服务器
/// 在这里照实各出一条，合成一条是核心 `mcp_held` 的事。
/// 要（重新）登录照 Codex 自己离线那一判（[`needs_login`]），只看那一份所在的那个家的 `.credentials.json`；项目级那一条看每个家。
/// 项目里从工作目录往上到项目根（[`super::assets`] 同一个认法），每一层的 `.codex/config.toml` 都列（离工作目录近的在前），只读一次。
pub fn read_at(user_config: Option<&Path>, project_dir: Option<&Path>, look: &McpLook) -> McpRead {
    let mut out = McpRead::default();
    // 读哪几份：（那一份 · 它所在那个家 · 号名）。
    let mut files: Vec<(std::path::PathBuf, std::path::PathBuf, Option<String>)> = look
        .homes
        .iter()
        .filter(|(who, _)| who.is_some())
        .map(|(who, home)| {
            (
                home.join(super::accounts::CONFIG_FILE),
                home.clone(),
                who.clone(),
            )
        })
        .collect();
    if let Some(bare) = user_config.filter(|b| !files.iter().any(|(f, _, _)| f == b)) {
        let home = bare.parent().map(Path::to_path_buf).unwrap_or_default();
        files.insert(0, (bare.to_path_buf(), home, None));
    }
    let tokens: Vec<(Option<String>, Map<String, Value>)> = files
        .iter()
        .map(|(_, home, who)| (who.clone(), tokens_in(home)))
        .collect();
    let mut dirs: Vec<String> = Vec::new();
    for ((path, _, _), (who, held)) in files.iter().zip(&tokens) {
        if let Some(doc) = read_doc(path, &mut out.problems) {
            let from = out.entries.len();
            push(&mut out, &doc, "user", path);
            judge(
                &mut out.entries[from..],
                &[(who.clone(), held)],
                look.now_ms,
            );
            dirs.extend(
                super::accounts::item_at(&doc, &["projects"])
                    .and_then(Item::as_table_like)
                    .into_iter()
                    .flat_map(|t| t.iter().map(|(k, _)| k.to_string())),
            );
        }
    }
    dirs.sort();
    dirs.dedup();
    out.dirs = dirs;
    if let Some(cwd) = project_dir {
        for layer in super::assets::project_layers(cwd) {
            let file = layer
                .join(super::assets::PROJECT_CONFIG_DIR)
                .join(super::accounts::CONFIG_FILE);
            if let Some(doc) = read_doc(&file, &mut out.problems) {
                let from = out.entries.len();
                push(&mut out, &doc, "project", &file);
                let all: Vec<(Option<String>, &Map<String, Value>)> = tokens
                    .iter()
                    .map(|(who, held)| (who.clone(), held))
                    .collect();
                judge(&mut out.entries[from..], &all, look.now_ms);
            }
        }
    }
    out
}

/// 一个家里存成文件的那一份 MCP 登录（[`LOGINS`] 那一份，顶层「键 → 一条」）；不在 · 读不懂 ⇒ 空（判不了，不出声）。
fn tokens_in(home: &Path) -> Map<String, Value> {
    Ro::read_capped(home.join(CREDENTIALS_FILE), CONFIG_CAP)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| match v {
            Value::Object(m) => Some(m),
            _ => None,
        })
        .unwrap_or_default()
}

/// 这几条（没停用的）在给的那几个家里要不要（重新）登录：哪个家里 [`needs_login`] ⇒ 要登录，`login_in` 记那一位（号名；没设账号的那个家 ⇒ `None`）。
fn judge(entries: &mut [McpEntry], homes: &[(Option<String>, &Map<String, Value>)], now_ms: u64) {
    for e in entries
        .iter_mut()
        .filter(|e| e.status != McpStatus::Disabled)
    {
        let mut names: Vec<Option<String>> = Vec::new();
        for (who, held) in homes {
            if needs_login(&e.name, &e.server, held, now_ms) {
                names.push(who.clone());
            }
        }
        if !names.is_empty() {
            names.sort();
            names.dedup();
            e.status = McpStatus::NeedsLogin;
            e.login_in = names;
            e.seen_ms = Some(now_ms);
        }
    }
}

/// 令牌快过期的那一段（毫秒）：Codex 离过期这么近就当要续（`REFRESH_SKEW_MILLIS`）。
const REFRESH_SKEW_MS: u64 = 30_000;

/// Codex 自己离线判「这一条要（重新）登录」的那一判（codex-rs `rust-v0.167.0` `rmcp-client/src/oauth.rs`：
/// `codex_rmcp_client::oauth::oauth_token_status` 存着却 `codex_rmcp_client::oauth::oauth_tokens_are_usable` 不成 ⇒ `Reauthentication`）。只认存成文件的那一份。
/// 配了 bearer 变量或 `Authorization` 头 ⇒ 不走 OAuth，不判；没有 `url`（本机命令那种）⇒ 不判；
/// 存的那一份里没有它（`server_name` · `server_url` 对得上、不是执行端那种）⇒ 要联网发现才知道，不判。
/// 存着的：`client_id` 空 ⇒ 用不了；快过期 ⇒ 有签发方且有 refresh_token 才用得了；没过期 ⇒ 访问令牌不空才用得了。
/// 只看这几格是不是空、过期时刻，令牌的值不留。
fn needs_login(name: &str, server: &Value, held: &Map<String, Value>, now_ms: u64) -> bool {
    if server.get("bearer_token_env_var").is_some()
        || server
            .get("http_headers")
            .and_then(Value::as_object)
            .is_some_and(|h| h.keys().any(|k| k.eq_ignore_ascii_case("authorization")))
    {
        return false;
    }
    let Some(url) = server.get("url").and_then(Value::as_str) else {
        return false;
    };
    let filled = |e: &Value, k: &str| {
        e.get(k)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.trim().is_empty())
    };
    let Some(e) = held.values().find(|e| {
        e.get("executor_owned") != Some(&Value::Bool(true))
            && e.get("server_url").and_then(Value::as_str) == Some(url)
            && e.get("server_name").and_then(Value::as_str) == Some(name)
    }) else {
        return false;
    };
    if !filled(e, "client_id") {
        return true;
    }
    let expiring = e
        .get("expires_at")
        .and_then(Value::as_u64)
        .is_some_and(|at| now_ms.saturating_add(REFRESH_SKEW_MS) >= at);
    let usable = if expiring {
        filled(e, "issuer") && filled(e, "refresh_token")
    } else {
        filled(e, "access_token")
    };
    !usable
}

fn push(out: &mut McpRead, doc: &DocumentMut, scope: &'static str, file: &Path) {
    let servers = match servers_of(doc) {
        Ok(m) => m,
        Err(why) => {
            out.problems.push(copy_text(
                "reason.fs.readFailed",
                &[("path", &file.display().to_string()), ("why", &why)],
            ));
            return;
        }
    };
    for (name, server) in servers {
        let status = if server.get("enabled") == Some(&Value::Bool(false)) {
            McpStatus::Disabled
        } else {
            McpStatus::Unknown
        };
        out.entries.push(McpEntry {
            scope,
            name,
            server,
            source: file.display().to_string(),
            status,
            login_in: Vec::new(),
            seen_ms: None,
        });
    }
}

/// 读一份 `config.toml`：不在 ⇒ `None`、不出声；在而读不出 / 解不开 ⇒ `None` 并记一句（只带路径与位置，不带值）。
fn read_doc(path: &Path, problems: &mut Vec<String>) -> Option<DocumentMut> {
    let say = |why: &str| {
        copy_text(
            "reason.fs.readFailed",
            &[("path", &path.display().to_string()), ("why", why)],
        )
    };
    match Ro::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            problems.push(
                crate::common::said::Said::with_raw(say(&copy_core::io_reason(e.kind())), &e)
                    .into_note(),
            );
            return None;
        }
        Ok(_) => {}
    }
    let bytes = match Ro::read_capped(path, CONFIG_CAP) {
        Ok(b) => b,
        Err(e) => {
            problems.push(e.wrap(|why| say(why)).into_note());
            return None;
        }
    };
    match parse(&String::from_utf8_lossy(&bytes)) {
        Ok(doc) => Some(doc),
        Err(why) => {
            problems.push(say(&why));
            None
        }
    }
}

// ───────────────────────── 账号面那一格（McpCells） ─────────────────────────

/// 文本 ⇒ 文档（经 `config.toml` 那一个读法 [`super::accounts::config_doc`]）；解不开 ⇒ 一句（只带行号）。
fn parse(text: &str) -> Result<DocumentMut, String> {
    super::accounts::config_doc(text).map_err(|line| {
        copy_text(
            "beAcctMcpShare.read.unreadable",
            &[("line", &line.to_string())],
        )
    })
}

/// 文档里 `mcp_servers` 那一节 ⇒ 名 → 定义（那一节没有 ⇒ 空表；不是表 ⇒ 一句）。
fn servers_of(doc: &DocumentMut) -> Result<Map<String, Value>, String> {
    let Some(item) = super::accounts::item_at(doc, &[SERVERS_KEY]) else {
        return Ok(Map::new());
    };
    let Some(t) = item.as_table_like() else {
        return Err(copy_text("beAcctMcpShare.read.serversNotObject", &[]));
    };
    let mut out = Map::new();
    for (name, def) in t.iter() {
        let v = item_json(def).ok_or_else(|| copy_text("reason.content.unparsable", &[]))?;
        if !v.is_object() {
            return Err(copy_text("beAcctMcpShare.read.serversNotObject", &[]));
        }
        out.insert(name.to_string(), v);
    }
    Ok(out)
}

/// 注册表 `McpCells.read`：`config.toml` 原文 ⇒ 各服务器。
pub(crate) fn servers_in(text: &str) -> Result<Map<String, Value>, String> {
    servers_of(&parse(text)?)
}

/// 注册表 `McpCells.edit`：换一个服务器（`None` = 删），只动 `[mcp_servers.<名>]` 那一节（连它底下的子表）。
/// 改 ⇒ 新的一节放在原来那一节的位置；加 ⇒ 放在末尾；删一个不在的 ⇒ 原文原样。文件还不在 ⇒ 交空串，出来只有这一节。
pub(crate) fn server_set(text: &str, name: &str, def: Option<&Value>) -> Result<String, String> {
    let mut doc = parse(text)?;
    servers_of(&doc)?;
    let unreadable = || copy_text("reason.content.unparsable", &[]);
    let max_pos = max_position(doc.as_table());
    if doc.get(SERVERS_KEY).is_none() {
        let mut t = Table::new();
        t.set_implicit(true);
        doc.insert(SERVERS_KEY, Item::Table(t));
    }
    let servers = doc
        .get_mut(SERVERS_KEY)
        .and_then(Item::as_table_mut)
        .ok_or_else(unreadable)?;
    match def {
        None => {
            if servers.remove(name).is_none() {
                return Ok(text.to_string());
            }
        }
        Some(v) => {
            let Value::Object(m) = v else {
                return Err(unreadable());
            };
            let mut t = json_table(m).ok_or_else(unreadable)?;
            let at = servers
                .get(name)
                .and_then(Item::as_table)
                .and_then(Table::position)
                .unwrap_or(max_pos + 1);
            t.set_position(Some(at));
            servers.insert(name, Item::Table(t));
        }
    }
    let out = doc.to_string();
    // 末了核一次：新原文解开 ＝ 原文只换了这一条。
    let mut want = servers_in(text)?;
    match def {
        Some(v) => {
            want.insert(name.to_string(), v.clone());
        }
        None => {
            want.remove(name);
        }
    }
    if servers_in(&out)? != want {
        return Err(unreadable());
    }
    Ok(out)
}

/// 整份文档里各表记的位置里最大的那个。
fn max_position(t: &Table) -> isize {
    let mut m = t.position().unwrap_or(0);
    for (_, item) in t.iter() {
        match item {
            Item::Table(sub) => m = m.max(max_position(sub)),
            Item::ArrayOfTables(a) => {
                for sub in a.iter() {
                    m = m.max(max_position(sub));
                }
            }
            _ => {}
        }
    }
    m
}

// ───────────────────────── TOML ⇄ JSON ─────────────────────────

fn item_json(item: &Item) -> Option<Value> {
    match item {
        Item::None => None,
        Item::Value(v) => value_json(v),
        Item::Table(t) => Some(Value::Object(
            t.iter()
                .map(|(k, v)| Some((k.to_string(), item_json(v)?)))
                .collect::<Option<Map<_, _>>>()?,
        )),
        Item::ArrayOfTables(a) => Some(Value::Array(
            a.iter()
                .map(|t| item_json(&Item::Table(t.clone())))
                .collect::<Option<Vec<_>>>()?,
        )),
    }
}

fn value_json(v: &toml_edit::Value) -> Option<Value> {
    use toml_edit::Value as T;
    Some(match v {
        T::String(s) => Value::String(s.value().clone()),
        T::Integer(i) => Value::Number((*i.value()).into()),
        T::Float(f) => Value::Number(Number::from_f64(*f.value())?),
        T::Boolean(b) => Value::Bool(*b.value()),
        T::Datetime(d) => Value::String(d.value().to_string()),
        T::Array(a) => Value::Array(a.iter().map(value_json).collect::<Option<Vec<_>>>()?),
        T::InlineTable(t) => Value::Object(
            t.iter()
                .map(|(k, v)| Some((k.to_string(), value_json(v)?)))
                .collect::<Option<Map<_, _>>>()?,
        ),
    })
}

/// 一个服务器的定义 ⇒ 它那一节：标量 · 数组是键值，对象是它底下的子表（同 Codex 自己写的样子）。`null` 写不成 TOML ⇒ `None`。
fn json_table(m: &Map<String, Value>) -> Option<Table> {
    let mut t = Table::new();
    t.set_implicit(false);
    for (k, v) in m {
        let item = match v {
            Value::Object(sub) => Item::Table(json_table(sub)?),
            v => Item::Value(json_value(v)?),
        };
        t.insert(k, item);
    }
    Some(t)
}

fn json_value(v: &Value) -> Option<toml_edit::Value> {
    Some(match v {
        Value::Null => return None,
        Value::Bool(b) => (*b).into(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.into(),
            None => n.as_f64()?.into(),
        },
        Value::String(s) => s.as_str().into(),
        Value::Array(xs) => {
            let mut a = Array::new();
            for x in xs {
                a.push(json_value(x)?);
            }
            a.into()
        }
        Value::Object(m) => {
            let mut t = InlineTable::new();
            for (k, x) in m {
                t.insert(k, json_value(x)?);
            }
            t.into()
        }
    })
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/mcp_tests.rs"]
mod tests;

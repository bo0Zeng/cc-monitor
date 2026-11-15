//! Claude 的 **MCP 布局**：`.claude.json` 顶层 `mcpServers`（user）· `projects[<目录>].mcpServers`（local）·
//! `<目录>/.mcp.json` 的 `mcpServers`（project）。读法宽容：缺 ⇒ 那一段空；坏 ⇒ 那一段空并说出来（`problems`）。
//! 逐号读：各号的家里各一份 `<家>/.claude.json`；没设账号那一份找哪里与资产目录同一处（`assets::claude_json`）。
//!
//! 状态（翻成中立的 [`McpStatus`]）只出配置层说得准的两种：停用 —— `projects[<目录>].disabledMcpServers`
//! （Claude 连之前先看它）；要登录 —— 各号家目录里 [`NEEDS_AUTH_CACHE`]：Claude 连 http / sse 服务器碰到要登录时记一笔
//! `{timestamp, id?, ttlMs?}`，有效期内下次直接跳过不连（缺省 15 分钟，记了 `ttlMs` 按它；登录成功整份删）。
//! 过了有效期 Claude 会重连，那一笔就不再算数 ⇒ 这里同样不算。stdio 的 Claude 不拿这份跳过 ⇒ 不算。

use crate::platform::fs::Ro;
use cap_core::{Cap, Over};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::accounts::{config_path_in, CONFIG_CAP};
use super::assets::{PROJECT_MCP_CAP, PROJECT_MCP_FILE};
use crate::agents::{McpEntry, McpLook, McpRead, McpStatus};
use crate::common::said::IntoNote as _;

/// 在 Claude 的会话里登录 MCP 服务器的那一行（注册表 `McpFace.login`）：`/mcp`，服务器在它的菜单里选（斜杠命令不收名字）。
pub(crate) fn login_argv(_server: &str) -> Vec<String> {
    vec!["/mcp".to_string()]
}
/// 各号家目录里那份「要登录」缓存的文件名（每号各有一份的那张表 `accounts.rs::NATIVE_IDENTITY` 也列它：每号各一份）。
pub(crate) const NEEDS_AUTH_CACHE: &str = "mcp-needs-auth-cache.json";
/// 条目没带 `ttlMs` 时 Claude 的缺省有效期（用户自己配的 http / sse 那一种）。
const NEEDS_AUTH_TTL_MS: i128 = 15 * 60 * 1000;
/// Claude 容忍的钟差：记录时刻比此刻晚不到这么多也算。
const CLOCK_SLACK_MS: i128 = 60 * 1000;
/// 那份缓存的上限（一个服务器一小条）：超了那一句进返回值的 `problems`，这个号的状态判不出。
const MAX_CACHE_BYTES: Cap = Cap {
    bytes: 1024 * 1024,
    over: Over::DegradeAndSay,
};

/// 注册表那一格的实现：没设账号那一份 `.claude.json` 照这台机器的环境找，各号那一份在各号的家里。
pub(crate) fn read(project_dir: Option<&Path>, look: &McpLook) -> McpRead {
    read_at(super::assets::claude_json().as_deref(), project_dir, look)
}

/// [`read`] 的本体，逐号读：`look.homes` 里设了账号的每个家各一份 `<家>/.claude.json`，再加没设账号那一份
/// （`claude_json`，位置是参数，判据拿夹具喂它；与某个号那一份是同一个文件 ⇒ 不再读，比的是真路径）。每一份出用户级 ＋ `projects[<目录>]`
/// 那一段本地级，状态照那一份自己的停用表、照它所在那个家的「要登录」缓存判（没设账号那一份照 `look.bare` 那个家、记成 `None` 那一位，
/// 那个家同时是某个号的家也一样）。
/// 项目级 `<目录>/.mcp.json` 只读一次、各号都用它 ⇒ 读到的每一份都停用它才算停用；要登录哪个号（连没设账号那个家）的缓存记着都算。
/// 几个号里的同一服务器在这里照实各出一条，合成一条由核心 `mcp_held` 做。
pub fn read_at(claude_json: Option<&Path>, project_dir: Option<&Path>, look: &McpLook) -> McpRead {
    let mut out = McpRead::default();
    // 各号的家 ＋ 没设账号时的家（`look.bare`，与某个号同一个目录 ⇒ 只算那个号那一份）的缓存先各读一遍
    // （超上限 / 读不出 ⇒ 那一句只说一次，这个家的状态照旧判不出）。
    let accounts: Vec<(&str, &Path)> = look
        .homes
        .iter()
        .filter_map(|(who, home)| who.as_deref().map(|w| (w, home.as_path())))
        .collect();
    let mut cache_at = |home: &Path| {
        read_json(
            &home.join(NEEDS_AUTH_CACHE),
            MAX_CACHE_BYTES,
            &mut out.problems,
        )
    };
    let caches: Vec<Option<Value>> = accounts.iter().map(|(_, h)| cache_at(h)).collect();
    let bare_cache: Option<Value> = match look.bare.as_deref() {
        Some(b) => match accounts.iter().position(|(_, h)| same_file(h, b)) {
            Some(i) => caches[i].clone(),
            None => cache_at(b),
        },
        None => None,
    };
    let named: Vec<(Option<&str>, &Value)> = accounts
        .iter()
        .zip(&caches)
        .filter_map(|((who, _), c)| c.as_ref().map(|c| (Some(*who), c)))
        .collect();
    let bare_only: Vec<(Option<&str>, &Value)> = bare_cache.iter().map(|c| (None, c)).collect();
    // 读哪几份：没设账号那一份在前（同 Claude 不设账号时读的那一份），再各号的家各一份。
    let mut files: Vec<(PathBuf, Option<&str>)> = accounts
        .iter()
        .map(|(who, home)| (config_path_in(home), Some(*who)))
        .collect();
    if let Some(bare) = claude_json.filter(|b| !files.iter().any(|(f, _)| same_file(f, b))) {
        files.insert(0, (bare.to_path_buf(), None));
    }
    let key = project_dir.map(|d| d.to_string_lossy().into_owned());
    let mut offs: Vec<Vec<String>> = Vec::new();
    let mut dirs: Vec<String> = Vec::new();
    for (path, whose) in &files {
        let Some(cj) = read_json(path, CONFIG_CAP, &mut out.problems) else {
            continue;
        };
        let src = path.display().to_string();
        let mut got = Vec::new();
        push(&mut got, cj.get("mcpServers"), "user", &src);
        let mut off = Vec::new();
        if let Some(k) = &key {
            let here = cj.get("projects").and_then(|p| p.get(k.as_str()));
            push(
                &mut got,
                here.and_then(|p| p.get("mcpServers")),
                "local",
                &src,
            );
            off = here
                .and_then(|p| p.get("disabledMcpServers"))
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
        }
        // 出自哪个号的家 ⇒ 只看那个家的缓存；没设账号那一份 ⇒ 没设账号时的家（`look.bare`），不带号。
        let these: Vec<(Option<&str>, &Value)> = match whose {
            Some(w) => named
                .iter()
                .filter(|(who, _)| *who == Some(*w))
                .copied()
                .collect(),
            None => bare_only.clone(),
        };
        judge(&mut got, &off, &these, look.now_ms);
        out.entries.extend(got);
        offs.push(off);
        dirs.extend(
            cj.get("projects")
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|o| o.keys().cloned()),
        );
    }
    dirs.sort();
    dirs.dedup();
    out.dirs = dirs;
    if let Some(dir) = project_dir {
        let file = dir.join(PROJECT_MCP_FILE);
        if let Some(v) = read_json(&file, PROJECT_MCP_CAP, &mut out.problems) {
            let mut got = Vec::new();
            push(
                &mut got,
                v.get("mcpServers"),
                "project",
                &file.display().to_string(),
            );
            let off: Vec<String> = offs
                .split_first()
                .map(|(head, rest)| {
                    head.iter()
                        .filter(|n| rest.iter().all(|o| o.contains(n)))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            let all: Vec<(Option<&str>, &Value)> =
                named.iter().chain(&bare_only).copied().collect();
            judge(&mut got, &off, &all, look.now_ms);
            out.entries.extend(got);
        }
    }
    out
}

/// 两个路径是不是同一份：两边都认得出真路径 ⇒ 比真路径（`a/../a/x` 与 `a/x`、链接与它指的那一份算同一份）；认不出 ⇒ 照原样比。
fn same_file(a: &Path, b: &Path) -> bool {
    match (Ro::canonicalize(a), Ro::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// 逐条判状态：停用压过要登录（Claude 先看停用、停用的根本不连）。`caches` 是这几条该看的那几个家的缓存
/// （号名，没设账号那个家 ⇒ `None` · 缓存）；哪个家的缓存记着 ⇒ `login_in` 记上那一位。
fn judge(
    entries: &mut [McpEntry],
    disabled: &[String],
    caches: &[(Option<&str>, &Value)],
    now_ms: u64,
) {
    for e in entries {
        if disabled.contains(&e.name) {
            e.status = McpStatus::Disabled;
            continue;
        }
        let remote = matches!(
            e.server.get("type").and_then(Value::as_str),
            Some("http" | "sse")
        );
        if !remote {
            continue;
        }
        for (who, cache) in caches {
            let Some(at) = fresh_at(cache.get(&e.name), now_ms) else {
                continue;
            };
            e.seen_ms = e.seen_ms.max(Some(at));
            e.login_in.push(who.map(str::to_string));
        }
        if !e.login_in.is_empty() {
            e.status = McpStatus::NeedsLogin;
            e.login_in.sort();
            e.login_in.dedup();
        }
    }
}

/// 缓存里一条还在有效期内 ⇒ 它的时刻（epoch ms）。形状不对 ⇒ 不算（同 Claude：读不出就当没有）。
fn fresh_at(item: Option<&Value>, now_ms: u64) -> Option<u64> {
    let at = item?.get("timestamp")?.as_u64()?;
    let ttl = item?
        .get("ttlMs")
        .and_then(Value::as_u64)
        .map_or(NEEDS_AUTH_TTL_MS, i128::from);
    let age = i128::from(now_ms) - i128::from(at);
    (age > -CLOCK_SLACK_MS && age < ttl).then_some(at)
}

fn push(out: &mut Vec<McpEntry>, servers: Option<&Value>, scope: &'static str, source: &str) {
    for (name, server) in servers.and_then(Value::as_object).into_iter().flatten() {
        out.push(McpEntry {
            scope,
            name: name.clone(),
            server: server.clone(),
            source: source.to_string(),
            status: Default::default(),
            login_in: Vec::new(),
            seen_ms: None,
        });
    }
}

/// 不在 ⇒ `None`、不出声；在而读不出 / 不是 JSON ⇒ `None` 并记一句（「这台没有」与「那份坏了」不合成一句）。
fn read_json(path: &Path, cap: Cap, problems: &mut Vec<String>) -> Option<Value> {
    match Ro::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            problems.push(
                crate::common::said::Said::with_raw(
                    copy_core::copy_text(
                        "reason.fs.readFailed",
                        &[
                            ("path", &path.display().to_string()),
                            ("why", &copy_core::io_reason(e.kind())),
                        ],
                    ),
                    &e,
                )
                .into_note(),
            );
            return None;
        }
        Ok(_) => {}
    }
    match Ro::read_json(path, cap) {
        Ok(v) => Some(v),
        // 那一句只带原因词进说明；原话记日志（说明没有复制详情那一格）。
        Err(e) => {
            problems.push(
                e.wrap(|why| {
                    copy_core::copy_text(
                        "reason.fs.readFailed",
                        &[("path", &path.display().to_string()), ("why", why)],
                    )
                })
                .into_note(),
            );
            None
        }
    }
}

/// 注册表 `RecordFace.mcp_said` 那一格：「延后加载的工具变了」那种附件里的三张表（`pendingMcpServers` · `needsAuthMcpServers` ·
/// `failedMcpServers`，后者每项 `{name, error}`）。Claude Code 每次只写它这一次算了的那几张 ⇒ 没写的那一格 `None`。
pub(crate) fn said_of(v: &Value) -> Option<crate::agents::McpSaid> {
    if v.get("type").and_then(Value::as_str) != Some("attachment") {
        return None;
    }
    let a = v.get("attachment")?;
    if a.get("type").and_then(Value::as_str) != Some("deferred_tools_delta") {
        return None;
    }
    let names = |k: &str| {
        a.get(k).and_then(Value::as_array).map(|xs| {
            xs.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
    };
    let failed = a.get(FAILED).and_then(Value::as_array).map(|xs| {
        xs.iter()
            .filter_map(|x| {
                let name = x.get("name")?.as_str()?.to_string();
                Some((
                    name,
                    x.get("error").and_then(Value::as_str).map(str::to_string),
                ))
            })
            .collect::<Vec<_>>()
    });
    for k in a.as_object()?.keys() {
        if k.ends_with("McpServers") && ![PENDING, NEEDS_AUTH, FAILED].contains(&k.as_str()) {
            super::drift::record(super::drift::DriftFace::UnknownMcpList, k);
        }
    }
    Some(crate::agents::McpSaid {
        pending: names(PENDING),
        needs_login: names(NEEDS_AUTH),
        failed,
    })
}

/// 那三张表在附件里的键名（Claude Code 的字；认得的只有这三张）。
const PENDING: &str = "pendingMcpServers";
const NEEDS_AUTH: &str = "needsAuthMcpServers";
const FAILED: &str = "failedMcpServers";

// ───────────────────────── MCP 登录（各号之间同步的那一格） ─────────────────────────

/// MCP 登录：`<配置目录>/.credentials.json` 的 `mcpOAuth`，键是「服务器名|定义摘要」，一条里有令牌、过期时刻与客户端登记
/// （注册表 `AccountsFace.mcp_logins`）。与订阅号登录同一份文件。
/// 锁照 claude 自己存这一条时拿的：先那一条的续期锁 —— `<配置目录>/mcp-refresh-<键里非字母数字换成 _>.lock` 的 proper-lockfile 锁
/// （`mkdir … .lock.lock`，过期门限 60 秒），claude 续某一条时拿它、锁里先重读盘：盘上已经是新的就直接用，否则拿盘上的刷新令牌去续；
/// 再整份写锁 —— `mkdir <配置目录>/.storage-write.lock`（过期门限 15 秒），claude 整份写这份文件都在它里面
/// （10-12 沙箱实测 2.1.296）。
pub(crate) const LOGINS: crate::agents::McpLogins = crate::agents::McpLogins {
    file: super::CREDENTIALS_NAME,
    read: logins_in,
    edit: login_set,
    lock: &[
        crate::agents::McpLoginLock::EntryMkdir {
            name: renew_lock_of,
            stale_ms: 60_000,
        },
        crate::agents::McpLoginLock::WholeMkdir {
            name: super::STORAGE_WRITE_LOCK,
            stale_ms: super::STORAGE_WRITE_STALE_MS,
        },
    ],
    access: "accessToken",
    refresh: "refreshToken",
    expires_ms: "expiresAt",
};

/// MCP 登录那一节的顶层键。
const LOGINS_KEY: &str = "mcpOAuth";

fn renew_lock_of(key: &str) -> String {
    let safe: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("mcp-refresh-{safe}.lock.lock")
}

/// 整份凭据原文 ⇒ 顶层对象（不是 JSON 对象 ⇒ 一句「不是 JSON」，不带原文片段）。
fn creds_doc(text: &str) -> Result<serde_json::Map<String, Value>, String> {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(m)) => Ok(m),
        _ => Err(copy_core::copy_text("beOauthStore.read.notJson", &[])),
    }
}

/// 凭据原文 ⇒ `mcpOAuth` 那一节（不在 ⇒ 空表；不是对象 ⇒ 读不懂）。
fn logins_in(text: &str) -> Result<serde_json::Map<String, Value>, String> {
    match creds_doc(text)?.remove(LOGINS_KEY) {
        None => Ok(serde_json::Map::new()),
        Some(Value::Object(m)) => Ok(m),
        Some(_) => Err(copy_core::copy_text("reason.content.unparsable", &[])),
    }
}

/// 换 `mcpOAuth` 里那一条（`None` = 删），整份照 claude 自己存盘的写法写回（紧凑 JSON）；别的顶层格 · 别的条的值原样。
fn login_set(text: &str, key: &str, to: Option<&Value>) -> Result<String, String> {
    let mut doc = creds_doc(text)?;
    let sec = doc
        .entry(LOGINS_KEY.to_string())
        .or_insert_with(|| Value::Object(Default::default()));
    let Value::Object(m) = sec else {
        return Err(copy_core::copy_text("reason.content.unparsable", &[]));
    };
    match to {
        Some(v) => {
            m.insert(key.to_string(), v.clone());
        }
        None => {
            m.remove(key);
        }
    }
    serde_json::to_string(&Value::Object(doc))
        .map_err(|_| copy_core::copy_text("reason.content.unparsable", &[]))
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/mcp_tests.rs"]
mod tests;

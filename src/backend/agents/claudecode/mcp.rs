//! Claude 的 **MCP 布局**：`.claude.json` 顶层 `mcpServers`（user）· `projects[<目录>].mcpServers`（local）·
//! `<目录>/.mcp.json` 的 `mcpServers`（project）。读法宽容：缺 ⇒ 那一段空；坏 ⇒ 那一段空并说出来（`problems`）。
//! `.claude.json` 找哪一份与资产目录同一处（`assets::claude_json`）。
//!
//! 状态（翻成中立的 [`McpStatus`]）只出配置层说得准的两种：停用 —— `projects[<目录>].disabledMcpServers`
//! （Claude 连之前先看它）；要登录 —— 各号家目录里 [`NEEDS_AUTH_CACHE`]：Claude 连 http / sse 服务器碰到要登录时记一笔
//! `{timestamp, id?, ttlMs?}`，有效期内下次直接跳过不连（缺省 15 分钟，记了 `ttlMs` 按它；登录成功整份删）。
//! 过了有效期 Claude 会重连，那一笔就不再算数 ⇒ 这里同样不算。stdio 的 Claude 不拿这份跳过 ⇒ 不算。

use std::path::Path;

use serde_json::Value;

use super::accounts::MAX_CONFIG_BYTES;
use super::assets::{MAX_PROJECT_MCP_BYTES, PROJECT_MCP_FILE};
use crate::agents::{McpEntry, McpLook, McpRead, McpStatus};
use crate::common::said::IntoNote as _;

/// 各号家目录里那份「要登录」缓存的文件名（每号各有一份的那张表 `accounts.rs::NATIVE_IDENTITY` 也列它：每号各一份）。
pub(crate) const NEEDS_AUTH_CACHE: &str = "mcp-needs-auth-cache.json";
/// 条目没带 `ttlMs` 时 Claude 的缺省有效期（用户自己配的 http / sse 那一种）。
const NEEDS_AUTH_TTL_MS: i128 = 15 * 60 * 1000;
/// Claude 容忍的钟差：记录时刻比此刻晚不到这么多也算。
const CLOCK_SLACK_MS: i128 = 60 * 1000;
/// 那份缓存的上限（一个服务器一小条）。
const MAX_CACHE_BYTES: u64 = 1024 * 1024;

/// 注册表那一格的实现：按这台机器的环境现解 `.claude.json`。
pub(crate) fn read(project_dir: Option<&Path>, look: &McpLook) -> McpRead {
    read_at(super::assets::claude_json().as_deref(), project_dir, look)
}

/// [`read`] 的本体，`.claude.json` 的位置是参数（判据拿夹具喂它）。
pub(crate) fn read_at(
    claude_json: Option<&Path>,
    project_dir: Option<&Path>,
    look: &McpLook,
) -> McpRead {
    let mut out = McpRead::default();
    let mut disabled: Vec<String> = Vec::new();
    if let Some(path) = claude_json {
        if let Some(cj) = read_json(path, MAX_CONFIG_BYTES, &mut out.problems) {
            let src = path.display().to_string();
            push(&mut out.entries, cj.get("mcpServers"), "user", &src);
            if let Some(dir) = project_dir {
                let local = cj
                    .get("projects")
                    .and_then(|p| p.get(dir.to_string_lossy().as_ref()))
                    .and_then(|p| p.get("mcpServers"));
                push(&mut out.entries, local, "local", &src);
                disabled = cj
                    .get("projects")
                    .and_then(|p| p.get(dir.to_string_lossy().as_ref()))
                    .and_then(|p| p.get("disabledMcpServers"))
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
            }
            let mut dirs: Vec<String> = cj
                .get("projects")
                .and_then(Value::as_object)
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default();
            dirs.sort();
            out.dirs = dirs;
        }
    }
    if let Some(dir) = project_dir {
        let file = dir.join(PROJECT_MCP_FILE);
        if let Some(v) = read_json(&file, MAX_PROJECT_MCP_BYTES, &mut out.problems) {
            push(
                &mut out.entries,
                v.get("mcpServers"),
                "project",
                &file.display().to_string(),
            );
        }
    }
    judge(&mut out, &disabled, look);
    out
}

/// 逐条判状态：停用压过要登录（Claude 先看停用、停用的根本不连）。
fn judge(out: &mut McpRead, disabled: &[String], look: &McpLook) {
    let caches: Vec<(Option<&str>, Value)> = look
        .homes
        .iter()
        .filter_map(|(who, home)| {
            read_json(
                &home.join(NEEDS_AUTH_CACHE),
                MAX_CACHE_BYTES,
                &mut out.problems,
            )
            .map(|v| (who.as_deref(), v))
        })
        .collect();
    for e in &mut out.entries {
        if disabled.iter().any(|d| *d == e.name) {
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
        let mut hit = false;
        for (who, cache) in &caches {
            let Some(at) = fresh_at(cache.get(&e.name), look.now_ms) else {
                continue;
            };
            hit = true;
            e.seen_ms = e.seen_ms.max(Some(at));
            if let Some(w) = who {
                e.login_in.push((*w).to_string());
            }
        }
        if hit {
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
fn read_json(path: &Path, cap: u64, problems: &mut Vec<String>) -> Option<Value> {
    match std::fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            problems.push(
                crate::common::said::Said::with_raw(
                    copy_core::copy_text(
                        "beMcp.read.statFailed",
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
    match crate::common::fs::read_json_capped(path, cap) {
        Ok(v) => Some(v),
        // 那一句只带原因词进说明；原话记日志（说明没有复制详情那一格）。
        Err(e) => {
            problems.push(
                e.wrap(|why| {
                    copy_core::copy_text(
                        "beMcp.read.readFailed",
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
            super::drift::record(super::drift::DriftFace::UnknownMcpList, k, None);
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

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/mcp_tests.rs"]
mod tests;

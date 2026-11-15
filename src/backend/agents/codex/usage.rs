//! Codex 报一个号的用量与开窗那一句（注册表 `DefaultUpstream.usage` · `.warm` 那两格）。
//!
//! 报用量不花模型额度：起一次性 app-server（[`super::app_server`]），握手后发 `account/rateLimits/read`，读到回包就收掉；
//! 进程由通用的报用量那一路起、喂、读。回包形（codex-rs `rust-v0.167.0` 的 `GetAccountRateLimitsResponse`，camelCase）：
//! `rateLimits`（单桶那一份）· `rateLimitsByLimitId`（多桶，键是 `limitId`）；一桶里 `primary` · `secondary` 两窗各是
//! `{usedPercent, windowDurationMins, resetsAt}`（`resetsAt` 是 unix 秒）。读默认那一族（`codex`）：多桶里有它就读它，否则读单桶那一份。
//!
//! 开窗：`codex exec --ephemeral --skip-git-repo-check <一句>`（不落会话记录；目录不是 git 仓也照跑）。

use super::app_server;
use crate::agents::{QuotaWindow, UsageFace, WarmFace};
use serde_json::Value;

/// 注册表 `DefaultUpstream.usage` 那一格。
pub(crate) const FACE: UsageFace = UsageFace {
    program: app_server::PROGRAM,
    args: app_server::ARGS,
    dir_env: super::parse::HOME_ENV,
    // 会盖掉那个号自己登录的那几格（同登录那一面）。
    env_remove: super::sign_in::FACE.env_remove,
    env_set: &[],
    stdin: &[
        app_server::INITIALIZE,
        app_server::INITIALIZED,
        RATE_LIMITS_READ,
    ],
    enough: Some(app_server::is_reply),
    read,
};

/// 读额度那一条请求（不带参数）。
const RATE_LIMITS_READ: &str = r#"{"id":2,"method":"account/rateLimits/read"}"#;

/// 注册表 `DefaultUpstream.warm` 那一格。
pub(crate) const WARM: WarmFace = WarmFace {
    args: &[
        "exec",
        "--ephemeral",
        "--skip-git-repo-check",
        "Reply with the single letter a.",
    ],
    env_set: &[],
};

/// 输出 ⇒ 默认那一族的两窗；回包是 `error` · 没等到 · 一窗都没带 · 不是默认那一族 ⇒ 读不懂（不猜）。
fn read(stdout: &str, _now: u64) -> Result<Vec<QuotaWindow>, String> {
    let result = app_server::reply_in(stdout)?;
    let snap = result
        .get("rateLimitsByLimitId")
        .and_then(|m| m.get(super::parse::DEFAULT_LIMIT))
        .or_else(|| result.get("rateLimits"))
        .filter(|s| s.is_object())
        .ok_or_else(|| "rateLimits".to_string())?;
    if snap
        .get("limitId")
        .and_then(Value::as_str)
        .is_some_and(|id| id != super::parse::DEFAULT_LIMIT)
    {
        return Err("rateLimits.limitId".to_string());
    }
    let windows: Vec<QuotaWindow> = super::parse::SIDES
        .iter()
        .filter_map(|side| {
            let w = snap.get(*side).filter(|w| w.is_object())?;
            let used = w.get("usedPercent")?.as_f64().filter(|n| n.is_finite())?;
            Some(super::parse::window(
                side,
                used,
                w.get("windowDurationMins").and_then(Value::as_i64),
                w.get("resetsAt").and_then(Value::as_u64),
            ))
        })
        .collect();
    if windows.is_empty() {
        return Err("rateLimits.primary".to_string());
    }
    Ok(windows)
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/usage_tests.rs"]
mod tests;

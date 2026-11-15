//! Codex 按轮分叉（注册表 `RecordFace.branch` 的自己叉那一形）：`codex fork` 只叉整条 ⇒ 经一次性 app-server
//! （[`super::app_server`]）发 `thread/fork{threadId, lastTurnId}`，新会话含那一轮为止；新会话的 rollout 由 Codex 自己写。
//!
//! 依据（codex-rs，核到 tag `rust-v0.167.0`）：`lastTurnId` 不是实验字段（包含那一轮、之后的不要；那一轮不能还在跑）；
//! `experimentalPredictionMode` · `experimentalDisablePredictionMcp` · `beforeTurnId` · `path` 都是实验的、不发。
//! 回包 `ThreadForkResponse.thread.id` 是新会话。
//!
//! 哪一轮：叉在的那一条消息前面最近的那个 `task_started` 的 `turn_id`（[`super::record::turn_id`]）；
//! 那一条照记录页认 id 的同一个读法认（[`super::record::record_id`]）。

use crate::agents::{ForkFail, OwnFork};
use crate::platform::fs::Ro;
use serde_json::{json, Value};
use std::io::BufRead;
use std::path::Path;

/// 注册表那一格：找源会话 · 认那一轮 · 叫 Codex 叉 ⇒ 新会话的 sid。
pub(crate) fn fork(f: &OwnFork<'_>) -> Result<String, ForkFail> {
    let root = super::parse::sessions_root(f.home);
    let file = super::history::find_session(&root, f.sid).map_err(|e| {
        tracing::debug!("codex fork: {e}");
        ForkFail::Failed
    })?;
    let turn = turn_of(&file, f.message).ok_or_else(|| ForkFail::NoTurn {
        message: f.message.to_string(),
    })?;
    let result = super::app_server::ask(f.home, "thread/fork", &fork_params(f.sid, &turn))
        .map_err(|e| {
            tracing::debug!("codex fork: {e}");
            ForkFail::Failed
        })?;
    forked_id(&result).ok_or_else(|| {
        tracing::debug!("codex fork: reply without thread.id");
        ForkFail::Failed
    })
}

/// 记录里 id 是 `message` 的那一行所在的那一轮（它前面最近的 `task_started`）；第一轮之前 · 记录里没有 ⇒ `None`。
pub(crate) fn turn_of(file: &Path, message: &str) -> Option<String> {
    let mut r = std::io::BufReader::new(Ro::open(file).ok()?);
    let (mut start, mut turn) = (0u64, None::<String>);
    let mut line = String::new();
    loop {
        line.clear();
        let n = r.read_line(&mut line).ok()?;
        if n == 0 {
            return None;
        }
        if let Ok(v) = serde_json::from_str::<Value>(line.trim_end()) {
            if let Some(t) = super::record::turn_id(&v) {
                turn = Some(t.to_string());
            } else if super::record::record_id(&v, start) == message {
                return turn;
            }
        }
        start += n as u64;
    }
}

/// 发给 app-server 的那一条的参数：只有源会话与叉到哪一轮。
pub(crate) fn fork_params(sid: &str, turn: &str) -> Value {
    json!({"threadId": sid, "lastTurnId": turn})
}

/// 回包里新会话的 id（`thread.id`，非空串）。
pub(crate) fn forked_id(result: &Value) -> Option<String> {
    result
        .get("thread")?
        .get("id")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/codex/branch_tests.rs"]
mod tests;

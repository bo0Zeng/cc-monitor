//! Claude 的进程状态文件（`<家>/sessions/<PID>.json`）里两格的读法：`kind`（是不是后台会话）· `status`（此刻在干什么）。
//! 通用层只拿这里翻好的结论（`agents::pidfile_background` · `agents::pidfile_activity`），不认 Claude 的词。

use super::drift::{record, DriftFace};
use crate::agents::SessionActivity;
use serde_json::Value;

/// 人坐在终端里对话的那种会话。
const INTERACTIVE: &str = "interactive";
/// 后台任务（`--fork-session --resume` 起的那种）。
const BACKGROUND: &str = "bg";

/// 是不是后台会话：`kind` 在且不是 `interactive`。不写 `kind` ⇒ 交互。
/// 认不出的 `kind` 也当后台（只有 `interactive` 成 tab），并记一笔漂移账。
pub(crate) fn background_of(v: &Value) -> bool {
    match v.get("kind").and_then(Value::as_str) {
        None | Some(INTERACTIVE) => false,
        Some(BACKGROUND) => true,
        Some(other) => {
            record(DriftFace::UnknownSessionKind, other, None);
            true
        }
    }
}

/// 此刻在干什么：`busy` ⇒ 在干活 · `waiting` ⇒ 等人 · `idle` / `shell` ⇒ 闲着。
/// 没写 ⇒ `None`；认不出的词 ⇒ `None`（说不清，不猜）并记一笔漂移账。
pub(crate) fn activity_of(v: &Value) -> Option<SessionActivity> {
    match v.get("status").and_then(Value::as_str)? {
        "busy" => Some(SessionActivity::Working),
        "waiting" => Some(SessionActivity::NeedsYou),
        "idle" | "shell" => Some(SessionActivity::Idle),
        other => {
            record(DriftFace::UnknownSessionStatus, other, None);
            None
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/pidfile_tests.rs"]
mod tests;

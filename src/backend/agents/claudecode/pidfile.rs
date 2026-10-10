//! Claude 的进程状态文件（`<家>/sessions/<PID>.json`）里两格的读法：`kind`（是不是后台会话）· `status`（此刻在干什么）。
//! 通用层只拿这里翻好的结论（`agents::pidfile_background` · `agents::pidfile_activity`），不认 Claude 的词。

use super::drift::{record, DriftFace};
use crate::agents::{SessionActivity, WaitOn};
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

/// 此刻在干什么：`busy` ⇒ 在干活 · `waiting` ⇒ 等人 · `idle` ⇒ 闲着 ·
/// `shell` ⇒ 一轮停了、它在后台起的命令还在跑（那一家：空闲且有没跑完的后台 shell 任务；不是「claude 退了只剩 shell」）。
/// 没写 ⇒ `None`；认不出的词 ⇒ `None`（说不清，不猜）并记一笔漂移账。
pub(crate) fn activity_of(v: &Value) -> Option<SessionActivity> {
    match v.get("status").and_then(Value::as_str)? {
        "busy" => Some(SessionActivity::Working),
        "waiting" => Some(SessionActivity::NeedsYou),
        "idle" => Some(SessionActivity::Idle),
        "shell" => Some(SessionActivity::BackgroundWork),
        other => {
            record(DriftFace::UnknownSessionStatus, other, None);
            None
        }
    }
}

/// 在等人时等的是什么框（`waitingFor`）：那一家写的全集就这六个词（它自己把前四个归「权限」类、`input needed` 归「提问」类、
/// `dialog open` 不归类）。没写 ⇒ `None`；认不出的词 ⇒ `None`（说不清，不猜）并记一笔漂移账。
pub(crate) fn wait_of(v: &Value) -> Option<WaitOn> {
    match v.get("waitingFor").and_then(Value::as_str)? {
        "permission prompt" => Some(WaitOn::Permission),
        "sandbox request" => Some(WaitOn::Network),
        "worker request" => Some(WaitOn::Worker),
        "goal proposal" => Some(WaitOn::Goal),
        "input needed" => Some(WaitOn::Input),
        "dialog open" => Some(WaitOn::Dialog),
        other => {
            record(DriftFace::UnknownWaitingFor, other, None);
            None
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/pidfile_tests.rs"]
mod tests;

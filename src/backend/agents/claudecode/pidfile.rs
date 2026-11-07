//! Claude 的进程状态文件（`<家>/sessions/<PID>.json`）怎么读：一份 ⇒ 一行 [`ProcFact`]（[`proc_of`]，注册表 `LocalFace.proc_of`）。
//! 字段名与那几格的词只住这里；通用层只拿翻好的那一行（`agents::proc_fact`），不认 Claude 的词。

use super::drift::{record, DriftFace};
use crate::agents::{ProcFact, SessionActivity, WaitOn};
use serde_json::Value;

/// 一份进程状态文件 ⇒ 它说的那一份事实。各格照那一家写的原样取：串格不是串 ⇒ 没有；
/// `procStart` 是 starttime ticks 的十进制串（也容忍裸数字）；`attachable` 只认真布尔（字符串 `"false"` 之类当没写）。
pub(crate) fn proc_of(v: &Value) -> ProcFact {
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    ProcFact {
        kind: super::AGENT_KIND,
        sid: s("sessionId"),
        cwd: s("cwd"),
        name: s("name"),
        proc_start: proc_start_of(v),
        background: background_of(v),
        activity: activity_of(v),
        status_word: s("status"),
        wait: wait_of(v),
        waiting_word: s("waitingFor"),
        attachable: v.get("attachable").and_then(Value::as_bool),
        status_since_ms: v.get("statusUpdatedAt").and_then(Value::as_u64),
        started_ms: v.get("startedAt").and_then(Value::as_u64),
    }
}

/// `procStart`：Linux 上 CC 照抄 `/proc/<pid>/stat` 的 starttime ticks，写成十进制串；也容忍裸数字。别的 ⇒ `None`。
fn proc_start_of(v: &Value) -> Option<u64> {
    let field = v.get("procStart")?;
    if let Some(s) = field.as_str() {
        return s.trim().parse::<u64>().ok();
    }
    field.as_u64()
}

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
            record(DriftFace::UnknownSessionKind, other);
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
            record(DriftFace::UnknownSessionStatus, other);
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
            record(DriftFace::UnknownWaitingFor, other);
            None
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/pidfile_tests.rs"]
mod tests;

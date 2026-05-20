use serde::Serialize;

pub mod events {
    pub const JSONL_LINE: &str = "jsonl-line";
    pub const SESSION_ENDED: &str = "session-ended";
    // FOCUS_SWITCH 已删除：Win11 默认终端 (WindowsTerminal.exe) 是单进程多窗口架构，
    // OS GetForegroundWindow 只能拿到 WT 主进程 PID，无法区分 tab/window 内跑哪个
    // claude session。在 WT 默认环境下永远不工作；非 WT 终端可工作但不值为少数场景维护。
    //
    // SUBAGENT_LINE 已废弃：subagent 不走实时 watcher，由前端 invoke
    // `load_subagent` 在用户展开 Task 折叠卡时按需加载。
}

#[derive(Debug, Serialize, Clone)]
pub struct JsonlLinePayload {
    pub session_id: String,
    pub cwd: Option<String>,
    pub path: String,
    pub message: crate::messages::JsonlRecord,
}

#[derive(Debug, Serialize, Clone)]
pub struct SessionEndedPayload {
    pub session_id: String,
}

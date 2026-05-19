use serde::Serialize;

pub mod events {
    pub const JSONL_LINE: &str = "jsonl-line";
    pub const FOCUS_SWITCH: &str = "focus-switch";
    pub const SESSION_ENDED: &str = "session-ended";
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
pub struct FocusSwitchPayload {
    pub session_id: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct SessionEndedPayload {
    pub session_id: String,
}

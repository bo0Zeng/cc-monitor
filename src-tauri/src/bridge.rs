use serde::Serialize;

pub mod events {
    pub const JSONL_LINE: &str = "jsonl-line";
    pub const FOCUS_SWITCH: &str = "focus-switch";
    pub const SUBAGENT_LINE: &str = "subagent-line";
    pub const SESSION_ENDED: &str = "session-ended";
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

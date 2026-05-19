use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type")]
pub enum JsonlRecord {
    #[serde(rename = "user")]
    User {
        uuid: String,
        timestamp: String,
        message: ApiMessage,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(rename = "sessionId", default)]
        session_id: Option<String>,
        #[serde(rename = "isSidechain", default)]
        is_sidechain: bool,
        #[serde(rename = "parentUuid", default)]
        parent_uuid: Option<String>,
    },
    #[serde(rename = "assistant")]
    Assistant {
        uuid: String,
        timestamp: String,
        message: ApiMessage,
        #[serde(rename = "sessionId", default)]
        session_id: Option<String>,
        #[serde(rename = "isSidechain", default)]
        is_sidechain: bool,
        #[serde(rename = "requestId", default)]
        request_id: Option<String>,
        #[serde(rename = "parentUuid", default)]
        parent_uuid: Option<String>,
    },

    #[serde(rename = "ai-title")]
    AiTitle {
        #[serde(rename = "aiTitle")]
        ai_title: String,
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    #[serde(rename = "system")]
    System {
        #[serde(default)]
        subtype: Option<String>,
        #[serde(rename = "durationMs", default)]
        duration_ms: Option<u64>,
        #[serde(rename = "messageCount", default)]
        message_count: Option<u32>,
        timestamp: String,
        #[serde(rename = "sessionId", default)]
        session_id: Option<String>,
    },

    #[serde(rename = "attachment")]
    Attachment {},
    #[serde(rename = "permission-mode")]
    PermissionMode {},
    #[serde(rename = "last-prompt")]
    LastPrompt {},
    #[serde(rename = "file-history-snapshot")]
    FileHistorySnapshot {},

    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ApiMessage {
    pub role: String,
    pub content: serde_json::Value,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(rename = "cache_creation_input_tokens", default)]
    pub cache_creation: u32,
    #[serde(rename = "cache_read_input_tokens", default)]
    pub cache_read: u32,
    #[serde(default)]
    pub output_tokens: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
        #[serde(default)]
        signature: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: serde_json::Value,
        #[serde(default)]
        is_error: bool,
    },
}

impl JsonlRecord {
    pub fn is_displayable(&self) -> bool {
        matches!(
            self,
            Self::User { .. }
                | Self::Assistant { .. }
                | Self::AiTitle { .. }
                | Self::System { .. }
        )
    }
}

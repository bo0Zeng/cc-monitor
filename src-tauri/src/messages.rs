use serde::{Deserialize, Serialize};

/// issue #12: jsonl 顶层 `forkedFrom` 字段 —— `/branch` 命令分叉出新 session 时
/// 写入。`sessionId` 是 parent session 的 sessionId，`messageUuid` 是被 fork 处
/// 的 parent 消息 uuid（指明从哪条消息后开始分叉）。
///
/// 典型情况下整个 session 的所有记录共享同一个 forkedFrom（一次性写入元数据）。
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ForkedFrom {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "messageUuid")]
    pub message_uuid: String,
}

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
        // issue #12: fork session 的所有记录都带这个字段；非 fork session 缺失
        #[serde(rename = "forkedFrom", default)]
        forked_from: Option<ForkedFrom>,
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
        // issue #12: 同 User 的 forked_from
        #[serde(rename = "forkedFrom", default)]
        forked_from: Option<ForkedFrom>,
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
        // issue #8: system 记录大多有 uuid+parentUuid 并参与 jsonl 链 ——
        // 前端 BranchFolder 需要拿到它才能完整算 ESC 回退主线。Option 兜没有这些字段的少数情况。
        #[serde(default)]
        uuid: Option<String>,
        #[serde(rename = "parentUuid", default)]
        parent_uuid: Option<String>,
    },

    // issue #8: attachment 不渲染卡片，但有 uuid+parentUuid 并夹在 user→assistant
    // 之间（实测 5% 的 user/assistant 直接 parent 是 attachment）。如果不把它
    // emit 给前端，前端的 parent 链就断在 attachment 处 → 主线检测全部失败 →
    // 整段消息被错误折叠到"已被 ESC 回退"。所以本变体含完整字段且进 is_displayable()。
    #[serde(rename = "attachment")]
    Attachment {
        uuid: String,
        timestamp: String,
        #[serde(rename = "parentUuid", default)]
        parent_uuid: Option<String>,
    },
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

/// ApiMessage.content 的强类型 schema（仅作文档参考）。当前 monitor 反序列化
/// `content` 为 `serde_json::Value`，TS 端做形状判断（详 `src/cards/index.ts`）。
/// 保留此类型供后续做 Rust 端 typed parsing 时使用，无外部调用方。
#[allow(dead_code)]
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
    /// 是否应该被 emit 到前端。
    ///
    /// 两类记录都返回 true：
    /// 1. 渲染目标：User / Assistant / AiTitle / System —— 前端会建卡 / 改标题等
    /// 2. 仅链路用：Attachment —— 不渲染，但 issue #8 ESC 回退主线检测
    ///    需要完整 uuid+parentUuid 链，attachment 夹在 user/assistant 之间，
    ///    不 emit 会让前端 parent 链断成碎片 → 主线全错 → 全部消息被错折叠
    pub fn is_displayable(&self) -> bool {
        matches!(
            self,
            Self::User { .. }
                | Self::Assistant { .. }
                | Self::AiTitle { .. }
                | Self::System { .. }
                | Self::Attachment { .. }
        )
    }
}

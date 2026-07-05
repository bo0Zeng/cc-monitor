//! Claude Code `projects/**/*.jsonl` 单行记录的 Rust schema。
//!
//! `JsonlRecord` enum 按 `type` 字段反序列化（user / assistant / system / summary /
//! ai-title / attachment / permission-mode / last-prompt / file-history-snapshot 等），
//! 未知 type 用 `#[serde(other)] Unknown` 兜底——遵循 INVARIANT § 18「宽容 schema」：
//! 非核心字段一律 `Option<T>` / `#[serde(default)]`，避免 Claude Code 写法变动导致整行解析失败。
//!
//! 这些类型在前端 `cards/index.ts` 有对应的 TS 镜像（ApiMessage / ContentBlock 等）。

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
        // Claude Code 注入的 meta 消息（skill/command 展开的 prompt、system-reminder、
        // caveat 等）带 isMeta:true —— 不是用户真正输入。仍 emit（它含 uuid+parentUuid，
        // 是 parent 链一环，漏掉会断链同 attachment #8），但前端据此 flag 跳过建卡
        // （cards/index.ts renderMessage），否则整段 skill prompt 会当用户气泡渲染。
        #[serde(rename = "isMeta", default)]
        is_meta: bool,
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
        // issue #21: API 最终失败时 CLI 写的合成 assistant 消息（重试耗尽 / 不可重试）。
        // isApiErrorMessage:true 是判定主键；error 是机器可读分类（"authentication_failed"
        // / "invalid_request" / "server_error" / "unknown" 等，勿穷举）；apiErrorStatus
        // 仅 HTTP 类错误有。报错文本在 message.content[0].text。前端据此渲染红色报错卡，
        // 否则会被当普通 assistant 回复（用户误以为 LLM 还在跑）。
        //
        // error 用 Value 而非 String：实测 47 条全是 string，但 system 侧同名字段就是
        // 对象——若某版 CLI 把它写成对象而这里钉死 String，serde 整行失败 → 这条报错
        // 消息本身被吞（报错可见化 feature 被报错字段漂移杀掉）。§18 宽容 schema。
        #[serde(rename = "isApiErrorMessage", default)]
        is_api_error_message: bool,
        #[serde(default)]
        error: Option<serde_json::Value>,
        #[serde(rename = "apiErrorStatus", default)]
        api_error_status: Option<u32>,
    },

    #[serde(rename = "ai-title")]
    AiTitle {
        #[serde(rename = "aiTitle")]
        ai_title: String,
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    // Claude Code v2.1.x 起把 ai-title schema 改为 custom-title / customTitle
    // （旧 ai-title 在历史 jsonl 里仍可能出现，两个都保留）。前端按相同语义
    // 处理 —— 写到同一个 Tab 标题字段。
    #[serde(rename = "custom-title")]
    CustomTitle {
        #[serde(rename = "customTitle")]
        custom_title: String,
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
        // issue #21: subtype="api_error"（每次 API 调用失败将重试时写一条）。level
        // 实测只有 "error"；retryAttempt/maxRetries 给前端渲染「重试中 N/M」；error
        // 对象有两种 shape（随 CLI 版本变化，新版有现成的 .formatted 一行文案）——
        // 用 Value 透传，前端防御性取字段。
        #[serde(default)]
        level: Option<String>,
        #[serde(rename = "retryAttempt", default)]
        retry_attempt: Option<u32>,
        #[serde(rename = "maxRetries", default)]
        max_retries: Option<u32>,
        #[serde(default)]
        error: Option<serde_json::Value>,
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
    /// Batch10-F31 (issue #36)：CC 2.1.x 队列操作记录。enqueue 带 content——前端
    /// 用它豁免"被消费的队列消息"（永久裸 user 叶，CC 的回复链挂在 interrupt 叶
    /// 下而非队列消息下）不被 ESC 回退折叠。无 uuid/parentUuid，不渲染卡片。
    #[serde(rename = "queue-operation")]
    QueueOperation {
        #[serde(default)]
        operation: Option<String>,
        #[serde(default)]
        content: Option<String>,
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
                | Self::CustomTitle { .. }
                | Self::System { .. }
                | Self::Attachment { .. }
                | Self::QueueOperation { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> JsonlRecord {
        serde_json::from_str(line).unwrap_or_else(|e| {
            panic!("parse failed for {line}: {e}");
        })
    }

    /// Batch10-F31 (issue #36)：queue-operation 解析（真实样本行）+ displayable。
    #[test]
    fn queue_operation_parses_and_is_displayable() {
        let r = parse(
            r#"{"type": "queue-operation", "operation": "enqueue", "timestamp": "2026-07-05T06:12:29.248Z", "sessionId": "0cbbdbae", "content": "这是登录门户"}"#,
        );
        match &r {
            JsonlRecord::QueueOperation { operation, content } => {
                assert_eq!(operation.as_deref(), Some("enqueue"));
                assert_eq!(content.as_deref(), Some("这是登录门户"));
            }
            other => panic!("expected QueueOperation, got {other:?}"),
        }
        assert!(r.is_displayable(), "要 emit 给前端做折叠豁免");
        // dequeue（无 content）也能安全解析
        let r = parse(
            r#"{"type": "queue-operation", "operation": "dequeue", "timestamp": "t", "sessionId": "s"}"#,
        );
        assert!(matches!(
            r,
            JsonlRecord::QueueOperation { content: None, .. }
        ));
    }

    #[test]
    fn user_minimal_golden_sample_parses() {
        // 仅含必填字段（uuid / timestamp / message）的 user 行也应反序列化成功；
        // 其余字段（cwd / sessionId / isSidechain / parentUuid / forkedFrom）走 default
        let line = r#"{
            "type":"user",
            "uuid":"u-1",
            "timestamp":"2026-05-20T01:23:45.678Z",
            "message":{"role":"user","content":"hi"}
        }"#;
        let r = parse(line);
        assert!(r.is_displayable());
        match r {
            JsonlRecord::User {
                uuid,
                is_sidechain,
                is_meta,
                forked_from,
                ..
            } => {
                assert_eq!(uuid, "u-1");
                assert!(!is_sidechain, "isSidechain 缺省应 false");
                assert!(!is_meta, "isMeta 缺省应 false");
                assert!(forked_from.is_none(), "forkedFrom 缺省应 None");
            }
            other => panic!("expected User, got {other:?}"),
        }
    }

    #[test]
    fn assistant_api_error_message_fields_parse() {
        // issue #21：API 最终失败的合成 assistant 消息。isApiErrorMessage 是前端
        // 渲染红色报错卡的判定主键，error/apiErrorStatus 是辅助展示字段。
        let line = r#"{
            "type":"assistant",
            "uuid":"a-err",
            "timestamp":"2026-06-12T01:00:00Z",
            "parentUuid":"prev",
            "isApiErrorMessage":true,
            "error":"authentication_failed",
            "apiErrorStatus":403,
            "message":{"role":"assistant","model":"<synthetic>","content":[{"type":"text","text":"Please run /login · API Error: 403 Request not allowed"}]}
        }"#;
        let r = parse(line);
        assert!(r.is_displayable());
        match r {
            JsonlRecord::Assistant {
                is_api_error_message,
                error,
                api_error_status,
                ..
            } => {
                assert!(
                    is_api_error_message,
                    "isApiErrorMessage:true 必须解析为 true"
                );
                assert_eq!(error, Some(serde_json::json!("authentication_failed")));
                assert_eq!(api_error_status, Some(403));
            }
            other => panic!("expected Assistant, got {other:?}"),
        }
        // §18 类型漂移容忍：error 写成对象（同 system 侧 shape）整行仍须可解析——
        // 钉死 String 会让这条报错消息本身被 serde 吞掉。
        let drifted = parse(
            r#"{"type":"assistant","uuid":"a-2","timestamp":"2026-06-12T01:00:00Z","isApiErrorMessage":true,"error":{"status":500},"message":{"role":"assistant","content":[{"type":"text","text":"API Error: 500"}]}}"#,
        );
        match drifted {
            JsonlRecord::Assistant {
                is_api_error_message,
                error,
                ..
            } => {
                assert!(is_api_error_message);
                assert!(error.is_some(), "对象形态的 error 应透传不丢行");
            }
            other => panic!("expected Assistant, got {other:?}"),
        }
        // 普通 assistant 缺省应 false/None（不误判成报错卡）
        let normal = parse(
            r#"{"type":"assistant","uuid":"a-1","timestamp":"2026-06-12T01:00:00Z","message":{"role":"assistant","content":"hi"}}"#,
        );
        match normal {
            JsonlRecord::Assistant {
                is_api_error_message,
                error,
                ..
            } => {
                assert!(!is_api_error_message);
                assert!(error.is_none());
            }
            other => panic!("expected Assistant, got {other:?}"),
        }
    }

    #[test]
    fn system_api_error_retry_fields_parse() {
        // issue #21：API 调用失败将重试的中间态。retryAttempt/maxRetries 给前端
        // 渲染「重试 N/M」，error 对象 shape 随 CLI 版本变化 → Value 透传。
        let line = r#"{
            "type":"system",
            "subtype":"api_error",
            "level":"error",
            "retryAttempt":2,
            "maxRetries":10,
            "retryInMs":521.4,
            "timestamp":"2026-06-12T01:00:00Z",
            "uuid":"sys-err",
            "parentUuid":"prev",
            "error":{"formatted":"529 Overloaded","status":529}
        }"#;
        let r = parse(line);
        assert!(r.is_displayable());
        match r {
            JsonlRecord::System {
                subtype,
                level,
                retry_attempt,
                max_retries,
                error,
                ..
            } => {
                assert_eq!(subtype.as_deref(), Some("api_error"));
                assert_eq!(level.as_deref(), Some("error"));
                assert_eq!(retry_attempt, Some(2));
                assert_eq!(max_retries, Some(10));
                let e = error.expect("error 对象应透传");
                assert_eq!(e["formatted"], "529 Overloaded");
            }
            other => panic!("expected System, got {other:?}"),
        }
    }

    #[test]
    fn user_is_meta_flag_parses_and_stays_displayable() {
        // Claude Code 注入的 meta user 消息（skill/command 展开 prompt、system-reminder、
        // caveat 等）带 isMeta:true —— 不是用户真输入。必须解析出 is_meta，前端据此跳过
        // 建卡（否则 /code-review 等 skill 的整段 prompt 会当用户气泡渲染）。仍 displayable：
        // 它含 uuid+parentUuid 是 parent 链一环，漏 emit 会断链（同 attachment #8）。
        let line = r#"{
            "type":"user",
            "uuid":"u-meta",
            "timestamp":"2026-06-08T01:00:00Z",
            "isMeta":true,
            "parentUuid":"prev-uuid",
            "message":{"role":"user","content":[{"type":"text","text":"You are reviewing for recall..."}]}
        }"#;
        let r = parse(line);
        assert!(r.is_displayable(), "meta user 仍须 emit 保 parent 链完整");
        match r {
            JsonlRecord::User {
                is_meta,
                uuid,
                parent_uuid,
                ..
            } => {
                assert!(is_meta, "isMeta:true 必须解析为 true");
                assert_eq!(uuid, "u-meta");
                assert_eq!(parent_uuid.as_deref(), Some("prev-uuid"));
            }
            other => panic!("expected User, got {other:?}"),
        }
    }

    #[test]
    fn custom_title_v21_schema_hits_custom_title_variant() {
        // v2.4.3 真实事故回归测试：Claude Code v2.1.x 把 ai-title → custom-title /
        // customTitle。messages.rs 若漏 CustomTitle 变体，整个 type 走 Unknown →
        // 不 emit → 前端拿不到标题（Tab 永远只显示项目名）。
        let line = r#"{"type":"custom-title","customTitle":"我的会话","sessionId":"s-42"}"#;
        let r = parse(line);
        assert!(r.is_displayable());
        match r {
            JsonlRecord::CustomTitle {
                custom_title,
                session_id,
            } => {
                assert_eq!(custom_title, "我的会话");
                assert_eq!(session_id, "s-42");
            }
            other => panic!("v2.1.x custom-title 未命中 CustomTitle 变体，got {other:?}"),
        }
    }

    #[test]
    fn ai_title_legacy_schema_still_works() {
        // 历史 jsonl 仍可能有 ai-title（v2.0 及之前）。两个 schema 必须共存兼容。
        let line = r#"{"type":"ai-title","aiTitle":"old","sessionId":"s-1"}"#;
        let r = parse(line);
        assert!(matches!(r, JsonlRecord::AiTitle { .. }));
        assert!(r.is_displayable());
    }

    #[test]
    fn attachment_preserves_uuid_chain_and_is_displayable() {
        // issue #8: attachment 不渲染但**必须 emit**——前端 BranchFolder 需要
        // attachment 的 uuid+parentUuid 才能完整跟 parent 链。漏 emit → ESC 回退
        // 误判 → 整段消息被错误折叠到"已被回退"。
        let line = r#"{
            "type":"attachment",
            "uuid":"att-1",
            "timestamp":"2026-05-20T01:00:00Z",
            "parentUuid":"prev-msg-uuid"
        }"#;
        let r = parse(line);
        // 先校验 displayable（不 move r），再 destructure 取字段
        assert!(r.is_displayable(), "attachment 必须 emit 保 parent 链完整");
        match r {
            JsonlRecord::Attachment {
                uuid, parent_uuid, ..
            } => {
                assert_eq!(uuid, "att-1");
                assert_eq!(parent_uuid.as_deref(), Some("prev-msg-uuid"));
            }
            other => panic!("expected Attachment, got {other:?}"),
        }
    }

    #[test]
    fn unknown_type_does_not_panic_and_not_displayable() {
        let r = parse(r#"{"type":"future-unknown-type","x":1}"#);
        assert!(matches!(r, JsonlRecord::Unknown));
        assert!(!r.is_displayable());
    }

    #[test]
    fn system_record_keeps_uuid_for_branch_detection() {
        // issue #8 配套：system 大多有 uuid+parentUuid 参与 parent 链。
        let line = r#"{
            "type":"system",
            "subtype":"turn_duration",
            "durationMs":1234,
            "timestamp":"2026-05-20T01:00:00Z",
            "uuid":"sys-1",
            "parentUuid":"prev"
        }"#;
        let r = parse(line);
        match r {
            JsonlRecord::System {
                uuid,
                parent_uuid,
                duration_ms,
                ..
            } => {
                assert_eq!(uuid.as_deref(), Some("sys-1"));
                assert_eq!(parent_uuid.as_deref(), Some("prev"));
                assert_eq!(duration_ms, Some(1234));
            }
            _ => panic!("expected System, got {r:?}"),
        }
    }
}

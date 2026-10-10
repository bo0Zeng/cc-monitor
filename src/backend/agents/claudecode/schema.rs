//! Claude Code `projects/**/*.jsonl` 单行记录的 Rust schema —— **Claude 盘上格式的读法，只住这一家的适配层**。
//! 线上不出这些类型：记录帧与历史读出的是通用记录（`agents/record.rs`），由 `record_of.rs` 从这里翻过去；
//! 界面不认这一层（没有 ts 导出）。
//!
//! `JsonlRecord` enum 按 `type` 字段反序列化（user / assistant / system / summary /
//! ai-title / attachment / permission-mode / last-prompt / file-history-snapshot 等）。
//! 遵循 INVARIANT § 18「宽容 schema」：非核心字段一律 `Option<T>` / `#[serde(default)]`，
//! 避免 Claude Code 写法变动导致整行解析失败。
//!
//! **F63（issue #49）起看不懂的记录不静默丢**（INVARIANT § 18.1）：未知 `type` 落到
//! `#[serde(other)] Unknown`（仅 serde 内部落点），但 **`Unknown` 绝不出 `parse::parse_line`**
//! ——它连同「已知 type 解析失败但仍是合法 JSON」的行一起被抢救成 `Unrecognized`
//! （留原文 + uuid/parentUuid/timestamp），进链防孤儿化误折叠。详见 `Unrecognized` 变体注释。

use serde::{Deserialize, Serialize};

use crate::agents::{ApiReason, ChildRunTag, StepResult, ToolCard, ToolStep, UserText};
use std::collections::BTreeMap;

impl JsonlRecord {
    /// assistant 记录填上 `toolCards`（`content` 里每个 `tool_use` 按本家工具词表判一次）与 `childRuns`（派出子运行的那几次）；别的类型原样。
    pub(crate) fn with_tool_cards(mut self) -> Self {
        if let Self::Assistant {
            message,
            tool_cards,
            child_runs,
            ..
        } = &mut self
        {
            *child_runs = super::runs::links_in_content(&message.content)
                .into_iter()
                .filter_map(|l| {
                    Some((
                        l.tool?,
                        ChildRunTag {
                            label: l.label.unwrap_or_default(),
                            kind: l.kind,
                        },
                    ))
                })
                .collect();
            *tool_cards = message
                .content
                .as_array()
                .into_iter()
                .flatten()
                .filter(|b| b.get("type").and_then(serde_json::Value::as_str) == Some("tool_use"))
                .filter_map(|b| {
                    let id = b.get("id")?.as_str()?;
                    let card = super::cards::tool_card(b.get("name")?.as_str()?)?;
                    Some((id.to_string(), card))
                })
                .collect();
        }
        self
    }

    /// 过程一步一行（`steps.rs`）：assistant 填 `toolSteps` 与报错的 `apiReason` · user 填 `toolResults` · 要重试的 system 填 `apiReason`；别的原样。
    pub(crate) fn with_steps(mut self) -> Self {
        use super::steps;
        match &mut self {
            Self::Assistant {
                message,
                tool_steps,
                is_api_error_message,
                error,
                api_error_status,
                api_reason,
                ..
            } => {
                *tool_steps = message
                    .content
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|b| {
                        b.get("type").and_then(serde_json::Value::as_str) == Some("tool_use")
                    })
                    .filter_map(|b| {
                        let id = b.get("id")?.as_str()?;
                        let name = b.get("name")?.as_str()?;
                        let input = b.get("input").unwrap_or(&serde_json::Value::Null);
                        Some((id.to_string(), steps::step_of(name, input)))
                    })
                    .collect();
                if *is_api_error_message {
                    let text = super::text::extract_text_blocks(&message.content);
                    *api_reason = Some(steps::api_reason(*api_error_status, error.as_ref(), &text));
                }
            }
            Self::User {
                message,
                tool_use_result,
                tool_results,
                ..
            } => {
                *tool_results = message
                    .content
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|b| {
                        b.get("type").and_then(serde_json::Value::as_str) == Some("tool_result")
                    })
                    .filter_map(|b| {
                        let id = b.get("tool_use_id")?.as_str()?;
                        Some((
                            id.to_string(),
                            steps::result_of(b, tool_use_result.as_ref()),
                        ))
                    })
                    .collect();
                // 原样那一格只喂判定，判完就放（读文件的结果里有整份内容）。
                *tool_use_result = None;
            }
            Self::System {
                subtype,
                error,
                api_reason,
                ..
            } if subtype.as_deref() == Some("api_error") => {
                *api_reason = Some(steps::api_reason(None, error.as_ref(), ""));
            }
            _ => {}
        }
        self
    }

    /// user 记录与排队消息填上「谁说的」（[`UserText`]，判定只在 `text.rs`）；别的类型原样。
    pub(crate) fn with_user_text(mut self) -> Self {
        match &mut self {
            Self::User {
                message,
                is_meta,
                is_sidechain,
                is_compact_summary,
                parent_uuid,
                origin,
                user_text,
                ..
            } => {
                *user_text = super::text::user_text(&super::text::Facts {
                    content: &message.content,
                    is_meta: *is_meta,
                    is_sidechain: *is_sidechain,
                    is_compact_summary: *is_compact_summary,
                    has_parent: parent_uuid.as_deref().is_some_and(|p| !p.is_empty()),
                    origin: origin.as_ref().filter(|o| o.is_object()),
                });
            }
            Self::QueueOperation {
                content, user_text, ..
            } => *user_text = content.as_deref().map(super::text::queued_text),
            _ => {}
        }
        self
    }
}

/// issue #12: jsonl 顶层 `forkedFrom` 字段 —— `/branch` 命令分叉出新 session 时
/// 写入。`sessionId` 是 parent session 的 sessionId（**全前缀共享同一个**）；`messageUuid`
/// 则是**该条记录自身的 uuid**（= 它在父会话里的原 uuid），逐条不同。
///
/// **实证**（本机原生 branch 会话 fe4aad07/0473c3a0 逐条比对，F62 落盘亦按此）：`/branch`
/// 把 root→分叉点的线性前缀复制进新文件，每条 `sessionId` 改新 id、`forkedFrom.messageUuid`
/// = 自身 uuid（**不是**"整段共享同一个 messageUuid"——早期注释误述，勿据此把 F62 改回错的）。
/// `analyze_jsonl` 取首条 forkedFrom 的 sessionId 认 parent，故只需前缀共享 sessionId 即可。
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
        // 子运行归属由适配层的 `run_of` 判、经运行表与 `history-run` 给界面；记录成品不带这一格（界面不认它）。
        #[serde(rename = "isSidechain", default, skip_serializing)]
        is_sidechain: bool,
        // 下面三格只喂「谁说的」那一判（`text.rs::user_text`），不上线：界面只读成品 `userText`。
        #[serde(rename = "isMeta", default, skip_serializing)]
        is_meta: bool,
        #[serde(rename = "isCompactSummary", default, skip_serializing)]
        is_compact_summary: bool,
        #[serde(default, skip_serializing)]
        origin: Option<serde_json::Value>,
        #[serde(rename = "parentUuid", default)]
        parent_uuid: Option<String>,
        // issue #12: fork session 的所有记录都带这个字段；非 fork session 缺失
        #[serde(rename = "forkedFrom", default)]
        forked_from: Option<ForkedFrom>,
        /// 这条是谁说的、要显示的正文 —— 判定只在 `agents/claudecode/text.rs::user_text`（Codex 在 `agents/codex/record.rs`），
        /// 界面渲染 / 分叉折叠只读这个成品（不自己再判）。原文里没有这一格：解析完由 [`JsonlRecord::with_user_text`] 填。
        #[serde(rename = "userText", skip_deserializing, default)]
        user_text: UserText,
        // 只喂结果一句那一判（`steps.rs::result_of`），不上线。
        #[serde(rename = "toolUseResult", default, skip_serializing)]
        tool_use_result: Option<serde_json::Value>,
        /// 〔判定只在后端〕这条里每个 `tool_result` 的结果一句：`tool_use_id` → [`StepResult`]（读了几行 · `+N −M` · 被拒 · 提问 / 计划答了什么）。
        /// 原文里没有这一格：解析完由 [`JsonlRecord::with_steps`] 填；界面只按它拼字，不认 `toolUseResult` 的形状。
        #[serde(
            rename = "toolResults",
            skip_deserializing,
            default,
            skip_serializing_if = "BTreeMap::is_empty"
        )]
        tool_results: BTreeMap<String, StepResult>,
    },
    #[serde(rename = "assistant")]
    Assistant {
        uuid: String,
        timestamp: String,
        message: ApiMessage,
        #[serde(rename = "sessionId", default)]
        session_id: Option<String>,
        // 子运行归属由适配层的 `run_of` 判、经运行表与 `history-run` 给界面；记录成品不带这一格（界面不认它）。
        #[serde(rename = "isSidechain", default, skip_serializing)]
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
        // 仅 HTTP 类错误有。报错文本在 message.content[0].text。翻成通用记录的 `error`，界面据此画红色报错卡，
        // 否则会被当普通 assistant 回复（以为 LLM 还在跑）。
        //
        // error 用 Value 而非 String：实测 47 条全是 string，但 system 侧同名字段就是
        // 对象——若某版 CLI 把它写成对象而这里钉死 String，serde 整行失败 → 这条报错
        // 消息本身被吞（报错可见化 feature 被报错字段漂移杀掉）。§18 宽容 schema。
        #[serde(rename = "isApiErrorMessage", default)]
        is_api_error_message: bool,
        #[serde(default)]
        // 形状随 CLI 版本漂，用 Value 收（§18「宽容 schema」），读的一方先判形状。
        error: Option<serde_json::Value>,
        #[serde(rename = "apiErrorStatus", default)]
        api_error_status: Option<u32>,
        /// 〔判定只在后端〕这条消息里每个 `tool_use` 的卡型：`tool_use.id` → [`ToolCard`]（普通工具卡不列）。
        /// 原文里没有这一格：解析完由 [`JsonlRecord::with_tool_cards`] 按本家工具词表（`cards::tool_card`）填；界面只按它画、不认工具名。
        #[serde(
            rename = "toolCards",
            skip_deserializing,
            default,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        tool_cards: std::collections::BTreeMap<String, ToolCard>,
        /// 这条消息里派出子运行的那几次工具调用：`tool_use.id` ⇒ 标签与类别（通用形，界面按它给卡起名，不读工具入参）。
        /// 原文里没有这一格：解析完由 [`JsonlRecord::with_tool_cards`] 按本家的派出链接（`runs::links_in_content`）填。
        #[serde(
            rename = "childRuns",
            skip_deserializing,
            default,
            skip_serializing_if = "std::collections::BTreeMap::is_empty"
        )]
        child_runs: BTreeMap<String, ChildRunTag>,
        /// 〔判定只在后端〕这条消息里每个 `tool_use` 的一行人话：`tool_use.id` → [`ToolStep`]（工具名 · 主参数 · 说明 · 认不认得）。
        /// 原文里没有这一格：解析完由 [`JsonlRecord::with_steps`] 填；界面不认入参结构。
        #[serde(
            rename = "toolSteps",
            skip_deserializing,
            default,
            skip_serializing_if = "BTreeMap::is_empty"
        )]
        tool_steps: BTreeMap<String, ToolStep>,
        /// 〔判定只在后端〕`isApiErrorMessage` 的那条：原因种类（[`ApiReason`]）。别的记录缺。
        #[serde(
            rename = "apiReason",
            skip_deserializing,
            default,
            skip_serializing_if = "Option::is_none"
        )]
        api_reason: Option<ApiReason>,
    },

    #[serde(rename = "ai-title")]
    AiTitle {
        #[serde(rename = "aiTitle")]
        ai_title: String,
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    // Claude Code v2.1.x 起把 ai-title schema 改为 custom-title / customTitle
    // （旧 ai-title 在历史 jsonl 里仍可能出现，两个都保留）。两个都翻成通用记录的 `title`（`by` 分谁起的）。
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
        // 主线外清单（`chain.rs`）要拿到它才能完整算 ESC 回退主线。Option 兜没有这些字段的少数情况。
        #[serde(default)]
        uuid: Option<String>,
        #[serde(rename = "parentUuid", default)]
        parent_uuid: Option<String>,
        // issue #21: subtype="api_error"（每次 API 调用失败将重试时写一条）。level
        // 实测只有 "error"；retryAttempt/maxRetries 翻成通用记录 `retry` 的 attempt / max（界面「重试中 N/M」）；error
        // 对象有两种 shape（随 CLI 版本变化，新版有现成的 .formatted 一行文案）——用 Value 收，原因种类由 `steps::api_reason` 判。
        #[serde(default)]
        level: Option<String>,
        #[serde(rename = "retryAttempt", default)]
        retry_attempt: Option<u32>,
        #[serde(rename = "maxRetries", default)]
        max_retries: Option<u32>,
        #[serde(default)]
        // 形状随 CLI 版本漂，用 Value 收（§18「宽容 schema」），读的一方先判形状。
        error: Option<serde_json::Value>,
        /// 〔判定只在后端〕`subtype == "api_error"`（要重试的那一次）的原因种类。别的记录缺。
        #[serde(
            rename = "apiReason",
            skip_deserializing,
            default,
            skip_serializing_if = "Option::is_none"
        )]
        api_reason: Option<ApiReason>,
    },

    // issue #8: attachment 不渲染卡片，但有 uuid+parentUuid 并夹在 user→assistant
    // 之间（实测 5% 的 user/assistant 直接 parent 是 attachment）。如果主线检测不认它，
    // parent 链就断在 attachment 处 → 主线检测全部失败 → 整段消息被错误折叠到"已被 ESC 回退"。
    // 所以本变体含完整字段、出链事实（`chain.rs`）；不出通用记录。
    #[serde(rename = "attachment")]
    Attachment {
        uuid: String,
        timestamp: String,
        #[serde(rename = "parentUuid", default)]
        parent_uuid: Option<String>,
    },
    /// Batch10-F31 (issue #36)：CC 2.1.x 队列操作记录。enqueue 带 content —— 主线判定
    /// 用它豁免"被消费的队列消息"（永久裸 user 叶，CC 的回复链挂在 interrupt 叶
    /// 下而非队列消息下）不被 ESC 回退折叠；remove 那一条翻成通用记录的 `queued`。无 uuid/parentUuid。
    #[serde(rename = "queue-operation")]
    QueueOperation {
        #[serde(default)]
        operation: Option<String>,
        #[serde(default)]
        content: Option<String>,
        /// P0c：**`remove` 那一支要建卡，卡上要有时间**。
        ///
        /// 原来这个变体只留 `operation` + `content`（issue #36 只需要 content 喂折叠豁免集合）。
        /// 而 `remove` 是**用户打断时说的那句话在 jsonl 里唯一的存在** ——
        /// 它没有 `user` 记录、没有 `uuid`、没有 `parentUuid`，
        /// 时间戳是它**仅有的**可用于排序与展示的元数据，原文里一直有，只是我们没收。
        #[serde(default)]
        timestamp: Option<String>,
        /// `content` 是谁说的（排队消息没有记录级字段，只认具名框与固定句）；没有 `content` ⇒ 缺。
        /// 只有人说的那一支建卡（`remove`：插进正在跑的那一轮、没有 user 记录的那句话）。
        #[serde(
            rename = "userText",
            skip_deserializing,
            default,
            skip_serializing_if = "Option::is_none"
        )]
        user_text: Option<UserText>,
    },
    #[serde(rename = "permission-mode")]
    PermissionMode {},
    #[serde(rename = "last-prompt")]
    LastPrompt {},
    #[serde(rename = "file-history-snapshot")]
    FileHistorySnapshot {},
    // Claude 自己的状态行（07-16 语料：mode 6472 / agent-name 2020 / file-history-delta 181 / pr-link 58 / relocated 19 /
    // worktree-state 19 / frame-link 2，都没有链身份）：认识、不上屏、不进链。从前落在 `Unrecognized` 里，
    // 记录出 `unread` 之后会被它们刷满 ⇒ 10-10 登成认识的类型（同 `permission-mode`）。
    #[serde(rename = "mode")]
    Mode {},
    #[serde(rename = "agent-name")]
    AgentName {},
    #[serde(rename = "file-history-delta")]
    FileHistoryDelta {},
    #[serde(rename = "pr-link")]
    PrLink {},
    #[serde(rename = "relocated")]
    Relocated {},
    #[serde(rename = "worktree-state")]
    WorktreeState {},
    #[serde(rename = "frame-link")]
    FrameLink {},

    /// F63 (issue #49)：**看不懂的记录 —— 留原文 + 留链上的身份**。
    ///
    /// 由 `parse::parse_line` 构造，**绝不从真实 jsonl 反序列化得来**（故 type 名带
    /// `cc-monitor-` 前缀防撞未来的真类型）。两条来源：
    /// 1. 未知 `type` —— serde 落到下面的 `Unknown`，`parse_line` 抢救成本变体
    /// 2. 已知 `type` 但字段解析失败（`from_str` 返回 Err）且原文仍是合法 JSON
    ///
    /// **为什么不直接给 `Unknown` 加字段**：`#[serde(other)]` 编译期强制 unit variant
    /// （实测 `error: #[serde(other)] must be on a unit variant`）。故 `Unknown` 只当
    /// serde 落点，抢救在 `parse_line` 做——那里手里正好有原始字符串。
    ///
    /// **为什么要留 uuid/parentUuid**：主线判定（`chain.rs`）判 root 的依据是
    /// 「parentUuid 不在集合里」。记录一旦丢失，它的 children 就成孤儿 root →
    /// 多 root 时死胡同那一支**整棵判成回退掉的**。不 track 它们的 uuid+parentUuid，
    /// parent 链断成碎片 → 大量误折叠；同类故障咬过一次（1 条重复 attachment 折掉
    /// 1541/4331 条，2026-06-13）。
    ///
    /// **实测（2026-07-16，本机 771 会话 / 157,385 行）**：当前 7 个未知 type
    /// （mode 6472 / agent-name 2020 / file-history-delta 181 / pr-link 58 /
    /// relocated 19 / worktree-state 19 / frame-link 2，共 8,774 条）**uuid 与
    /// parentUuid 全为 0** —— 即此刻并没有在误折叠，本变体是**保险 + 诚实**：
    /// ①不再静默丢 5.6% 的行；②Claude 哪天发一个带链身份的新类型时自动扛住。
    ///
    /// **带链身份的**出链事实（照 `Attachment` 先例：**不进界面但进链**，`chain.rs`）；
    /// 不出通用记录（`record_of` 认不出 ⇒ 缺，界面照占号）。
    /// **没有链身份的**（uuid 与 parentUuid 都缺 —— 上面实测的那 7 种今天全是）
    /// 在这里就滤掉：它们不建卡、不进链，没有任何读者，却要付每条的固定开销（去重入集合 · sink · 门控）。「不静默」那一半由 `drift_ledger`（`UnknownRecordType` 面，解析时记）接着管 ——
    /// 诊断面照旧看得见「多了一种没见过的类型」。
    #[serde(rename = "cc-monitor-unrecognized")]
    Unrecognized {
        #[serde(default)]
        uuid: Option<String>,
        #[serde(rename = "parentUuid", default)]
        parent_uuid: Option<String>,
        #[serde(default)]
        timestamp: Option<String>,
        /// 原文里的 `type`（若有）——诊断 / 记账按它分类
        #[serde(rename = "originalType", default)]
        original_type: Option<String>,
        /// 抢救时的原文行（已剥 UTF-8 BOM + `.trim()`，JSON 载荷完整）。
        /// 诊断 / 未来从这里抽 `pr-link`.prUrl、`agent-name`.agentName 等字段用。
        raw: String,
        /// 为什么没认出来：`unknown-type` / `parse-failed: <serde 原文>`
        reason: String,
    },

    /// serde 对未知 `type` 的落点。**不出 `parse_line`** —— 它会被抢救成
    /// `Unrecognized`（带原文与身份）。
    ///
    /// 保留它是**设计选择**而非硬性必需：去掉 `#[serde(other)]`，未知 `type` 会走
    /// `parse_line` 的 `Err → salvage` 路径同样被抢救，只是 `reason` 变成
    /// `parse-failed: unknown variant …` 而非干净的 `unknown-type`。留着是为了
    /// **reason 分类清晰**（未知 type vs 已知 type 解析失败，两类诊断意义不同）。
    /// 「必须是 unit variant」只是 `#[serde(other)]` 的编译期约束，不是"不留就崩"。
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ApiMessage {
    pub role: String,
    // 字符串或块数组两形都有（§18 宽容 schema）；翻成通用记录的内容块在 `record_of::blocks_of`。
    pub content: serde_json::Value,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: Option<Usage>,
    /// Batch14-F42：一轮结束判定（assistant 终结记录带 `end_turn`；
    /// aterm/HANDOFF 同判据）。老记录/流式中间记录无此字段 → None 不序列化。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
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

impl JsonlRecord {
    /// 这条记录自己的 `cwd`（只有 user 记录带）—— 行成品里那一格（原 monitor `lib·rs` 那个 `extract_cwd`〔散文墓碑〕）。
    pub fn cwd(&self) -> Option<&str> {
        match self {
            Self::User { cwd, .. } => cwd.as_deref(),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/schema_tests.rs"]
mod tests;

// 「每个承载标题的记录都被标题抽取接住」随 schema 从 monitor 搬来（原 `history_title_coverage.rs`）。
#[cfg(test)]
#[path = "../../../../tests/backend/agents/claudecode/schema_title_coverage.rs"]
mod title_coverage;

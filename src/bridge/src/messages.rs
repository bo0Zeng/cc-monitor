//! Claude Code `projects/**/*.jsonl` 单行记录的 Rust schema。
//!
//! `JsonlRecord` enum 按 `type` 字段反序列化（user / assistant / system / summary /
//! ai-title / attachment / permission-mode / last-prompt / file-history-snapshot 等）。
//! 遵循 INVARIANT § 18「宽容 schema」：非核心字段一律 `Option<T>` / `#[serde(default)]`，
//! 避免 Claude Code 写法变动导致整行解析失败。
//!
//! **F63（issue #49）起看不懂的记录不静默丢**（INVARIANT § 18.1）：未知 `type` 落到
//! `#[serde(other)] Unknown`（仅 serde 内部落点），但 **`Unknown` 绝不出 `parser::parse_line`**
//! ——它连同「已知 type 解析失败但仍是合法 JSON」的行一起被抢救成 `Unrecognized`
//! （留原文 + uuid/parentUuid/timestamp），进链防孤儿化误折叠。详见 `Unrecognized` 变体注释。
//!
//! 这些类型在前端 `cards/index.ts` 有对应的 TS 镜像（ApiMessage / ContentBlock 等）。

use serde::{Deserialize, Serialize};

/// issue #12: jsonl 顶层 `forkedFrom` 字段 —— `/branch` 命令分叉出新 session 时
/// 写入。`sessionId` 是 parent session 的 sessionId（**全前缀共享同一个**）；`messageUuid`
/// 则是**该条记录自身的 uuid**（= 它在父会话里的原 uuid），逐条不同。
///
/// **实证**（本机原生 branch 会话 fe4aad07/0473c3a0 逐条比对，F62 落盘亦按此）：`/branch`
/// 把 root→分叉点的线性前缀复制进新文件，每条 `sessionId` 改新 id、`forkedFrom.messageUuid`
/// = 自身 uuid（**不是**"整段共享同一个 messageUuid"——早期注释误述，勿据此把 F62 改回错的）。
/// `analyze_jsonl` 取首条 forkedFrom 的 sessionId 认 parent，故只需前缀共享 sessionId 即可。
#[derive(Debug, Deserialize, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct ForkedFrom {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "messageUuid")]
    pub message_uuid: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
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
        // `serde_json::Value` 没有天然的 TS 对应；用 `unknown` 而不是 `any`——
        // 前端必须先 typeof/形状守卫才能读，这与 §18「宽容 schema」的读法一致。
        #[cfg_attr(test, ts(type = "unknown"))]
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
        // **C03 大整数策略**（默认 `number`，但绝不许是 ts-rs 的默认 `bigint`）。
        // 上限论证**按量纲单独算**，不套用字节数/时间戳那两条：这里的量纲是**时长(ms)**，
        // `Number.MAX_SAFE_INTEGER` = 2^53-1 ms ≈ **28.5 万年**。单条 system 记录的
        // 耗时不可能接近它 ⇒ f64 精度足够，且 Tauri 的 IPC 是 `serde_json::to_string`
        // ⇒ 线上是 JSON 文本 ⇒ `JSON.parse` 永远给不出 BigInt，写 `bigint` 才是错的。
        // **注意 Option 的外层**：`ts(type = …)` 覆盖的是**整个**类型，不只是内层。
        // 写 `"number"` 会生成 `durationMs: number` —— 丢掉 `| null`，而这个字段
        // **没有** `skip_serializing_if` ⇒ None 会被序列化成 `null`（不是省略）。
        // 我本轮就先写错了一次，是盯生成物发现的 ⇒ 已补一条守卫机检这个形状。
        #[cfg_attr(test, ts(type = "number | null"))]
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
        // `serde_json::Value` 没有天然的 TS 对应；用 `unknown` 而不是 `any`——
        // 前端必须先 typeof/形状守卫才能读，这与 §18「宽容 schema」的读法一致。
        #[cfg_attr(test, ts(type = "unknown"))]
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
        /// P0c：**`remove` 那一支要建卡，卡上要有时间**。
        ///
        /// 原来这个变体只留 `operation` + `content`（issue #36 只需要 content 喂折叠豁免集合）。
        /// 而 `remove` 是**用户打断时说的那句话在 jsonl 里唯一的存在** ——
        /// 它没有 `user` 记录、没有 `uuid`、没有 `parentUuid`，
        /// 时间戳是它**仅有的**可用于排序与展示的元数据，原文里一直有，只是我们没收。
        #[serde(default)]
        timestamp: Option<String>,
    },
    #[serde(rename = "permission-mode")]
    PermissionMode {},
    #[serde(rename = "last-prompt")]
    LastPrompt {},
    #[serde(rename = "file-history-snapshot")]
    FileHistorySnapshot {},

    /// F63 (issue #49)：**看不懂的记录 —— 留原文 + 留链上的身份**。
    ///
    /// 由 `parser::parse_line` 构造，**绝不从真实 jsonl 反序列化得来**（故 type 名带
    /// `cc-monitor-` 前缀防撞未来的真类型）。两条来源：
    /// 1. 未知 `type` —— serde 落到下面的 `Unknown`，`parse_line` 抢救成本变体
    /// 2. 已知 `type` 但字段解析失败（`from_str` 返回 Err）且原文仍是合法 JSON
    ///
    /// **为什么不直接给 `Unknown` 加字段**：`#[serde(other)]` 编译期强制 unit variant
    /// （实测 `error: #[serde(other)] must be on a unit variant`）。故 `Unknown` 只当
    /// serde 落点，抢救在 `parse_line` 做——那里手里正好有原始字符串。
    ///
    /// **为什么要留 uuid/parentUuid**：`branching.ts:100-106` 判 root 的依据是
    /// 「parentUuid 不在集合里」。记录一旦丢失，它的 children 就成孤儿 root →
    /// `branching.ts:48-50` 多 root 时死胡同 plain user root **整棵折叠**。
    /// `branching.ts:24` 早预警过：「必须 track 它们的 uuid+parentUuid，否则 parent
    /// 链断成碎片 → 大量误折叠」；同类故障咬过一次（1 条重复 attachment 折掉
    /// 1541/4331 条，见 2026-06-13 排查总结）。
    ///
    /// **实测（2026-07-16，本机 771 会话 / 157,385 行）**：当前 7 个未知 type
    /// （mode 6472 / agent-name 2020 / file-history-delta 181 / pr-link 58 /
    /// relocated 19 / worktree-state 19 / frame-link 2，共 8,774 条）**uuid 与
    /// parentUuid 全为 0** —— 即此刻并没有在误折叠，本变体是**保险 + 诚实**：
    /// ①不再静默丢 5.6% 的行；②Claude 哪天发一个带链身份的新类型时自动扛住。
    ///
    /// 进 `is_displayable()`（照 `Attachment` 先例：**不渲染卡片但进链**）。
    /// 前端 `cards/index.ts:350` `default => skip` 已能优雅跳过，无需建卡。
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
pub struct ApiMessage {
    pub role: String,
    // 同上：`unknown` 逼前端先做形状判断（`cards/index.ts` 的 `ContentBlock` 就是那层解释模型）。
    #[cfg_attr(test, ts(type = "unknown"))]
    pub content: serde_json::Value,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: Option<Usage>,
    /// Batch14-F42：一轮结束判定（assistant 终结记录带 `end_turn`；
    /// aterm/HANDOFF 同判据）。老记录/流式中间记录无此字段 → None 不序列化。
    // 同 `JsonlLinePayload.origin`：`skip_serializing_if` ⇒ 必须 `ts(optional)`。
    // 这里还叠了 C02 实测过的一层：`default` + `skip_serializing_if` 会走 ts-rs 的
    // `maybe_omitted && has_default` 回退，生成**过宽**的 `stop_reason?: string | null`。
    // 显式 `ts(optional)` 才得到诚实的 `stop_reason?: string`。
    #[cfg_attr(test, ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../../../src/generated/"))]
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
    /// 2. 仅链路用：Attachment / Unrecognized —— 不渲染，但 issue #8 ESC 回退主线检测
    ///    需要完整 uuid+parentUuid 链，attachment 夹在 user/assistant 之间，
    ///    不 emit 会让前端 parent 链断成碎片 → 主线全错 → 全部消息被错折叠
    ///
    /// **返回 false 的两类要分清**（F63/#49「零信息损失」的口径）：
    /// - `PermissionMode` / `LastPrompt` / `FileHistorySnapshot` = **已知类型的明示
    ///   决定**（我们认识它、判断它不该显示）→ 不算「丢」。实测 16,121 条。
    /// - `Unknown` = **不认识**，曾经从这里被静默丢弃（实测 8,774 条 / 5.6%）。
    ///   F63 起它不再出 `parse_line`（被抢救成 `Unrecognized`），此处 false 只是
    ///   兜底——真走到说明 `parse_line` 的后处理漏了，属 bug。
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
                | Self::Unrecognized { .. }
        )
    }
}

#[cfg(test)]
#[path = "../../../tests/bridge/messages_tests.rs"]
mod tests;

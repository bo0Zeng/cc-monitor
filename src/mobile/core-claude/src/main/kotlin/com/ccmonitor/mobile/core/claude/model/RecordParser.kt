package com.ccmonitor.mobile.core.claude.model

/**
 * 会话记录解析：按 [AgentKind] 把一行原始记录解析成统一模型 [JsonlRecord]。
 * Claude 与 Codex 的消息、推理、工具都映射到 [JsonlRecord]/[ContentBlock]；Claude 专有字段（stopReason/requestId/usage）Codex 留 null。
 * 实现按种类从 `AgentProfile.recordParser` 取。
 *
 * [parse] 永不抛：坏行/缺字段/未知型 → [JsonlRecord.Unknown]，保渲染管线不崩。
 */
interface RecordParser {
    fun parse(raw: String): JsonlRecord
}

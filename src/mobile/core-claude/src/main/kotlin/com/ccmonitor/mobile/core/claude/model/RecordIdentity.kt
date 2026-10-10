package com.ccmonitor.mobile.core.claude.model

/**
 * 给一条 [JsonlRecord] 赋 uuid（返回副本）。
 *
 * Codex 记录多数没有稳定 uuid（工具结果恒无、user message 常无），而渲染键 `uuid#block` 靠 uuid 唯一，
 * 多条 null 会撞键让 LazyColumn 崩。消费方按文件序补合成 uuid（`cx-<index>`，append-only 稳定）；已有的不动。
 *
 * [JsonlRecord.Title] 的 uuid 是恒 null 的 getter（无字段）→ 无法赋值、原样返回（它也不参与渲染键）。
 */
fun JsonlRecord.withUuid(newUuid: String): JsonlRecord =
    when (this) {
        is JsonlRecord.User -> copy(uuid = newUuid)
        is JsonlRecord.Assistant -> copy(uuid = newUuid)
        is JsonlRecord.System -> copy(uuid = newUuid)
        is JsonlRecord.Attachment -> copy(uuid = newUuid)
        is JsonlRecord.Unknown -> copy(uuid = newUuid)
        is JsonlRecord.Title -> this
    }

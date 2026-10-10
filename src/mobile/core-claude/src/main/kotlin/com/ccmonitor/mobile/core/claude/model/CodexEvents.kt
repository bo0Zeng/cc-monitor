package com.ccmonitor.mobile.core.claude.model

import com.squareup.moshi.Moshi

/**
 * Codex `event_msg` 共享读取：turn-end 与用量共用一处 unwrap。
 */
private val codexEventMoshi = Moshi.Builder().build()
private val codexEventAnyAdapter = codexEventMoshi.adapter(Any::class.java)

/**
 * 记录是 Codex `event_msg`（解析成 [JsonlRecord.Unknown] type="event_msg"，保留 rawJson）→ 返 `payload` Map，否则 null。
 * 非 event_msg / 缺 rawJson / 坏 JSON 一律 null 不抛：Codex rollout 格式每版都可能变。
 */
internal fun codexEventPayload(record: JsonlRecord): Map<*, *>? {
    if (record !is JsonlRecord.Unknown || record.type != "event_msg") return null
    val raw = record.rawJson ?: return null
    return runCatching { (codexEventAnyAdapter.fromJson(raw) as? Map<*, *>)?.get("payload") as? Map<*, *> }.getOrNull()
}

/**
 * 从 Codex `session_meta`（[JsonlRecord.Unknown] type="session_meta"）读 `payload.cwd`。
 * Codex 的工作目录在 session_meta（Claude 在 User 记录里），供 resume 定位。非 session_meta / 缺 / 坏 → null。
 */
fun codexSessionMetaCwd(record: JsonlRecord): String? {
    if (record !is JsonlRecord.Unknown || record.type != "session_meta") return null
    val raw = record.rawJson ?: return null
    return runCatching {
        ((codexEventAnyAdapter.fromJson(raw) as? Map<*, *>)?.get("payload") as? Map<*, *>)?.get("cwd") as? String
    }.getOrNull()?.takeIf { it.isNotBlank() }
}

package com.ccmonitor.mobile.core.claude.link.wire

import com.ccmonitor.mobile.core.claude.link.wire.gen.JsonValue
import com.ccmonitor.mobile.core.claude.link.wire.gen.SyncHead
import com.ccmonitor.mobile.core.claude.link.wire.gen.SyncPatch
import com.squareup.moshi.JsonAdapter
import com.squareup.moshi.JsonDataException

/** 带键集合那一答：项与头都照原样留着（逐项存盘，用时各按自己的类型解）。 */
typealias RawPatch = SyncPatch<Any, Any>

/** [RawPatch] 的编解码。 */
val RAW_PATCH: JsonAdapter<RawPatch> = SyncPatch.adapter(JsonValue, JsonValue)

/** 核心交来的那一答 ⇒ [RawPatch]；不是那一形 ⇒ `null`。 */
fun rawPatch(data: Any?): RawPatch? =
    try {
        RAW_PATCH.fromJsonValue(data)
    } catch (_: JsonDataException) {
        null
    }

/**
 * 手上那一份带键集合：记号 ＋ 这一窗的先后 ＋ 各项 ＋ 头；空的那一份 ＝ 没问过。
 * [apply] 照后端那一答换、没有判断，与 Rust 那一份（`cmd_wire::sync::Hand::apply`）照同一份金样 `sync-patch.golden.jsonl`。
 */
data class SyncHand(
    val held: String? = null,
    val order: List<String> = emptyList(),
    val items: Map<String, Any> = emptyMap(),
    val head: SyncHead<Any>? = null,
) {
    /** `same` ⇒ 只换记号；`whole` ⇒ 从空的起；带了 `order` / `head` 就换；`up` 盖上；只留键在 `order` 里的。 */
    fun apply(p: RawPatch): SyncHand {
        if (p.same) return copy(held = p.held)
        val base = if (p.whole) SyncHand() else this
        val order = p.order ?: base.order
        val keep = order.toSet()
        return SyncHand(p.held, order, (base.items + p.up).filterKeys { it in keep }, p.head ?: base.head)
    }
}

/** 存起来的那一份头 ⇒ [SyncHead]（`info` 照原样）；不是那一形 ⇒ `null`。 */
fun rawHead(data: Any?): SyncHead<Any>? =
    try {
        SyncHead.adapter(JsonValue).fromJsonValue(data)
    } catch (_: JsonDataException) {
        null
    }

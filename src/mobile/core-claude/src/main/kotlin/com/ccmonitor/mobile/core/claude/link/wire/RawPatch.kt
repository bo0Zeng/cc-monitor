package com.ccmonitor.mobile.core.claude.link.wire

import com.ccmonitor.mobile.core.wire.gen.JsonValue
import com.ccmonitor.mobile.core.wire.gen.SyncPatch
import com.squareup.moshi.JsonAdapter
import com.squareup.moshi.JsonDataException

/** 带键集合那一答：项与头都照原样留着（用时各按自己的类型解）。照答换手上那一份在嵌的核心里，手机这一侧只读整份那一形。 */
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

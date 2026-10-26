package com.ccmonitor.mobile.core.claude.link.wire

import com.ccmonitor.mobile.core.claude.link.wire.gen.Commands
import com.squareup.moshi.JsonAdapter
import com.squareup.moshi.JsonDataException

/**
 * 把核心交来的那份 `data` 装进线上类型（`gen` 包，由 `tests/scripts/gen-kotlin-wire.mjs` 从 `cmd-wire` 那一份类型生成）：
 * 用生成的那个编解码（按类取，不经反射）。多出来的键照收；不可空的那一格缺了或是 null · 型不对 ⇒ `null`。
 */
object Wire {
    inline fun <reified T : Any> bind(data: Any?): T? = bind(T::class.java, data)

    fun <T : Any> bind(
        type: Class<T>,
        data: Any?,
    ): T? =
        try {
            adapter(type).fromJsonValue(data)
        } catch (_: JsonDataException) {
            null
        }

    /** 带类型参数的那几样（实例化好的编解码交进来）：同 [bind]。 */
    fun <T> bind(
        ad: JsonAdapter<T>,
        data: Any?,
    ): T? =
        try {
            ad.fromJsonValue(data)
        } catch (_: JsonDataException) {
            null
        }

    /** 反过来：线上类型出成线上那一份（交 `CoreLink.call` 的 `args`）；值为 null 的格不交（那台照缺省收，同 serde 的 Option）。 */
    inline fun <reified T : Any> args(value: T): Map<String, Any?> = args(T::class.java, value)

    @Suppress("UNCHECKED_CAST")
    fun <T : Any> args(
        type: Class<T>,
        value: T,
    ): Map<String, Any?> = withoutNulls(adapter(type).toJsonValue(value)) as Map<String, Any?>

    private fun withoutNulls(v: Any?): Any? =
        when (v) {
            is Map<*, *> -> v.entries.filter { it.value != null }.associate { (k, x) -> k to withoutNulls(x) }
            is List<*> -> v.map(::withoutNulls)
            else -> v
        }

    @Suppress("UNCHECKED_CAST")
    private fun <T : Any> adapter(type: Class<T>): JsonAdapter<T> =
        (Commands.BY_CLASS[type] ?: error("no generated adapter for " + type.name)) as JsonAdapter<T>
}

package com.ccmonitor.mobile.core.claude.link

import com.squareup.moshi.Moshi

/** 帧客户端用的 JSON 读写（一份 moshi，`Any` 形：对象 ⇒ `Map`，数 ⇒ `Double`）。 */
internal object Json {
    private val any = Moshi.Builder().build().adapter(Any::class.java)

    /** 解一行；不是 JSON 对象 ⇒ `null`。 */
    fun obj(line: String): Map<String, Any?>? =
        @Suppress("UNCHECKED_CAST")
        (runCatching { any.fromJson(line) }.getOrNull() as? Map<String, Any?>)

    /** 写成一行（moshi 不出裸换行）。 */
    fun line(value: Any?): String = any.toJson(value)
}

/** 读一格：要什么类型就按什么类型取，类型不对 ⇒ `null`。 */
fun Map<String, Any?>.str(key: String): String? = this[key] as? String

fun Map<String, Any?>.num(key: String): Long? = (this[key] as? Number)?.toLong()

fun Map<String, Any?>.bool(key: String): Boolean? = this[key] as? Boolean

@Suppress("UNCHECKED_CAST")
fun Map<String, Any?>.obj(key: String): Map<String, Any?>? = this[key] as? Map<String, Any?>

@Suppress("UNCHECKED_CAST")
fun Map<String, Any?>.objs(key: String): List<Map<String, Any?>>? = (this[key] as? List<*>)?.map { it as? Map<String, Any?> ?: return null }

fun Map<String, Any?>.strs(key: String): List<String>? = (this[key] as? List<*>)?.map { it as? String ?: return null }

/** 解码里「缺一格」的信号：只在 [decoding] 里抛、在那里接住，不往外漏。 */
internal class Malformed : RuntimeException()

/** 缺这一格 ⇒ 整份解不出（[decoding] 回 `null`）。 */
internal fun <T> T?.need(): T = this ?: throw Malformed()

/** 一份成品的解码：块里任何一格 [need] 落空 ⇒ `null`（两端契约对不上），不补默认值。 */
internal inline fun <T> decoding(block: () -> T): T? =
    try {
        block()
    } catch (_: Malformed) {
        null
    }

@Suppress("UNCHECKED_CAST")
internal fun Any?.asObj(): Map<String, Any?> = (this as? Map<String, Any?>).need()

internal fun Any?.asList(): List<Any?> = (this as? List<*>).need()

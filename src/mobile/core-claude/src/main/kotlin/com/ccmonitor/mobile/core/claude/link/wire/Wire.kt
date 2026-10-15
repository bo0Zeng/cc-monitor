package com.ccmonitor.mobile.core.claude.link.wire

import com.squareup.moshi.JsonDataException
import com.squareup.moshi.Moshi

/**
 * 这个类声明手机读那一份回包里的哪几格（出口声明）：[name] 是它对上的那个线上类型在生成物里的名字
 * （`src/frontend/ui/generated/<name>.ts`，由 `cmd-wire` 的 Rust 类型导出）。只给判据读源码用，不进字节码。
 */
@Target(AnnotationTarget.CLASS)
@Retention(AnnotationRetention.SOURCE)
annotation class WireOf(
    val name: String,
)

/**
 * 把核心交来的那份 `data` 装进声明好的类里，用 Moshi 在编译期生成的解码器（`@JsonClass(generateAdapter = true)`）：
 * 不靠运行时按名字反射，混淆过的包里照样成立。形状由嵌的 Rust 核心按 `cmd-wire` 那一份类型判过
 * （判不过的到不了这里，落 `Reply.Unreadable`）；这里多出来的键照收，声明了不可空的那一格缺了或是 null · 型不对 ⇒ `null`。
 */
object Wire {
    private val moshi: Moshi = Moshi.Builder().build()

    inline fun <reified T : Any> bind(data: Any?): T? = bind(T::class.java, data)

    fun <T : Any> bind(
        type: Class<T>,
        data: Any?,
    ): T? =
        try {
            moshi.adapter(type).fromJsonValue(data)
        } catch (_: JsonDataException) {
            null
        }

    /** 反过来：声明好的入参类出成线上那一份（交 `CoreLink.call` 的 `args`）；可空那一格为 null ⇒ 不交那个键。 */
    inline fun <reified T : Any> args(value: T): Map<String, Any?> = args(T::class.java, value)

    @Suppress("UNCHECKED_CAST")
    fun <T : Any> args(
        type: Class<T>,
        value: T,
    ): Map<String, Any?> = moshi.adapter(type).toJsonValue(value) as Map<String, Any?>
}

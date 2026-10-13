package com.ccmonitor.mobile.core.claude.link.wire

import java.lang.reflect.Constructor
import java.lang.reflect.InvocationTargetException
import java.lang.reflect.ParameterizedType
import java.lang.reflect.Type
import java.lang.reflect.WildcardType

/**
 * 这个类声明手机读那一份回包里的哪几格（出口声明）：[name] 是它对上的那个线上类型在生成物里的名字
 * （`src/frontend/ui/generated/<name>.ts`，由 `cmd-wire` 的 Rust 类型导出）。判据照它核每一格在不在、型对不对。
 */
@Target(AnnotationTarget.CLASS)
@Retention(AnnotationRetention.RUNTIME)
annotation class WireOf(
    val name: String,
)

/** 线上那一格的键和 Kotlin 的参数名不同（线上是 `snake_case`）时写它。 */
@Target(AnnotationTarget.VALUE_PARAMETER)
@Retention(AnnotationRetention.RUNTIME)
annotation class Key(
    val name: String,
)

/**
 * 把核心交来的那份 `data` 装进声明好的类里。**不判形状**：形状由嵌的 Rust 核心按 `cmd-wire` 那一份类型判过
 * （判不过的到不了这里，落 `Reply.Unreadable`），这里只按构造参数名（`-java-parameters`）一格一格取、按参数的型装：
 * 字 · 数 · 真假 · 列表 · 嵌套的声明类 · 枚举（线上字 ＝ 常量名的小写）· `Any?`（原样）。
 * 装不进（声明类和线上对不上：判据该先红）⇒ `null`。
 */
object Wire {
    inline fun <reified T : Any> bind(data: Any?): T? = bind(T::class.java, data)

    fun <T : Any> bind(
        type: Class<T>,
        data: Any?,
    ): T? =
        try {
            type.cast(value(type, data))
        } catch (_: Mismatch) {
            null
        } catch (_: InvocationTargetException) {
            null // 不可空的那一格来了 null（Kotlin 构造里的空值检查）
        } catch (_: IllegalArgumentException) {
            null // 基本型那一格来了 null
        }

    private class Mismatch : RuntimeException()

    private fun value(
        type: Type,
        v: Any?,
    ): Any? {
        if (v == null) return null
        return when (type) {
            is ParameterizedType -> parameterized(type, v)
            is WildcardType -> value(type.upperBounds.first(), v)
            is Class<*> -> plain(type, v)
            else -> throw Mismatch()
        }
    }

    private fun parameterized(
        type: ParameterizedType,
        v: Any?,
    ): Any? {
        val raw = type.rawType as Class<*>
        val arg = type.actualTypeArguments
        return when {
            List::class.java.isAssignableFrom(raw) -> (v as? List<*> ?: throw Mismatch()).map { value(arg[0], it) }
            Map::class.java.isAssignableFrom(raw) ->
                (v as? Map<*, *> ?: throw Mismatch()).entries.associate { (k, x) -> (k as String) to value(arg[1], x) }
            else -> throw Mismatch()
        }
    }

    private fun plain(
        type: Class<*>,
        v: Any,
    ): Any =
        when {
            type == String::class.java -> v as? String ?: throw Mismatch()
            type == java.lang.Long::class.java || type == Long::class.javaPrimitiveType -> (v as? Number ?: throw Mismatch()).toLong()
            type == java.lang.Integer::class.java || type == Int::class.javaPrimitiveType -> (v as? Number ?: throw Mismatch()).toInt()
            type == java.lang.Double::class.java || type == Double::class.javaPrimitiveType -> (v as? Number ?: throw Mismatch()).toDouble()
            type == java.lang.Boolean::class.java || type == Boolean::class.javaPrimitiveType -> v as? Boolean ?: throw Mismatch()
            type == Any::class.java -> v
            type.isEnum -> type.enumConstants.firstOrNull { (it as Enum<*>).name.lowercase() == v } ?: throw Mismatch()
            type.isAnnotationPresent(WireOf::class.java) -> record(type, v as? Map<*, *> ?: throw Mismatch())
            else -> throw Mismatch()
        }

    private fun record(
        type: Class<*>,
        m: Map<*, *>,
    ): Any {
        val ctor = primary(type)
        val args =
            ctor.parameters
                .mapIndexed { i, p ->
                    val key = p.getAnnotation(Key::class.java)?.name ?: p.name
                    value(ctor.genericParameterTypes[i], m[key])
                }.toTypedArray()
        return ctor.newInstance(*args)
    }

    /** 声明类的主构造（参数最多、不是编译器补的默认参数那一份）。 */
    private fun primary(type: Class<*>): Constructor<*> =
        type.declaredConstructors
            .filter { c -> c.parameterTypes.none { it.name == "kotlin.jvm.internal.DefaultConstructorMarker" } }
            .maxByOrNull { it.parameterCount }
            ?.also { check(it.parameters.all { p -> p.isNamePresent }) { "${type.name}: -java-parameters" } }
            ?: throw Mismatch()
}

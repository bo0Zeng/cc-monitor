package com.ccmonitor.mobile.core.claude.link

import com.squareup.moshi.JsonAdapter
import com.squareup.moshi.JsonReader
import com.squareup.moshi.JsonWriter
import com.squareup.moshi.Moshi

/**
 * 帧客户端用的 JSON 读写（一份 moshi，`Any` 形：对象 ⇒ `Map`，数 ⇒ `Double`）。写回去时整数照整数写（`12` 不写成 `12.0`）：
 * 读进来的一份原样交回核心（文件页的原行）时，那边按整数类型解。
 */
internal object Json {
    private object WholeNumbers : JsonAdapter<Double>() {
        override fun fromJson(reader: JsonReader): Double = reader.nextDouble()

        override fun toJson(
            writer: JsonWriter,
            value: Double?,
        ) {
            when {
                value == null -> writer.nullValue()
                value % 1.0 == 0.0 && kotlin.math.abs(value) < WHOLE -> writer.value(value.toLong())
                else -> writer.value(value)
            }
        }

        /** 2^53：再大的 Double 已经不准到个位，照原样写。 */
        private const val WHOLE = 9.007199254740992E15
    }

    private val any =
        Moshi
            .Builder()
            .add(Double::class.java, WholeNumbers)
            .add(Double::class.javaObjectType, WholeNumbers)
            .build()
            .adapter(Any::class.java)

    /** 解一行；不是 JSON 对象 ⇒ `null`。 */
    fun obj(line: String): Map<String, Any?>? =
        @Suppress("UNCHECKED_CAST")
        (runCatching { any.fromJson(line) }.getOrNull() as? Map<String, Any?>)

    /** 写成一行（moshi 不出裸换行）。 */
    fun line(value: Any?): String = any.toJson(value)

    /** 一条命令的入参写成一行：值是 null 的键照写（后端有几格要显式交 null，缺了不认）。 */
    fun args(value: Map<String, Any?>): String = any.serializeNulls().toJson(value)

    /** 解一行成 `Any` 形（不限对象）；不是 JSON ⇒ `null`。 */
    fun value(line: String): Any? = runCatching { any.fromJson(line) }.getOrNull()
}

/**
 * 本地缓存那一侧用：核心交来的成品原样写成一行 JSON、读回来再交给同一个解码器（[HistoryRead.decodeRows] · [HistoryList.page]）。
 * 只搬不认：不挑格、不改格，手机不在缓存上另算一份。
 */
object RawJson {
    fun write(value: Any?): String = Json.line(value)

    fun read(text: String): Any? = Json.value(text)
}

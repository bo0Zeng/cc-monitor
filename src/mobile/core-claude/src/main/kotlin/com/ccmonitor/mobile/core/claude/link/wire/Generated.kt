// 生成的文件（tests/scripts/gen-kotlin-wire.mjs 读 ts-rs 导出的声明出的）。不要手改：改 Rust 那一份类型，再跑 `npm run gen:types`。
@file:Suppress("ktlint", "detekt:all")

package com.ccmonitor.mobile.core.claude.link.wire.gen

import com.squareup.moshi.JsonAdapter

import com.squareup.moshi.JsonDataException
import com.squareup.moshi.JsonReader
import com.squareup.moshi.JsonWriter

/** 线上原样的一段 JSON（声明里认不出的那几形）：读进来什么、写回去就是什么。 */
@JvmInline
value class Raw(val json: String)

/**
 * 对象的一格：线上键 · 它的编解码（头一回用到才取：各类的编解码互相引用，不按初始化先后）·
 * 可缺（写回时值为 null 就不写，Rust 那头 skip_serializing_if）· 可空（缺席照 null 读）。
 */
class Field(val key: String, ad: () -> JsonAdapter<*>, val optional: Boolean, val nullable: Boolean) {
    val ad: JsonAdapter<*> by lazy(ad)
}

/** 两端契约对不上（缺格 · 型不对 · 认不出的词）：说出是哪一格。 */
fun mismatch(what: String): JsonDataException = JsonDataException("两端契约对不上：" + what)

private fun JsonReader.notNull(what: String) {
    if (peek() == JsonReader.Token.NULL) throw mismatch(what + " 不该是 null（" + path + "）")
}

private inline fun <T> prim(
    crossinline read: (JsonReader) -> T,
    crossinline write: (JsonWriter, T) -> Unit,
    what: String,
): JsonAdapter<T> =
    object : JsonAdapter<T>() {
        override fun fromJson(reader: JsonReader): T {
            reader.notNull(what)
            return read(reader)
        }

        override fun toJson(writer: JsonWriter, value: T?) {
            write(writer, value ?: throw mismatch(what + " 不该是 null"))
        }
    }

val Str: JsonAdapter<String> = prim({ it.nextString() }, { w, v -> w.value(v) }, "字串")
val Bool: JsonAdapter<Boolean> = prim({ it.nextBoolean() }, { w, v -> w.value(v) }, "真假")
val Lng: JsonAdapter<Long> = prim({ it.nextLong() }, { w, v -> w.value(v) }, "整数")
val Dbl: JsonAdapter<Double> = prim({ it.nextDouble() }, { w, v -> w.value(v) }, "小数")
val RawJson: JsonAdapter<Raw> =
    prim({ r -> r.nextSource().use { Raw(it.readUtf8()) } }, { w, v -> w.valueSink().use { it.writeUtf8(v.json) } }, "那一段")

fun <T> opt(ad: JsonAdapter<T>): JsonAdapter<T?> =
    object : JsonAdapter<T?>() {
        override fun fromJson(reader: JsonReader): T? = if (reader.peek() == JsonReader.Token.NULL) reader.nextNull() else ad.fromJson(reader)

        override fun toJson(writer: JsonWriter, value: T?) {
            if (value == null) {
                val was = writer.serializeNulls
                writer.serializeNulls = true
                writer.nullValue()
                writer.serializeNulls = was
            } else {
                ad.toJson(writer, value)
            }
        }
    }

@Suppress("UNCHECKED_CAST")
fun <T> arr(ad: JsonAdapter<T>): JsonAdapter<List<T>> =
    prim({ r ->
        val out = ArrayList<T>()
        r.beginArray()
        while (r.hasNext()) out.add(ad.fromJson(r) as T)
        r.endArray()
        out
    }, { w, v ->
        w.beginArray()
        v.forEach { ad.toJson(w, it) }
        w.endArray()
    }, "数组")

@Suppress("UNCHECKED_CAST")
fun <T> dict(ad: JsonAdapter<T>): JsonAdapter<Map<String, T>> =
    prim({ r ->
        val out = LinkedHashMap<String, T>()
        r.beginObject()
        while (r.hasNext()) out[r.nextName()] = ad.fromJson(r) as T
        r.endObject()
        out
    }, { w, v ->
        w.beginObject()
        v.forEach { (k, x) ->
            w.name(k)
            ad.toJson(w, x)
        }
        w.endObject()
    }, "键集合")

fun <E : Enum<E>> enumOf(all: List<E>, wire: (E) -> String): JsonAdapter<E> =
    prim({ r ->
        val s = r.nextString()
        all.firstOrNull { wire(it) == s } ?: throw mismatch("认不出的词 \"" + s + "\"（" + r.path + "）")
    }, { w, v -> w.value(wire(v)) }, "词")

/**
 * 读进来的那一份记着各格到的先后，写回照它（ts-rs 导出时把 serde flatten 进来的格挪到了最后，声明里的先后不是 Rust 写的先后）；
 * 自己造的照声明的先后写（读的那一头不看先后）。不进 equals。
 */
interface Ordered {
    var wireOrder: IntArray?
}

/** 一个对象：读时认不得的格跳过，缺了非可空的格是契约对不上。 */
class ObjAd<T>(
    private val fields: List<Field>,
    private val build: (Array<Any?>) -> T,
    private val get: (T) -> Array<Any?>,
) : JsonAdapter<T>() {
    private val index = fields.withIndex().associate { (i, f) -> f.key to i }

    override fun fromJson(reader: JsonReader): T {
        reader.notNull("对象")
        val a = arrayOfNulls<Any?>(fields.size)
        val seen = BooleanArray(fields.size)
        val order = ArrayList<Int>(fields.size)
        reader.beginObject()
        while (reader.hasNext()) {
            val i = index[reader.nextName()]
            if (i == null) {
                reader.skipValue()
            } else {
                a[i] = fields[i].ad.fromJson(reader)
                if (!seen[i]) order.add(i)
                seen[i] = true
            }
        }
        reader.endObject()
        fields.forEachIndexed { i, f ->
            if (!seen[i] && !f.nullable) throw mismatch("缺格 \"" + f.key + "\"（" + reader.path + "）")
        }
        val built = build(a)
        (built as? Ordered)?.wireOrder = order.toIntArray()
        return built
    }

    override fun toJson(writer: JsonWriter, value: T?) {
        writer.beginObject()
        writeFields(writer, value ?: throw mismatch("对象不该是 null"))
        writer.endObject()
    }

    @Suppress("UNCHECKED_CAST")
    fun writeFields(writer: JsonWriter, value: T) {
        val v = get(value)
        val read = (value as? Ordered)?.wireOrder
        val order = if (read == null) fields.indices.toList() else read.toList() + fields.indices.filter { it !in read }
        for (i in order) {
            val f = fields[i]
            if (v[i] == null && (f.optional || read != null && i !in read)) continue
            writer.name(f.key)
            (f.ad as JsonAdapter<Any?>).toJson(writer, v[i])
        }
    }
}

fun <T> obj(fields: List<Field>, build: (Array<Any?>) -> T, get: (T) -> Array<Any?>): ObjAd<T> = ObjAd(fields, build, get)

/** 判别格那一格：读时只认那一个词，写时写它。 */
fun tag(word: String): JsonAdapter<String> =
    prim({ r ->
        val s = r.nextString()
        if (s != word) throw mismatch("判别格该是 \"" + word + "\"，来的是 \"" + s + "\"（" + r.path + "）")
        s
    }, { w, _ -> w.value(word) }, "判别格")

/** 带判别格的并集的一形：判别格的那个词 · 它的类 · 其余各格。 */
class Variant<T : Any>(val tag: String, val cls: Class<out T>, val ad: ObjAd<out T>)

/** 带判别格的并集（serde 的 internally tagged）：判别格是各形里的一格（位置照读进来的 · 照声明）。 */
fun <T : Any> tagged(disc: String, variants: List<Variant<T>>): JsonAdapter<T> =
    object : JsonAdapter<T>() {
        override fun fromJson(reader: JsonReader): T {
            reader.notNull("并集")
            val tag =
                reader.peekJson().use { p ->
                    p.beginObject()
                    var t: String? = null
                    while (p.hasNext()) {
                        if (p.nextName() == disc) {
                            t = p.nextString()
                            break
                        }
                        p.skipValue()
                    }
                    t
                } ?: throw mismatch("缺判别格 \"" + disc + "\"（" + reader.path + "）")
            val v = variants.firstOrNull { it.tag == tag } ?: throw mismatch("认不出的一形 \"" + tag + "\"（" + reader.path + "）")
            return v.ad.fromJson(reader) as T
        }

        @Suppress("UNCHECKED_CAST")
        override fun toJson(writer: JsonWriter, value: T?) {
            val x = value ?: throw mismatch("并集不该是 null")
            val v = variants.first { it.cls.isInstance(x) }
            (v.ad as ObjAd<T>).toJson(writer, x)
        }
    }

/** 每一形一个键的并集（serde 的 externally tagged）：{"那个键": 值}。 */
class Ext<T : Any>(val key: String, val cls: Class<out T>, ad: () -> JsonAdapter<*>, val wrap: (Any?) -> T, val unwrap: (T) -> Any?) {
    val ad: JsonAdapter<*> by lazy(ad)
}

fun <T : Any> external(variants: List<Ext<T>>): JsonAdapter<T> =
    object : JsonAdapter<T>() {
        override fun fromJson(reader: JsonReader): T {
            reader.notNull("并集")
            reader.beginObject()
            val key = reader.nextName()
            val v = variants.firstOrNull { it.key == key } ?: throw mismatch("认不出的一形 \"" + key + "\"（" + reader.path + "）")
            val x = v.wrap(v.ad.fromJson(reader))
            reader.endObject()
            return x
        }

        @Suppress("UNCHECKED_CAST")
        override fun toJson(writer: JsonWriter, value: T?) {
            val x = value ?: throw mismatch("并集不该是 null")
            val v = variants.first { it.cls.isInstance(x) }
            writer.beginObject()
            writer.name(v.key)
            (v.ad as JsonAdapter<Any?>).toJson(writer, v.unwrap(x))
            writer.endObject()
        }
    }

/** 各形按 JSON 记号分得开的并集（serde 的 untagged：串 · 数 · 真假 · 对象 · 数组各至多一形）。 */
class Tok<T : Any>(val token: JsonReader.Token, val cls: Class<out T>, ad: () -> JsonAdapter<*>, val wrap: (Any?) -> T, val unwrap: (T) -> Any?) {
    val ad: JsonAdapter<*> by lazy(ad)
}

fun <T : Any> byToken(variants: List<Tok<T>>): JsonAdapter<T> =
    object : JsonAdapter<T>() {
        override fun fromJson(reader: JsonReader): T {
            val tok = reader.peek()
            val v = variants.firstOrNull { it.token == tok } ?: throw mismatch("这一格不该是 " + tok + "（" + reader.path + "）")
            return v.wrap(v.ad.fromJson(reader))
        }

        @Suppress("UNCHECKED_CAST")
        override fun toJson(writer: JsonWriter, value: T?) {
            val x = value ?: throw mismatch("并集不该是 null")
            val v = variants.first { it.cls.isInstance(x) }
            (v.ad as JsonAdapter<Any?>).toJson(writer, v.unwrap(x))
        }
    }

/** 定长数组（Rust 的元组）。 */
fun <T> tuple(lazyAds: List<() -> JsonAdapter<*>>, build: (Array<Any?>) -> T, get: (T) -> Array<Any?>): JsonAdapter<T> =
    prim({ r ->
        val ads = lazyAds.map { it() }
        val a = arrayOfNulls<Any?>(ads.size)
        r.beginArray()
        ads.forEachIndexed { i, ad -> a[i] = ad.fromJson(r) }
        r.endArray()
        build(a)
    }, { w, v ->
        val a = get(v)
        val ads = lazyAds.map { it() }
        w.beginArray()
        @Suppress("UNCHECKED_CAST")
        ads.forEachIndexed { i, ad -> (ad as JsonAdapter<Any?>).toJson(w, a[i]) }
        w.endArray()
    }, "元组")

@Suppress("UNCHECKED_CAST")
data class AccountRow(
    val keyMasked: String?,
    val baseUrl: String?,
    val badge: Badge,
    val plate: String,
    val quota: String?,
    val unselectable: String?,
    val login: Boolean,
    val newSession: Boolean,
    val name: String,
    val label: String,
    val email: String,
    val configDir: String?,
    val isDefault: Boolean,
    val mode: String,
    val exists: Boolean,
    val loggedIn: Boolean,
    val authKind: String,
    val authReady: Boolean,
    val selectable: Boolean,
    val color: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountRow> = obj(
            listOf(
            Field("keyMasked", { opt(Str) }, optional = false, nullable = true),
            Field("baseUrl", { opt(Str) }, optional = false, nullable = true),
            Field("badge", { Badge.ADAPTER }, optional = false, nullable = false),
            Field("plate", { Str }, optional = false, nullable = false),
            Field("quota", { opt(Str) }, optional = false, nullable = true),
            Field("unselectable", { opt(Str) }, optional = false, nullable = true),
            Field("login", { Bool }, optional = false, nullable = false),
            Field("newSession", { Bool }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("label", { Str }, optional = false, nullable = false),
            Field("email", { Str }, optional = false, nullable = false),
            Field("configDir", { opt(Str) }, optional = false, nullable = true),
            Field("isDefault", { Bool }, optional = false, nullable = false),
            Field("mode", { Str }, optional = false, nullable = false),
            Field("exists", { Bool }, optional = false, nullable = false),
            Field("loggedIn", { Bool }, optional = false, nullable = false),
            Field("authKind", { Str }, optional = false, nullable = false),
            Field("authReady", { Bool }, optional = false, nullable = false),
            Field("selectable", { Bool }, optional = false, nullable = false),
            Field("color", { Lng }, optional = false, nullable = false),
            ),
            { a -> AccountRow(a[0] as String?, a[1] as String?, a[2] as Badge, a[3] as String, a[4] as String?, a[5] as String?, a[6] as Boolean, a[7] as Boolean, a[8] as String, a[9] as String, a[10] as String, a[11] as String?, a[12] as Boolean, a[13] as String, a[14] as Boolean, a[15] as Boolean, a[16] as String, a[17] as Boolean, a[18] as Boolean, a[19] as Long) },
            { v -> arrayOf(v.keyMasked, v.baseUrl, v.badge, v.plate, v.quota, v.unselectable, v.login, v.newSession, v.name, v.label, v.email, v.configDir, v.isDefault, v.mode, v.exists, v.loggedIn, v.authKind, v.authReady, v.selectable, v.color) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AccountsList(
    val meta: ListMeta,
    val accounts: List<AccountRow>,
    val notice: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountsList> = obj(
            listOf(
            Field("meta", { ListMeta.ADAPTER }, optional = false, nullable = false),
            Field("accounts", { arr(AccountRow.ADAPTER) }, optional = false, nullable = false),
            Field("notice", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> AccountsList(a[0] as ListMeta, a[1] as List<AccountRow>, a[2] as String?) },
            { v -> arrayOf(v.meta, v.accounts, v.notice) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AgentFacts(
    val kind: String,
    val text: String,
    val models: List<AgentPick>,
    val modelDefault: AgentPick?,
    val modelNote: String?,
    val permissionModes: List<AgentPick>,
    val permissionDefault: AgentPick?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AgentFacts> = obj(
            listOf(
            Field("kind", { Str }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("models", { arr(AgentPick.ADAPTER) }, optional = false, nullable = false),
            Field("modelDefault", { opt(AgentPick.ADAPTER) }, optional = false, nullable = true),
            Field("modelNote", { opt(Str) }, optional = false, nullable = true),
            Field("permissionModes", { arr(AgentPick.ADAPTER) }, optional = false, nullable = false),
            Field("permissionDefault", { opt(AgentPick.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> AgentFacts(a[0] as String, a[1] as String, a[2] as List<AgentPick>, a[3] as AgentPick?, a[4] as String?, a[5] as List<AgentPick>, a[6] as AgentPick?) },
            { v -> arrayOf(v.kind, v.text, v.models, v.modelDefault, v.modelNote, v.permissionModes, v.permissionDefault) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AgentPick(
    val value: String,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AgentPick> = obj(
            listOf(
            Field("value", { Str }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> AgentPick(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.value, v.text) },
        )
    }
}

sealed interface Allow {
    data object Yes : Allow
    @Suppress("UNCHECKED_CAST")
    data class No(
        val why: String,
    ) : Allow, Ordered {
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<Allow> =
            tagged<Allow>(
                "can",
                listOf(
                Variant("yes", Allow.Yes::class.java, obj(
                        listOf(
                        Field("can", { tag("yes") }, optional = false, nullable = false),
                        ),
                        { Allow.Yes },
                        { arrayOf("yes") },
                    )),
                Variant("no", Allow.No::class.java, obj(
                        listOf(
                        Field("can", { tag("no") }, optional = false, nullable = false),
                        Field("why", { Str }, optional = false, nullable = false),
                        ),
                        { a -> Allow.No(a[1] as String) },
                        { v -> arrayOf("no", v.why) },
                    )),
                ),
            )
    }
}

sealed interface Answer {
    data object Approved : Answer
    @Suppress("UNCHECKED_CAST")
    data class Picked(
        val options: List<String>,
    ) : Answer, Ordered {
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<Answer> =
            tagged<Answer>(
                "kind",
                listOf(
                Variant("approved", Answer.Approved::class.java, obj(
                        listOf(
                        Field("kind", { tag("approved") }, optional = false, nullable = false),
                        ),
                        { Answer.Approved },
                        { arrayOf("approved") },
                    )),
                Variant("picked", Answer.Picked::class.java, obj(
                        listOf(
                        Field("kind", { tag("picked") }, optional = false, nullable = false),
                        Field("options", { arr(Str) }, optional = false, nullable = false),
                        ),
                        { a -> Answer.Picked(a[1] as List<String>) },
                        { v -> arrayOf("picked", v.options) },
                    )),
                ),
            )
    }
}

enum class AnswerResult(val wire: String) {
    ANSWERED("answered"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<AnswerResult> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class Answered(
    val result: AnswerResult,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Answered> = obj(
            listOf(
            Field("result", { AnswerResult.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> Answered(a[0] as AnswerResult) },
            { v -> arrayOf(v.result) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AnsweredRow(
    val sid: String,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AnsweredRow> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> AnsweredRow(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.sid, v.said) },
        )
    }
}

enum class ApiReason(val wire: String) {
    OVERLOADED("overloaded"),
    QUOTA("quota"),
    NETWORK("network"),
    AUTH("auth"),
    CONTEXT("context"),
    UNKNOWN("unknown"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ApiReason> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ArriveAsk(
    val by: ArriveBy,
    val since: Long,
    val session: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ArriveAsk> = obj(
            listOf(
            Field("by", { ArriveBy.ADAPTER }, optional = false, nullable = false),
            Field("since", { Lng }, optional = false, nullable = false),
            Field("session", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ArriveAsk(a[0] as ArriveBy, a[1] as Long, a[2] as String?) },
            { v -> arrayOf(v.by, v.since, v.session) },
        )
    }
}

sealed interface ArriveBy {
    @Suppress("UNCHECKED_CAST")
    data class Sid(
        val sid: String,
    ) : ArriveBy, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Cwd(
        val cwd: String,
    ) : ArriveBy, Ordered {
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<ArriveBy> =
            tagged<ArriveBy>(
                "kind",
                listOf(
                Variant("sid", ArriveBy.Sid::class.java, obj(
                        listOf(
                        Field("kind", { tag("sid") }, optional = false, nullable = false),
                        Field("sid", { Str }, optional = false, nullable = false),
                        ),
                        { a -> ArriveBy.Sid(a[1] as String) },
                        { v -> arrayOf("sid", v.sid) },
                    )),
                Variant("cwd", ArriveBy.Cwd::class.java, obj(
                        listOf(
                        Field("kind", { tag("cwd") }, optional = false, nullable = false),
                        Field("cwd", { Str }, optional = false, nullable = false),
                        ),
                        { a -> ArriveBy.Cwd(a[1] as String) },
                        { v -> arrayOf("cwd", v.cwd) },
                    )),
                ),
            )
    }
}

enum class AskKind(val wire: String) {
    APPROVE("approve"),
    ANSWER("answer"),
    PLAN("plan"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<AskKind> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class AskOption(
    val label: String,
    val description: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AskOption> = obj(
            listOf(
            Field("label", { Str }, optional = false, nullable = false),
            Field("description", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> AskOption(a[0] as String, a[1] as String?) },
            { v -> arrayOf(v.label, v.description) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AskQuestion(
    val header: String? = null,
    val question: String,
    val multi: Boolean,
    val options: List<AskOption>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AskQuestion> = obj(
            listOf(
            Field("header", { opt(Str) }, optional = true, nullable = true),
            Field("question", { Str }, optional = false, nullable = false),
            Field("multi", { Bool }, optional = false, nullable = false),
            Field("options", { arr(AskOption.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> AskQuestion(a[0] as String?, a[1] as String, a[2] as Boolean, a[3] as List<AskOption>) },
            { v -> arrayOf(v.header, v.question, v.multi, v.options) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Attached(
    val kind: AttachedKind,
    val since: Long,
    val lastActivity: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Attached> = obj(
            listOf(
            Field("kind", { AttachedKind.ADAPTER }, optional = false, nullable = false),
            Field("since", { Lng }, optional = false, nullable = false),
            Field("last_activity", { Lng }, optional = false, nullable = false),
            ),
            { a -> Attached(a[0] as AttachedKind, a[1] as Long, a[2] as Long) },
            { v -> arrayOf(v.kind, v.since, v.lastActivity) },
        )
    }
}

enum class AttachedKind(val wire: String) {
    TERMINAL_WINDOW("terminal-window"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<AttachedKind> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class Badge(
    val text: String,
    val warn: Boolean,
    val title: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Badge> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            Field("warn", { Bool }, optional = false, nullable = false),
            Field("title", { Str }, optional = false, nullable = false),
            ),
            { a -> Badge(a[0] as String, a[1] as Boolean, a[2] as String) },
            { v -> arrayOf(v.text, v.warn, v.title) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BatchOutcome(
    val key: String,
    val state: ItemState,
    val path: WirePath? = null,
    val newName: WirePath? = null,
    val bytes: Long? = null,
    val files: Long? = null,
    val dirs: Long? = null,
    val removed: Long? = null,
    val before: Long? = null,
    val code: String? = null,
    val reason: String? = null,
    val detail: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BatchOutcome> = obj(
            listOf(
            Field("key", { Str }, optional = false, nullable = false),
            Field("state", { ItemState.ADAPTER }, optional = false, nullable = false),
            Field("path", { opt(WirePath.ADAPTER) }, optional = true, nullable = true),
            Field("new_name", { opt(WirePath.ADAPTER) }, optional = true, nullable = true),
            Field("bytes", { opt(Lng) }, optional = true, nullable = true),
            Field("files", { opt(Lng) }, optional = true, nullable = true),
            Field("dirs", { opt(Lng) }, optional = true, nullable = true),
            Field("removed", { opt(Lng) }, optional = true, nullable = true),
            Field("before", { opt(Lng) }, optional = true, nullable = true),
            Field("code", { opt(Str) }, optional = true, nullable = true),
            Field("reason", { opt(Str) }, optional = true, nullable = true),
            Field("detail", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> BatchOutcome(a[0] as String, a[1] as ItemState, a[2] as WirePath?, a[3] as WirePath?, a[4] as Long?, a[5] as Long?, a[6] as Long?, a[7] as Long?, a[8] as Long?, a[9] as String?, a[10] as String?, a[11] as String?) },
            { v -> arrayOf(v.key, v.state, v.path, v.newName, v.bytes, v.files, v.dirs, v.removed, v.before, v.code, v.reason, v.detail) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BatchReply(
    val outcomes: List<BatchOutcome>,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BatchReply> = obj(
            listOf(
            Field("outcomes", { arr(BatchOutcome.ADAPTER) }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> BatchReply(a[0] as List<BatchOutcome>, a[1] as String) },
            { v -> arrayOf(v.outcomes, v.said) },
        )
    }
}

sealed interface Block {
    @Suppress("UNCHECKED_CAST")
    data class Text(
        val text: String,
    ) : Block, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Thinking(
        val text: String,
    ) : Block, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class ToolUse(
        val id: String,
        val name: String,
        val input: Raw,
    ) : Block, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class ToolResult(
        val `for`: String,
        val content: List<Block>,
        val isError: Boolean,
    ) : Block, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Image(
        val source: Raw,
    ) : Block, Ordered {
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<Block> =
            tagged<Block>(
                "type",
                listOf(
                Variant("text", Block.Text::class.java, obj(
                        listOf(
                        Field("type", { tag("text") }, optional = false, nullable = false),
                        Field("text", { Str }, optional = false, nullable = false),
                        ),
                        { a -> Block.Text(a[1] as String) },
                        { v -> arrayOf("text", v.text) },
                    )),
                Variant("thinking", Block.Thinking::class.java, obj(
                        listOf(
                        Field("type", { tag("thinking") }, optional = false, nullable = false),
                        Field("text", { Str }, optional = false, nullable = false),
                        ),
                        { a -> Block.Thinking(a[1] as String) },
                        { v -> arrayOf("thinking", v.text) },
                    )),
                Variant("tool_use", Block.ToolUse::class.java, obj(
                        listOf(
                        Field("type", { tag("tool_use") }, optional = false, nullable = false),
                        Field("id", { Str }, optional = false, nullable = false),
                        Field("name", { Str }, optional = false, nullable = false),
                        Field("input", { RawJson }, optional = false, nullable = false),
                        ),
                        { a -> Block.ToolUse(a[1] as String, a[2] as String, a[3] as Raw) },
                        { v -> arrayOf("tool_use", v.id, v.name, v.input) },
                    )),
                Variant("tool_result", Block.ToolResult::class.java, obj(
                        listOf(
                        Field("type", { tag("tool_result") }, optional = false, nullable = false),
                        Field("for", { Str }, optional = false, nullable = false),
                        Field("content", { arr(Block.ADAPTER) }, optional = false, nullable = false),
                        Field("isError", { Bool }, optional = false, nullable = false),
                        ),
                        { a -> Block.ToolResult(a[1] as String, a[2] as List<Block>, a[3] as Boolean) },
                        { v -> arrayOf("tool_result", v.`for`, v.content, v.isError) },
                    )),
                Variant("image", Block.Image::class.java, obj(
                        listOf(
                        Field("type", { tag("image") }, optional = false, nullable = false),
                        Field("source", { RawJson }, optional = false, nullable = false),
                        ),
                        { a -> Block.Image(a[1] as Raw) },
                        { v -> arrayOf("image", v.source) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class Bookmarks(
    val dirs: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Bookmarks> = obj(
            listOf(
            Field("dirs", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> Bookmarks(a[0] as List<String>) },
            { v -> arrayOf(v.dirs) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BusFailed(
    val id: String,
    val why: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BusFailed> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("why", { Str }, optional = false, nullable = false),
            ),
            { a -> BusFailed(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.id, v.why) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BusNote(
    val removed: List<String>,
    val failed: List<BusFailed>,
    val unread: String?,
    val said: String?,
    val detail: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BusNote> = obj(
            listOf(
            Field("removed", { arr(Str) }, optional = false, nullable = false),
            Field("failed", { arr(BusFailed.ADAPTER) }, optional = false, nullable = false),
            Field("unread", { opt(Str) }, optional = false, nullable = true),
            Field("said", { opt(Str) }, optional = false, nullable = true),
            Field("detail", { Str }, optional = false, nullable = false),
            ),
            { a -> BusNote(a[0] as List<String>, a[1] as List<BusFailed>, a[2] as String?, a[3] as String?, a[4] as String) },
            { v -> arrayOf(v.removed, v.failed, v.unread, v.said, v.detail) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Can(
    val resume: CanResume,
    val accounts: Boolean,
    val fork: Boolean,
    val delete: CanDelete,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Can> = obj(
            listOf(
            Field("resume", { CanResume.ADAPTER }, optional = false, nullable = false),
            Field("accounts", { Bool }, optional = false, nullable = false),
            Field("fork", { Bool }, optional = false, nullable = false),
            Field("delete", { CanDelete.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> Can(a[0] as CanResume, a[1] as Boolean, a[2] as Boolean, a[3] as CanDelete) },
            { v -> arrayOf(v.resume, v.accounts, v.fork, v.delete) },
        )
    }
}

sealed interface CanClose {
    data object Quiet : CanClose
    data object Menu : CanClose
    @Suppress("UNCHECKED_CAST")
    data class No(
        val why: String,
    ) : CanClose, Ordered {
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<CanClose> =
            tagged<CanClose>(
                "can",
                listOf(
                Variant("quiet", CanClose.Quiet::class.java, obj(
                        listOf(
                        Field("can", { tag("quiet") }, optional = false, nullable = false),
                        ),
                        { CanClose.Quiet },
                        { arrayOf("quiet") },
                    )),
                Variant("menu", CanClose.Menu::class.java, obj(
                        listOf(
                        Field("can", { tag("menu") }, optional = false, nullable = false),
                        ),
                        { CanClose.Menu },
                        { arrayOf("menu") },
                    )),
                Variant("no", CanClose.No::class.java, obj(
                        listOf(
                        Field("can", { tag("no") }, optional = false, nullable = false),
                        Field("why", { Str }, optional = false, nullable = false),
                        ),
                        { a -> CanClose.No(a[1] as String) },
                        { v -> arrayOf("no", v.why) },
                    )),
                ),
            )
    }
}

enum class CanDelete(val wire: String) {
    YES("yes"),
    LIVE("live"),
    UNSURE("unsure"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<CanDelete> = enumOf(entries) { it.wire }
    }
}

enum class CanResume(val wire: String) {
    YES("yes"),
    SWITCH("switch"),
    BG("bg"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<CanResume> = enumOf(entries) { it.wire }
    }
}

sealed interface CanTerm {
    @Suppress("UNCHECKED_CAST")
    data class Yes(
        val tmux: Boolean,
    ) : CanTerm, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class No(
        val why: String,
    ) : CanTerm, Ordered {
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<CanTerm> =
            tagged<CanTerm>(
                "can",
                listOf(
                Variant("yes", CanTerm.Yes::class.java, obj(
                        listOf(
                        Field("can", { tag("yes") }, optional = false, nullable = false),
                        Field("tmux", { Bool }, optional = false, nullable = false),
                        ),
                        { a -> CanTerm.Yes(a[1] as Boolean) },
                        { v -> arrayOf("yes", v.tmux) },
                    )),
                Variant("no", CanTerm.No::class.java, obj(
                        listOf(
                        Field("can", { tag("no") }, optional = false, nullable = false),
                        Field("why", { Str }, optional = false, nullable = false),
                        ),
                        { a -> CanTerm.No(a[1] as String) },
                        { v -> arrayOf("no", v.why) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class ChildRunTag(
    val label: String,
    val kind: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ChildRunTag> = obj(
            listOf(
            Field("label", { Str }, optional = false, nullable = false),
            Field("kind", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> ChildRunTag(a[0] as String, a[1] as String?) },
            { v -> arrayOf(v.label, v.kind) },
        )
    }
}

enum class CommandScope(val wire: String) {
    BUILTIN("builtin"),
    PROJECT("project"),
    USER("user"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<CommandScope> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class Crumb(
    val name: WirePath,
    val path: WirePath,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Crumb> = obj(
            listOf(
            Field("name", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> Crumb(a[0] as WirePath, a[1] as WirePath) },
            { v -> arrayOf(v.name, v.path) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Cursor(
    val x: Long,
    val y: Long,
    val visible: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Cursor> = obj(
            listOf(
            Field("x", { Lng }, optional = false, nullable = false),
            Field("y", { Lng }, optional = false, nullable = false),
            Field("visible", { Bool }, optional = false, nullable = false),
            ),
            { a -> Cursor(a[0] as Long, a[1] as Long, a[2] as Boolean) },
            { v -> arrayOf(v.x, v.y, v.visible) },
        )
    }
}

sealed interface FaceLive {
    @Suppress("UNCHECKED_CAST")
    data class Yes(
        val activity: WireSessionActivity? = null,
    ) : FaceLive, Ordered {
        override var wireOrder: IntArray? = null
    }
    data object No : FaceLive

    companion object {
        val ADAPTER: JsonAdapter<FaceLive> =
            tagged<FaceLive>(
                "is",
                listOf(
                Variant("yes", FaceLive.Yes::class.java, obj(
                        listOf(
                        Field("is", { tag("yes") }, optional = false, nullable = false),
                        Field("activity", { opt(WireSessionActivity.ADAPTER) }, optional = true, nullable = true),
                        ),
                        { a -> FaceLive.Yes(a[1] as WireSessionActivity?) },
                        { v -> arrayOf("yes", v.activity) },
                    )),
                Variant("no", FaceLive.No::class.java, obj(
                        listOf(
                        Field("is", { tag("no") }, optional = false, nullable = false),
                        ),
                        { FaceLive.No },
                        { arrayOf("no") },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class Fact(
    val account: String,
    val kind: QuotaKind,
    val w5h: WinFact,
    val w7d: WinFact,
    val weekly: Boolean? = null,
    val login: LoginState,
    val seenAt: Long? = null,
    val blocked: FactBlocked? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Fact> = obj(
            listOf(
            Field("account", { Str }, optional = false, nullable = false),
            Field("kind", { QuotaKind.ADAPTER }, optional = false, nullable = false),
            Field("w5h", { WinFact.ADAPTER }, optional = false, nullable = false),
            Field("w7d", { WinFact.ADAPTER }, optional = false, nullable = false),
            Field("weekly", { opt(Bool) }, optional = true, nullable = true),
            Field("login", { LoginState.ADAPTER }, optional = false, nullable = false),
            Field("seenAt", { opt(Lng) }, optional = true, nullable = true),
            Field("blocked", { opt(FactBlocked.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> Fact(a[0] as String, a[1] as QuotaKind, a[2] as WinFact, a[3] as WinFact, a[4] as Boolean?, a[5] as LoginState, a[6] as Long?, a[7] as FactBlocked?) },
            { v -> arrayOf(v.account, v.kind, v.w5h, v.w7d, v.weekly, v.login, v.seenAt, v.blocked) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FactBlocked(
    val why: FactWhy,
    val until: Long? = null,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FactBlocked> = obj(
            listOf(
            Field("why", { FactWhy.ADAPTER }, optional = false, nullable = false),
            Field("until", { opt(Lng) }, optional = true, nullable = true),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> FactBlocked(a[0] as FactWhy, a[1] as Long?, a[2] as String) },
            { v -> arrayOf(v.why, v.until, v.text) },
        )
    }
}

enum class FactWhy(val wire: String) {
    REFUSED("refused"),
    OVERAGE("overage"),
    USED("used"),
    LOGIN("login"),
    KEY("key"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<FactWhy> = enumOf(entries) { it.wire }
    }
}

enum class FileClass(val wire: String) {
    FOLDER("folder"),
    TEXT("text"),
    CODE("code"),
    IMAGE("image"),
    ARCHIVE("archive"),
    PDF("pdf"),
    LINK("link"),
    OTHER("other"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<FileClass> = enumOf(entries) { it.wire }
    }
}

enum class FileKind(val wire: String) {
    DIR("dir"),
    FILE("file"),
    SYMLINK("symlink"),
    OTHER("other"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<FileKind> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesPathBytesReply(
    val path: WirePath,
    val bytes: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesPathBytesReply> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("bytes", { Lng }, optional = false, nullable = false),
            ),
            { a -> FilesPathBytesReply(a[0] as WirePath, a[1] as Long) },
            { v -> arrayOf(v.path, v.bytes) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesPathReply(
    val path: WirePath,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesPathReply> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> FilesPathReply(a[0] as WirePath) },
            { v -> arrayOf(v.path) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesStageChunkReply(
    val bytes: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesStageChunkReply> = obj(
            listOf(
            Field("bytes", { Lng }, optional = false, nullable = false),
            ),
            { a -> FilesStageChunkReply(a[0] as Long) },
            { v -> arrayOf(v.bytes) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesWrittenReply(
    val path: WirePath,
    val bytes: Long,
    val sha256: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesWrittenReply> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("bytes", { Lng }, optional = false, nullable = false),
            Field("sha256", { Str }, optional = false, nullable = false),
            ),
            { a -> FilesWrittenReply(a[0] as WirePath, a[1] as Long, a[2] as String) },
            { v -> arrayOf(v.path, v.bytes, v.sha256) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FirstSay(
    val result: String,
    val why: String?,
    val said: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FirstSay> = obj(
            listOf(
            Field("result", { Str }, optional = false, nullable = false),
            Field("why", { opt(Str) }, optional = false, nullable = true),
            Field("said", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> FirstSay(a[0] as String, a[1] as String?, a[2] as String?) },
            { v -> arrayOf(v.result, v.why, v.said) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ForkFacts(
    val agent: String,
    val launch: ForkLaunch,
    val turn: Long?,
    val startText: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ForkFacts> = obj(
            listOf(
            Field("agent", { Str }, optional = false, nullable = false),
            Field("launch", { ForkLaunch.ADAPTER }, optional = false, nullable = false),
            Field("turn", { opt(Lng) }, optional = false, nullable = true),
            Field("startText", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ForkFacts(a[0] as String, a[1] as ForkLaunch, a[2] as Long?, a[3] as String?) },
            { v -> arrayOf(v.agent, v.launch, v.turn, v.startText) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ForkLaunch(
    val cwd: ForkSlot<String>,
    val account: ForkSlot<String?>,
    val terminal: ForkSlot<ForkTerminal>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ForkLaunch> = obj(
            listOf(
            Field("cwd", { ForkSlot.adapter(Str) }, optional = false, nullable = false),
            Field("account", { ForkSlot.adapter(opt(Str)) }, optional = false, nullable = false),
            Field("terminal", { ForkSlot.adapter(ForkTerminal.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> ForkLaunch(a[0] as ForkSlot<String>, a[1] as ForkSlot<String?>, a[2] as ForkSlot<ForkTerminal>) },
            { v -> arrayOf(v.cwd, v.account, v.terminal) },
        )
    }
}

sealed interface ForkSlot<T> {
    @Suppress("UNCHECKED_CAST")
    data class Known<T>(
        val value: T,
        val from: ForkWhence,
    ) : ForkSlot<T>, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Unknown<T>(
        val why: ForkWhy,
    ) : ForkSlot<T>, Ordered {
        override var wireOrder: IntArray? = null
    }

    companion object {
        fun <T> adapter(adT: JsonAdapter<T>): JsonAdapter<ForkSlot<T>> =
            tagged<ForkSlot<T>>(
                "kind",
                listOf(
                Variant("known", ForkSlot.Known::class.java as Class<out ForkSlot<T>>, obj(
                        listOf(
                        Field("kind", { tag("known") }, optional = false, nullable = false),
                        Field("value", { adT }, optional = false, nullable = false),
                        Field("from", { ForkWhence.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> ForkSlot.Known<T>(a[1] as T, a[2] as ForkWhence) },
                        { v -> arrayOf("known", v.value, v.from) },
                    )),
                Variant("unknown", ForkSlot.Unknown::class.java as Class<out ForkSlot<T>>, obj(
                        listOf(
                        Field("kind", { tag("unknown") }, optional = false, nullable = false),
                        Field("why", { ForkWhy.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> ForkSlot.Unknown<T>(a[1] as ForkWhy) },
                        { v -> arrayOf("unknown", v.why) },
                    )),
                ),
            )
    }
}

sealed interface ForkTerminal {
    @Suppress("UNCHECKED_CAST")
    data class Tmux(
        val terminal: String,
    ) : ForkTerminal, Ordered {
        override var wireOrder: IntArray? = null
    }
    data object None : ForkTerminal

    companion object {
        val ADAPTER: JsonAdapter<ForkTerminal> =
            tagged<ForkTerminal>(
                "host",
                listOf(
                Variant("tmux", ForkTerminal.Tmux::class.java, obj(
                        listOf(
                        Field("host", { tag("tmux") }, optional = false, nullable = false),
                        Field("terminal", { Str }, optional = false, nullable = false),
                        ),
                        { a -> ForkTerminal.Tmux(a[1] as String) },
                        { v -> arrayOf("tmux", v.terminal) },
                    )),
                Variant("none", ForkTerminal.None::class.java, obj(
                        listOf(
                        Field("host", { tag("none") }, optional = false, nullable = false),
                        ),
                        { ForkTerminal.None },
                        { arrayOf("none") },
                    )),
                ),
            )
    }
}

enum class ForkWhence(val wire: String) {
    RECORD("record"),
    PROCESS("process"),
    TERMINAL_LIST("terminal_list"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ForkWhence> = enumOf(entries) { it.wire }
    }
}

enum class ForkWhy(val wire: String) {
    EXITED("exited"),
    NO_CWD("no_cwd"),
    LIVE_NO_ACCOUNT("live_no_account"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ForkWhy> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ForkedFrom(
    val label: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ForkedFrom> = obj(
            listOf(
            Field("label", { Str }, optional = false, nullable = false),
            ),
            { a -> ForkedFrom(a[0] as String) },
            { v -> arrayOf(v.label) },
        )
    }
}

sealed interface Gate {
    data class Cap(
        val value: Long,
    ) : Gate
    data class No(
        val value: String,
    ) : Gate

    companion object {
        val ADAPTER: JsonAdapter<Gate> =
            external<Gate>(
                listOf(
                Ext("cap", Gate.Cap::class.java, { Lng }, { Gate.Cap(it as Long) }, { (it as Gate.Cap).value }),
                Ext("no", Gate.No::class.java, { Str }, { Gate.No(it as String) }, { (it as Gate.No).value }),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class Gates(
    val preview: PreviewGate,
    val view: Gate? = null,
    val edit: Gate,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Gates> = obj(
            listOf(
            Field("preview", { PreviewGate.ADAPTER }, optional = false, nullable = false),
            Field("view", { opt(Gate.ADAPTER) }, optional = true, nullable = true),
            Field("edit", { Gate.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> Gates(a[0] as PreviewGate, a[1] as Gate?, a[2] as Gate) },
            { v -> arrayOf(v.preview, v.view, v.edit) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Held(
    val path: String,
    val next: Long,
    val nextByte: Long,
    val mark: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Held> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("next", { Lng }, optional = false, nullable = false),
            Field("next_byte", { Lng }, optional = false, nullable = false),
            Field("mark", { Str }, optional = false, nullable = false),
            ),
            { a -> Held(a[0] as String, a[1] as Long, a[2] as Long, a[3] as String) },
            { v -> arrayOf(v.path, v.next, v.nextByte, v.mark) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HexBytes(
    val b16: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HexBytes> = obj(
            listOf(
            Field("b16", { Str }, optional = false, nullable = false),
            ),
            { a -> HexBytes(a[0] as String) },
            { v -> arrayOf(v.b16) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Hidden(
    val by: HiddenBy,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Hidden> = obj(
            listOf(
            Field("by", { HiddenBy.ADAPTER }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> Hidden(a[0] as HiddenBy, a[1] as String) },
            { v -> arrayOf(v.by, v.said) },
        )
    }
}

enum class HiddenBy(val wire: String) {
    SELF("self"),
    ALIAS("alias"),
    LAUNCH("launch"),
    CHILD("child"),
    BG("bg"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<HiddenBy> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryGroup(
    val key: String,
    val agent: String,
    val projectName: String,
    val projectPath: String,
    val projectDir: String,
    val count: Long,
    val hasLive: Boolean?,
    val starred: Boolean,
    val lastActivity: Long,
    val atText: String?,
    val order: Long,
    val failed: String?,
    val origin: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryGroup> = obj(
            listOf(
            Field("key", { Str }, optional = false, nullable = false),
            Field("agent", { Str }, optional = false, nullable = false),
            Field("projectName", { Str }, optional = false, nullable = false),
            Field("projectPath", { Str }, optional = false, nullable = false),
            Field("projectDir", { Str }, optional = false, nullable = false),
            Field("count", { Lng }, optional = false, nullable = false),
            Field("hasLive", { opt(Bool) }, optional = false, nullable = true),
            Field("starred", { Bool }, optional = false, nullable = false),
            Field("lastActivity", { Lng }, optional = false, nullable = false),
            Field("atText", { opt(Str) }, optional = false, nullable = true),
            Field("order", { Lng }, optional = false, nullable = false),
            Field("failed", { opt(Str) }, optional = false, nullable = true),
            Field("origin", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> HistoryGroup(a[0] as String, a[1] as String, a[2] as String, a[3] as String, a[4] as String, a[5] as Long, a[6] as Boolean?, a[7] as Boolean, a[8] as Long, a[9] as String?, a[10] as Long, a[11] as String?, a[12] as String?) },
            { v -> arrayOf(v.key, v.agent, v.projectName, v.projectPath, v.projectDir, v.count, v.hasLive, v.starred, v.lastActivity, v.atText, v.order, v.failed, v.origin) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryHead(
    val groups: List<HistoryGroup>,
    val notice: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryHead> = obj(
            listOf(
            Field("groups", { arr(HistoryGroup.ADAPTER) }, optional = false, nullable = false),
            Field("notice", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> HistoryHead(a[0] as List<HistoryGroup>, a[1] as String?) },
            { v -> arrayOf(v.groups, v.notice) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryLines(
    val from: Long,
    val next: Long,
    val eof: Boolean,
    val end: Long,
    val lines: List<RecordLine>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryLines> = obj(
            listOf(
            Field("from", { Lng }, optional = false, nullable = false),
            Field("next", { Lng }, optional = false, nullable = false),
            Field("eof", { Bool }, optional = false, nullable = false),
            Field("end", { Lng }, optional = false, nullable = false),
            Field("lines", { arr(RecordLine.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> HistoryLines(a[0] as Long, a[1] as Long, a[2] as Boolean, a[3] as Long, a[4] as List<RecordLine>) },
            { v -> arrayOf(v.from, v.next, v.eof, v.end, v.lines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryRead(
    val rows: List<ReadRow>,
    val next: Long,
    val eof: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryRead> = obj(
            listOf(
            Field("rows", { arr(ReadRow.ADAPTER) }, optional = false, nullable = false),
            Field("next", { Lng }, optional = false, nullable = false),
            Field("eof", { Bool }, optional = false, nullable = false),
            ),
            { a -> HistoryRead(a[0] as List<ReadRow>, a[1] as Long, a[2] as Boolean) },
            { v -> arrayOf(v.rows, v.next, v.eof) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryRow(
    val agent: String,
    val sessionId: String,
    val projectPath: String,
    val projectName: String,
    val aiTitle: String?,
    val firstUserExcerpt: String,
    val title: String,
    val startedAt: Long,
    val updatedAt: Long,
    val jsonlPath: String,
    val messageCountApprox: Long,
    val isBg: Boolean,
    val starred: Boolean,
    val customTitle: String?,
    val hidden: Hidden?,
    val forkedFromSessionId: String? = null,
    val forkedFromMessageUuid: String? = null,
    val forkedFrom: ForkedFrom? = null,
    val projectDir: String,
    val group: String,
    val lastAccount: String? = null,
    val label: String,
    val untitled: Boolean,
    val face: SessionFace,
    val can: Can,
    val agentTag: String?,
    val context: Boolean? = null,
    val at: Long,
    val atText: String,
    val sectionText: String,
    val spanText: String,
    val origin: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryRow> = obj(
            listOf(
            Field("agent", { Str }, optional = false, nullable = false),
            Field("sessionId", { Str }, optional = false, nullable = false),
            Field("projectPath", { Str }, optional = false, nullable = false),
            Field("projectName", { Str }, optional = false, nullable = false),
            Field("aiTitle", { opt(Str) }, optional = false, nullable = true),
            Field("firstUserExcerpt", { Str }, optional = false, nullable = false),
            Field("title", { Str }, optional = false, nullable = false),
            Field("startedAt", { Lng }, optional = false, nullable = false),
            Field("updatedAt", { Lng }, optional = false, nullable = false),
            Field("jsonlPath", { Str }, optional = false, nullable = false),
            Field("messageCountApprox", { Lng }, optional = false, nullable = false),
            Field("isBg", { Bool }, optional = false, nullable = false),
            Field("starred", { Bool }, optional = false, nullable = false),
            Field("customTitle", { opt(Str) }, optional = false, nullable = true),
            Field("hidden", { opt(Hidden.ADAPTER) }, optional = false, nullable = true),
            Field("forkedFromSessionId", { opt(Str) }, optional = true, nullable = true),
            Field("forkedFromMessageUuid", { opt(Str) }, optional = true, nullable = true),
            Field("forkedFrom", { opt(ForkedFrom.ADAPTER) }, optional = true, nullable = true),
            Field("projectDir", { Str }, optional = false, nullable = false),
            Field("group", { Str }, optional = false, nullable = false),
            Field("lastAccount", { opt(Str) }, optional = true, nullable = true),
            Field("label", { Str }, optional = false, nullable = false),
            Field("untitled", { Bool }, optional = false, nullable = false),
            Field("face", { SessionFace.ADAPTER }, optional = false, nullable = false),
            Field("can", { Can.ADAPTER }, optional = false, nullable = false),
            Field("agentTag", { opt(Str) }, optional = false, nullable = true),
            Field("context", { opt(Bool) }, optional = true, nullable = true),
            Field("at", { Lng }, optional = false, nullable = false),
            Field("atText", { Str }, optional = false, nullable = false),
            Field("sectionText", { Str }, optional = false, nullable = false),
            Field("spanText", { Str }, optional = false, nullable = false),
            Field("origin", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> HistoryRow(a[0] as String, a[1] as String, a[2] as String, a[3] as String, a[4] as String?, a[5] as String, a[6] as String, a[7] as Long, a[8] as Long, a[9] as String, a[10] as Long, a[11] as Boolean, a[12] as Boolean, a[13] as String?, a[14] as Hidden?, a[15] as String?, a[16] as String?, a[17] as ForkedFrom?, a[18] as String, a[19] as String, a[20] as String?, a[21] as String, a[22] as Boolean, a[23] as SessionFace, a[24] as Can, a[25] as String?, a[26] as Boolean?, a[27] as Long, a[28] as String, a[29] as String, a[30] as String, a[31] as String?) },
            { v -> arrayOf(v.agent, v.sessionId, v.projectPath, v.projectName, v.aiTitle, v.firstUserExcerpt, v.title, v.startedAt, v.updatedAt, v.jsonlPath, v.messageCountApprox, v.isBg, v.starred, v.customTitle, v.hidden, v.forkedFromSessionId, v.forkedFromMessageUuid, v.forkedFrom, v.projectDir, v.group, v.lastAccount, v.label, v.untitled, v.face, v.can, v.agentTag, v.context, v.at, v.atText, v.sectionText, v.spanText, v.origin) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistorySearchReply(
    val sessions: List<SearchRow>,
    val unreadable: Long,
    val skipped: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistorySearchReply> = obj(
            listOf(
            Field("sessions", { arr(SearchRow.ADAPTER) }, optional = false, nullable = false),
            Field("unreadable", { Lng }, optional = false, nullable = false),
            Field("skipped", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> HistorySearchReply(a[0] as List<SearchRow>, a[1] as Long, a[2] as List<String>) },
            { v -> arrayOf(v.sessions, v.unreadable, v.skipped) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryTail(
    val how: TailHow,
    val from: Long,
    val rows: List<ReadRow>,
    val next: Long,
    val eof: Boolean,
    val held: Held,
    val total: Long,
    val tailFrom: Long,
    val splitAt: Long,
    val end: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryTail> = obj(
            listOf(
            Field("how", { TailHow.ADAPTER }, optional = false, nullable = false),
            Field("from", { Lng }, optional = false, nullable = false),
            Field("rows", { arr(ReadRow.ADAPTER) }, optional = false, nullable = false),
            Field("next", { Lng }, optional = false, nullable = false),
            Field("eof", { Bool }, optional = false, nullable = false),
            Field("held", { Held.ADAPTER }, optional = false, nullable = false),
            Field("total", { Lng }, optional = false, nullable = false),
            Field("tail_from", { Lng }, optional = false, nullable = false),
            Field("split_at", { Lng }, optional = false, nullable = false),
            Field("end", { Lng }, optional = false, nullable = false),
            ),
            { a -> HistoryTail(a[0] as TailHow, a[1] as Long, a[2] as List<ReadRow>, a[3] as Long, a[4] as Boolean, a[5] as Held, a[6] as Long, a[7] as Long, a[8] as Long, a[9] as Long) },
            { v -> arrayOf(v.how, v.from, v.rows, v.next, v.eof, v.held, v.total, v.tailFrom, v.splitAt, v.end) },
        )
    }
}

enum class ItemState(val wire: String) {
    DONE("done"),
    SKIPPED("skipped"),
    EXISTS("exists"),
    FAILED("failed"),
    CANCELLED("cancelled"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ItemState> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class Killed(
    val session: String,
    val killed: Boolean,
    val bus: BusNote,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Killed> = obj(
            listOf(
            Field("session", { Str }, optional = false, nullable = false),
            Field("killed", { Bool }, optional = false, nullable = false),
            Field("bus", { BusNote.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> Killed(a[0] as String, a[1] as Boolean, a[2] as BusNote) },
            { v -> arrayOf(v.session, v.killed, v.bus) },
        )
    }
}

enum class Landed(val wire: String) {
    STARTED("started"),
    IN_PLACE("in_place"),
    RUNNING("running"),
    IDLE("idle"),
    WINDOW("window"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<Landed> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class LaunchedAccount(
    val name: String,
    val configDir: String,
    val model: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LaunchedAccount> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("configDir", { Str }, optional = false, nullable = false),
            Field("model", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> LaunchedAccount(a[0] as String, a[1] as String, a[2] as String?) },
            { v -> arrayOf(v.name, v.configDir, v.model) },
        )
    }
}

sealed interface LineRecord {
    @Suppress("UNCHECKED_CAST")
    data class Said(
        val agent: String,
        val id: String,
        val at: String? = null,
        val timeText: String? = null,
        val atMs: Long? = null,
        val who: UserText,
        val blocks: List<Block>,
        val results: Map<String, StepResult>? = null,
        val cwd: String? = null,
    ) : LineRecord, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Reply(
        val agent: String,
        val id: String,
        val at: String? = null,
        val timeText: String? = null,
        val atMs: Long? = null,
        val blocks: List<Block>,
        val model: String? = null,
        val autoReply: Boolean,
        val cards: Map<String, ToolCard>? = null,
        val steps: Map<String, ToolStep>? = null,
        val runs: Map<String, ChildRunTag>? = null,
        val error: ReplyError? = null,
    ) : LineRecord, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Retry(
        val agent: String,
        val id: String,
        val at: String? = null,
        val timeText: String? = null,
        val atMs: Long? = null,
        val reason: ApiReason,
        val attempt: Long? = null,
        val max: Long? = null,
    ) : LineRecord, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Title(
        val agent: String,
        val id: String,
        val at: String? = null,
        val timeText: String? = null,
        val atMs: Long? = null,
        val text: String,
        val by: TitleBy,
    ) : LineRecord, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Unread(
        val agent: String,
        val id: String,
        val at: String? = null,
        val timeText: String? = null,
        val atMs: Long? = null,
        val why: UnreadWhy,
        val `type`: String?,
        val text: String,
        val tone: LineRecordUnreadTone,
        val excerpt: String,
        val count: Long,
    ) : LineRecord, Ordered {
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<LineRecord> =
            tagged<LineRecord>(
                "t",
                listOf(
                Variant("said", LineRecord.Said::class.java, obj(
                        listOf(
                        Field("agent", { Str }, optional = false, nullable = false),
                        Field("id", { Str }, optional = false, nullable = false),
                        Field("at", { opt(Str) }, optional = true, nullable = true),
                        Field("timeText", { opt(Str) }, optional = true, nullable = true),
                        Field("atMs", { opt(Lng) }, optional = true, nullable = true),
                        Field("t", { tag("said") }, optional = false, nullable = false),
                        Field("who", { UserText.ADAPTER }, optional = false, nullable = false),
                        Field("blocks", { arr(Block.ADAPTER) }, optional = false, nullable = false),
                        Field("results", { opt(dict(StepResult.ADAPTER)) }, optional = true, nullable = true),
                        Field("cwd", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> LineRecord.Said(a[0] as String, a[1] as String, a[2] as String?, a[3] as String?, a[4] as Long?, a[6] as UserText, a[7] as List<Block>, a[8] as Map<String, StepResult>?, a[9] as String?) },
                        { v -> arrayOf(v.agent, v.id, v.at, v.timeText, v.atMs, "said", v.who, v.blocks, v.results, v.cwd) },
                    )),
                Variant("reply", LineRecord.Reply::class.java, obj(
                        listOf(
                        Field("agent", { Str }, optional = false, nullable = false),
                        Field("id", { Str }, optional = false, nullable = false),
                        Field("at", { opt(Str) }, optional = true, nullable = true),
                        Field("timeText", { opt(Str) }, optional = true, nullable = true),
                        Field("atMs", { opt(Lng) }, optional = true, nullable = true),
                        Field("t", { tag("reply") }, optional = false, nullable = false),
                        Field("blocks", { arr(Block.ADAPTER) }, optional = false, nullable = false),
                        Field("model", { opt(Str) }, optional = true, nullable = true),
                        Field("autoReply", { Bool }, optional = false, nullable = false),
                        Field("cards", { opt(dict(ToolCard.ADAPTER)) }, optional = true, nullable = true),
                        Field("steps", { opt(dict(ToolStep.ADAPTER)) }, optional = true, nullable = true),
                        Field("runs", { opt(dict(ChildRunTag.ADAPTER)) }, optional = true, nullable = true),
                        Field("error", { opt(ReplyError.ADAPTER) }, optional = true, nullable = true),
                        ),
                        { a -> LineRecord.Reply(a[0] as String, a[1] as String, a[2] as String?, a[3] as String?, a[4] as Long?, a[6] as List<Block>, a[7] as String?, a[8] as Boolean, a[9] as Map<String, ToolCard>?, a[10] as Map<String, ToolStep>?, a[11] as Map<String, ChildRunTag>?, a[12] as ReplyError?) },
                        { v -> arrayOf(v.agent, v.id, v.at, v.timeText, v.atMs, "reply", v.blocks, v.model, v.autoReply, v.cards, v.steps, v.runs, v.error) },
                    )),
                Variant("retry", LineRecord.Retry::class.java, obj(
                        listOf(
                        Field("agent", { Str }, optional = false, nullable = false),
                        Field("id", { Str }, optional = false, nullable = false),
                        Field("at", { opt(Str) }, optional = true, nullable = true),
                        Field("timeText", { opt(Str) }, optional = true, nullable = true),
                        Field("atMs", { opt(Lng) }, optional = true, nullable = true),
                        Field("t", { tag("retry") }, optional = false, nullable = false),
                        Field("reason", { ApiReason.ADAPTER }, optional = false, nullable = false),
                        Field("attempt", { opt(Lng) }, optional = true, nullable = true),
                        Field("max", { opt(Lng) }, optional = true, nullable = true),
                        ),
                        { a -> LineRecord.Retry(a[0] as String, a[1] as String, a[2] as String?, a[3] as String?, a[4] as Long?, a[6] as ApiReason, a[7] as Long?, a[8] as Long?) },
                        { v -> arrayOf(v.agent, v.id, v.at, v.timeText, v.atMs, "retry", v.reason, v.attempt, v.max) },
                    )),
                Variant("title", LineRecord.Title::class.java, obj(
                        listOf(
                        Field("agent", { Str }, optional = false, nullable = false),
                        Field("id", { Str }, optional = false, nullable = false),
                        Field("at", { opt(Str) }, optional = true, nullable = true),
                        Field("timeText", { opt(Str) }, optional = true, nullable = true),
                        Field("atMs", { opt(Lng) }, optional = true, nullable = true),
                        Field("t", { tag("title") }, optional = false, nullable = false),
                        Field("text", { Str }, optional = false, nullable = false),
                        Field("by", { TitleBy.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> LineRecord.Title(a[0] as String, a[1] as String, a[2] as String?, a[3] as String?, a[4] as Long?, a[6] as String, a[7] as TitleBy) },
                        { v -> arrayOf(v.agent, v.id, v.at, v.timeText, v.atMs, "title", v.text, v.by) },
                    )),
                Variant("unread", LineRecord.Unread::class.java, obj(
                        listOf(
                        Field("agent", { Str }, optional = false, nullable = false),
                        Field("id", { Str }, optional = false, nullable = false),
                        Field("at", { opt(Str) }, optional = true, nullable = true),
                        Field("timeText", { opt(Str) }, optional = true, nullable = true),
                        Field("atMs", { opt(Lng) }, optional = true, nullable = true),
                        Field("t", { tag("unread") }, optional = false, nullable = false),
                        Field("why", { UnreadWhy.ADAPTER }, optional = false, nullable = false),
                        Field("type", { opt(Str) }, optional = false, nullable = true),
                        Field("text", { Str }, optional = false, nullable = false),
                        Field("tone", { LineRecordUnreadTone.ADAPTER }, optional = false, nullable = false),
                        Field("excerpt", { Str }, optional = false, nullable = false),
                        Field("count", { Lng }, optional = false, nullable = false),
                        ),
                        { a -> LineRecord.Unread(a[0] as String, a[1] as String, a[2] as String?, a[3] as String?, a[4] as Long?, a[6] as UnreadWhy, a[7] as String?, a[8] as String, a[9] as LineRecordUnreadTone, a[10] as String, a[11] as Long) },
                        { v -> arrayOf(v.agent, v.id, v.at, v.timeText, v.atMs, "unread", v.why, v.`type`, v.text, v.tone, v.excerpt, v.count) },
                    )),
                ),
            )
    }
}

enum class LineRecordUnreadTone(val wire: String) {
    WARN("warn"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<LineRecordUnreadTone> = enumOf(entries) { it.wire }
    }
}

enum class LinkTo(val wire: String) {
    DIR("dir"),
    FILE("file"),
    MISSING("missing"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<LinkTo> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ListMeta(
    val enabled: Boolean,
    val acctsDir: String,
    val manifestPath: String,
    val updatedAt: String?,
    val sharedStore: String?,
    val count: Long,
    val error: String?,
    val unsupported: String?,
    val nextDefault: String?,
    val effectiveDefault: String?,
    val home: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ListMeta> = obj(
            listOf(
            Field("enabled", { Bool }, optional = false, nullable = false),
            Field("acctsDir", { Str }, optional = false, nullable = false),
            Field("manifestPath", { Str }, optional = false, nullable = false),
            Field("updatedAt", { opt(Str) }, optional = false, nullable = true),
            Field("sharedStore", { opt(Str) }, optional = false, nullable = true),
            Field("count", { Lng }, optional = false, nullable = false),
            Field("error", { opt(Str) }, optional = false, nullable = true),
            Field("unsupported", { opt(Str) }, optional = false, nullable = true),
            Field("nextDefault", { opt(Str) }, optional = false, nullable = true),
            Field("effectiveDefault", { opt(Str) }, optional = false, nullable = true),
            Field("home", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ListMeta(a[0] as Boolean, a[1] as String, a[2] as String, a[3] as String?, a[4] as String?, a[5] as Long, a[6] as String?, a[7] as String?, a[8] as String?, a[9] as String?, a[10] as String?) },
            { v -> arrayOf(v.enabled, v.acctsDir, v.manifestPath, v.updatedAt, v.sharedStore, v.count, v.error, v.unsupported, v.nextDefault, v.effectiveDefault, v.home) },
        )
    }
}

sealed interface LoginStartReq {
    @Suppress("UNCHECKED_CAST")
    data class Account(
        val agent: String,
        val account: String,
    ) : LoginStartReq, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Mcp(
        val agent: String,
        val account: String?,
        val server: String,
        val projectDir: String?,
    ) : LoginStartReq, Ordered {
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<LoginStartReq> =
            tagged<LoginStartReq>(
                "kind",
                listOf(
                Variant("account", LoginStartReq.Account::class.java, obj(
                        listOf(
                        Field("kind", { tag("account") }, optional = false, nullable = false),
                        Field("agent", { Str }, optional = false, nullable = false),
                        Field("account", { Str }, optional = false, nullable = false),
                        ),
                        { a -> LoginStartReq.Account(a[1] as String, a[2] as String) },
                        { v -> arrayOf("account", v.agent, v.account) },
                    )),
                Variant("mcp", LoginStartReq.Mcp::class.java, obj(
                        listOf(
                        Field("kind", { tag("mcp") }, optional = false, nullable = false),
                        Field("agent", { Str }, optional = false, nullable = false),
                        Field("account", { opt(Str) }, optional = false, nullable = true),
                        Field("server", { Str }, optional = false, nullable = false),
                        Field("projectDir", { opt(Str) }, optional = false, nullable = true),
                        ),
                        { a -> LoginStartReq.Mcp(a[1] as String, a[2] as String?, a[3] as String, a[4] as String?) },
                        { v -> arrayOf("mcp", v.agent, v.account, v.server, v.projectDir) },
                    )),
                ),
            )
    }
}

enum class LoginState(val wire: String) {
    OK("ok"),
    NEEDS_LOGIN("needsLogin"),
    NEEDS_KEY("needsKey"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<LoginState> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class LsEntry(
    val path: WirePath,
    val name: WirePath,
    val kind: FileKind,
    val size: Long? = null,
    val sizeText: String? = null,
    val mtimeSecs: Long? = null,
    val mtimeText: String? = null,
    val mtimeFull: String? = null,
    val linkDir: Boolean? = null,
    val linkTo: LinkTo? = null,
    val `class`: FileClass,
    val typeText: String,
    val gates: Gates? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LsEntry> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("name", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("kind", { FileKind.ADAPTER }, optional = false, nullable = false),
            Field("size", { opt(Lng) }, optional = true, nullable = true),
            Field("size_text", { opt(Str) }, optional = true, nullable = true),
            Field("mtime_secs", { opt(Lng) }, optional = true, nullable = true),
            Field("mtime_text", { opt(Str) }, optional = true, nullable = true),
            Field("mtime_full", { opt(Str) }, optional = true, nullable = true),
            Field("link_dir", { opt(Bool) }, optional = true, nullable = true),
            Field("link_to", { opt(LinkTo.ADAPTER) }, optional = true, nullable = true),
            Field("class", { FileClass.ADAPTER }, optional = false, nullable = false),
            Field("type_text", { Str }, optional = false, nullable = false),
            Field("gates", { opt(Gates.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> LsEntry(a[0] as WirePath, a[1] as WirePath, a[2] as FileKind, a[3] as Long?, a[4] as String?, a[5] as Long?, a[6] as String?, a[7] as String?, a[8] as Boolean?, a[9] as LinkTo?, a[10] as FileClass, a[11] as String, a[12] as Gates?) },
            { v -> arrayOf(v.path, v.name, v.kind, v.size, v.sizeText, v.mtimeSecs, v.mtimeText, v.mtimeFull, v.linkDir, v.linkTo, v.`class`, v.typeText, v.gates) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LsHead(
    val up: WirePath?,
    val crumbs: List<Crumb>,
    val truncated: Boolean,
    val total: Long,
    val unreadable: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LsHead> = obj(
            listOf(
            Field("up", { opt(WirePath.ADAPTER) }, optional = false, nullable = true),
            Field("crumbs", { arr(Crumb.ADAPTER) }, optional = false, nullable = false),
            Field("truncated", { Bool }, optional = false, nullable = false),
            Field("total", { Lng }, optional = false, nullable = false),
            Field("unreadable", { Lng }, optional = false, nullable = false),
            ),
            { a -> LsHead(a[0] as WirePath?, a[1] as List<Crumb>, a[2] as Boolean, a[3] as Long, a[4] as Long) },
            { v -> arrayOf(v.up, v.crumbs, v.truncated, v.total, v.unreadable) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ManualFix(
    val command: String,
    val args: LoginStartReq,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ManualFix> = obj(
            listOf(
            Field("command", { Str }, optional = false, nullable = false),
            Field("args", { LoginStartReq.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> ManualFix(a[0] as String, a[1] as LoginStartReq) },
            { v -> arrayOf(v.command, v.args) },
        )
    }
}

sealed interface ManualItem {
    @Suppress("UNCHECKED_CAST")
    data class AccountLogin(
        val agent: String,
        val account: String,
        val said: String,
        val fix: ManualFix,
    ) : ManualItem, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class McpLogin(
        val account: String?,
        val server: String,
        val scope: String,
        val said: String,
        val fix: ManualFix?,
        val why: String? = null,
    ) : ManualItem, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class SecretFile(
        val said: String,
        val fix: ManualFix?,
        val why: String? = null,
    ) : ManualItem, Ordered {
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<ManualItem> =
            tagged<ManualItem>(
                "kind",
                listOf(
                Variant("accountLogin", ManualItem.AccountLogin::class.java, obj(
                        listOf(
                        Field("kind", { tag("accountLogin") }, optional = false, nullable = false),
                        Field("agent", { Str }, optional = false, nullable = false),
                        Field("account", { Str }, optional = false, nullable = false),
                        Field("said", { Str }, optional = false, nullable = false),
                        Field("fix", { ManualFix.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> ManualItem.AccountLogin(a[1] as String, a[2] as String, a[3] as String, a[4] as ManualFix) },
                        { v -> arrayOf("accountLogin", v.agent, v.account, v.said, v.fix) },
                    )),
                Variant("mcpLogin", ManualItem.McpLogin::class.java, obj(
                        listOf(
                        Field("kind", { tag("mcpLogin") }, optional = false, nullable = false),
                        Field("account", { opt(Str) }, optional = false, nullable = true),
                        Field("server", { Str }, optional = false, nullable = false),
                        Field("scope", { Str }, optional = false, nullable = false),
                        Field("said", { Str }, optional = false, nullable = false),
                        Field("fix", { opt(ManualFix.ADAPTER) }, optional = false, nullable = true),
                        Field("why", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> ManualItem.McpLogin(a[1] as String?, a[2] as String, a[3] as String, a[4] as String, a[5] as ManualFix?, a[6] as String?) },
                        { v -> arrayOf("mcpLogin", v.account, v.server, v.scope, v.said, v.fix, v.why) },
                    )),
                Variant("secretFile", ManualItem.SecretFile::class.java, obj(
                        listOf(
                        Field("kind", { tag("secretFile") }, optional = false, nullable = false),
                        Field("said", { Str }, optional = false, nullable = false),
                        Field("fix", { opt(ManualFix.ADAPTER) }, optional = false, nullable = true),
                        Field("why", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> ManualItem.SecretFile(a[1] as String, a[2] as ManualFix?, a[3] as String?) },
                        { v -> arrayOf("secretFile", v.said, v.fix, v.why) },
                    )),
                ),
            )
    }
}

enum class ModeSwitchResult(val wire: String) {
    DONE("done"),
    UNSURE("unsure"),
    REFUSED("refused"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ModeSwitchResult> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ModeSwitched(
    val result: ModeSwitchResult,
    val mode: String?,
    val modeText: String?,
    val why: Refused? = null,
    val said: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ModeSwitched> = obj(
            listOf(
            Field("result", { ModeSwitchResult.ADAPTER }, optional = false, nullable = false),
            Field("mode", { opt(Str) }, optional = false, nullable = true),
            Field("modeText", { opt(Str) }, optional = false, nullable = true),
            Field("why", { opt(Refused.ADAPTER) }, optional = true, nullable = true),
            Field("said", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> ModeSwitched(a[0] as ModeSwitchResult, a[1] as String?, a[2] as String?, a[3] as Refused?, a[4] as String?) },
            { v -> arrayOf(v.result, v.mode, v.modeText, v.why, v.said) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ModelSwitched(
    val result: ModeSwitchResult,
    val model: String?,
    val effort: String? = null,
    val why: Refused? = null,
    val said: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ModelSwitched> = obj(
            listOf(
            Field("result", { ModeSwitchResult.ADAPTER }, optional = false, nullable = false),
            Field("model", { opt(Str) }, optional = false, nullable = true),
            Field("effort", { opt(Str) }, optional = true, nullable = true),
            Field("why", { opt(Refused.ADAPTER) }, optional = true, nullable = true),
            Field("said", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> ModelSwitched(a[0] as ModeSwitchResult, a[1] as String?, a[2] as String?, a[3] as Refused?, a[4] as String?) },
            { v -> arrayOf(v.result, v.model, v.effort, v.why, v.said) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Needs(
    val kind: NeedsKind,
    val tool: String?,
    val call: String?,
    val what: String?,
    val sinceMs: Long?,
    val text: String,
    val head: String,
    val headCode: String?,
    val tone: Tone,
    val rank: Long,
    val waitedMs: Long?,
    val waitedText: String?,
    val ask: NeedsAsk?,
    val manualText: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Needs> = obj(
            listOf(
            Field("kind", { NeedsKind.ADAPTER }, optional = false, nullable = false),
            Field("tool", { opt(Str) }, optional = false, nullable = true),
            Field("call", { opt(Str) }, optional = false, nullable = true),
            Field("what", { opt(Str) }, optional = false, nullable = true),
            Field("sinceMs", { opt(Lng) }, optional = false, nullable = true),
            Field("text", { Str }, optional = false, nullable = false),
            Field("head", { Str }, optional = false, nullable = false),
            Field("headCode", { opt(Str) }, optional = false, nullable = true),
            Field("tone", { Tone.ADAPTER }, optional = false, nullable = false),
            Field("rank", { Lng }, optional = false, nullable = false),
            Field("waitedMs", { opt(Lng) }, optional = false, nullable = true),
            Field("waitedText", { opt(Str) }, optional = false, nullable = true),
            Field("ask", { opt(NeedsAsk.ADAPTER) }, optional = false, nullable = true),
            Field("manualText", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> Needs(a[0] as NeedsKind, a[1] as String?, a[2] as String?, a[3] as String?, a[4] as Long?, a[5] as String, a[6] as String, a[7] as String?, a[8] as Tone, a[9] as Long, a[10] as Long?, a[11] as String?, a[12] as NeedsAsk?, a[13] as String?) },
            { v -> arrayOf(v.kind, v.tool, v.call, v.what, v.sinceMs, v.text, v.head, v.headCode, v.tone, v.rank, v.waitedMs, v.waitedText, v.ask, v.manualText) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class NeedsAsk(
    val id: String,
    val kind: AskKind,
    val tool: String,
    val run: NeedsAskRun?,
    val what: String?,
    val detail: String,
    val choices: List<NeedsChoice>,
    val questions: List<NeedsQuestion>,
    val sinceMs: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<NeedsAsk> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("kind", { AskKind.ADAPTER }, optional = false, nullable = false),
            Field("tool", { Str }, optional = false, nullable = false),
            Field("run", { opt(NeedsAskRun.ADAPTER) }, optional = false, nullable = true),
            Field("what", { opt(Str) }, optional = false, nullable = true),
            Field("detail", { Str }, optional = false, nullable = false),
            Field("choices", { arr(NeedsChoice.ADAPTER) }, optional = false, nullable = false),
            Field("questions", { arr(NeedsQuestion.ADAPTER) }, optional = false, nullable = false),
            Field("sinceMs", { Lng }, optional = false, nullable = false),
            ),
            { a -> NeedsAsk(a[0] as String, a[1] as AskKind, a[2] as String, a[3] as NeedsAskRun?, a[4] as String?, a[5] as String, a[6] as List<NeedsChoice>, a[7] as List<NeedsQuestion>, a[8] as Long) },
            { v -> arrayOf(v.id, v.kind, v.tool, v.run, v.what, v.detail, v.choices, v.questions, v.sinceMs) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class NeedsAskRun(
    val id: String,
    val kind: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<NeedsAskRun> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("kind", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> NeedsAskRun(a[0] as String, a[1] as String?) },
            { v -> arrayOf(v.id, v.kind) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class NeedsChoice(
    val id: String,
    val text: String,
    val tone: Tone,
    val note: NeedsNote,
    val row: NeedsChoiceRow,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<NeedsChoice> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("tone", { Tone.ADAPTER }, optional = false, nullable = false),
            Field("note", { NeedsNote.ADAPTER }, optional = false, nullable = false),
            Field("row", { NeedsChoiceRow.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> NeedsChoice(a[0] as String, a[1] as String, a[2] as Tone, a[3] as NeedsNote, a[4] as NeedsChoiceRow) },
            { v -> arrayOf(v.id, v.text, v.tone, v.note, v.row) },
        )
    }
}

enum class NeedsChoiceRow(val wire: String) {
    MAIN("main"),
    HOLD("hold"),
    RELEASE("release"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<NeedsChoiceRow> = enumOf(entries) { it.wire }
    }
}

sealed interface NeedsItem {
    @Suppress("UNCHECKED_CAST")
    data class Session(
        val rank: Long,
        val session: NeedsRow,
    ) : NeedsItem, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Plan(
        val rank: Long,
        val workspace: String,
        val slice: String,
        val k: Long,
    ) : NeedsItem, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Manual(
        val rank: Long,
        val manual: ManualItem,
    ) : NeedsItem, Ordered {
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<NeedsItem> =
            tagged<NeedsItem>(
                "kind",
                listOf(
                Variant("session", NeedsItem.Session::class.java, obj(
                        listOf(
                        Field("kind", { tag("session") }, optional = false, nullable = false),
                        Field("rank", { Lng }, optional = false, nullable = false),
                        Field("session", { NeedsRow.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> NeedsItem.Session(a[1] as Long, a[2] as NeedsRow) },
                        { v -> arrayOf("session", v.rank, v.session) },
                    )),
                Variant("plan", NeedsItem.Plan::class.java, obj(
                        listOf(
                        Field("kind", { tag("plan") }, optional = false, nullable = false),
                        Field("rank", { Lng }, optional = false, nullable = false),
                        Field("workspace", { Str }, optional = false, nullable = false),
                        Field("slice", { Str }, optional = false, nullable = false),
                        Field("k", { Lng }, optional = false, nullable = false),
                        ),
                        { a -> NeedsItem.Plan(a[1] as Long, a[2] as String, a[3] as String, a[4] as Long) },
                        { v -> arrayOf("plan", v.rank, v.workspace, v.slice, v.k) },
                    )),
                Variant("manual", NeedsItem.Manual::class.java, obj(
                        listOf(
                        Field("kind", { tag("manual") }, optional = false, nullable = false),
                        Field("rank", { Lng }, optional = false, nullable = false),
                        Field("manual", { ManualItem.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> NeedsItem.Manual(a[1] as Long, a[2] as ManualItem) },
                        { v -> arrayOf("manual", v.rank, v.manual) },
                    )),
                ),
            )
    }
}

enum class NeedsKind(val wire: String) {
    APPROVE("approve"),
    ANSWER("answer"),
    PLAN("plan"),
    NETWORK("network"),
    WORKER("worker"),
    GOAL("goal"),
    CHOOSE("choose"),
    UNKNOWN("unknown"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<NeedsKind> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class NeedsList(
    val items: List<NeedsItem>,
    val answered: List<AnsweredRow>,
    val manualUnreadable: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<NeedsList> = obj(
            listOf(
            Field("items", { arr(NeedsItem.ADAPTER) }, optional = false, nullable = false),
            Field("answered", { arr(AnsweredRow.ADAPTER) }, optional = false, nullable = false),
            Field("manualUnreadable", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> NeedsList(a[0] as List<NeedsItem>, a[1] as List<AnsweredRow>, a[2] as String?) },
            { v -> arrayOf(v.items, v.answered, v.manualUnreadable) },
        )
    }
}

enum class NeedsNote(val wire: String) {
    NONE("none"),
    OPTIONAL("optional"),
    REQUIRED("required"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<NeedsNote> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class NeedsQuestion(
    val id: String,
    val text: String,
    val header: String?,
    val multi: Boolean,
    val options: List<NeedsQuestionOption>,
    val other: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<NeedsQuestion> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("header", { opt(Str) }, optional = false, nullable = true),
            Field("multi", { Bool }, optional = false, nullable = false),
            Field("options", { arr(NeedsQuestionOption.ADAPTER) }, optional = false, nullable = false),
            Field("other", { Bool }, optional = false, nullable = false),
            ),
            { a -> NeedsQuestion(a[0] as String, a[1] as String, a[2] as String?, a[3] as Boolean, a[4] as List<NeedsQuestionOption>, a[5] as Boolean) },
            { v -> arrayOf(v.id, v.text, v.header, v.multi, v.options, v.other) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class NeedsQuestionOption(
    val id: String,
    val text: String,
    val detail: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<NeedsQuestionOption> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("detail", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> NeedsQuestionOption(a[0] as String, a[1] as String, a[2] as String?) },
            { v -> arrayOf(v.id, v.text, v.detail) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class NeedsRow(
    val sid: String,
    val label: String,
    val face: SessionFace,
    val needs: Needs,
    val hidden: Hidden?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<NeedsRow> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("label", { Str }, optional = false, nullable = false),
            Field("face", { SessionFace.ADAPTER }, optional = false, nullable = false),
            Field("needs", { Needs.ADAPTER }, optional = false, nullable = false),
            Field("hidden", { opt(Hidden.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> NeedsRow(a[0] as String, a[1] as String, a[2] as SessionFace, a[3] as Needs, a[4] as Hidden?) },
            { v -> arrayOf(v.sid, v.label, v.face, v.needs, v.hidden) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class OpenFacts(
    val recent: List<RecentDir>,
    val tmux: Boolean,
    val run: Run,
    val agents: List<AgentFacts>,
    val fork: ForkFacts?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<OpenFacts> = obj(
            listOf(
            Field("recent", { arr(RecentDir.ADAPTER) }, optional = false, nullable = false),
            Field("tmux", { Bool }, optional = false, nullable = false),
            Field("run", { Run.ADAPTER }, optional = false, nullable = false),
            Field("agents", { arr(AgentFacts.ADAPTER) }, optional = false, nullable = false),
            Field("fork", { opt(ForkFacts.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> OpenFacts(a[0] as List<RecentDir>, a[1] as Boolean, a[2] as Run, a[3] as List<AgentFacts>, a[4] as ForkFacts?) },
            { v -> arrayOf(v.recent, v.tmux, v.run, v.agents, v.fork) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Opened(
    val landed: Landed,
    val session: String?,
    val line: String?,
    val sid: String?,
    val agent: String?,
    val cwd: String?,
    val account: LaunchedAccount?,
    val note: String?,
    val prompt: FirstSay?,
    val arrival: ArriveAsk?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Opened> = obj(
            listOf(
            Field("landed", { Landed.ADAPTER }, optional = false, nullable = false),
            Field("session", { opt(Str) }, optional = false, nullable = true),
            Field("line", { opt(Str) }, optional = false, nullable = true),
            Field("sid", { opt(Str) }, optional = false, nullable = true),
            Field("agent", { opt(Str) }, optional = false, nullable = true),
            Field("cwd", { opt(Str) }, optional = false, nullable = true),
            Field("account", { opt(LaunchedAccount.ADAPTER) }, optional = false, nullable = true),
            Field("note", { opt(Str) }, optional = false, nullable = true),
            Field("prompt", { opt(FirstSay.ADAPTER) }, optional = false, nullable = true),
            Field("arrival", { opt(ArriveAsk.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> Opened(a[0] as Landed, a[1] as String?, a[2] as String?, a[3] as String?, a[4] as String?, a[5] as String?, a[6] as LaunchedAccount?, a[7] as String?, a[8] as FirstSay?, a[9] as ArriveAsk?) },
            { v -> arrayOf(v.landed, v.session, v.line, v.sid, v.agent, v.cwd, v.account, v.note, v.prompt, v.arrival) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Pasted(
    val id: String? = null,
    val start: Long,
    val end: Long,
    val bodyStart: Long,
    val bodyEnd: Long,
    val lines: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Pasted> = obj(
            listOf(
            Field("id", { opt(Str) }, optional = true, nullable = true),
            Field("start", { Lng }, optional = false, nullable = false),
            Field("end", { Lng }, optional = false, nullable = false),
            Field("bodyStart", { Lng }, optional = false, nullable = false),
            Field("bodyEnd", { Lng }, optional = false, nullable = false),
            Field("lines", { Lng }, optional = false, nullable = false),
            ),
            { a -> Pasted(a[0] as String?, a[1] as Long, a[2] as Long, a[3] as Long, a[4] as Long, a[5] as Long) },
            { v -> arrayOf(v.id, v.start, v.end, v.bodyStart, v.bodyEnd, v.lines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PatchHunk(
    val oldStart: Long,
    val oldLines: Long,
    val newStart: Long,
    val newLines: Long,
    val lines: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PatchHunk> = obj(
            listOf(
            Field("oldStart", { Lng }, optional = false, nullable = false),
            Field("oldLines", { Lng }, optional = false, nullable = false),
            Field("newStart", { Lng }, optional = false, nullable = false),
            Field("newLines", { Lng }, optional = false, nullable = false),
            Field("lines", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> PatchHunk(a[0] as Long, a[1] as Long, a[2] as Long, a[3] as Long, a[4] as List<String>) },
            { v -> arrayOf(v.oldStart, v.oldLines, v.newStart, v.newLines, v.lines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanTitle(
    val waiting: String,
    val approved: String,
    val kept: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanTitle> = obj(
            listOf(
            Field("waiting", { Str }, optional = false, nullable = false),
            Field("approved", { Str }, optional = false, nullable = false),
            Field("kept", { Str }, optional = false, nullable = false),
            ),
            { a -> PlanTitle(a[0] as String, a[1] as String, a[2] as String) },
            { v -> arrayOf(v.waiting, v.approved, v.kept) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Preview(
    val screen: String,
    val cols: Long,
    val rows: Long,
    val cursor: Cursor,
    val lines: List<ScreenLine>,
    val scrollbackLines: Long,
    val capped: Boolean,
    val capturedAt: Long,
    val capturedAtText: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Preview> = obj(
            listOf(
            Field("screen", { Str }, optional = false, nullable = false),
            Field("cols", { Lng }, optional = false, nullable = false),
            Field("rows", { Lng }, optional = false, nullable = false),
            Field("cursor", { Cursor.ADAPTER }, optional = false, nullable = false),
            Field("lines", { arr(ScreenLine.ADAPTER) }, optional = false, nullable = false),
            Field("scrollback_lines", { Lng }, optional = false, nullable = false),
            Field("capped", { Bool }, optional = false, nullable = false),
            Field("captured_at", { Lng }, optional = false, nullable = false),
            Field("captured_at_text", { Str }, optional = false, nullable = false),
            ),
            { a -> Preview(a[0] as String, a[1] as Long, a[2] as Long, a[3] as Cursor, a[4] as List<ScreenLine>, a[5] as Long, a[6] as Boolean, a[7] as Long, a[8] as String) },
            { v -> arrayOf(v.screen, v.cols, v.rows, v.cursor, v.lines, v.scrollbackLines, v.capped, v.capturedAt, v.capturedAtText) },
        )
    }
}

sealed interface PreviewGate {
    data class Text(
        val value: TextPreview,
    ) : PreviewGate
    data class Image(
        val value: Long,
    ) : PreviewGate
    data class No(
        val value: String,
    ) : PreviewGate

    companion object {
        val ADAPTER: JsonAdapter<PreviewGate> =
            external<PreviewGate>(
                listOf(
                Ext("text", PreviewGate.Text::class.java, { TextPreview.ADAPTER }, { PreviewGate.Text(it as TextPreview) }, { (it as PreviewGate.Text).value }),
                Ext("image", PreviewGate.Image::class.java, { Lng }, { PreviewGate.Image(it as Long) }, { (it as PreviewGate.Image).value }),
                Ext("no", PreviewGate.No::class.java, { Str }, { PreviewGate.No(it as String) }, { (it as PreviewGate.No).value }),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaAccount(
    val rows: List<List<QuotaCell>>,
    val fiveHour: String?,
    val warm: QuotaWarm,
    val agent: String,
    val account: String,
    val seenAt: Long,
    val seenAtText: String,
    val seenAtRelText: String? = null,
    val reading: QuotaReadingShown,
    val windowsSeen: Map<String, QuotaWindowSeen>? = null,
    val kind: QuotaKind,
    val state: QuotaState,
    val stale: Boolean,
    val limiting: String? = null,
    val slots: List<SlotShow>,
    val login: LoginState,
    val subId: String? = null,
    val windows: List<WindowShow>? = null,
    val usage: Usage,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaAccount> = obj(
            listOf(
            Field("rows", { arr(arr(QuotaCell.ADAPTER)) }, optional = false, nullable = false),
            Field("fiveHour", { opt(Str) }, optional = false, nullable = true),
            Field("warm", { QuotaWarm.ADAPTER }, optional = false, nullable = false),
            Field("agent", { Str }, optional = false, nullable = false),
            Field("account", { Str }, optional = false, nullable = false),
            Field("seenAt", { Lng }, optional = false, nullable = false),
            Field("seenAtText", { Str }, optional = false, nullable = false),
            Field("seenAtRelText", { opt(Str) }, optional = true, nullable = true),
            Field("reading", { QuotaReadingShown.ADAPTER }, optional = false, nullable = false),
            Field("windowsSeen", { opt(dict(QuotaWindowSeen.ADAPTER)) }, optional = true, nullable = true),
            Field("kind", { QuotaKind.ADAPTER }, optional = false, nullable = false),
            Field("state", { QuotaState.ADAPTER }, optional = false, nullable = false),
            Field("stale", { Bool }, optional = false, nullable = false),
            Field("limiting", { opt(Str) }, optional = true, nullable = true),
            Field("slots", { arr(SlotShow.ADAPTER) }, optional = false, nullable = false),
            Field("login", { LoginState.ADAPTER }, optional = false, nullable = false),
            Field("subId", { opt(Str) }, optional = true, nullable = true),
            Field("windows", { opt(arr(WindowShow.ADAPTER)) }, optional = true, nullable = true),
            Field("usage", { Usage.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> QuotaAccount(a[0] as List<List<QuotaCell>>, a[1] as String?, a[2] as QuotaWarm, a[3] as String, a[4] as String, a[5] as Long, a[6] as String, a[7] as String?, a[8] as QuotaReadingShown, a[9] as Map<String, QuotaWindowSeen>?, a[10] as QuotaKind, a[11] as QuotaState, a[12] as Boolean, a[13] as String?, a[14] as List<SlotShow>, a[15] as LoginState, a[16] as String?, a[17] as List<WindowShow>?, a[18] as Usage) },
            { v -> arrayOf(v.rows, v.fiveHour, v.warm, v.agent, v.account, v.seenAt, v.seenAtText, v.seenAtRelText, v.reading, v.windowsSeen, v.kind, v.state, v.stale, v.limiting, v.slots, v.login, v.subId, v.windows, v.usage) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaCandidate(
    val agent: String,
    val account: String,
    val kind: QuotaKind,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaCandidate> = obj(
            listOf(
            Field("agent", { Str }, optional = false, nullable = false),
            Field("account", { Str }, optional = false, nullable = false),
            Field("kind", { QuotaKind.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> QuotaCandidate(a[0] as String, a[1] as String, a[2] as QuotaKind) },
            { v -> arrayOf(v.agent, v.account, v.kind) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaCell(
    val text: String,
    val tone: Tone,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaCell> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            Field("tone", { Tone.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> QuotaCell(a[0] as String, a[1] as Tone) },
            { v -> arrayOf(v.text, v.tone) },
        )
    }
}

enum class QuotaKind(val wire: String) {
    SUB("sub"),
    API("api"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<QuotaKind> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaNames(
    val accounts: Map<String, String>,
    val slots: Map<String, String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaNames> = obj(
            listOf(
            Field("accounts", { dict(Str) }, optional = false, nullable = false),
            Field("slots", { dict(Str) }, optional = false, nullable = false),
            ),
            { a -> QuotaNames(a[0] as Map<String, String>, a[1] as Map<String, String>) },
            { v -> arrayOf(v.accounts, v.slots) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaOverageShown(
    val status: QuotaStatus? = null,
    val disabled: String? = null,
    val inUse: Boolean,
    val resetsAt: Long? = null,
    val resetsAtText: String? = null,
    val resetsAtRelText: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaOverageShown> = obj(
            listOf(
            Field("status", { opt(QuotaStatus.ADAPTER) }, optional = true, nullable = true),
            Field("disabled", { opt(Str) }, optional = true, nullable = true),
            Field("inUse", { Bool }, optional = false, nullable = false),
            Field("resetsAt", { opt(Lng) }, optional = true, nullable = true),
            Field("resetsAtText", { opt(Str) }, optional = true, nullable = true),
            Field("resetsAtRelText", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> QuotaOverageShown(a[0] as QuotaStatus?, a[1] as String?, a[2] as Boolean, a[3] as Long?, a[4] as String?, a[5] as String?) },
            { v -> arrayOf(v.status, v.disabled, v.inUse, v.resetsAt, v.resetsAtText, v.resetsAtRelText) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaRead(
    val state: QuotaReadState,
    val reason: String?,
    val detail: String?,
    val path: String?,
    val now: Long,
    val accounts: List<QuotaAccount>,
    val unseen: List<QuotaUnseen>,
    val candidates: List<QuotaCandidate>,
    val text: String?,
    val fiveHour: String?,
    val names: QuotaNames,
    val verdict: QuotaVerdict?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaRead> = obj(
            listOf(
            Field("state", { QuotaReadState.ADAPTER }, optional = false, nullable = false),
            Field("reason", { opt(Str) }, optional = false, nullable = true),
            Field("detail", { opt(Str) }, optional = false, nullable = true),
            Field("path", { opt(Str) }, optional = false, nullable = true),
            Field("now", { Lng }, optional = false, nullable = false),
            Field("accounts", { arr(QuotaAccount.ADAPTER) }, optional = false, nullable = false),
            Field("unseen", { arr(QuotaUnseen.ADAPTER) }, optional = false, nullable = false),
            Field("candidates", { arr(QuotaCandidate.ADAPTER) }, optional = false, nullable = false),
            Field("text", { opt(Str) }, optional = false, nullable = true),
            Field("fiveHour", { opt(Str) }, optional = false, nullable = true),
            Field("names", { QuotaNames.ADAPTER }, optional = false, nullable = false),
            Field("verdict", { opt(QuotaVerdict.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> QuotaRead(a[0] as QuotaReadState, a[1] as String?, a[2] as String?, a[3] as String?, a[4] as Long, a[5] as List<QuotaAccount>, a[6] as List<QuotaUnseen>, a[7] as List<QuotaCandidate>, a[8] as String?, a[9] as String?, a[10] as QuotaNames, a[11] as QuotaVerdict?) },
            { v -> arrayOf(v.state, v.reason, v.detail, v.path, v.now, v.accounts, v.unseen, v.candidates, v.text, v.fiveHour, v.names, v.verdict) },
        )
    }
}

enum class QuotaReadState(val wire: String) {
    PRESENT("present"),
    ABSENT("absent"),
    UNREADABLE("unreadable"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<QuotaReadState> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaReadingShown(
    val status: QuotaStatus? = null,
    val refused: Boolean,
    val limiting: String? = null,
    val windows: List<QuotaWindowRead>,
    val overage: QuotaOverageShown? = null,
    val resetsAt: Long? = null,
    val resetsAtText: String? = null,
    val resetsAtRelText: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaReadingShown> = obj(
            listOf(
            Field("status", { opt(QuotaStatus.ADAPTER) }, optional = true, nullable = true),
            Field("refused", { Bool }, optional = false, nullable = false),
            Field("limiting", { opt(Str) }, optional = true, nullable = true),
            Field("windows", { arr(QuotaWindowRead.ADAPTER) }, optional = false, nullable = false),
            Field("overage", { opt(QuotaOverageShown.ADAPTER) }, optional = true, nullable = true),
            Field("resetsAt", { opt(Lng) }, optional = true, nullable = true),
            Field("resetsAtText", { opt(Str) }, optional = true, nullable = true),
            Field("resetsAtRelText", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> QuotaReadingShown(a[0] as QuotaStatus?, a[1] as Boolean, a[2] as String?, a[3] as List<QuotaWindowRead>, a[4] as QuotaOverageShown?, a[5] as Long?, a[6] as String?, a[7] as String?) },
            { v -> arrayOf(v.status, v.refused, v.limiting, v.windows, v.overage, v.resetsAt, v.resetsAtText, v.resetsAtRelText) },
        )
    }
}

enum class QuotaSource(val wire: String) {
    HEADERS("headers"),
    USAGE("usage"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<QuotaSource> = enumOf(entries) { it.wire }
    }
}

enum class QuotaState(val wire: String) {
    OK("ok"),
    NEAR("near"),
    REFUSED("refused"),
    OVERAGE_IN_USE("overageInUse"),
    UNSEEN("unseen"),
    RESET_SINCE_SEEN("resetSinceSeen"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<QuotaState> = enumOf(entries) { it.wire }
    }
}

enum class QuotaStatus(val wire: String) {
    ALLOWED("allowed"),
    WARNING("warning"),
    REJECTED("rejected"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<QuotaStatus> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaUnseen(
    val rows: List<List<QuotaCell>>,
    val fiveHour: String?,
    val warm: QuotaWarm,
    val agent: String,
    val account: String,
    val kind: QuotaKind,
    val login: LoginState,
    val subId: String? = null,
    val usage: QuotaUnseenUsage,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaUnseen> = obj(
            listOf(
            Field("rows", { arr(arr(QuotaCell.ADAPTER)) }, optional = false, nullable = false),
            Field("fiveHour", { opt(Str) }, optional = false, nullable = true),
            Field("warm", { QuotaWarm.ADAPTER }, optional = false, nullable = false),
            Field("agent", { Str }, optional = false, nullable = false),
            Field("account", { Str }, optional = false, nullable = false),
            Field("kind", { QuotaKind.ADAPTER }, optional = false, nullable = false),
            Field("login", { LoginState.ADAPTER }, optional = false, nullable = false),
            Field("subId", { opt(Str) }, optional = true, nullable = true),
            Field("usage", { QuotaUnseenUsage.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> QuotaUnseen(a[0] as List<List<QuotaCell>>, a[1] as String?, a[2] as QuotaWarm, a[3] as String, a[4] as String, a[5] as QuotaKind, a[6] as LoginState, a[7] as String?, a[8] as QuotaUnseenUsage) },
            { v -> arrayOf(v.rows, v.fiveHour, v.warm, v.agent, v.account, v.kind, v.login, v.subId, v.usage) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaUnseenUsage(
    val slot: String? = null,
    val window: String? = null,
    val value: String,
    val text: String,
    val tone: QuotaUnseenUsageTone,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaUnseenUsage> = obj(
            listOf(
            Field("slot", { opt(Str) }, optional = true, nullable = true),
            Field("window", { opt(Str) }, optional = true, nullable = true),
            Field("value", { Str }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("tone", { QuotaUnseenUsageTone.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> QuotaUnseenUsage(a[0] as String?, a[1] as String?, a[2] as String, a[3] as String, a[4] as QuotaUnseenUsageTone) },
            { v -> arrayOf(v.slot, v.window, v.value, v.text, v.tone) },
        )
    }
}

enum class QuotaUnseenUsageTone(val wire: String) {
    PLAIN("plain"),
    FAIL("fail"),
    WARN("warn"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<QuotaUnseenUsageTone> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaVerdict(
    val text: String,
    val sub: String?,
    val tone: QuotaVerdictTone,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaVerdict> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            Field("sub", { opt(Str) }, optional = false, nullable = true),
            Field("tone", { QuotaVerdictTone.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> QuotaVerdict(a[0] as String, a[1] as String?, a[2] as QuotaVerdictTone) },
            { v -> arrayOf(v.text, v.sub, v.tone) },
        )
    }
}

enum class QuotaVerdictTone(val wire: String) {
    PLAIN("plain"),
    FAIL("fail"),
    WARN("warn"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<QuotaVerdictTone> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaWarm(
    val act: QuotaWarmAct,
    val at: Long? = null,
    val atText: String? = null,
    val atRelText: String? = null,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaWarm> = obj(
            listOf(
            Field("act", { QuotaWarmAct.ADAPTER }, optional = false, nullable = false),
            Field("at", { opt(Lng) }, optional = true, nullable = true),
            Field("atText", { opt(Str) }, optional = true, nullable = true),
            Field("atRelText", { opt(Str) }, optional = true, nullable = true),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> QuotaWarm(a[0] as QuotaWarmAct, a[1] as Long?, a[2] as String?, a[3] as String?, a[4] as String) },
            { v -> arrayOf(v.act, v.at, v.atText, v.atRelText, v.text) },
        )
    }
}

enum class QuotaWarmAct(val wire: String) {
    SEND("send"),
    WAIT("wait"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<QuotaWarmAct> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaWindowRead(
    val name: String,
    val used: Double? = null,
    val warnedAt: Double? = null,
    val resetsAt: Long? = null,
    val resetsAtText: String? = null,
    val resetsAtRelText: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaWindowRead> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("used", { opt(Dbl) }, optional = true, nullable = true),
            Field("warnedAt", { opt(Dbl) }, optional = true, nullable = true),
            Field("resetsAt", { opt(Lng) }, optional = true, nullable = true),
            Field("resetsAtText", { opt(Str) }, optional = true, nullable = true),
            Field("resetsAtRelText", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> QuotaWindowRead(a[0] as String, a[1] as Double?, a[2] as Double?, a[3] as Long?, a[4] as String?, a[5] as String?) },
            { v -> arrayOf(v.name, v.used, v.warnedAt, v.resetsAt, v.resetsAtText, v.resetsAtRelText) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaWindowSeen(
    val at: Long,
    val atText: String,
    val atRelText: String? = null,
    val from: QuotaSource,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaWindowSeen> = obj(
            listOf(
            Field("at", { Lng }, optional = false, nullable = false),
            Field("atText", { Str }, optional = false, nullable = false),
            Field("atRelText", { opt(Str) }, optional = true, nullable = true),
            Field("from", { QuotaSource.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> QuotaWindowSeen(a[0] as Long, a[1] as String, a[2] as String?, a[3] as QuotaSource) },
            { v -> arrayOf(v.at, v.atText, v.atRelText, v.from) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ReadChunkReply(
    val path: WirePath,
    val offset: Long,
    val size: Long,
    val eof: Boolean,
    val content: HexBytes,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ReadChunkReply> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("offset", { Lng }, optional = false, nullable = false),
            Field("size", { Lng }, optional = false, nullable = false),
            Field("eof", { Bool }, optional = false, nullable = false),
            Field("content", { HexBytes.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> ReadChunkReply(a[0] as WirePath, a[1] as Long, a[2] as Long, a[3] as Boolean, a[4] as HexBytes) },
            { v -> arrayOf(v.path, v.offset, v.size, v.eof, v.content) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ReadRow(
    val end: Long?,
    val hash: Long,
    val record: LineRecord? = null,
    val cwd: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ReadRow> = obj(
            listOf(
            Field("end", { opt(Lng) }, optional = false, nullable = true),
            Field("hash", { Lng }, optional = false, nullable = false),
            Field("record", { opt(LineRecord.ADAPTER) }, optional = true, nullable = true),
            Field("cwd", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> ReadRow(a[0] as Long?, a[1] as Long, a[2] as LineRecord?, a[3] as String?) },
            { v -> arrayOf(v.end, v.hash, v.record, v.cwd) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ReadTextReply(
    val path: WirePath,
    val text: String,
    val bytes: Long,
    val sha256: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ReadTextReply> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("bytes", { Lng }, optional = false, nullable = false),
            Field("sha256", { Str }, optional = false, nullable = false),
            ),
            { a -> ReadTextReply(a[0] as WirePath, a[1] as String, a[2] as Long, a[3] as String) },
            { v -> arrayOf(v.path, v.text, v.bytes, v.sha256) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RecentDir(
    val cwd: String,
    val lastMs: Long,
    val lastText: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RecentDir> = obj(
            listOf(
            Field("cwd", { Str }, optional = false, nullable = false),
            Field("lastMs", { Lng }, optional = false, nullable = false),
            Field("lastText", { Str }, optional = false, nullable = false),
            ),
            { a -> RecentDir(a[0] as String, a[1] as Long, a[2] as String) },
            { v -> arrayOf(v.cwd, v.lastMs, v.lastText) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RecordLine(
    val sessionId: String,
    val path: String,
    val seq: Long,
    val end: Long?,
    val cwd: String?,
    val record: LineRecord,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RecordLine> = obj(
            listOf(
            Field("session_id", { Str }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            Field("seq", { Lng }, optional = false, nullable = false),
            Field("end", { opt(Lng) }, optional = false, nullable = true),
            Field("cwd", { opt(Str) }, optional = false, nullable = true),
            Field("record", { LineRecord.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> RecordLine(a[0] as String, a[1] as String, a[2] as Long, a[3] as Long?, a[4] as String?, a[5] as LineRecord) },
            { v -> arrayOf(v.sessionId, v.path, v.seq, v.end, v.cwd, v.record) },
        )
    }
}

enum class Refused(val wire: String) {
    NOT_YOURS("not_yours"),
    NOT_MANAGED("not_managed"),
    OTHER_WINDOWS("other_windows"),
    NOT_KNOWN("not_known"),
    AMBIGUOUS("ambiguous"),
    ENDED("ended"),
    SCREEN_CHANGED("screen_changed"),
    WAITING("waiting"),
    ELSEWHERE("elsewhere"),
    NOT_RESUMED("not_resumed"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<Refused> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ReplyError(
    val reason: ApiReason,
    val status: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ReplyError> = obj(
            listOf(
            Field("reason", { ApiReason.ADAPTER }, optional = false, nullable = false),
            Field("status", { opt(Lng) }, optional = true, nullable = true),
            ),
            { a -> ReplyError(a[0] as ApiReason, a[1] as Long?) },
            { v -> arrayOf(v.reason, v.status) },
        )
    }
}

enum class ResultKind(val wire: String) {
    LAUNCHED("launched"),
    HANDBACK("handback"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ResultKind> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class RotAccount(
    val plate: RotPlate,
    val full: List<String>,
    val account: String,
    val kind: QuotaKind,
    val w5h: WinFact,
    val w7d: WinFact,
    val weekly: Boolean? = null,
    val login: LoginState,
    val seenAt: Long? = null,
    val blocked: FactBlocked? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotAccount> = obj(
            listOf(
            Field("plate", { RotPlate.ADAPTER }, optional = false, nullable = false),
            Field("full", { arr(Str) }, optional = false, nullable = false),
            Field("account", { Str }, optional = false, nullable = false),
            Field("kind", { QuotaKind.ADAPTER }, optional = false, nullable = false),
            Field("w5h", { WinFact.ADAPTER }, optional = false, nullable = false),
            Field("w7d", { WinFact.ADAPTER }, optional = false, nullable = false),
            Field("weekly", { opt(Bool) }, optional = true, nullable = true),
            Field("login", { LoginState.ADAPTER }, optional = false, nullable = false),
            Field("seenAt", { opt(Lng) }, optional = true, nullable = true),
            Field("blocked", { opt(FactBlocked.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> RotAccount(a[0] as RotPlate, a[1] as List<String>, a[2] as String, a[3] as QuotaKind, a[4] as WinFact, a[5] as WinFact, a[6] as Boolean?, a[7] as LoginState, a[8] as Long?, a[9] as FactBlocked?) },
            { v -> arrayOf(v.plate, v.full, v.account, v.kind, v.w5h, v.w7d, v.weekly, v.login, v.seenAt, v.blocked) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotBoard(
    val all: List<RotCell>,
    val rows: List<RotRow>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotBoard> = obj(
            listOf(
            Field("all", { arr(RotCell.ADAPTER) }, optional = false, nullable = false),
            Field("rows", { arr(RotRow.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> RotBoard(a[0] as List<RotCell>, a[1] as List<RotRow>) },
            { v -> arrayOf(v.all, v.rows) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotCell(
    val key: String,
    val kind: RotFormKind,
    val window: String? = null,
    val label: String,
    val help: String? = null,
    val v: Raw,
    val num: Double? = null,
    val from: RotFrom,
    val nullable: String? = null,
    val choices: List<RotChoice>? = null,
    val min: Double? = null,
    val max: Double? = null,
    val segs: List<RotDaySeg>? = null,
    val lead: String? = null,
    val under: String? = null,
    val same: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotCell> = obj(
            listOf(
            Field("key", { Str }, optional = false, nullable = false),
            Field("kind", { RotFormKind.ADAPTER }, optional = false, nullable = false),
            Field("window", { opt(Str) }, optional = true, nullable = true),
            Field("label", { Str }, optional = false, nullable = false),
            Field("help", { opt(Str) }, optional = true, nullable = true),
            Field("v", { RawJson }, optional = false, nullable = false),
            Field("num", { opt(Dbl) }, optional = true, nullable = true),
            Field("from", { RotFrom.ADAPTER }, optional = false, nullable = false),
            Field("nullable", { opt(Str) }, optional = true, nullable = true),
            Field("choices", { opt(arr(RotChoice.ADAPTER)) }, optional = true, nullable = true),
            Field("min", { opt(Dbl) }, optional = true, nullable = true),
            Field("max", { opt(Dbl) }, optional = true, nullable = true),
            Field("segs", { opt(arr(RotDaySeg.ADAPTER)) }, optional = true, nullable = true),
            Field("lead", { opt(Str) }, optional = true, nullable = true),
            Field("under", { opt(Str) }, optional = true, nullable = true),
            Field("same", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> RotCell(a[0] as String, a[1] as RotFormKind, a[2] as String?, a[3] as String, a[4] as String?, a[5] as Raw, a[6] as Double?, a[7] as RotFrom, a[8] as String?, a[9] as List<RotChoice>?, a[10] as Double?, a[11] as Double?, a[12] as List<RotDaySeg>?, a[13] as String?, a[14] as String?, a[15] as String?) },
            { v -> arrayOf(v.key, v.kind, v.window, v.label, v.help, v.v, v.num, v.from, v.nullable, v.choices, v.min, v.max, v.segs, v.lead, v.under, v.same) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotChoice(
    val v: String,
    val label: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotChoice> = obj(
            listOf(
            Field("v", { Str }, optional = false, nullable = false),
            Field("label", { Str }, optional = false, nullable = false),
            ),
            { a -> RotChoice(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.v, v.label) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotDaySeg(
    val from: Long,
    val to: Long,
    val fromText: String,
    val toText: String,
    val v: Double,
    val text: String,
    val tip: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotDaySeg> = obj(
            listOf(
            Field("from", { Lng }, optional = false, nullable = false),
            Field("to", { Lng }, optional = false, nullable = false),
            Field("fromText", { Str }, optional = false, nullable = false),
            Field("toText", { Str }, optional = false, nullable = false),
            Field("v", { Dbl }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("tip", { Str }, optional = false, nullable = false),
            ),
            { a -> RotDaySeg(a[0] as Long, a[1] as Long, a[2] as String, a[3] as String, a[4] as Double, a[5] as String, a[6] as String) },
            { v -> arrayOf(v.from, v.to, v.fromText, v.toText, v.v, v.text, v.tip) },
        )
    }
}

data object RotDone {
    val ADAPTER: JsonAdapter<RotDone> = obj(emptyList(), { RotDone }, { emptyArray() })
}

enum class RotFormKind(val wire: String) {
    PERCENT("percent"),
    MINUTES("minutes"),
    TOGGLE("toggle"),
    CHOICE("choice"),
    DAY_SEGMENTS("daySegments"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<RotFormKind> = enumOf(entries) { it.wire }
    }
}

enum class RotFrom(val wire: String) {
    OWN("own"),
    ALL("all"),
    DEFAULT("default"),
    SAME("same"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<RotFrom> = enumOf(entries) { it.wire }
    }
}

sealed interface RotHint {
    @Suppress("UNCHECKED_CAST")
    data class Hold(
        val until: Long? = null,
        val account: String? = null,
        val untilText: String? = null,
    ) : RotHint, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class StrategyError(
        val error: RotStrategyError,
        val path: String? = null,
    ) : RotHint, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class TzUnknown(
        val zone: String,
    ) : RotHint, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class OldScripts(
        val n: Long,
        val files: List<RotOldFile>,
    ) : RotHint, Ordered {
        override var wireOrder: IntArray? = null
    }
    data object Newer : RotHint
    @Suppress("UNCHECKED_CAST")
    data class Older(
        val n: Long,
    ) : RotHint, Ordered {
        override var wireOrder: IntArray? = null
    }
    data object WordsBad : RotHint
    @Suppress("UNCHECKED_CAST")
    data class BookUnreadable(
        val text: String,
    ) : RotHint, Ordered {
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<RotHint> =
            tagged<RotHint>(
                "k",
                listOf(
                Variant("hold", RotHint.Hold::class.java, obj(
                        listOf(
                        Field("k", { tag("hold") }, optional = false, nullable = false),
                        Field("until", { opt(Lng) }, optional = true, nullable = true),
                        Field("account", { opt(Str) }, optional = true, nullable = true),
                        Field("untilText", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> RotHint.Hold(a[1] as Long?, a[2] as String?, a[3] as String?) },
                        { v -> arrayOf("hold", v.until, v.account, v.untilText) },
                    )),
                Variant("strategyError", RotHint.StrategyError::class.java, obj(
                        listOf(
                        Field("k", { tag("strategyError") }, optional = false, nullable = false),
                        Field("error", { RotStrategyError.ADAPTER }, optional = false, nullable = false),
                        Field("path", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> RotHint.StrategyError(a[1] as RotStrategyError, a[2] as String?) },
                        { v -> arrayOf("strategyError", v.error, v.path) },
                    )),
                Variant("tzUnknown", RotHint.TzUnknown::class.java, obj(
                        listOf(
                        Field("k", { tag("tzUnknown") }, optional = false, nullable = false),
                        Field("zone", { Str }, optional = false, nullable = false),
                        ),
                        { a -> RotHint.TzUnknown(a[1] as String) },
                        { v -> arrayOf("tzUnknown", v.zone) },
                    )),
                Variant("oldScripts", RotHint.OldScripts::class.java, obj(
                        listOf(
                        Field("k", { tag("oldScripts") }, optional = false, nullable = false),
                        Field("n", { Lng }, optional = false, nullable = false),
                        Field("files", { arr(RotOldFile.ADAPTER) }, optional = false, nullable = false),
                        ),
                        { a -> RotHint.OldScripts(a[1] as Long, a[2] as List<RotOldFile>) },
                        { v -> arrayOf("oldScripts", v.n, v.files) },
                    )),
                Variant("newer", RotHint.Newer::class.java, obj(
                        listOf(
                        Field("k", { tag("newer") }, optional = false, nullable = false),
                        ),
                        { RotHint.Newer },
                        { arrayOf("newer") },
                    )),
                Variant("older", RotHint.Older::class.java, obj(
                        listOf(
                        Field("k", { tag("older") }, optional = false, nullable = false),
                        Field("n", { Lng }, optional = false, nullable = false),
                        ),
                        { a -> RotHint.Older(a[1] as Long) },
                        { v -> arrayOf("older", v.n) },
                    )),
                Variant("wordsBad", RotHint.WordsBad::class.java, obj(
                        listOf(
                        Field("k", { tag("wordsBad") }, optional = false, nullable = false),
                        ),
                        { RotHint.WordsBad },
                        { arrayOf("wordsBad") },
                    )),
                Variant("bookUnreadable", RotHint.BookUnreadable::class.java, obj(
                        listOf(
                        Field("k", { tag("bookUnreadable") }, optional = false, nullable = false),
                        Field("text", { Str }, optional = false, nullable = false),
                        ),
                        { a -> RotHint.BookUnreadable(a[1] as String) },
                        { v -> arrayOf("bookUnreadable", v.text) },
                    )),
                ),
            )
    }
}

sealed interface RotLogCause {
    data object Order : RotLogCause
    data object Unusable : RotLogCause
    data object Temp : RotLogCause
    data object UseNow : RotLogCause
    @Suppress("UNCHECKED_CAST")
    data class Hold(
        val until: Long? = null,
    ) : RotLogCause, Ordered {
        override var wireOrder: IntArray? = null
    }
    data object Error : RotLogCause

    companion object {
        val ADAPTER: JsonAdapter<RotLogCause> =
            tagged<RotLogCause>(
                "k",
                listOf(
                Variant("order", RotLogCause.Order::class.java, obj(
                        listOf(
                        Field("k", { tag("order") }, optional = false, nullable = false),
                        ),
                        { RotLogCause.Order },
                        { arrayOf("order") },
                    )),
                Variant("unusable", RotLogCause.Unusable::class.java, obj(
                        listOf(
                        Field("k", { tag("unusable") }, optional = false, nullable = false),
                        ),
                        { RotLogCause.Unusable },
                        { arrayOf("unusable") },
                    )),
                Variant("temp", RotLogCause.Temp::class.java, obj(
                        listOf(
                        Field("k", { tag("temp") }, optional = false, nullable = false),
                        ),
                        { RotLogCause.Temp },
                        { arrayOf("temp") },
                    )),
                Variant("useNow", RotLogCause.UseNow::class.java, obj(
                        listOf(
                        Field("k", { tag("useNow") }, optional = false, nullable = false),
                        ),
                        { RotLogCause.UseNow },
                        { arrayOf("useNow") },
                    )),
                Variant("hold", RotLogCause.Hold::class.java, obj(
                        listOf(
                        Field("k", { tag("hold") }, optional = false, nullable = false),
                        Field("until", { opt(Lng) }, optional = true, nullable = true),
                        ),
                        { a -> RotLogCause.Hold(a[1] as Long?) },
                        { v -> arrayOf("hold", v.until) },
                    )),
                Variant("error", RotLogCause.Error::class.java, obj(
                        listOf(
                        Field("k", { tag("error") }, optional = false, nullable = false),
                        ),
                        { RotLogCause.Error },
                        { arrayOf("error") },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotLogRow(
    val because: String,
    val at: Long,
    val from: String,
    val to: String,
    val cause: RotLogCause,
    val why: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotLogRow> = obj(
            listOf(
            Field("because", { Str }, optional = false, nullable = false),
            Field("at", { Lng }, optional = false, nullable = false),
            Field("from", { Str }, optional = false, nullable = false),
            Field("to", { Str }, optional = false, nullable = false),
            Field("cause", { RotLogCause.ADAPTER }, optional = false, nullable = false),
            Field("why", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> RotLogRow(a[0] as String, a[1] as Long, a[2] as String, a[3] as String, a[4] as RotLogCause, a[5] as String?) },
            { v -> arrayOf(v.because, v.at, v.from, v.to, v.cause, v.why) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotMark(
    val glyph: String? = null,
    val dashed: Boolean,
    val tip: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotMark> = obj(
            listOf(
            Field("glyph", { opt(Str) }, optional = true, nullable = true),
            Field("dashed", { Bool }, optional = false, nullable = false),
            Field("tip", { Str }, optional = false, nullable = false),
            ),
            { a -> RotMark(a[0] as String?, a[1] as Boolean, a[2] as String) },
            { v -> arrayOf(v.glyph, v.dashed, v.tip) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotNote(
    val hint: RotHint,
    val tone: Tone,
    val text: String,
    val detail: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotNote> = obj(
            listOf(
            Field("hint", { RotHint.ADAPTER }, optional = false, nullable = false),
            Field("tone", { Tone.ADAPTER }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("detail", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> RotNote(a[0] as RotHint, a[1] as Tone, a[2] as String, a[3] as String?) },
            { v -> arrayOf(v.hint, v.tone, v.text, v.detail) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotOldFile(
    val name: String,
    val path: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotOldFile> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            ),
            { a -> RotOldFile(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.name, v.path) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotPast(
    val from: Long,
    val uses: List<RotPastUse>,
    val lanes: List<RotPastLane>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotPast> = obj(
            listOf(
            Field("from", { Lng }, optional = false, nullable = false),
            Field("uses", { arr(RotPastUse.ADAPTER) }, optional = false, nullable = false),
            Field("lanes", { arr(RotPastLane.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> RotPast(a[0] as Long, a[1] as List<RotPastUse>, a[2] as List<RotPastLane>) },
            { v -> arrayOf(v.from, v.uses, v.lanes) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotPastLane(
    val account: String,
    val segs: List<RotPastSeg>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotPastLane> = obj(
            listOf(
            Field("account", { Str }, optional = false, nullable = false),
            Field("segs", { arr(RotPastSeg.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> RotPastLane(a[0] as String, a[1] as List<RotPastSeg>) },
            { v -> arrayOf(v.account, v.segs) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotPastSeg(
    val from: Long,
    val to: Long,
    val sendable: Boolean,
    val reset: Boolean,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotPastSeg> = obj(
            listOf(
            Field("from", { Lng }, optional = false, nullable = false),
            Field("to", { Lng }, optional = false, nullable = false),
            Field("sendable", { Bool }, optional = false, nullable = false),
            Field("reset", { Bool }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> RotPastSeg(a[0] as Long, a[1] as Long, a[2] as Boolean, a[3] as Boolean, a[4] as String) },
            { v -> arrayOf(v.from, v.to, v.sendable, v.reset, v.text) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotPastUse(
    val from: Long,
    val to: Long,
    val account: String? = null,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotPastUse> = obj(
            listOf(
            Field("from", { Lng }, optional = false, nullable = false),
            Field("to", { Lng }, optional = false, nullable = false),
            Field("account", { opt(Str) }, optional = true, nullable = true),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> RotPastUse(a[0] as Long, a[1] as Long, a[2] as String?, a[3] as String) },
            { v -> arrayOf(v.from, v.to, v.account, v.text) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotPlate(
    val color: Long,
    val label: String,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotPlate> = obj(
            listOf(
            Field("color", { Lng }, optional = false, nullable = false),
            Field("label", { Str }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> RotPlate(a[0] as Long, a[1] as String, a[2] as String) },
            { v -> arrayOf(v.color, v.label, v.text) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotRow(
    val account: String,
    val line: String,
    val hover: String,
    val cells: List<RotCell>,
    val rules: List<String>,
    val marks: List<RotMark>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotRow> = obj(
            listOf(
            Field("account", { Str }, optional = false, nullable = false),
            Field("line", { Str }, optional = false, nullable = false),
            Field("hover", { Str }, optional = false, nullable = false),
            Field("cells", { arr(RotCell.ADAPTER) }, optional = false, nullable = false),
            Field("rules", { arr(Str) }, optional = false, nullable = false),
            Field("marks", { arr(RotMark.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> RotRow(a[0] as String, a[1] as String, a[2] as String, a[3] as List<RotCell>, a[4] as List<String>, a[5] as List<RotMark>) },
            { v -> arrayOf(v.account, v.line, v.hover, v.cells, v.rules, v.marks) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotStrategyError(
    val line: Long? = null,
    val msg: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotStrategyError> = obj(
            listOf(
            Field("line", { opt(Lng) }, optional = true, nullable = true),
            Field("msg", { Str }, optional = false, nullable = false),
            ),
            { a -> RotStrategyError(a[0] as Long?, a[1] as String) },
            { v -> arrayOf(v.line, v.msg) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotStrategyId(
    val id: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotStrategyId> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            ),
            { a -> RotStrategyId(a[0] as String) },
            { v -> arrayOf(v.id) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotStrategyItem(
    val id: String,
    val description: String,
    val summary: String,
    val rev: String,
    val error: RotStrategyError? = null,
    val inUse: List<String>,
    val forNew: Boolean,
    val wordsBad: Boolean,
    val tzUnknown: Boolean,
    val hand: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotStrategyItem> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("description", { Str }, optional = false, nullable = false),
            Field("summary", { Str }, optional = false, nullable = false),
            Field("rev", { Str }, optional = false, nullable = false),
            Field("error", { opt(RotStrategyError.ADAPTER) }, optional = true, nullable = true),
            Field("inUse", { arr(Str) }, optional = false, nullable = false),
            Field("forNew", { Bool }, optional = false, nullable = false),
            Field("wordsBad", { Bool }, optional = false, nullable = false),
            Field("tzUnknown", { Bool }, optional = false, nullable = false),
            Field("hand", { Bool }, optional = false, nullable = false),
            ),
            { a -> RotStrategyItem(a[0] as String, a[1] as String, a[2] as String, a[3] as String, a[4] as RotStrategyError?, a[5] as List<String>, a[6] as Boolean, a[7] as Boolean, a[8] as Boolean, a[9] as Boolean) },
            { v -> arrayOf(v.id, v.description, v.summary, v.rev, v.error, v.inUse, v.forNew, v.wordsBad, v.tzUnknown, v.hand) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotStrategyRev(
    val rev: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotStrategyRev> = obj(
            listOf(
            Field("rev", { Str }, optional = false, nullable = false),
            ),
            { a -> RotStrategyRev(a[0] as String) },
            { v -> arrayOf(v.rev) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotTryDecision(
    val pick: String? = null,
    val holdUntil: Long? = null,
    val holdAccount: String? = null,
    val order: List<String>,
    val why: List<RotTryRow>,
    val fromTemp: Boolean,
    val skipped: List<String>,
    val temp: List<String>,
    val nextNode: Long? = null,
    val strategyError: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotTryDecision> = obj(
            listOf(
            Field("pick", { opt(Str) }, optional = true, nullable = true),
            Field("holdUntil", { opt(Lng) }, optional = true, nullable = true),
            Field("holdAccount", { opt(Str) }, optional = true, nullable = true),
            Field("order", { arr(Str) }, optional = false, nullable = false),
            Field("why", { arr(RotTryRow.ADAPTER) }, optional = false, nullable = false),
            Field("fromTemp", { Bool }, optional = false, nullable = false),
            Field("skipped", { arr(Str) }, optional = false, nullable = false),
            Field("temp", { arr(Str) }, optional = false, nullable = false),
            Field("nextNode", { opt(Lng) }, optional = true, nullable = true),
            Field("strategyError", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> RotTryDecision(a[0] as String?, a[1] as Long?, a[2] as String?, a[3] as List<String>, a[4] as List<RotTryRow>, a[5] as Boolean, a[6] as List<String>, a[7] as List<String>, a[8] as Long?, a[9] as String?) },
            { v -> arrayOf(v.pick, v.holdUntil, v.holdAccount, v.order, v.why, v.fromTemp, v.skipped, v.temp, v.nextNode, v.strategyError) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotTryLane(
    val account: String,
    val segs: List<RotPastSeg>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotTryLane> = obj(
            listOf(
            Field("account", { Str }, optional = false, nullable = false),
            Field("segs", { arr(RotPastSeg.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> RotTryLane(a[0] as String, a[1] as List<RotPastSeg>) },
            { v -> arrayOf(v.account, v.segs) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotTryRow(
    val account: String,
    val why: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotTryRow> = obj(
            listOf(
            Field("account", { Str }, optional = false, nullable = false),
            Field("why", { Str }, optional = false, nullable = false),
            ),
            { a -> RotTryRow(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.account, v.why) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotTrySwitch(
    val at: Long,
    val atText: String,
    val account: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotTrySwitch> = obj(
            listOf(
            Field("at", { Lng }, optional = false, nullable = false),
            Field("atText", { Str }, optional = false, nullable = false),
            Field("account", { Str }, optional = false, nullable = false),
            ),
            { a -> RotTrySwitch(a[0] as Long, a[1] as String, a[2] as String) },
            { v -> arrayOf(v.at, v.atText, v.account) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotTryTick(
    val at: Long,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotTryTick> = obj(
            listOf(
            Field("at", { Lng }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> RotTryTick(a[0] as Long, a[1] as String) },
            { v -> arrayOf(v.at, v.text) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotTryTimeline(
    val switches: List<RotTrySwitch>,
    val uses: List<RotTryUse>,
    val lanes: List<RotTryLane>,
    val truncated: Boolean,
    val ticks: List<RotTryTick>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotTryTimeline> = obj(
            listOf(
            Field("switches", { arr(RotTrySwitch.ADAPTER) }, optional = false, nullable = false),
            Field("uses", { arr(RotTryUse.ADAPTER) }, optional = false, nullable = false),
            Field("lanes", { arr(RotTryLane.ADAPTER) }, optional = false, nullable = false),
            Field("truncated", { Bool }, optional = false, nullable = false),
            Field("ticks", { arr(RotTryTick.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> RotTryTimeline(a[0] as List<RotTrySwitch>, a[1] as List<RotTryUse>, a[2] as List<RotTryLane>, a[3] as Boolean, a[4] as List<RotTryTick>) },
            { v -> arrayOf(v.switches, v.uses, v.lanes, v.truncated, v.ticks) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotTryUse(
    val from: Long,
    val to: Long,
    val account: String? = null,
    val fromTemp: Boolean,
    val capUnknown: Boolean,
    val skipped: List<String>,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotTryUse> = obj(
            listOf(
            Field("from", { Lng }, optional = false, nullable = false),
            Field("to", { Lng }, optional = false, nullable = false),
            Field("account", { opt(Str) }, optional = true, nullable = true),
            Field("fromTemp", { Bool }, optional = false, nullable = false),
            Field("capUnknown", { Bool }, optional = false, nullable = false),
            Field("skipped", { arr(Str) }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> RotTryUse(a[0] as Long, a[1] as Long, a[2] as String?, a[3] as Boolean, a[4] as Boolean, a[5] as List<String>, a[6] as String) },
            { v -> arrayOf(v.from, v.to, v.account, v.fromTemp, v.capUnknown, v.skipped, v.text) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotationFactsReply(
    val now: Long,
    val accounts: List<Fact>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationFactsReply> = obj(
            listOf(
            Field("now", { Lng }, optional = false, nullable = false),
            Field("accounts", { arr(Fact.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> RotationFactsReply(a[0] as Long, a[1] as List<Fact>) },
            { v -> arrayOf(v.now, v.accounts) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotationStrategiesReply(
    val strategies: List<RotStrategyItem>,
    val forNew: String,
    val oldScripts: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationStrategiesReply> = obj(
            listOf(
            Field("strategies", { arr(RotStrategyItem.ADAPTER) }, optional = false, nullable = false),
            Field("forNew", { Str }, optional = false, nullable = false),
            Field("oldScripts", { Lng }, optional = false, nullable = false),
            ),
            { a -> RotationStrategiesReply(a[0] as List<RotStrategyItem>, a[1] as String, a[2] as Long) },
            { v -> arrayOf(v.strategies, v.forNew, v.oldScripts) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotationTempReply(
    val temp: List<String>,
    val off: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationTempReply> = obj(
            listOf(
            Field("temp", { arr(Str) }, optional = false, nullable = false),
            Field("off", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> RotationTempReply(a[0] as List<String>, a[1] as List<String>) },
            { v -> arrayOf(v.temp, v.off) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotationTryReply(
    val now: Long,
    val description: String,
    val source: String? = null,
    val useNowRefused: Long? = null,
    val decision: RotTryDecision,
    val timeline: RotTryTimeline,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationTryReply> = obj(
            listOf(
            Field("now", { Lng }, optional = false, nullable = false),
            Field("description", { Str }, optional = false, nullable = false),
            Field("source", { opt(Str) }, optional = true, nullable = true),
            Field("useNowRefused", { opt(Lng) }, optional = true, nullable = true),
            Field("decision", { RotTryDecision.ADAPTER }, optional = false, nullable = false),
            Field("timeline", { RotTryTimeline.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> RotationTryReply(a[0] as Long, a[1] as String, a[2] as String?, a[3] as Long?, a[4] as RotTryDecision, a[5] as RotTryTimeline) },
            { v -> arrayOf(v.now, v.description, v.source, v.useNowRefused, v.decision, v.timeline) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotationViewReply(
    val now: Long,
    val nowText: String,
    val title: String?,
    val strategy: RotStrategyItem?,
    val board: RotBoard,
    val start: String,
    val current: String,
    val lineup: List<String>,
    val off: List<String>,
    val accounts: List<RotAccount>,
    val listed: Long,
    val decision: RotTryDecision?,
    val timeline: RotTryTimeline?,
    val past: RotPast?,
    val log: List<RotLogRow>,
    val hints: List<RotNote>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationViewReply> = obj(
            listOf(
            Field("now", { Lng }, optional = false, nullable = false),
            Field("nowText", { Str }, optional = false, nullable = false),
            Field("title", { opt(Str) }, optional = false, nullable = true),
            Field("strategy", { opt(RotStrategyItem.ADAPTER) }, optional = false, nullable = true),
            Field("board", { RotBoard.ADAPTER }, optional = false, nullable = false),
            Field("start", { Str }, optional = false, nullable = false),
            Field("current", { Str }, optional = false, nullable = false),
            Field("lineup", { arr(Str) }, optional = false, nullable = false),
            Field("off", { arr(Str) }, optional = false, nullable = false),
            Field("accounts", { arr(RotAccount.ADAPTER) }, optional = false, nullable = false),
            Field("listed", { Lng }, optional = false, nullable = false),
            Field("decision", { opt(RotTryDecision.ADAPTER) }, optional = false, nullable = true),
            Field("timeline", { opt(RotTryTimeline.ADAPTER) }, optional = false, nullable = true),
            Field("past", { opt(RotPast.ADAPTER) }, optional = false, nullable = true),
            Field("log", { arr(RotLogRow.ADAPTER) }, optional = false, nullable = false),
            Field("hints", { arr(RotNote.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> RotationViewReply(a[0] as Long, a[1] as String, a[2] as String?, a[3] as RotStrategyItem?, a[4] as RotBoard, a[5] as String, a[6] as String, a[7] as List<String>, a[8] as List<String>, a[9] as List<RotAccount>, a[10] as Long, a[11] as RotTryDecision?, a[12] as RotTryTimeline?, a[13] as RotPast?, a[14] as List<RotLogRow>, a[15] as List<RotNote>) },
            { v -> arrayOf(v.now, v.nowText, v.title, v.strategy, v.board, v.start, v.current, v.lineup, v.off, v.accounts, v.listed, v.decision, v.timeline, v.past, v.log, v.hints) },
        )
    }
}

enum class Run(val wire: String) {
    TMUX("tmux"),
    DIRECT("direct"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<Run> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ScreenLine(
    val text: String,
    val spans: List<Span>? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ScreenLine> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            Field("spans", { opt(arr(Span.ADAPTER)) }, optional = true, nullable = true),
            ),
            { a -> ScreenLine(a[0] as String, a[1] as List<Span>?) },
            { v -> arrayOf(v.text, v.spans) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SearchHit(
    val uuid: String,
    val tsMs: Long,
    val kind: String,
    val before: String,
    val matched: String,
    val after: String,
    val tool: String? = null,
    val offset: Long,
    val line: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SearchHit> = obj(
            listOf(
            Field("uuid", { Str }, optional = false, nullable = false),
            Field("tsMs", { Lng }, optional = false, nullable = false),
            Field("kind", { Str }, optional = false, nullable = false),
            Field("before", { Str }, optional = false, nullable = false),
            Field("matched", { Str }, optional = false, nullable = false),
            Field("after", { Str }, optional = false, nullable = false),
            Field("tool", { opt(Str) }, optional = true, nullable = true),
            Field("offset", { Lng }, optional = false, nullable = false),
            Field("line", { Lng }, optional = false, nullable = false),
            ),
            { a -> SearchHit(a[0] as String, a[1] as Long, a[2] as String, a[3] as String, a[4] as String, a[5] as String, a[6] as String?, a[7] as Long, a[8] as Long) },
            { v -> arrayOf(v.uuid, v.tsMs, v.kind, v.before, v.matched, v.after, v.tool, v.offset, v.line) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SearchRow(
    val atText: String,
    val spanText: String,
    val label: String,
    val starred: Boolean,
    val hidden: Hidden?,
    val agent: String,
    val sessionId: String,
    val projectPath: String,
    val projectName: String,
    val jsonlPath: String,
    val title: String,
    val updatedAt: Long,
    val hits: List<SearchHit>,
    val moreHits: Long,
    val hitsTruncated: Boolean,
    val isBg: Boolean,
    val can: Can,
    val forkedFrom: ForkedFrom? = null,
    val face: SessionFace,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SearchRow> = obj(
            listOf(
            Field("atText", { Str }, optional = false, nullable = false),
            Field("spanText", { Str }, optional = false, nullable = false),
            Field("label", { Str }, optional = false, nullable = false),
            Field("starred", { Bool }, optional = false, nullable = false),
            Field("hidden", { opt(Hidden.ADAPTER) }, optional = false, nullable = true),
            Field("agent", { Str }, optional = false, nullable = false),
            Field("sessionId", { Str }, optional = false, nullable = false),
            Field("projectPath", { Str }, optional = false, nullable = false),
            Field("projectName", { Str }, optional = false, nullable = false),
            Field("jsonlPath", { Str }, optional = false, nullable = false),
            Field("title", { Str }, optional = false, nullable = false),
            Field("updatedAt", { Lng }, optional = false, nullable = false),
            Field("hits", { arr(SearchHit.ADAPTER) }, optional = false, nullable = false),
            Field("moreHits", { Lng }, optional = false, nullable = false),
            Field("hitsTruncated", { Bool }, optional = false, nullable = false),
            Field("isBg", { Bool }, optional = false, nullable = false),
            Field("can", { Can.ADAPTER }, optional = false, nullable = false),
            Field("forkedFrom", { opt(ForkedFrom.ADAPTER) }, optional = true, nullable = true),
            Field("face", { SessionFace.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> SearchRow(a[0] as String, a[1] as String, a[2] as String, a[3] as Boolean, a[4] as Hidden?, a[5] as String, a[6] as String, a[7] as String, a[8] as String, a[9] as String, a[10] as String, a[11] as Long, a[12] as List<SearchHit>, a[13] as Long, a[14] as Boolean, a[15] as Boolean, a[16] as Can, a[17] as ForkedFrom?, a[18] as SessionFace) },
            { v -> arrayOf(v.atText, v.spanText, v.label, v.starred, v.hidden, v.agent, v.sessionId, v.projectPath, v.projectName, v.jsonlPath, v.title, v.updatedAt, v.hits, v.moreHits, v.hitsTruncated, v.isBg, v.can, v.forkedFrom, v.face) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Sent(
    val result: SentResult,
    val why: Refused? = null,
    val said: String? = null,
    val screen: String? = null,
    val resumed: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Sent> = obj(
            listOf(
            Field("result", { SentResult.ADAPTER }, optional = false, nullable = false),
            Field("why", { opt(Refused.ADAPTER) }, optional = true, nullable = true),
            Field("said", { opt(Str) }, optional = true, nullable = true),
            Field("screen", { opt(Str) }, optional = true, nullable = true),
            Field("resumed", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> Sent(a[0] as SentResult, a[1] as Refused?, a[2] as String?, a[3] as String?, a[4] as Boolean?) },
            { v -> arrayOf(v.result, v.why, v.said, v.screen, v.resumed) },
        )
    }
}

enum class SentResult(val wire: String) {
    DELIVERED("delivered"),
    UNSURE("unsure"),
    REFUSED("refused"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<SentResult> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionAct(
    val say: String,
    val confirm: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionAct> = obj(
            listOf(
            Field("say", { Str }, optional = false, nullable = false),
            Field("confirm", { Bool }, optional = false, nullable = false),
            ),
            { a -> SessionAct(a[0] as String, a[1] as Boolean) },
            { v -> arrayOf(v.say, v.confirm) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionCan(
    val close: CanClose,
    val resume: Allow,
    val term: CanTerm,
    val reattach: Allow,
    val model: Allow,
    val permission: Allow,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionCan> = obj(
            listOf(
            Field("close", { CanClose.ADAPTER }, optional = false, nullable = false),
            Field("resume", { Allow.ADAPTER }, optional = false, nullable = false),
            Field("term", { CanTerm.ADAPTER }, optional = false, nullable = false),
            Field("reattach", { Allow.ADAPTER }, optional = false, nullable = false),
            Field("model", { Allow.ADAPTER }, optional = false, nullable = false),
            Field("permission", { Allow.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> SessionCan(a[0] as CanClose, a[1] as Allow, a[2] as CanTerm, a[3] as Allow, a[4] as Allow, a[5] as Allow) },
            { v -> arrayOf(v.close, v.resume, v.term, v.reattach, v.model, v.permission) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionCommand(
    val name: String,
    val text: String?,
    val scope: CommandScope,
    val scopeText: String,
    val needsArgs: Boolean,
    val confirm: Boolean,
    val needsArgsText: String?,
    val confirmText: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionCommand> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("text", { opt(Str) }, optional = false, nullable = true),
            Field("scope", { CommandScope.ADAPTER }, optional = false, nullable = false),
            Field("scopeText", { Str }, optional = false, nullable = false),
            Field("needsArgs", { Bool }, optional = false, nullable = false),
            Field("confirm", { Bool }, optional = false, nullable = false),
            Field("needsArgsText", { opt(Str) }, optional = false, nullable = true),
            Field("confirmText", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> SessionCommand(a[0] as String, a[1] as String?, a[2] as CommandScope, a[3] as String, a[4] as Boolean, a[5] as Boolean, a[6] as String?, a[7] as String?) },
            { v -> arrayOf(v.name, v.text, v.scope, v.scopeText, v.needsArgs, v.confirm, v.needsArgsText, v.confirmText) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionCommands(
    val commands: List<SessionCommand>,
    val skills: List<SessionSkill>,
    val problems: List<String>,
    val compact: SessionAct?,
    val clear: SessionAct?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionCommands> = obj(
            listOf(
            Field("commands", { arr(SessionCommand.ADAPTER) }, optional = false, nullable = false),
            Field("skills", { arr(SessionSkill.ADAPTER) }, optional = false, nullable = false),
            Field("problems", { arr(Str) }, optional = false, nullable = false),
            Field("compact", { opt(SessionAct.ADAPTER) }, optional = false, nullable = true),
            Field("clear", { opt(SessionAct.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> SessionCommands(a[0] as List<SessionCommand>, a[1] as List<SessionSkill>, a[2] as List<String>, a[3] as SessionAct?, a[4] as SessionAct?) },
            { v -> arrayOf(v.commands, v.skills, v.problems, v.compact, v.clear) },
        )
    }
}

enum class SessionDot(val wire: String) {
    RUNNING("running"),
    NEEDS_YOU("needs-you"),
    BACKGROUND("background"),
    IDLE("idle"),
    EXITED("exited"),
    ENDED("ended"),
    GONE("gone"),
    UNKNOWN("unknown"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<SessionDot> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionFace(
    val dot: SessionDot,
    val live: FaceLive,
    val settled: Boolean,
    val name: String,
    val tip: String,
    val line: String,
    val tone: Tone,
    val order: Long,
    val can: SessionCan,
    val account: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionFace> = obj(
            listOf(
            Field("dot", { SessionDot.ADAPTER }, optional = false, nullable = false),
            Field("live", { FaceLive.ADAPTER }, optional = false, nullable = false),
            Field("settled", { Bool }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("tip", { Str }, optional = false, nullable = false),
            Field("line", { Str }, optional = false, nullable = false),
            Field("tone", { Tone.ADAPTER }, optional = false, nullable = false),
            Field("order", { Lng }, optional = false, nullable = false),
            Field("can", { SessionCan.ADAPTER }, optional = false, nullable = false),
            Field("account", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> SessionFace(a[0] as SessionDot, a[1] as FaceLive, a[2] as Boolean, a[3] as String, a[4] as String, a[5] as String, a[6] as Tone, a[7] as Long, a[8] as SessionCan, a[9] as String?) },
            { v -> arrayOf(v.dot, v.live, v.settled, v.name, v.tip, v.line, v.tone, v.order, v.can, v.account) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionSkill(
    val name: String,
    val text: String?,
    val say: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionSkill> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("text", { opt(Str) }, optional = false, nullable = true),
            Field("say", { Str }, optional = false, nullable = false),
            ),
            { a -> SessionSkill(a[0] as String, a[1] as String?, a[2] as String) },
            { v -> arrayOf(v.name, v.text, v.say) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SlotShow(
    val slot: String,
    val pct: Long? = null,
    val resetsAt: Long? = null,
    val resetsAtText: String? = null,
    val resetsAtRelText: String? = null,
    val resetText: String? = null,
    val full: Boolean? = null,
    val text: String,
    val tone: SlotShowTone,
    val perPctText: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SlotShow> = obj(
            listOf(
            Field("slot", { Str }, optional = false, nullable = false),
            Field("pct", { opt(Lng) }, optional = true, nullable = true),
            Field("resetsAt", { opt(Lng) }, optional = true, nullable = true),
            Field("resetsAtText", { opt(Str) }, optional = true, nullable = true),
            Field("resetsAtRelText", { opt(Str) }, optional = true, nullable = true),
            Field("resetText", { opt(Str) }, optional = true, nullable = true),
            Field("full", { opt(Bool) }, optional = true, nullable = true),
            Field("text", { Str }, optional = false, nullable = false),
            Field("tone", { SlotShowTone.ADAPTER }, optional = false, nullable = false),
            Field("perPctText", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> SlotShow(a[0] as String, a[1] as Long?, a[2] as Long?, a[3] as String?, a[4] as String?, a[5] as String?, a[6] as Boolean?, a[7] as String, a[8] as SlotShowTone, a[9] as String?) },
            { v -> arrayOf(v.slot, v.pct, v.resetsAt, v.resetsAtText, v.resetsAtRelText, v.resetText, v.full, v.text, v.tone, v.perPctText) },
        )
    }
}

enum class SlotShowTone(val wire: String) {
    PLAIN("plain"),
    FAIL("fail"),
    WARN("warn"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<SlotShowTone> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class Span(
    val from: Long,
    val to: Long,
    val fg: String? = null,
    val bg: String? = null,
    val bold: Boolean? = null,
    val dim: Boolean? = null,
    val italic: Boolean? = null,
    val underline: Boolean? = null,
    val inverse: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Span> = obj(
            listOf(
            Field("from", { Lng }, optional = false, nullable = false),
            Field("to", { Lng }, optional = false, nullable = false),
            Field("fg", { opt(Str) }, optional = true, nullable = true),
            Field("bg", { opt(Str) }, optional = true, nullable = true),
            Field("bold", { opt(Bool) }, optional = true, nullable = true),
            Field("dim", { opt(Bool) }, optional = true, nullable = true),
            Field("italic", { opt(Bool) }, optional = true, nullable = true),
            Field("underline", { opt(Bool) }, optional = true, nullable = true),
            Field("inverse", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> Span(a[0] as Long, a[1] as Long, a[2] as String?, a[3] as String?, a[4] as Boolean?, a[5] as Boolean?, a[6] as Boolean?, a[7] as Boolean?, a[8] as Boolean?) },
            { v -> arrayOf(v.from, v.to, v.fg, v.bg, v.bold, v.dim, v.italic, v.underline, v.inverse) },
        )
    }
}

sealed interface Speaker {
    data object Human : Speaker
    @Suppress("UNCHECKED_CAST")
    data class SlashCommand(
        val name: String,
        val args: String,
    ) : Speaker, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class BashInput(
        val command: String,
    ) : Speaker, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class BashOutput(
        val stdout: String,
        val stderr: String,
    ) : Speaker, Ordered {
        override var wireOrder: IntArray? = null
    }
    data object CommandOutput : Speaker
    @Suppress("UNCHECKED_CAST")
    data class TaskNotification(
        val taskId: String? = null,
        val status: String? = null,
        val summary: String? = null,
        val toolUseId: String? = null,
    ) : Speaker, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class AgentMessage(
        val from: String? = null,
        val name: String? = null,
        val handback: Boolean,
        val body: String? = null,
    ) : Speaker, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class PeerSession(
        val from: String? = null,
        val body: String? = null,
    ) : Speaker, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Coordinator(
        val body: String? = null,
    ) : Speaker, Ordered {
        override var wireOrder: IntArray? = null
    }
    data object AgentTask : Speaker
    @Suppress("UNCHECKED_CAST")
    data class System(
        val body: String? = null,
    ) : Speaker, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class CompactSummary(
        val charsText: String,
    ) : Speaker, Ordered {
        override var wireOrder: IntArray? = null
    }
    data object Interrupt : Speaker
    data object ToolResult : Speaker

    companion object {
        val ADAPTER: JsonAdapter<Speaker> =
            tagged<Speaker>(
                "kind",
                listOf(
                Variant("human", Speaker.Human::class.java, obj(
                        listOf(
                        Field("kind", { tag("human") }, optional = false, nullable = false),
                        ),
                        { Speaker.Human },
                        { arrayOf("human") },
                    )),
                Variant("slashCommand", Speaker.SlashCommand::class.java, obj(
                        listOf(
                        Field("kind", { tag("slashCommand") }, optional = false, nullable = false),
                        Field("name", { Str }, optional = false, nullable = false),
                        Field("args", { Str }, optional = false, nullable = false),
                        ),
                        { a -> Speaker.SlashCommand(a[1] as String, a[2] as String) },
                        { v -> arrayOf("slashCommand", v.name, v.args) },
                    )),
                Variant("bashInput", Speaker.BashInput::class.java, obj(
                        listOf(
                        Field("kind", { tag("bashInput") }, optional = false, nullable = false),
                        Field("command", { Str }, optional = false, nullable = false),
                        ),
                        { a -> Speaker.BashInput(a[1] as String) },
                        { v -> arrayOf("bashInput", v.command) },
                    )),
                Variant("bashOutput", Speaker.BashOutput::class.java, obj(
                        listOf(
                        Field("kind", { tag("bashOutput") }, optional = false, nullable = false),
                        Field("stdout", { Str }, optional = false, nullable = false),
                        Field("stderr", { Str }, optional = false, nullable = false),
                        ),
                        { a -> Speaker.BashOutput(a[1] as String, a[2] as String) },
                        { v -> arrayOf("bashOutput", v.stdout, v.stderr) },
                    )),
                Variant("commandOutput", Speaker.CommandOutput::class.java, obj(
                        listOf(
                        Field("kind", { tag("commandOutput") }, optional = false, nullable = false),
                        ),
                        { Speaker.CommandOutput },
                        { arrayOf("commandOutput") },
                    )),
                Variant("taskNotification", Speaker.TaskNotification::class.java, obj(
                        listOf(
                        Field("kind", { tag("taskNotification") }, optional = false, nullable = false),
                        Field("taskId", { opt(Str) }, optional = true, nullable = true),
                        Field("status", { opt(Str) }, optional = true, nullable = true),
                        Field("summary", { opt(Str) }, optional = true, nullable = true),
                        Field("toolUseId", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> Speaker.TaskNotification(a[1] as String?, a[2] as String?, a[3] as String?, a[4] as String?) },
                        { v -> arrayOf("taskNotification", v.taskId, v.status, v.summary, v.toolUseId) },
                    )),
                Variant("agentMessage", Speaker.AgentMessage::class.java, obj(
                        listOf(
                        Field("kind", { tag("agentMessage") }, optional = false, nullable = false),
                        Field("from", { opt(Str) }, optional = true, nullable = true),
                        Field("name", { opt(Str) }, optional = true, nullable = true),
                        Field("handback", { Bool }, optional = false, nullable = false),
                        Field("body", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> Speaker.AgentMessage(a[1] as String?, a[2] as String?, a[3] as Boolean, a[4] as String?) },
                        { v -> arrayOf("agentMessage", v.from, v.name, v.handback, v.body) },
                    )),
                Variant("peerSession", Speaker.PeerSession::class.java, obj(
                        listOf(
                        Field("kind", { tag("peerSession") }, optional = false, nullable = false),
                        Field("from", { opt(Str) }, optional = true, nullable = true),
                        Field("body", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> Speaker.PeerSession(a[1] as String?, a[2] as String?) },
                        { v -> arrayOf("peerSession", v.from, v.body) },
                    )),
                Variant("coordinator", Speaker.Coordinator::class.java, obj(
                        listOf(
                        Field("kind", { tag("coordinator") }, optional = false, nullable = false),
                        Field("body", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> Speaker.Coordinator(a[1] as String?) },
                        { v -> arrayOf("coordinator", v.body) },
                    )),
                Variant("agentTask", Speaker.AgentTask::class.java, obj(
                        listOf(
                        Field("kind", { tag("agentTask") }, optional = false, nullable = false),
                        ),
                        { Speaker.AgentTask },
                        { arrayOf("agentTask") },
                    )),
                Variant("system", Speaker.System::class.java, obj(
                        listOf(
                        Field("kind", { tag("system") }, optional = false, nullable = false),
                        Field("body", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> Speaker.System(a[1] as String?) },
                        { v -> arrayOf("system", v.body) },
                    )),
                Variant("compactSummary", Speaker.CompactSummary::class.java, obj(
                        listOf(
                        Field("kind", { tag("compactSummary") }, optional = false, nullable = false),
                        Field("charsText", { Str }, optional = false, nullable = false),
                        ),
                        { a -> Speaker.CompactSummary(a[1] as String) },
                        { v -> arrayOf("compactSummary", v.charsText) },
                    )),
                Variant("interrupt", Speaker.Interrupt::class.java, obj(
                        listOf(
                        Field("kind", { tag("interrupt") }, optional = false, nullable = false),
                        ),
                        { Speaker.Interrupt },
                        { arrayOf("interrupt") },
                    )),
                Variant("toolResult", Speaker.ToolResult::class.java, obj(
                        listOf(
                        Field("kind", { tag("toolResult") }, optional = false, nullable = false),
                        ),
                        { Speaker.ToolResult },
                        { arrayOf("toolResult") },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class StartedBy(
    val client: String?,
    val mine: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<StartedBy> = obj(
            listOf(
            Field("client", { opt(Str) }, optional = false, nullable = true),
            Field("mine", { Bool }, optional = false, nullable = false),
            ),
            { a -> StartedBy(a[0] as String?, a[1] as Boolean) },
            { v -> arrayOf(v.client, v.mine) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class StatReply(
    val path: WirePath,
    val up: WirePath?,
    val kind: FileKind,
    val size: Long,
    val readonly: Boolean,
    val mode: Long? = null,
    val mtimeSecs: Long? = null,
    val mtimeText: String? = null,
    val mtimeFull: String? = null,
    val owner: String?,
    val linkTarget: WirePath?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<StatReply> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("up", { opt(WirePath.ADAPTER) }, optional = false, nullable = true),
            Field("kind", { FileKind.ADAPTER }, optional = false, nullable = false),
            Field("size", { Lng }, optional = false, nullable = false),
            Field("readonly", { Bool }, optional = false, nullable = false),
            Field("mode", { opt(Lng) }, optional = true, nullable = true),
            Field("mtime_secs", { opt(Lng) }, optional = true, nullable = true),
            Field("mtime_text", { opt(Str) }, optional = true, nullable = true),
            Field("mtime_full", { opt(Str) }, optional = true, nullable = true),
            Field("owner", { opt(Str) }, optional = false, nullable = true),
            Field("link_target", { opt(WirePath.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> StatReply(a[0] as WirePath, a[1] as WirePath?, a[2] as FileKind, a[3] as Long, a[4] as Boolean, a[5] as Long?, a[6] as Long?, a[7] as String?, a[8] as String?, a[9] as String?, a[10] as WirePath?) },
            { v -> arrayOf(v.path, v.up, v.kind, v.size, v.readonly, v.mode, v.mtimeSecs, v.mtimeText, v.mtimeFull, v.owner, v.linkTarget) },
        )
    }
}

sealed interface StepAsk {
    @Suppress("UNCHECKED_CAST")
    data class Questions(
        val questions: List<AskQuestion>,
    ) : StepAsk, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Plan(
        val text: String,
        val title: PlanTitle,
    ) : StepAsk, Ordered {
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<StepAsk> =
            tagged<StepAsk>(
                "kind",
                listOf(
                Variant("questions", StepAsk.Questions::class.java, obj(
                        listOf(
                        Field("kind", { tag("questions") }, optional = false, nullable = false),
                        Field("questions", { arr(AskQuestion.ADAPTER) }, optional = false, nullable = false),
                        ),
                        { a -> StepAsk.Questions(a[1] as List<AskQuestion>) },
                        { v -> arrayOf("questions", v.questions) },
                    )),
                Variant("plan", StepAsk.Plan::class.java, obj(
                        listOf(
                        Field("kind", { tag("plan") }, optional = false, nullable = false),
                        Field("text", { Str }, optional = false, nullable = false),
                        Field("title", { PlanTitle.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> StepAsk.Plan(a[1] as String, a[2] as PlanTitle) },
                        { v -> arrayOf("plan", v.text, v.title) },
                    )),
                ),
            )
    }
}

sealed interface StepBody {
    @Suppress("UNCHECKED_CAST")
    data class Changes(
        val changes: List<TextChange>,
    ) : StepBody, Ordered {
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Command(
        val line: String,
    ) : StepBody, Ordered {
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<StepBody> =
            tagged<StepBody>(
                "kind",
                listOf(
                Variant("changes", StepBody.Changes::class.java, obj(
                        listOf(
                        Field("kind", { tag("changes") }, optional = false, nullable = false),
                        Field("changes", { arr(TextChange.ADAPTER) }, optional = false, nullable = false),
                        ),
                        { a -> StepBody.Changes(a[1] as List<TextChange>) },
                        { v -> arrayOf("changes", v.changes) },
                    )),
                Variant("command", StepBody.Command::class.java, obj(
                        listOf(
                        Field("kind", { tag("command") }, optional = false, nullable = false),
                        Field("line", { Str }, optional = false, nullable = false),
                        ),
                        { a -> StepBody.Command(a[1] as String) },
                        { v -> arrayOf("command", v.line) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class StepResult(
    val ok: Boolean,
    val rejected: Boolean? = null,
    val lines: Long? = null,
    val added: Long? = null,
    val removed: Long? = null,
    val files: Long? = null,
    val answer: Answer? = null,
    val exitCode: Long? = null,
    val file: String? = null,
    val patch: List<PatchHunk>? = null,
    val patchTruncated: Boolean? = null,
    val text: String,
    val timed: String? = null,
    val preview: String? = null,
    val chars: Long? = null,
    val kind: ResultKind? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<StepResult> = obj(
            listOf(
            Field("ok", { Bool }, optional = false, nullable = false),
            Field("rejected", { opt(Bool) }, optional = true, nullable = true),
            Field("lines", { opt(Lng) }, optional = true, nullable = true),
            Field("added", { opt(Lng) }, optional = true, nullable = true),
            Field("removed", { opt(Lng) }, optional = true, nullable = true),
            Field("files", { opt(Lng) }, optional = true, nullable = true),
            Field("answer", { opt(Answer.ADAPTER) }, optional = true, nullable = true),
            Field("exitCode", { opt(Lng) }, optional = true, nullable = true),
            Field("file", { opt(Str) }, optional = true, nullable = true),
            Field("patch", { opt(arr(PatchHunk.ADAPTER)) }, optional = true, nullable = true),
            Field("patchTruncated", { opt(Bool) }, optional = true, nullable = true),
            Field("text", { Str }, optional = false, nullable = false),
            Field("timed", { opt(Str) }, optional = true, nullable = true),
            Field("preview", { opt(Str) }, optional = true, nullable = true),
            Field("chars", { opt(Lng) }, optional = true, nullable = true),
            Field("kind", { opt(ResultKind.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> StepResult(a[0] as Boolean, a[1] as Boolean?, a[2] as Long?, a[3] as Long?, a[4] as Long?, a[5] as Long?, a[6] as Answer?, a[7] as Long?, a[8] as String?, a[9] as List<PatchHunk>?, a[10] as Boolean?, a[11] as String, a[12] as String?, a[13] as String?, a[14] as Long?, a[15] as ResultKind?) },
            { v -> arrayOf(v.ok, v.rejected, v.lines, v.added, v.removed, v.files, v.answer, v.exitCode, v.file, v.patch, v.patchTruncated, v.text, v.timed, v.preview, v.chars, v.kind) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SyncHead<H>(
    val total: Long,
    val info: H,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        fun <H> adapter(adH: JsonAdapter<H>): JsonAdapter<SyncHead<H>> = obj(
            listOf(
            Field("total", { Lng }, optional = false, nullable = false),
            Field("info", { adH }, optional = false, nullable = false),
            ),
            { a -> SyncHead<H>(a[0] as Long, a[1] as H) },
            { v -> arrayOf(v.total, v.info) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SyncPatch<H, T>(
    val held: String,
    val whole: Boolean,
    val same: Boolean,
    val order: List<String>?,
    val up: Map<String, T>,
    val head: SyncHead<H>?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        fun <H, T> adapter(adH: JsonAdapter<H>, adT: JsonAdapter<T>): JsonAdapter<SyncPatch<H, T>> = obj(
            listOf(
            Field("held", { Str }, optional = false, nullable = false),
            Field("whole", { Bool }, optional = false, nullable = false),
            Field("same", { Bool }, optional = false, nullable = false),
            Field("order", { opt(arr(Str)) }, optional = false, nullable = true),
            Field("up", { dict(adT) }, optional = false, nullable = false),
            Field("head", { opt(SyncHead.adapter(adH)) }, optional = false, nullable = true),
            ),
            { a -> SyncPatch<H, T>(a[0] as String, a[1] as Boolean, a[2] as Boolean, a[3] as List<String>?, a[4] as Map<String, T>, a[5] as SyncHead<H>?) },
            { v -> arrayOf(v.held, v.whole, v.same, v.order, v.up, v.head) },
        )
    }
}

enum class TailHow(val wire: String) {
    TAIL("tail"),
    RESUME("resume"),
    HANDOFF("handoff"),
    TRUNCATED("truncated"),
    REWRITTEN("rewritten"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<TailHow> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class TermSession(
    val sid: String,
    val agent: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TermSession> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("agent", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> TermSession(a[0] as String, a[1] as String?) },
            { v -> arrayOf(v.sid, v.agent) },
        )
    }
}

enum class TermState(val wire: String) {
    RUNNING("running"),
    IDLE("idle"),
    PROGRAM_EXITED("program_exited"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<TermState> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class TerminalCan(
    val input: TerminalCanInput,
    val end: TerminalCanEnd,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalCan> = obj(
            listOf(
            Field("input", { TerminalCanInput.ADAPTER }, optional = false, nullable = false),
            Field("end", { TerminalCanEnd.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> TerminalCan(a[0] as TerminalCanInput, a[1] as TerminalCanEnd) },
            { v -> arrayOf(v.input, v.end) },
        )
    }
}

sealed interface TerminalCanEnd {
    data class Flag(
        val value: Boolean,
    ) : TerminalCanEnd
    data class Obj(
        val value: TerminalCanEndObjValue,
    ) : TerminalCanEnd

    companion object {
        val ADAPTER: JsonAdapter<TerminalCanEnd> =
            byToken<TerminalCanEnd>(
                listOf(
                Tok(JsonReader.Token.BOOLEAN, TerminalCanEnd.Flag::class.java, { Bool }, { TerminalCanEnd.Flag(it as Boolean) }, { (it as TerminalCanEnd.Flag).value }),
                Tok(JsonReader.Token.BEGIN_OBJECT, TerminalCanEnd.Obj::class.java, { TerminalCanEndObjValue.ADAPTER }, { TerminalCanEnd.Obj(it as TerminalCanEndObjValue) }, { (it as TerminalCanEnd.Obj).value }),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class TerminalCanEndObjValue(
    val no: TerminalCanEndObjValueNo,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalCanEndObjValue> = obj(
            listOf(
            Field("no", { TerminalCanEndObjValueNo.ADAPTER }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> TerminalCanEndObjValue(a[0] as TerminalCanEndObjValueNo, a[1] as String) },
            { v -> arrayOf(v.no, v.said) },
        )
    }
}

enum class TerminalCanEndObjValueNo(val wire: String) {
    NOT_YOURS("not_yours"),
    NOT_MANAGED("not_managed"),
    OTHER_WINDOWS("other_windows"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<TerminalCanEndObjValueNo> = enumOf(entries) { it.wire }
    }
}

sealed interface TerminalCanInput {
    data class Flag(
        val value: Boolean,
    ) : TerminalCanInput
    data class Obj(
        val value: TerminalCanInputObjValue,
    ) : TerminalCanInput

    companion object {
        val ADAPTER: JsonAdapter<TerminalCanInput> =
            byToken<TerminalCanInput>(
                listOf(
                Tok(JsonReader.Token.BOOLEAN, TerminalCanInput.Flag::class.java, { Bool }, { TerminalCanInput.Flag(it as Boolean) }, { (it as TerminalCanInput.Flag).value }),
                Tok(JsonReader.Token.BEGIN_OBJECT, TerminalCanInput.Obj::class.java, { TerminalCanInputObjValue.ADAPTER }, { TerminalCanInput.Obj(it as TerminalCanInputObjValue) }, { (it as TerminalCanInput.Obj).value }),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class TerminalCanInputObjValue(
    val no: TerminalCanInputObjValueNo,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalCanInputObjValue> = obj(
            listOf(
            Field("no", { TerminalCanInputObjValueNo.ADAPTER }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> TerminalCanInputObjValue(a[0] as TerminalCanInputObjValueNo, a[1] as String) },
            { v -> arrayOf(v.no, v.said) },
        )
    }
}

enum class TerminalCanInputObjValueNo(val wire: String) {
    NOT_YOURS("not_yours"),
    NOT_MANAGED("not_managed"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<TerminalCanInputObjValueNo> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class TerminalItem(
    val terminal: String,
    val host: TerminalItemHost,
    val tmuxName: String,
    val title: String,
    val program: String,
    val cwd: String,
    val session: TermSession? = null,
    val startedBy: StartedBy,
    val clients: List<Attached>,
    val input: TerminalItemInput,
    val state: TermState,
    val lastActivity: Long,
    val can: TerminalCan,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalItem> = obj(
            listOf(
            Field("terminal", { Str }, optional = false, nullable = false),
            Field("host", { TerminalItemHost.ADAPTER }, optional = false, nullable = false),
            Field("tmux_name", { Str }, optional = false, nullable = false),
            Field("title", { Str }, optional = false, nullable = false),
            Field("program", { Str }, optional = false, nullable = false),
            Field("cwd", { Str }, optional = false, nullable = false),
            Field("session", { opt(TermSession.ADAPTER) }, optional = true, nullable = true),
            Field("started_by", { StartedBy.ADAPTER }, optional = false, nullable = false),
            Field("clients", { arr(Attached.ADAPTER) }, optional = false, nullable = false),
            Field("input", { TerminalItemInput.ADAPTER }, optional = false, nullable = false),
            Field("state", { TermState.ADAPTER }, optional = false, nullable = false),
            Field("last_activity", { Lng }, optional = false, nullable = false),
            Field("can", { TerminalCan.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> TerminalItem(a[0] as String, a[1] as TerminalItemHost, a[2] as String, a[3] as String, a[4] as String, a[5] as String, a[6] as TermSession?, a[7] as StartedBy, a[8] as List<Attached>, a[9] as TerminalItemInput, a[10] as TermState, a[11] as Long, a[12] as TerminalCan) },
            { v -> arrayOf(v.terminal, v.host, v.tmuxName, v.title, v.program, v.cwd, v.session, v.startedBy, v.clients, v.input, v.state, v.lastActivity, v.can) },
        )
    }
}

enum class TerminalItemHost(val wire: String) {
    TMUX("tmux"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<TerminalItemHost> = enumOf(entries) { it.wire }
    }
}

enum class TerminalItemInput(val wire: String) {
    SHARED("shared"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<TerminalItemInput> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class TerminalsList(
    val terminals: List<TerminalItem>,
    val complete: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalsList> = obj(
            listOf(
            Field("terminals", { arr(TerminalItem.ADAPTER) }, optional = false, nullable = false),
            Field("complete", { Bool }, optional = false, nullable = false),
            ),
            { a -> TerminalsList(a[0] as List<TerminalItem>, a[1] as Boolean) },
            { v -> arrayOf(v.terminals, v.complete) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TextChange(
    val old: String,
    val new: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TextChange> = obj(
            listOf(
            Field("old", { Str }, optional = false, nullable = false),
            Field("new", { Str }, optional = false, nullable = false),
            ),
            { a -> TextChange(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.old, v.new) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TextPreview(
    val cap: Long,
    val lang: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TextPreview> = obj(
            listOf(
            Field("cap", { Lng }, optional = false, nullable = false),
            Field("lang", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> TextPreview(a[0] as Long, a[1] as String?) },
            { v -> arrayOf(v.cap, v.lang) },
        )
    }
}

enum class TitleBy(val wire: String) {
    AGENT("agent"),
    USER("user"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<TitleBy> = enumOf(entries) { it.wire }
    }
}

enum class Tone(val wire: String) {
    PLAIN("plain"),
    FAIL("fail"),
    NOW("now"),
    NEED("need"),
    WARN("warn"),
    BUSY("busy"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<Tone> = enumOf(entries) { it.wire }
    }
}

enum class ToolCard(val wire: String) {
    AGENT("agent"),
    INTERACTIVE("interactive"),
    DIFF("diff"),
    MD("md"),
    COMMAND("command"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ToolCard> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ToolStep(
    val tool: String,
    val arg: String? = null,
    val path: Boolean? = null,
    val note: String? = null,
    val file: String? = null,
    val body: StepBody? = null,
    val known: Boolean,
    val text: String? = null,
    val ask: StepAsk? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ToolStep> = obj(
            listOf(
            Field("tool", { Str }, optional = false, nullable = false),
            Field("arg", { opt(Str) }, optional = true, nullable = true),
            Field("path", { opt(Bool) }, optional = true, nullable = true),
            Field("note", { opt(Str) }, optional = true, nullable = true),
            Field("file", { opt(Str) }, optional = true, nullable = true),
            Field("body", { opt(StepBody.ADAPTER) }, optional = true, nullable = true),
            Field("known", { Bool }, optional = false, nullable = false),
            Field("text", { opt(Str) }, optional = true, nullable = true),
            Field("ask", { opt(StepAsk.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> ToolStep(a[0] as String, a[1] as String?, a[2] as Boolean?, a[3] as String?, a[4] as String?, a[5] as StepBody?, a[6] as Boolean, a[7] as String?, a[8] as StepAsk?) },
            { v -> arrayOf(v.tool, v.arg, v.path, v.note, v.file, v.body, v.known, v.text, v.ask) },
        )
    }
}

enum class UnreadWhy(val wire: String) {
    UNKNOWN_TYPE("unknownType"),
    PARSE_FAILED("parseFailed"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<UnreadWhy> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class Usage(
    val slot: String? = null,
    val window: String? = null,
    val value: String,
    val reset: String? = null,
    val text: String,
    val tone: UsageTone,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Usage> = obj(
            listOf(
            Field("slot", { opt(Str) }, optional = true, nullable = true),
            Field("window", { opt(Str) }, optional = true, nullable = true),
            Field("value", { Str }, optional = false, nullable = false),
            Field("reset", { opt(Str) }, optional = true, nullable = true),
            Field("text", { Str }, optional = false, nullable = false),
            Field("tone", { UsageTone.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> Usage(a[0] as String?, a[1] as String?, a[2] as String, a[3] as String?, a[4] as String, a[5] as UsageTone) },
            { v -> arrayOf(v.slot, v.window, v.value, v.reset, v.text, v.tone) },
        )
    }
}

enum class UsageTone(val wire: String) {
    PLAIN("plain"),
    FAIL("fail"),
    WARN("warn"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<UsageTone> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class UserText(
    val speaker: Speaker,
    val text: String,
    val pasted: List<Pasted>? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<UserText> = obj(
            listOf(
            Field("speaker", { Speaker.ADAPTER }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("pasted", { opt(arr(Pasted.ADAPTER)) }, optional = true, nullable = true),
            ),
            { a -> UserText(a[0] as Speaker, a[1] as String, a[2] as List<Pasted>?) },
            { v -> arrayOf(v.speaker, v.text, v.pasted) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class WinFact(
    val open: Boolean,
    val used: Double,
    val resets: Long? = null,
    val resetsText: String? = null,
    val rate: Double? = null,
    val tokensPerPct: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<WinFact> = obj(
            listOf(
            Field("open", { Bool }, optional = false, nullable = false),
            Field("used", { Dbl }, optional = false, nullable = false),
            Field("resets", { opt(Lng) }, optional = true, nullable = true),
            Field("resetsText", { opt(Str) }, optional = true, nullable = true),
            Field("rate", { opt(Dbl) }, optional = true, nullable = true),
            Field("tokensPerPct", { opt(Lng) }, optional = true, nullable = true),
            ),
            { a -> WinFact(a[0] as Boolean, a[1] as Double, a[2] as Long?, a[3] as String?, a[4] as Double?, a[5] as Long?) },
            { v -> arrayOf(v.open, v.used, v.resets, v.resetsText, v.rate, v.tokensPerPct) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class WindowShow(
    val name: String,
    val key: String? = null,
    val pct: Long? = null,
    val resetsAt: Long? = null,
    val resetsAtText: String? = null,
    val resetsAtRelText: String? = null,
    val seenAt: Long,
    val seenAtText: String? = null,
    val seenAtRelText: String? = null,
    val from: QuotaSource,
    val tokensPerPct: Long? = null,
    val perPctText: String? = null,
    val resetSinceSeen: Boolean? = null,
    val tone: WindowShowTone,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<WindowShow> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("key", { opt(Str) }, optional = true, nullable = true),
            Field("pct", { opt(Lng) }, optional = true, nullable = true),
            Field("resetsAt", { opt(Lng) }, optional = true, nullable = true),
            Field("resetsAtText", { opt(Str) }, optional = true, nullable = true),
            Field("resetsAtRelText", { opt(Str) }, optional = true, nullable = true),
            Field("seenAt", { Lng }, optional = false, nullable = false),
            Field("seenAtText", { opt(Str) }, optional = true, nullable = true),
            Field("seenAtRelText", { opt(Str) }, optional = true, nullable = true),
            Field("from", { QuotaSource.ADAPTER }, optional = false, nullable = false),
            Field("tokensPerPct", { opt(Lng) }, optional = true, nullable = true),
            Field("perPctText", { opt(Str) }, optional = true, nullable = true),
            Field("resetSinceSeen", { opt(Bool) }, optional = true, nullable = true),
            Field("tone", { WindowShowTone.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> WindowShow(a[0] as String, a[1] as String?, a[2] as Long?, a[3] as Long?, a[4] as String?, a[5] as String?, a[6] as Long, a[7] as String?, a[8] as String?, a[9] as QuotaSource, a[10] as Long?, a[11] as String?, a[12] as Boolean?, a[13] as WindowShowTone) },
            { v -> arrayOf(v.name, v.key, v.pct, v.resetsAt, v.resetsAtText, v.resetsAtRelText, v.seenAt, v.seenAtText, v.seenAtRelText, v.from, v.tokensPerPct, v.perPctText, v.resetSinceSeen, v.tone) },
        )
    }
}

enum class WindowShowTone(val wire: String) {
    PLAIN("plain"),
    FAIL("fail"),
    WARN("warn"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<WindowShowTone> = enumOf(entries) { it.wire }
    }
}

sealed interface WirePath {
    data class Text(
        val value: String,
    ) : WirePath
    data class Obj(
        val value: WirePathObjValue,
    ) : WirePath

    companion object {
        val ADAPTER: JsonAdapter<WirePath> =
            byToken<WirePath>(
                listOf(
                Tok(JsonReader.Token.STRING, WirePath.Text::class.java, { Str }, { WirePath.Text(it as String) }, { (it as WirePath.Text).value }),
                Tok(JsonReader.Token.BEGIN_OBJECT, WirePath.Obj::class.java, { WirePathObjValue.ADAPTER }, { WirePath.Obj(it as WirePathObjValue) }, { (it as WirePath.Obj).value }),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class WirePathObjValue(
    val b16: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<WirePathObjValue> = obj(
            listOf(
            Field("b16", { Str }, optional = false, nullable = false),
            ),
            { a -> WirePathObjValue(a[0] as String) },
            { v -> arrayOf(v.b16) },
        )
    }
}

enum class WireSessionActivity(val wire: String) {
    WORKING("working"),
    NEEDS_YOU("needs_you"),
    IDLE("idle"),
    BACKGROUND_WORK("background_work"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<WireSessionActivity> = enumOf(entries) { it.wire }
    }
}

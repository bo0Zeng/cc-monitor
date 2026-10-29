// 生成的文件（tests/scripts/gen-kotlin-wire.mjs 读 ts-rs 导出的声明出的）。不要手改：改 Rust 那一份类型，再跑 `npm run gen:types`。
@file:Suppress("ktlint", "detekt:all")

package com.ccmonitor.mobile.core.wire.gen

import com.squareup.moshi.JsonAdapter

import com.squareup.moshi.JsonDataException
import com.squareup.moshi.JsonReader
import com.squareup.moshi.JsonWriter

/** 线上原样的一段 JSON（声明里认不出的那几形）：读进来什么、写回去就是什么。 */
@JvmInline
value class Raw(val json: String) {
    /** 那一段读成 JSON 值（串 · 数 · 真假 · null · 列表 · 键值）。 */
    val value: Any? get() = JsonReader.of(okio.Buffer().writeUtf8(json)).readJsonValue()
}

/**
 * 对象的一格：线上键 · 它的编解码（头一回用到才取：各类的编解码互相引用，不按初始化先后）·
 * 可缺（写回时值为 null 就不写，Rust 那头 skip_serializing_if）· 可空（缺席照 null 读）。
 */
class Field(val key: String, ad: () -> JsonAdapter<*>, val optional: Boolean, val nullable: Boolean) {
    val ad: JsonAdapter<*> by lazy(ad)
}

/** 两端契约对不上（缺格 · 型不对 · 认不出的词）：说出是哪一格。 */
fun mismatch(what: String): JsonDataException = JsonDataException("wire contract mismatch: " + what)

private fun JsonReader.notNull(what: String) {
    if (peek() == JsonReader.Token.NULL) throw mismatch(what + " must not be null (at " + path + ")")
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
            write(writer, value ?: throw mismatch(what + " must not be null"))
        }
    }

val Str: JsonAdapter<String> = prim({ it.nextString() }, { w, v -> w.value(v) }, "string")
val Bool: JsonAdapter<Boolean> = prim({ it.nextBoolean() }, { w, v -> w.value(v) }, "boolean")
val Lng: JsonAdapter<Long> = prim({ it.nextLong() }, { w, v -> w.value(v) }, "integer")
val Int32: JsonAdapter<Int> = prim({ it.nextInt() }, { w, v -> w.value(v) }, "integer")
/** 无符号 64 位：照位存进 Long；读的时候不管它过没过 JSON 数（大于 Long 的照无符号读），写回照无符号的字。 */
val U64: JsonAdapter<Long> =
    prim({ it.nextString().toBigDecimal().toBigInteger().toLong() }, { w, v -> if (v >= 0) w.value(v) else w.valueSink().use { s -> s.writeUtf8(java.lang.Long.toUnsignedString(v)) } }, "integer")
val Dbl: JsonAdapter<Double> = prim({ it.nextDouble() }, { w, v -> w.value(v) }, "number")
/** 那一段原样（null 也是一段）。 */
val RawJson: JsonAdapter<Raw> =
    object : JsonAdapter<Raw>() {
        override fun fromJson(reader: JsonReader): Raw = reader.nextSource().use { Raw(it.readUtf8()) }

        override fun toJson(writer: JsonWriter, value: Raw?) {
            writer.valueSink().use { it.writeUtf8((value ?: throw mismatch("raw json must not be null")).json) }
        }
    }

/** 照原样留着、还没装进线上类型的那一段（带键集合手上那一份逐项存盘 · 用时才各按自己的类型解）。 */
val JsonValue: JsonAdapter<Any> =
    prim({ r -> r.readJsonValue() ?: throw mismatch("value must not be null (at " + r.path + ")") }, { w, v -> w.jsonValue(v) }, "value")

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
    }, "array")

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
    }, "map")

fun <E : Enum<E>> enumOf(all: List<E>, wire: (E) -> String): JsonAdapter<E> =
    prim({ r ->
        val s = r.nextString()
        all.firstOrNull { wire(it) == s } ?: throw mismatch("unknown word \"" + s + "\" (at " + r.path + ")")
    }, { w, v -> w.value(wire(v)) }, "enum word")

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
        reader.notNull("object")
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
            if (!seen[i] && !f.nullable) throw mismatch("missing field \"" + f.key + "\" (at " + reader.path + ")")
        }
        val built = build(a)
        (built as? Ordered)?.wireOrder = order.toIntArray()
        return built
    }

    override fun toJson(writer: JsonWriter, value: T?) {
        writer.beginObject()
        writeFields(writer, value ?: throw mismatch("object must not be null"))
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
        if (s != word) throw mismatch("tag should be \"" + word + "\", got \"" + s + "\" (at " + r.path + ")")
        s
    }, { w, _ -> w.value(word) }, "tag")

/** 带判别格的并集的一形：判别格的那个词 · 它的类 · 其余各格。 */
class Variant<T : Any>(val tag: String, val cls: Class<out T>, val ad: ObjAd<out T>)

/** 带判别格的并集（serde 的 internally tagged）：判别格是各形里的一格（位置照读进来的 · 照声明）。 */
fun <T : Any> tagged(disc: String, variants: List<Variant<T>>): JsonAdapter<T> =
    object : JsonAdapter<T>() {
        override fun fromJson(reader: JsonReader): T {
            reader.notNull("union")
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
                } ?: throw mismatch("missing tag \"" + disc + "\" (at " + reader.path + ")")
            val v = variants.firstOrNull { it.tag == tag } ?: throw mismatch("unknown variant \"" + tag + "\" (at " + reader.path + ")")
            return v.ad.fromJson(reader) as T
        }

        @Suppress("UNCHECKED_CAST")
        override fun toJson(writer: JsonWriter, value: T?) {
            val x = value ?: throw mismatch("union must not be null")
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
            reader.notNull("union")
            reader.beginObject()
            val key = reader.nextName()
            val v = variants.firstOrNull { it.key == key } ?: throw mismatch("unknown variant \"" + key + "\" (at " + reader.path + ")")
            val x = v.wrap(v.ad.fromJson(reader))
            reader.endObject()
            return x
        }

        @Suppress("UNCHECKED_CAST")
        override fun toJson(writer: JsonWriter, value: T?) {
            val x = value ?: throw mismatch("union must not be null")
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
            val v = variants.firstOrNull { it.token == tok } ?: throw mismatch("unexpected " + tok + " (at " + reader.path + ")")
            return v.wrap(v.ad.fromJson(reader))
        }

        @Suppress("UNCHECKED_CAST")
        override fun toJson(writer: JsonWriter, value: T?) {
            val x = value ?: throw mismatch("union must not be null")
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
    }, "tuple")

@Suppress("UNCHECKED_CAST")
data class AccountAddArgs(
    val name: String,
    val kind: AccountKind,
    val credFile: String? = null,
    val baseUrl: String? = null,
    val key: String? = null,
    val isDefault: Boolean? = null,
    val dryRun: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountAddArgs> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("kind", { AccountKind.ADAPTER }, optional = false, nullable = false),
            Field("credFile", { opt(Str) }, optional = true, nullable = true),
            Field("baseUrl", { opt(Str) }, optional = true, nullable = true),
            Field("key", { opt(Str) }, optional = true, nullable = true),
            Field("isDefault", { opt(Bool) }, optional = true, nullable = true),
            Field("dryRun", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> AccountAddArgs(a[0] as String, a[1] as AccountKind, a[2] as String?, a[3] as String?, a[4] as String?, a[5] as Boolean?, a[6] as Boolean?) },
            { v -> arrayOf(v.name, v.kind, v.credFile, v.baseUrl, v.key, v.isDefault, v.dryRun) },
        )
    }
}

sealed interface AccountAsk {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    data object Follow : AccountAsk {
        override val kind: String get() = "follow"
    }
    data object Base : AccountAsk {
        override val kind: String get() = "base"
    }
    @Suppress("UNCHECKED_CAST")
    data class Named(
        val name: String,
    ) : AccountAsk, Ordered {
        override val kind: String get() = "named"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<AccountAsk> =
            tagged<AccountAsk>(
                "kind",
                listOf(
                Variant("follow", AccountAsk.Follow::class.java, obj(
                        listOf(
                        Field("kind", { tag("follow") }, optional = false, nullable = false),
                        ),
                        { AccountAsk.Follow },
                        { arrayOf("follow") },
                    )),
                Variant("base", AccountAsk.Base::class.java, obj(
                        listOf(
                        Field("kind", { tag("base") }, optional = false, nullable = false),
                        ),
                        { AccountAsk.Base },
                        { arrayOf("base") },
                    )),
                Variant("named", AccountAsk.Named::class.java, obj(
                        listOf(
                        Field("kind", { tag("named") }, optional = false, nullable = false),
                        Field("name", { Str }, optional = false, nullable = false),
                        ),
                        { a -> AccountAsk.Named(a[1] as String) },
                        { v -> arrayOf("named", v.name) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class AccountChange(
    val applied: Boolean,
    val steps: List<String>,
    val notes: List<String>,
    val backup: String?,
    val account: AccountRef?,
    val aliasNames: List<String>,
    val keyMasked: String?,
    val keyProblem: String?,
    val aliases: List<AliasChange>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountChange> = obj(
            listOf(
            Field("applied", { Bool }, optional = false, nullable = false),
            Field("steps", { arr(Str) }, optional = false, nullable = false),
            Field("notes", { arr(Str) }, optional = false, nullable = false),
            Field("backup", { opt(Str) }, optional = false, nullable = true),
            Field("account", { opt(AccountRef.ADAPTER) }, optional = false, nullable = true),
            Field("aliasNames", { arr(Str) }, optional = false, nullable = false),
            Field("keyMasked", { opt(Str) }, optional = false, nullable = true),
            Field("keyProblem", { opt(Str) }, optional = false, nullable = true),
            Field("aliases", { arr(AliasChange.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> AccountChange(a[0] as Boolean, a[1] as List<String>, a[2] as List<String>, a[3] as String?, a[4] as AccountRef?, a[5] as List<String>, a[6] as String?, a[7] as String?, a[8] as List<AliasChange>) },
            { v -> arrayOf(v.applied, v.steps, v.notes, v.backup, v.account, v.aliasNames, v.keyMasked, v.keyProblem, v.aliases) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AccountInitArgs(
    val name: String,
    val dryRun: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountInitArgs> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("dryRun", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> AccountInitArgs(a[0] as String, a[1] as Boolean?) },
            { v -> arrayOf(v.name, v.dryRun) },
        )
    }
}

enum class AccountKind(val wire: String) {
    SUBSCRIPTION("subscription"),
    API_KEY("api-key"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<AccountKind> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class AccountLoginCmd(
    val cmd: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountLoginCmd> = obj(
            listOf(
            Field("cmd", { Str }, optional = false, nullable = false),
            ),
            { a -> AccountLoginCmd(a[0] as String) },
            { v -> arrayOf(v.cmd) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AccountMcpChoice(
    val from: String?,
    val holders: List<String>,
    val gone: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountMcpChoice> = obj(
            listOf(
            Field("from", { opt(Str) }, optional = false, nullable = true),
            Field("holders", { arr(Str) }, optional = false, nullable = false),
            Field("gone", { Bool }, optional = false, nullable = false),
            ),
            { a -> AccountMcpChoice(a[0] as String?, a[1] as List<String>, a[2] as Boolean) },
            { v -> arrayOf(v.from, v.holders, v.gone) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AccountMcpConflict(
    val name: String,
    val choices: List<AccountMcpChoice>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountMcpConflict> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("choices", { arr(AccountMcpChoice.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> AccountMcpConflict(a[0] as String, a[1] as List<AccountMcpChoice>) },
            { v -> arrayOf(v.name, v.choices) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AccountMcpNameArgs(
    val name: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountMcpNameArgs> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            ),
            { a -> AccountMcpNameArgs(a[0] as String) },
            { v -> arrayOf(v.name) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AccountMcpPickArgs(
    val name: String,
    val from: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountMcpPickArgs> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("from", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> AccountMcpPickArgs(a[0] as String, a[1] as String?) },
            { v -> arrayOf(v.name, v.from) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AccountMcpSyncArgs(
    val on: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountMcpSyncArgs> = obj(
            listOf(
            Field("on", { Bool }, optional = false, nullable = false),
            ),
            { a -> AccountMcpSyncArgs(a[0] as Boolean) },
            { v -> arrayOf(v.on) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AccountMcpView(
    val enabled: Boolean,
    val sync: Boolean,
    val servers: List<String>,
    val conflicts: List<AccountMcpConflict>,
    val changed: List<String>,
    val notes: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountMcpView> = obj(
            listOf(
            Field("enabled", { Bool }, optional = false, nullable = false),
            Field("sync", { Bool }, optional = false, nullable = false),
            Field("servers", { arr(Str) }, optional = false, nullable = false),
            Field("conflicts", { arr(AccountMcpConflict.ADAPTER) }, optional = false, nullable = false),
            Field("changed", { arr(Str) }, optional = false, nullable = false),
            Field("notes", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> AccountMcpView(a[0] as Boolean, a[1] as Boolean, a[2] as List<String>, a[3] as List<AccountMcpConflict>, a[4] as List<String>, a[5] as List<String>) },
            { v -> arrayOf(v.enabled, v.sync, v.servers, v.conflicts, v.changed, v.notes) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AccountNameArgs(
    val name: String,
    val dryRun: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountNameArgs> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("dryRun", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> AccountNameArgs(a[0] as String, a[1] as Boolean?) },
            { v -> arrayOf(v.name, v.dryRun) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AccountRef(
    val name: String,
    val configDir: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountRef> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("configDir", { Str }, optional = false, nullable = false),
            ),
            { a -> AccountRef(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.name, v.configDir) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AccountRemoveArgs(
    val name: String,
    val force: Boolean? = null,
    val dryRun: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountRemoveArgs> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("force", { opt(Bool) }, optional = true, nullable = true),
            Field("dryRun", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> AccountRemoveArgs(a[0] as String, a[1] as Boolean?, a[2] as Boolean?) },
            { v -> arrayOf(v.name, v.force, v.dryRun) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AccountRepairArgs(
    val dryRun: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountRepairArgs> = obj(
            listOf(
            Field("dryRun", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> AccountRepairArgs(a[0] as Boolean?) },
            { v -> arrayOf(v.dryRun) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AccountRollbackArgs(
    val backup: String? = null,
    val dryRun: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountRollbackArgs> = obj(
            listOf(
            Field("backup", { opt(Str) }, optional = true, nullable = true),
            Field("dryRun", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> AccountRollbackArgs(a[0] as String?, a[1] as Boolean?) },
            { v -> arrayOf(v.backup, v.dryRun) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AccountRow(
    val configDirShown: String?,
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
    val color: Int,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountRow> = obj(
            listOf(
            Field("configDirShown", { opt(Str) }, optional = false, nullable = true),
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
            Field("color", { Int32 }, optional = false, nullable = false),
            ),
            { a -> AccountRow(a[0] as String?, a[1] as String?, a[2] as String?, a[3] as Badge, a[4] as String, a[5] as String?, a[6] as String?, a[7] as Boolean, a[8] as Boolean, a[9] as String, a[10] as String, a[11] as String, a[12] as String?, a[13] as Boolean, a[14] as String, a[15] as Boolean, a[16] as Boolean, a[17] as String, a[18] as Boolean, a[19] as Boolean, a[20] as Int) },
            { v -> arrayOf(v.configDirShown, v.keyMasked, v.baseUrl, v.badge, v.plate, v.quota, v.unselectable, v.login, v.newSession, v.name, v.label, v.email, v.configDir, v.isDefault, v.mode, v.exists, v.loggedIn, v.authKind, v.authReady, v.selectable, v.color) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AccountUnavailable(
    val requested: String,
    val pinned: Boolean,
    val listKnown: Boolean,
    val alternative: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountUnavailable> = obj(
            listOf(
            Field("requested", { Str }, optional = false, nullable = false),
            Field("pinned", { Bool }, optional = false, nullable = false),
            Field("listKnown", { Bool }, optional = false, nullable = false),
            Field("alternative", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> AccountUnavailable(a[0] as String, a[1] as Boolean, a[2] as Boolean, a[3] as String?) },
            { v -> arrayOf(v.requested, v.pinned, v.listKnown, v.alternative) },
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
data class AccountsListReq(
    val agent: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AccountsListReq> = obj(
            listOf(
            Field("agent", { Str }, optional = false, nullable = false),
            ),
            { a -> AccountsListReq(a[0] as String) },
            { v -> arrayOf(v.agent) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AckReq(
    val ticket: String,
    val seq: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AckReq> = obj(
            listOf(
            Field("ticket", { Str }, optional = false, nullable = false),
            Field("seq", { Lng }, optional = false, nullable = false),
            ),
            { a -> AckReq(a[0] as String, a[1] as Long) },
            { v -> arrayOf(v.ticket, v.seq) },
        )
    }
}

enum class ActionTag(val wire: String) {
    SKIP("skip"),
    DEPLOY("deploy"),
    KEEP("keep"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ActionTag> = enumOf(entries) { it.wire }
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
data class AgentHomeCheck(
    val state: AgentHomeState,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AgentHomeCheck> = obj(
            listOf(
            Field("state", { AgentHomeState.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> AgentHomeCheck(a[0] as AgentHomeState) },
            { v -> arrayOf(v.state) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AgentHomeCheckReq(
    val path: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AgentHomeCheckReq> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            ),
            { a -> AgentHomeCheckReq(a[0] as String) },
            { v -> arrayOf(v.path) },
        )
    }
}

enum class AgentHomeState(val wire: String) {
    OK("ok"),
    MISSING("missing"),
    NOT_DIR("not_dir"),
    NO_RECORDS("no_records"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<AgentHomeState> = enumOf(entries) { it.wire }
    }
}

typealias AgentKind = String

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

@Suppress("UNCHECKED_CAST")
data class AliasBlockState(
    val present: Boolean,
    val conflictingFunctions: List<AliasNameClash>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AliasBlockState> = obj(
            listOf(
            Field("present", { Bool }, optional = false, nullable = false),
            Field("conflictingFunctions", { arr(AliasNameClash.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> AliasBlockState(a[0] as Boolean, a[1] as List<AliasNameClash>) },
            { v -> arrayOf(v.present, v.conflictingFunctions) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AliasChange(
    val path: String,
    val changed: Boolean,
    val added: List<String>,
    val removed: List<String>,
    val skipped: List<String>,
    val note: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AliasChange> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("changed", { Bool }, optional = false, nullable = false),
            Field("added", { arr(Str) }, optional = false, nullable = false),
            Field("removed", { arr(Str) }, optional = false, nullable = false),
            Field("skipped", { arr(Str) }, optional = false, nullable = false),
            Field("note", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> AliasChange(a[0] as String, a[1] as Boolean, a[2] as List<String>, a[3] as List<String>, a[4] as List<String>, a[5] as String?) },
            { v -> arrayOf(v.path, v.changed, v.added, v.removed, v.skipped, v.note) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AliasListing(
    val rcCandidates: List<AliasStartupFile>,
    val otherRc: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AliasListing> = obj(
            listOf(
            Field("rcCandidates", { arr(AliasStartupFile.ADAPTER) }, optional = false, nullable = false),
            Field("otherRc", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> AliasListing(a[0] as List<AliasStartupFile>, a[1] as String?) },
            { v -> arrayOf(v.rcCandidates, v.otherRc) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AliasNameClash(
    val name: String,
    val line: Long,
    val wins: AliasWins,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AliasNameClash> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("line", { Lng }, optional = false, nullable = false),
            Field("wins", { AliasWins.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> AliasNameClash(a[0] as String, a[1] as Long, a[2] as AliasWins) },
            { v -> arrayOf(v.name, v.line, v.wins) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AliasStartupFile(
    val path: String,
    val shown: String,
    val sourced: Boolean,
    val exists: Boolean,
    val block: AliasBlockState,
    val unreadable: String?,
    val policy: PsExecPolicy?,
    val blockLines: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AliasStartupFile> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("shown", { Str }, optional = false, nullable = false),
            Field("sourced", { Bool }, optional = false, nullable = false),
            Field("exists", { Bool }, optional = false, nullable = false),
            Field("block", { AliasBlockState.ADAPTER }, optional = false, nullable = false),
            Field("unreadable", { opt(Str) }, optional = false, nullable = true),
            Field("policy", { opt(PsExecPolicy.ADAPTER) }, optional = false, nullable = true),
            Field("blockLines", { Lng }, optional = false, nullable = false),
            ),
            { a -> AliasStartupFile(a[0] as String, a[1] as String, a[2] as Boolean, a[3] as Boolean, a[4] as AliasBlockState, a[5] as String?, a[6] as PsExecPolicy?, a[7] as Long) },
            { v -> arrayOf(v.path, v.shown, v.sourced, v.exists, v.block, v.unreadable, v.policy, v.blockLines) },
        )
    }
}

enum class AliasWins(val wire: String) {
    YOURS("yours"),
    LIST("list"),
    UNCLEAR("unclear"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<AliasWins> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class AliasesReadReq(
    val shell: ShellKind,
    val rcPath: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AliasesReadReq> = obj(
            listOf(
            Field("shell", { ShellKind.ADAPTER }, optional = false, nullable = false),
            Field("rcPath", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> AliasesReadReq(a[0] as ShellKind, a[1] as String?) },
            { v -> arrayOf(v.shell, v.rcPath) },
        )
    }
}

sealed interface Allow {
    /** 判别格（`can`）那个词。 */
    val can: String

    data object Yes : Allow {
        override val can: String get() = "yes"
    }
    @Suppress("UNCHECKED_CAST")
    data class No(
        val why: String,
    ) : Allow, Ordered {
        override val can: String get() = "no"
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

@Suppress("UNCHECKED_CAST")
data class AnnotateReq(
    val sid: String,
    val patch: Patch,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AnnotateReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("patch", { Patch.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> AnnotateReq(a[0] as String, a[1] as Patch) },
            { v -> arrayOf(v.sid, v.patch) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Annotated(
    val entry: Entry,
    val prev: Hide?,
    val prevTitle: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Annotated> = obj(
            listOf(
            Field("entry", { Entry.ADAPTER }, optional = false, nullable = false),
            Field("prev", { opt(Hide.ADAPTER) }, optional = false, nullable = true),
            Field("prev_title", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> Annotated(a[0] as Entry, a[1] as Hide?, a[2] as String?) },
            { v -> arrayOf(v.entry, v.prev, v.prevTitle) },
        )
    }
}

sealed interface Answer {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    data object Approved : Answer {
        override val kind: String get() = "approved"
    }
    @Suppress("UNCHECKED_CAST")
    data class Picked(
        val options: List<String>,
    ) : Answer, Ordered {
        override val kind: String get() = "picked"
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

@Suppress("UNCHECKED_CAST")
data class AnswerReq(
    val sid: String,
    val ask: String,
    val choice: String,
    val note: String? = null,
    val picks: Map<String, AskPick>? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AnswerReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("ask", { Str }, optional = false, nullable = false),
            Field("choice", { Str }, optional = false, nullable = false),
            Field("note", { opt(Str) }, optional = true, nullable = true),
            Field("picks", { opt(dict(AskPick.ADAPTER)) }, optional = true, nullable = true),
            ),
            { a -> AnswerReq(a[0] as String, a[1] as String, a[2] as String, a[3] as String?, a[4] as Map<String, AskPick>?) },
            { v -> arrayOf(v.sid, v.ask, v.choice, v.note, v.picks) },
        )
    }
}

enum class AnswerResult(val wire: String) {
    ANSWERED("answered"),
    GONE("gone"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<AnswerResult> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class Answered(
    val result: AnswerResult,
    val said: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Answered> = obj(
            listOf(
            Field("result", { AnswerResult.ADAPTER }, optional = false, nullable = false),
            Field("said", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> Answered(a[0] as AnswerResult, a[1] as String?) },
            { v -> arrayOf(v.result, v.said) },
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

enum class Arrival(val wire: String) {
    ARRIVED("arrived"),
    MISSED("missed"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<Arrival> = enumOf(entries) { it.wire }
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
            Field("since", { U64 }, optional = false, nullable = false),
            Field("session", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ArriveAsk(a[0] as ArriveBy, a[1] as Long, a[2] as String?) },
            { v -> arrayOf(v.by, v.since, v.session) },
        )
    }
}

sealed interface ArriveBy {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    @Suppress("UNCHECKED_CAST")
    data class Sid(
        val sid: String,
    ) : ArriveBy, Ordered {
        override val kind: String get() = "sid"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Cwd(
        val cwd: String,
    ) : ArriveBy, Ordered {
        override val kind: String get() = "cwd"
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

sealed interface ArriveEnd {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    @Suppress("UNCHECKED_CAST")
    data class Arrived(
        val sid: String,
    ) : ArriveEnd, Ordered {
        override val kind: String get() = "arrived"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Missed(
        val said: String,
        val terminal: String?,
    ) : ArriveEnd, Ordered {
        override val kind: String get() = "missed"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<ArriveEnd> =
            tagged<ArriveEnd>(
                "kind",
                listOf(
                Variant("arrived", ArriveEnd.Arrived::class.java, obj(
                        listOf(
                        Field("kind", { tag("arrived") }, optional = false, nullable = false),
                        Field("sid", { Str }, optional = false, nullable = false),
                        ),
                        { a -> ArriveEnd.Arrived(a[1] as String) },
                        { v -> arrayOf("arrived", v.sid) },
                    )),
                Variant("missed", ArriveEnd.Missed::class.java, obj(
                        listOf(
                        Field("kind", { tag("missed") }, optional = false, nullable = false),
                        Field("said", { Str }, optional = false, nullable = false),
                        Field("terminal", { opt(Str) }, optional = false, nullable = true),
                        ),
                        { a -> ArriveEnd.Missed(a[1] as String, a[2] as String?) },
                        { v -> arrayOf("missed", v.said, v.terminal) },
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
data class AskPick(
    val options: List<String>? = null,
    val text: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AskPick> = obj(
            listOf(
            Field("options", { opt(arr(Str)) }, optional = true, nullable = true),
            Field("text", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> AskPick(a[0] as List<String>?, a[1] as String?) },
            { v -> arrayOf(v.options, v.text) },
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
data class AskReply(
    val decision: Raw,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AskReply> = obj(
            listOf(
            Field("decision", { RawJson }, optional = false, nullable = false),
            ),
            { a -> AskReply(a[0] as Raw) },
            { v -> arrayOf(v.decision) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AskReq(
    val agent: String,
    val hook: Raw,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AskReq> = obj(
            listOf(
            Field("agent", { Str }, optional = false, nullable = false),
            Field("hook", { RawJson }, optional = false, nullable = false),
            ),
            { a -> AskReq(a[0] as String, a[1] as Raw) },
            { v -> arrayOf(v.agent, v.hook) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AssetsSyncReq(
    val origin: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AssetsSyncReq> = obj(
            listOf(
            Field("origin", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> AssetsSyncReq(a[0] as String?) },
            { v -> arrayOf(v.origin) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AssetsSyncRow(
    val origin: String,
    val peer: String?,
    val changed: Boolean,
    val pushed: Long,
    val error: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AssetsSyncRow> = obj(
            listOf(
            Field("origin", { Str }, optional = false, nullable = false),
            Field("peer", { opt(Str) }, optional = false, nullable = true),
            Field("changed", { Bool }, optional = false, nullable = false),
            Field("pushed", { Lng }, optional = false, nullable = false),
            Field("error", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> AssetsSyncRow(a[0] as String, a[1] as String?, a[2] as Boolean, a[3] as Long, a[4] as String?) },
            { v -> arrayOf(v.origin, v.peer, v.changed, v.pushed, v.error) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AssetsSynced(
    val self: String,
    val synced: List<AssetsSyncRow>,
    val reach: List<ReachRow>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AssetsSynced> = obj(
            listOf(
            Field("self", { Str }, optional = false, nullable = false),
            Field("synced", { arr(AssetsSyncRow.ADAPTER) }, optional = false, nullable = false),
            Field("reach", { arr(ReachRow.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> AssetsSynced(a[0] as String, a[1] as List<AssetsSyncRow>, a[2] as List<ReachRow>) },
            { v -> arrayOf(v.self, v.synced, v.reach) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AttachItem(
    val name: String,
    val bytes: Long,
    val dir: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AttachItem> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("bytes", { U64 }, optional = false, nullable = false),
            Field("dir", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> AttachItem(a[0] as String, a[1] as Long, a[2] as Boolean?) },
            { v -> arrayOf(v.name, v.bytes, v.dir) },
        )
    }
}

enum class AttachRefused(val wire: String) {
    IS_DIR("is_dir"),
    TOO_BIG("too_big"),
    TOO_MANY("too_many"),
    TOO_MUCH("too_much"),
    DIR_FULL("dir_full"),
    NOT_WRITABLE("not_writable"),
    NO_SESSION("no_session"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<AttachRefused> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class AttachVerdict(
    val ok: Boolean,
    val `class`: FileClass,
    val classText: String,
    val why: AttachRefused? = null,
    val said: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AttachVerdict> = obj(
            listOf(
            Field("ok", { Bool }, optional = false, nullable = false),
            Field("class", { FileClass.ADAPTER }, optional = false, nullable = false),
            Field("class_text", { Str }, optional = false, nullable = false),
            Field("why", { opt(AttachRefused.ADAPTER) }, optional = true, nullable = true),
            Field("said", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> AttachVerdict(a[0] as Boolean, a[1] as FileClass, a[2] as String, a[3] as AttachRefused?, a[4] as String?) },
            { v -> arrayOf(v.ok, v.`class`, v.classText, v.why, v.said) },
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
            Field("since", { U64 }, optional = false, nullable = false),
            Field("last_activity", { U64 }, optional = false, nullable = false),
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
data class Attachment(
    val name: String,
    val rel: String,
    val `class`: FileClass,
    val classText: String,
    val bytes: Long? = null,
    val sizeText: String? = null,
    val gone: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Attachment> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("rel", { Str }, optional = false, nullable = false),
            Field("class", { FileClass.ADAPTER }, optional = false, nullable = false),
            Field("classText", { Str }, optional = false, nullable = false),
            Field("bytes", { opt(U64) }, optional = true, nullable = true),
            Field("sizeText", { opt(Str) }, optional = true, nullable = true),
            Field("gone", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> Attachment(a[0] as String, a[1] as String, a[2] as FileClass, a[3] as String, a[4] as Long?, a[5] as String?, a[6] as Boolean?) },
            { v -> arrayOf(v.name, v.rel, v.`class`, v.classText, v.bytes, v.sizeText, v.gone) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AutoAnswer(
    val afterMin: Int,
    val say: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AutoAnswer> = obj(
            listOf(
            Field("afterMin", { Int32 }, optional = false, nullable = false),
            Field("say", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> AutoAnswer(a[0] as Int, a[1] as String?) },
            { v -> arrayOf(v.afterMin, v.say) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class AutoLaunchConfig(
    val autoLaunchEnabled: Boolean,
    val monitorExePath: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<AutoLaunchConfig> = obj(
            listOf(
            Field("auto_launch_enabled", { Bool }, optional = false, nullable = false),
            Field("monitor_exe_path", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> AutoLaunchConfig(a[0] as Boolean, a[1] as String?) },
            { v -> arrayOf(v.autoLaunchEnabled, v.monitorExePath) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BackendLogReq(
    val maxBytes: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BackendLogReq> = obj(
            listOf(
            Field("maxBytes", { opt(Lng) }, optional = true, nullable = true),
            ),
            { a -> BackendLogReq(a[0] as Long?) },
            { v -> arrayOf(v.maxBytes) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BackendStatus(
    val local: LocalBackend?,
    val origin: String,
    val channel: Boolean,
    val pid: Long?,
    val attempts: Long?,
    val detached: Boolean?,
    val health: HealthFace,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BackendStatus> = obj(
            listOf(
            Field("local", { opt(LocalBackend.ADAPTER) }, optional = false, nullable = true),
            Field("origin", { Str }, optional = false, nullable = false),
            Field("channel", { Bool }, optional = false, nullable = false),
            Field("pid", { opt(Lng) }, optional = false, nullable = true),
            Field("attempts", { opt(Lng) }, optional = false, nullable = true),
            Field("detached", { opt(Bool) }, optional = false, nullable = true),
            Field("health", { HealthFace.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> BackendStatus(a[0] as LocalBackend?, a[1] as String, a[2] as Boolean, a[3] as Long?, a[4] as Long?, a[5] as Boolean?, a[6] as HealthFace) },
            { v -> arrayOf(v.local, v.origin, v.channel, v.pid, v.attempts, v.detached, v.health) },
        )
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

sealed interface BatchAnswer {
    data class All(
        val value: Decide,
    ) : BatchAnswer
    data class Each(
        val value: Map<String, Decide>,
    ) : BatchAnswer

    companion object {
        val ADAPTER: JsonAdapter<BatchAnswer> =
            external<BatchAnswer>(
                listOf(
                Ext("all", BatchAnswer.All::class.java, { Decide.ADAPTER }, { BatchAnswer.All(it as Decide) }, { (it as BatchAnswer.All).value }),
                Ext("each", BatchAnswer.Each::class.java, { dict(Decide.ADAPTER) }, { BatchAnswer.Each(it as Map<String, Decide>) }, { (it as BatchAnswer.Each).value }),
                ),
            )
    }
}

enum class BatchEdge(val wire: String) {
    START("start"),
    END("end"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<BatchEdge> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class BatchKeep(
    val ticket: String,
    val plan: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BatchKeep> = obj(
            listOf(
            Field("ticket", { Str }, optional = false, nullable = false),
            Field("plan", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> BatchKeep(a[0] as String, a[1] as String?) },
            { v -> arrayOf(v.ticket, v.plan) },
        )
    }
}

sealed interface BatchLine {
    /** 判别格（`phase`）那个词。 */
    val phase: String

    val id: Long

    val title: String

    val kept: Boolean

    @Suppress("UNCHECKED_CAST")
    data class Running(
        override val id: Long,
        override val title: String,
        override val kept: Boolean,
        val current: String?,
        val frac: Double?,
    ) : BatchLine, Ordered {
        override val phase: String get() = "running"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Asking(
        override val id: Long,
        override val title: String,
        override val kept: Boolean,
        val conflicts: List<ClashRow>,
        val go: String,
    ) : BatchLine, Ordered {
        override val phase: String get() = "asking"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Cut(
        override val id: Long,
        override val title: String,
        override val kept: Boolean,
        val line: String,
        val resume: Boolean,
    ) : BatchLine, Ordered {
        override val phase: String get() = "cut"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Ended(
        override val id: Long,
        override val title: String,
        override val kept: Boolean,
        val said: String,
        val failedTitle: String,
        val failed: List<String>,
        val details: String?,
        val retry: Boolean,
        val undo: Boolean,
    ) : BatchLine, Ordered {
        override val phase: String get() = "ended"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Failed(
        override val id: Long,
        override val title: String,
        override val kept: Boolean,
        val said: String,
        val details: String?,
    ) : BatchLine, Ordered {
        override val phase: String get() = "failed"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Dropped(
        override val id: Long,
        override val title: String,
        override val kept: Boolean,
    ) : BatchLine, Ordered {
        override val phase: String get() = "dropped"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<BatchLine> =
            tagged<BatchLine>(
                "phase",
                listOf(
                Variant("running", BatchLine.Running::class.java, obj(
                        listOf(
                        Field("phase", { tag("running") }, optional = false, nullable = false),
                        Field("id", { Lng }, optional = false, nullable = false),
                        Field("title", { Str }, optional = false, nullable = false),
                        Field("kept", { Bool }, optional = false, nullable = false),
                        Field("current", { opt(Str) }, optional = false, nullable = true),
                        Field("frac", { opt(Dbl) }, optional = false, nullable = true),
                        ),
                        { a -> BatchLine.Running(a[1] as Long, a[2] as String, a[3] as Boolean, a[4] as String?, a[5] as Double?) },
                        { v -> arrayOf("running", v.id, v.title, v.kept, v.current, v.frac) },
                    )),
                Variant("asking", BatchLine.Asking::class.java, obj(
                        listOf(
                        Field("phase", { tag("asking") }, optional = false, nullable = false),
                        Field("id", { Lng }, optional = false, nullable = false),
                        Field("title", { Str }, optional = false, nullable = false),
                        Field("kept", { Bool }, optional = false, nullable = false),
                        Field("conflicts", { arr(ClashRow.ADAPTER) }, optional = false, nullable = false),
                        Field("go", { Str }, optional = false, nullable = false),
                        ),
                        { a -> BatchLine.Asking(a[1] as Long, a[2] as String, a[3] as Boolean, a[4] as List<ClashRow>, a[5] as String) },
                        { v -> arrayOf("asking", v.id, v.title, v.kept, v.conflicts, v.go) },
                    )),
                Variant("cut", BatchLine.Cut::class.java, obj(
                        listOf(
                        Field("phase", { tag("cut") }, optional = false, nullable = false),
                        Field("id", { Lng }, optional = false, nullable = false),
                        Field("title", { Str }, optional = false, nullable = false),
                        Field("kept", { Bool }, optional = false, nullable = false),
                        Field("line", { Str }, optional = false, nullable = false),
                        Field("resume", { Bool }, optional = false, nullable = false),
                        ),
                        { a -> BatchLine.Cut(a[1] as Long, a[2] as String, a[3] as Boolean, a[4] as String, a[5] as Boolean) },
                        { v -> arrayOf("cut", v.id, v.title, v.kept, v.line, v.resume) },
                    )),
                Variant("ended", BatchLine.Ended::class.java, obj(
                        listOf(
                        Field("phase", { tag("ended") }, optional = false, nullable = false),
                        Field("id", { Lng }, optional = false, nullable = false),
                        Field("title", { Str }, optional = false, nullable = false),
                        Field("kept", { Bool }, optional = false, nullable = false),
                        Field("said", { Str }, optional = false, nullable = false),
                        Field("failed_title", { Str }, optional = false, nullable = false),
                        Field("failed", { arr(Str) }, optional = false, nullable = false),
                        Field("details", { opt(Str) }, optional = false, nullable = true),
                        Field("retry", { Bool }, optional = false, nullable = false),
                        Field("undo", { Bool }, optional = false, nullable = false),
                        ),
                        { a -> BatchLine.Ended(a[1] as Long, a[2] as String, a[3] as Boolean, a[4] as String, a[5] as String, a[6] as List<String>, a[7] as String?, a[8] as Boolean, a[9] as Boolean) },
                        { v -> arrayOf("ended", v.id, v.title, v.kept, v.said, v.failedTitle, v.failed, v.details, v.retry, v.undo) },
                    )),
                Variant("failed", BatchLine.Failed::class.java, obj(
                        listOf(
                        Field("phase", { tag("failed") }, optional = false, nullable = false),
                        Field("id", { Lng }, optional = false, nullable = false),
                        Field("title", { Str }, optional = false, nullable = false),
                        Field("kept", { Bool }, optional = false, nullable = false),
                        Field("said", { Str }, optional = false, nullable = false),
                        Field("details", { opt(Str) }, optional = false, nullable = true),
                        ),
                        { a -> BatchLine.Failed(a[1] as Long, a[2] as String, a[3] as Boolean, a[4] as String, a[5] as String?) },
                        { v -> arrayOf("failed", v.id, v.title, v.kept, v.said, v.details) },
                    )),
                Variant("dropped", BatchLine.Dropped::class.java, obj(
                        listOf(
                        Field("phase", { tag("dropped") }, optional = false, nullable = false),
                        Field("id", { Lng }, optional = false, nullable = false),
                        Field("title", { Str }, optional = false, nullable = false),
                        Field("kept", { Bool }, optional = false, nullable = false),
                        ),
                        { a -> BatchLine.Dropped(a[1] as Long, a[2] as String, a[3] as Boolean) },
                        { v -> arrayOf("dropped", v.id, v.title, v.kept) },
                    )),
                ),
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
    val before: Int? = null,
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
            Field("bytes", { opt(U64) }, optional = true, nullable = true),
            Field("files", { opt(U64) }, optional = true, nullable = true),
            Field("dirs", { opt(U64) }, optional = true, nullable = true),
            Field("removed", { opt(U64) }, optional = true, nullable = true),
            Field("before", { opt(Int32) }, optional = true, nullable = true),
            Field("code", { opt(Str) }, optional = true, nullable = true),
            Field("reason", { opt(Str) }, optional = true, nullable = true),
            Field("detail", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> BatchOutcome(a[0] as String, a[1] as ItemState, a[2] as WirePath?, a[3] as WirePath?, a[4] as Long?, a[5] as Long?, a[6] as Long?, a[7] as Long?, a[8] as Int?, a[9] as String?, a[10] as String?, a[11] as String?) },
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

@Suppress("UNCHECKED_CAST")
data class Batched(
    val results: List<Outcome>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Batched> = obj(
            listOf(
            Field("results", { arr(Outcome.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> Batched(a[0] as List<Outcome>) },
            { v -> arrayOf(v.results) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BgTask(
    val call: String,
    val task: String?,
    val cmd: String?,
    val at: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BgTask> = obj(
            listOf(
            Field("call", { Str }, optional = false, nullable = false),
            Field("task", { opt(Str) }, optional = false, nullable = true),
            Field("cmd", { opt(Str) }, optional = false, nullable = true),
            Field("at", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> BgTask(a[0] as String, a[1] as String?, a[2] as String?, a[3] as String?) },
            { v -> arrayOf(v.call, v.task, v.cmd, v.at) },
        )
    }
}

sealed interface Block {
    /** 判别格（`type`）那个词。 */
    val `type`: String

    @Suppress("UNCHECKED_CAST")
    data class Text(
        val text: String,
    ) : Block, Ordered {
        override val `type`: String get() = "text"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Thinking(
        val text: String,
    ) : Block, Ordered {
        override val `type`: String get() = "thinking"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class ToolUse(
        val id: String,
        val name: String,
        val input: Raw,
    ) : Block, Ordered {
        override val `type`: String get() = "tool_use"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class ToolResult(
        val `for`: String,
        val content: List<Block>,
        val isError: Boolean,
    ) : Block, Ordered {
        override val `type`: String get() = "tool_result"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Image(
        val mediaType: String,
        val bytes: Long,
        val at: Long,
        val n: Int,
    ) : Block, Ordered {
        override val `type`: String get() = "image"
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
                        Field("mediaType", { Str }, optional = false, nullable = false),
                        Field("bytes", { U64 }, optional = false, nullable = false),
                        Field("at", { U64 }, optional = false, nullable = false),
                        Field("n", { Int32 }, optional = false, nullable = false),
                        ),
                        { a -> Block.Image(a[1] as String, a[2] as Long, a[3] as Long, a[4] as Int) },
                        { v -> arrayOf("image", v.mediaType, v.bytes, v.at, v.n) },
                    )),
                ),
            )
    }
}

data object BlockDone {
    val ADAPTER: JsonAdapter<BlockDone> = obj(emptyList(), { BlockDone }, { emptyArray() })
}

enum class BlockKind(val wire: String) {
    TEXT("text"),
    THINKING("thinking"),
    TOOL("tool"),
    OTHER("other"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<BlockKind> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class BlockReq(
    val rcPath: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BlockReq> = obj(
            listOf(
            Field("rcPath", { Str }, optional = false, nullable = false),
            ),
            { a -> BlockReq(a[0] as String) },
            { v -> arrayOf(v.rcPath) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BlockText(
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BlockText> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> BlockText(a[0] as String) },
            { v -> arrayOf(v.text) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BoardDown(
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BoardDown> = obj(
            listOf(
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> BoardDown(a[0] as String) },
            { v -> arrayOf(v.said) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BoardFilter(
    val tag: BoardTag,
    val label: String,
    val count: Int,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BoardFilter> = obj(
            listOf(
            Field("tag", { BoardTag.ADAPTER }, optional = false, nullable = false),
            Field("label", { Str }, optional = false, nullable = false),
            Field("count", { Int32 }, optional = false, nullable = false),
            ),
            { a -> BoardFilter(a[0] as BoardTag, a[1] as String, a[2] as Int) },
            { v -> arrayOf(v.tag, v.label, v.count) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BoardGroup(
    val machine: String,
    val name: String,
    val sub: String,
    val down: BoardDown? = null,
    val complete: Boolean,
    val rows: List<BoardRow>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BoardGroup> = obj(
            listOf(
            Field("machine", { Str }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("sub", { Str }, optional = false, nullable = false),
            Field("down", { opt(BoardDown.ADAPTER) }, optional = true, nullable = true),
            Field("complete", { Bool }, optional = false, nullable = false),
            Field("rows", { arr(BoardRow.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> BoardGroup(a[0] as String, a[1] as String, a[2] as String, a[3] as BoardDown?, a[4] as Boolean, a[5] as List<BoardRow>) },
            { v -> arrayOf(v.machine, v.name, v.sub, v.down, v.complete, v.rows) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BoardHere(
    val rows: List<BoardRow>,
    val complete: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BoardHere> = obj(
            listOf(
            Field("rows", { arr(BoardRow.ADAPTER) }, optional = false, nullable = false),
            Field("complete", { Bool }, optional = false, nullable = false),
            ),
            { a -> BoardHere(a[0] as List<BoardRow>, a[1] as Boolean) },
            { v -> arrayOf(v.rows, v.complete) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BoardHereReq(
    val client: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BoardHereReq> = obj(
            listOf(
            Field("client", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> BoardHereReq(a[0] as String?) },
            { v -> arrayOf(v.client) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BoardReq(
    val machines: List<String>,
    val client: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BoardReq> = obj(
            listOf(
            Field("machines", { arr(Str) }, optional = false, nullable = false),
            Field("client", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> BoardReq(a[0] as List<String>, a[1] as String?) },
            { v -> arrayOf(v.machines, v.client) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BoardRow(
    val key: String,
    val terminal: String,
    val name: String,
    val sid: String? = null,
    val dot: SessionDot,
    val dotName: String,
    val title: String,
    val windowTitle: String,
    val mono: Boolean,
    val badge: String? = null,
    val meta: String,
    val chips: List<String>,
    val atText: String,
    val tags: List<BoardTag>,
    val find: String,
    val canEnd: BoardRowCanEnd,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BoardRow> = obj(
            listOf(
            Field("key", { Str }, optional = false, nullable = false),
            Field("terminal", { Str }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("sid", { opt(Str) }, optional = true, nullable = true),
            Field("dot", { SessionDot.ADAPTER }, optional = false, nullable = false),
            Field("dot_name", { Str }, optional = false, nullable = false),
            Field("title", { Str }, optional = false, nullable = false),
            Field("window_title", { Str }, optional = false, nullable = false),
            Field("mono", { Bool }, optional = false, nullable = false),
            Field("badge", { opt(Str) }, optional = true, nullable = true),
            Field("meta", { Str }, optional = false, nullable = false),
            Field("chips", { arr(Str) }, optional = false, nullable = false),
            Field("at_text", { Str }, optional = false, nullable = false),
            Field("tags", { arr(BoardTag.ADAPTER) }, optional = false, nullable = false),
            Field("find", { Str }, optional = false, nullable = false),
            Field("can_end", { BoardRowCanEnd.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> BoardRow(a[0] as String, a[1] as String, a[2] as String, a[3] as String?, a[4] as SessionDot, a[5] as String, a[6] as String, a[7] as String, a[8] as Boolean, a[9] as String?, a[10] as String, a[11] as List<String>, a[12] as String, a[13] as List<BoardTag>, a[14] as String, a[15] as BoardRowCanEnd) },
            { v -> arrayOf(v.key, v.terminal, v.name, v.sid, v.dot, v.dotName, v.title, v.windowTitle, v.mono, v.badge, v.meta, v.chips, v.atText, v.tags, v.find, v.canEnd) },
        )
    }
}

sealed interface BoardRowCanEnd {
    data class Flag(
        val value: Boolean,
    ) : BoardRowCanEnd
    data class Obj(
        val value: BoardRowCanEndObjValue,
    ) : BoardRowCanEnd

    companion object {
        val ADAPTER: JsonAdapter<BoardRowCanEnd> =
            byToken<BoardRowCanEnd>(
                listOf(
                Tok(JsonReader.Token.BOOLEAN, BoardRowCanEnd.Flag::class.java, { Bool }, { BoardRowCanEnd.Flag(it as Boolean) }, { (it as BoardRowCanEnd.Flag).value }),
                Tok(JsonReader.Token.BEGIN_OBJECT, BoardRowCanEnd.Obj::class.java, { BoardRowCanEndObjValue.ADAPTER }, { BoardRowCanEnd.Obj(it as BoardRowCanEndObjValue) }, { (it as BoardRowCanEnd.Obj).value }),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class BoardRowCanEndObjValue(
    val no: Refused,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BoardRowCanEndObjValue> = obj(
            listOf(
            Field("no", { Refused.ADAPTER }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> BoardRowCanEndObjValue(a[0] as Refused, a[1] as String) },
            { v -> arrayOf(v.no, v.said) },
        )
    }
}

enum class BoardTag(val wire: String) {
    ALL("all"),
    SESSION("session"),
    BARE("bare"),
    BACKGROUND("background"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<BoardTag> = enumOf(entries) { it.wire }
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
data class BookmarksEditReq(
    val op: BookmarksEditReqOp,
    val dirs: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BookmarksEditReq> = obj(
            listOf(
            Field("op", { BookmarksEditReqOp.ADAPTER }, optional = false, nullable = false),
            Field("dirs", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> BookmarksEditReq(a[0] as BookmarksEditReqOp, a[1] as List<String>) },
            { v -> arrayOf(v.op, v.dirs) },
        )
    }
}

enum class BookmarksEditReqOp(val wire: String) {
    TOGGLE("toggle"),
    REMOVE("remove"),
    ADD("add"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<BookmarksEditReqOp> = enumOf(entries) { it.wire }
    }
}

data object BookmarksReadReq {
    val ADAPTER: JsonAdapter<BookmarksReadReq> = obj(emptyList(), { BookmarksReadReq }, { emptyArray() })
}

@Suppress("UNCHECKED_CAST")
data class Branch(
    val off: List<String>,
    val end: Long,
    val next: Long?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Branch> = obj(
            listOf(
            Field("off", { arr(Str) }, optional = false, nullable = false),
            Field("end", { Lng }, optional = false, nullable = false),
            Field("next", { opt(Lng) }, optional = false, nullable = true),
            ),
            { a -> Branch(a[0] as List<String>, a[1] as Long, a[2] as Long?) },
            { v -> arrayOf(v.off, v.end, v.next) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BranchHeld(
    val sid: String,
    val path: String? = null,
    val at: Long? = null,
    val sum: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BranchHeld> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("path", { opt(Str) }, optional = true, nullable = true),
            Field("at", { opt(U64) }, optional = true, nullable = true),
            Field("sum", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> BranchHeld(a[0] as String, a[1] as String?, a[2] as Long?, a[3] as String?) },
            { v -> arrayOf(v.sid, v.path, v.at, v.sum) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BranchReq(
    val path: String,
    val from: Long? = null,
    val limit: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BranchReq> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("from", { opt(Lng) }, optional = true, nullable = true),
            Field("limit", { opt(Lng) }, optional = true, nullable = true),
            ),
            { a -> BranchReq(a[0] as String, a[1] as Long?, a[2] as Long?) },
            { v -> arrayOf(v.path, v.from, v.limit) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BrowseReply(
    val added: Long,
    val removed: Long,
    val rejected: Long,
    val browseWatchCap: Long,
    val watching: Long,
    val watchFailed: Long,
    val watchError: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BrowseReply> = obj(
            listOf(
            Field("added", { U64 }, optional = false, nullable = false),
            Field("removed", { U64 }, optional = false, nullable = false),
            Field("rejected", { U64 }, optional = false, nullable = false),
            Field("browse_watch_cap", { U64 }, optional = false, nullable = false),
            Field("watching", { U64 }, optional = false, nullable = false),
            Field("watch_failed", { U64 }, optional = false, nullable = false),
            Field("watch_error", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> BrowseReply(a[0] as Long, a[1] as Long, a[2] as Long, a[3] as Long, a[4] as Long, a[5] as Long, a[6] as String?) },
            { v -> arrayOf(v.added, v.removed, v.rejected, v.browseWatchCap, v.watching, v.watchFailed, v.watchError) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BrowseReq(
    val dirs: List<WirePath>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BrowseReq> = obj(
            listOf(
            Field("dirs", { arr(WirePath.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> BrowseReq(a[0] as List<WirePath>) },
            { v -> arrayOf(v.dirs) },
        )
    }
}

enum class BusAct(val wire: String) {
    ASK("ask"),
    ANSWER("answer"),
    REQUEST("request"),
    INFORM("inform"),
    HANDBACK("handback"),
    CORRECT("correct"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<BusAct> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class BusAgent(
    val id: String,
    val title: String,
    val unread: Long,
    val live: Boolean?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BusAgent> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("title", { Str }, optional = false, nullable = false),
            Field("unread", { Lng }, optional = false, nullable = false),
            Field("live", { opt(Bool) }, optional = false, nullable = true),
            ),
            { a -> BusAgent(a[0] as String, a[1] as String, a[2] as Long, a[3] as Boolean?) },
            { v -> arrayOf(v.id, v.title, v.unread, v.live) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BusCommitReq(
    val id: String,
    val token: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BusCommitReq> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("token", { Str }, optional = false, nullable = false),
            ),
            { a -> BusCommitReq(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.id, v.token) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BusCommitted(
    val read: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BusCommitted> = obj(
            listOf(
            Field("read", { Lng }, optional = false, nullable = false),
            ),
            { a -> BusCommitted(a[0] as Long) },
            { v -> arrayOf(v.read) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BusEnvelope(
    val id: String,
    val ts: String,
    val from: String,
    val to: String,
    val tier: Long,
    val act: BusAct,
    val title: String,
    val re: String?,
    val refs: List<String>,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BusEnvelope> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("ts", { Str }, optional = false, nullable = false),
            Field("from", { Str }, optional = false, nullable = false),
            Field("to", { Str }, optional = false, nullable = false),
            Field("tier", { Lng }, optional = false, nullable = false),
            Field("act", { BusAct.ADAPTER }, optional = false, nullable = false),
            Field("title", { Str }, optional = false, nullable = false),
            Field("re", { opt(Str) }, optional = false, nullable = true),
            Field("refs", { arr(Str) }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> BusEnvelope(a[0] as String, a[1] as String, a[2] as String, a[3] as String, a[4] as Long, a[5] as BusAct, a[6] as String, a[7] as String?, a[8] as List<String>, a[9] as String) },
            { v -> arrayOf(v.id, v.ts, v.from, v.to, v.tier, v.act, v.title, v.re, v.refs, v.text) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BusEnvelopeReq(
    val from: String,
    val to: String,
    val machine: String? = null,
    val tier: Long? = null,
    val act: BusAct,
    val title: String,
    val re: String? = null,
    val refs: List<String>? = null,
    val text: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BusEnvelopeReq> = obj(
            listOf(
            Field("from", { Str }, optional = false, nullable = false),
            Field("to", { Str }, optional = false, nullable = false),
            Field("machine", { opt(Str) }, optional = true, nullable = true),
            Field("tier", { opt(Lng) }, optional = true, nullable = true),
            Field("act", { BusAct.ADAPTER }, optional = false, nullable = false),
            Field("title", { Str }, optional = false, nullable = false),
            Field("re", { opt(Str) }, optional = true, nullable = true),
            Field("refs", { opt(arr(Str)) }, optional = true, nullable = true),
            Field("text", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> BusEnvelopeReq(a[0] as String, a[1] as String, a[2] as String?, a[3] as Long?, a[4] as BusAct, a[5] as String, a[6] as String?, a[7] as List<String>?, a[8] as String?) },
            { v -> arrayOf(v.from, v.to, v.machine, v.tier, v.act, v.title, v.re, v.refs, v.text) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BusInbox(
    val messages: List<BusInboxItem>,
    val skipped: Long,
    val truncated: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BusInbox> = obj(
            listOf(
            Field("messages", { arr(BusInboxItem.ADAPTER) }, optional = false, nullable = false),
            Field("skipped", { Lng }, optional = false, nullable = false),
            Field("truncated", { Bool }, optional = false, nullable = false),
            ),
            { a -> BusInbox(a[0] as List<BusInboxItem>, a[1] as Long, a[2] as Boolean) },
            { v -> arrayOf(v.messages, v.skipped, v.truncated) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BusInboxItem(
    val actSaid: String,
    val id: String,
    val ts: String,
    val from: String,
    val to: String,
    val tier: Long,
    val act: BusAct,
    val title: String,
    val re: String?,
    val refs: List<String>,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BusInboxItem> = obj(
            listOf(
            Field("act_said", { Str }, optional = false, nullable = false),
            Field("id", { Str }, optional = false, nullable = false),
            Field("ts", { Str }, optional = false, nullable = false),
            Field("from", { Str }, optional = false, nullable = false),
            Field("to", { Str }, optional = false, nullable = false),
            Field("tier", { Lng }, optional = false, nullable = false),
            Field("act", { BusAct.ADAPTER }, optional = false, nullable = false),
            Field("title", { Str }, optional = false, nullable = false),
            Field("re", { opt(Str) }, optional = false, nullable = true),
            Field("refs", { arr(Str) }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> BusInboxItem(a[0] as String, a[1] as String, a[2] as String, a[3] as String, a[4] as String, a[5] as Long, a[6] as BusAct, a[7] as String, a[8] as String?, a[9] as List<String>, a[10] as String) },
            { v -> arrayOf(v.actSaid, v.id, v.ts, v.from, v.to, v.tier, v.act, v.title, v.re, v.refs, v.text) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BusInboxReq(
    val id: String,
    val lines: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BusInboxReq> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("lines", { opt(Lng) }, optional = true, nullable = true),
            ),
            { a -> BusInboxReq(a[0] as String, a[1] as Long?) },
            { v -> arrayOf(v.id, v.lines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BusList(
    val agents: List<BusAgent>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BusList> = obj(
            listOf(
            Field("agents", { arr(BusAgent.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> BusList(a[0] as List<BusAgent>) },
            { v -> arrayOf(v.agents) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BusPeekReq(
    val id: String,
    val maxLines: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BusPeekReq> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("max_lines", { opt(Lng) }, optional = true, nullable = true),
            ),
            { a -> BusPeekReq(a[0] as String, a[1] as Long?) },
            { v -> arrayOf(v.id, v.maxLines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BusPeeked(
    val text: String,
    val token: String?,
    val count: Long,
    val skipped: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BusPeeked> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            Field("token", { opt(Str) }, optional = false, nullable = true),
            Field("count", { Lng }, optional = false, nullable = false),
            Field("skipped", { Lng }, optional = false, nullable = false),
            ),
            { a -> BusPeeked(a[0] as String, a[1] as String?, a[2] as Long, a[3] as Long) },
            { v -> arrayOf(v.text, v.token, v.count, v.skipped) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BusSent(
    val envelope: BusEnvelope,
    val knock: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BusSent> = obj(
            listOf(
            Field("envelope", { BusEnvelope.ADAPTER }, optional = false, nullable = false),
            Field("knock", { Bool }, optional = false, nullable = false),
            ),
            { a -> BusSent(a[0] as BusEnvelope, a[1] as Boolean) },
            { v -> arrayOf(v.envelope, v.knock) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BusUnread(
    val unread: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BusUnread> = obj(
            listOf(
            Field("unread", { Lng }, optional = false, nullable = false),
            ),
            { a -> BusUnread(a[0] as Long) },
            { v -> arrayOf(v.unread) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class BusUnreadReq(
    val id: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<BusUnreadReq> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            ),
            { a -> BusUnreadReq(a[0] as String) },
            { v -> arrayOf(v.id) },
        )
    }
}

sealed interface CallEnd {
    /** 判别格（`end`）那个词。 */
    val end: String

    val call: Long

    @Suppress("UNCHECKED_CAST")
    data class Ok(
        override val call: Long,
        val data: Raw,
    ) : CallEnd, Ordered {
        override val end: String get() = "ok"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Failed(
        override val call: Long,
        val code: String,
        val message: String,
        val detail: String,
        val data: Raw,
    ) : CallEnd, Ordered {
        override val end: String get() = "failed"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Timeout(
        override val call: Long,
    ) : CallEnd, Ordered {
        override val end: String get() = "timeout"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Cancelled(
        override val call: Long,
    ) : CallEnd, Ordered {
        override val end: String get() = "cancelled"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Down(
        override val call: Long,
    ) : CallEnd, Ordered {
        override val end: String get() = "down"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Unreadable(
        override val call: Long,
        val detail: String,
    ) : CallEnd, Ordered {
        override val end: String get() = "unreadable"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<CallEnd> =
            tagged<CallEnd>(
                "end",
                listOf(
                Variant("ok", CallEnd.Ok::class.java, obj(
                        listOf(
                        Field("end", { tag("ok") }, optional = false, nullable = false),
                        Field("call", { Lng }, optional = false, nullable = false),
                        Field("data", { RawJson }, optional = false, nullable = false),
                        ),
                        { a -> CallEnd.Ok(a[1] as Long, a[2] as Raw) },
                        { v -> arrayOf("ok", v.call, v.data) },
                    )),
                Variant("failed", CallEnd.Failed::class.java, obj(
                        listOf(
                        Field("end", { tag("failed") }, optional = false, nullable = false),
                        Field("call", { Lng }, optional = false, nullable = false),
                        Field("code", { Str }, optional = false, nullable = false),
                        Field("message", { Str }, optional = false, nullable = false),
                        Field("detail", { Str }, optional = false, nullable = false),
                        Field("data", { RawJson }, optional = false, nullable = false),
                        ),
                        { a -> CallEnd.Failed(a[1] as Long, a[2] as String, a[3] as String, a[4] as String, a[5] as Raw) },
                        { v -> arrayOf("failed", v.call, v.code, v.message, v.detail, v.data) },
                    )),
                Variant("timeout", CallEnd.Timeout::class.java, obj(
                        listOf(
                        Field("end", { tag("timeout") }, optional = false, nullable = false),
                        Field("call", { Lng }, optional = false, nullable = false),
                        ),
                        { a -> CallEnd.Timeout(a[1] as Long) },
                        { v -> arrayOf("timeout", v.call) },
                    )),
                Variant("cancelled", CallEnd.Cancelled::class.java, obj(
                        listOf(
                        Field("end", { tag("cancelled") }, optional = false, nullable = false),
                        Field("call", { Lng }, optional = false, nullable = false),
                        ),
                        { a -> CallEnd.Cancelled(a[1] as Long) },
                        { v -> arrayOf("cancelled", v.call) },
                    )),
                Variant("down", CallEnd.Down::class.java, obj(
                        listOf(
                        Field("end", { tag("down") }, optional = false, nullable = false),
                        Field("call", { Lng }, optional = false, nullable = false),
                        ),
                        { a -> CallEnd.Down(a[1] as Long) },
                        { v -> arrayOf("down", v.call) },
                    )),
                Variant("unreadable", CallEnd.Unreadable::class.java, obj(
                        listOf(
                        Field("end", { tag("unreadable") }, optional = false, nullable = false),
                        Field("call", { Lng }, optional = false, nullable = false),
                        Field("detail", { Str }, optional = false, nullable = false),
                        ),
                        { a -> CallEnd.Unreadable(a[1] as Long, a[2] as String) },
                        { v -> arrayOf("unreadable", v.call, v.detail) },
                    )),
                ),
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
    /** 判别格（`can`）那个词。 */
    val can: String

    data object Quiet : CanClose {
        override val can: String get() = "quiet"
    }
    data object Menu : CanClose {
        override val can: String get() = "menu"
    }
    @Suppress("UNCHECKED_CAST")
    data class No(
        val why: String,
    ) : CanClose, Ordered {
        override val can: String get() = "no"
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

sealed interface CanSay {
    /** 判别格（`can`）那个词。 */
    val can: String

    @Suppress("UNCHECKED_CAST")
    data class Yes(
        val hint: String,
    ) : CanSay, Ordered {
        override val can: String get() = "yes"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class No(
        val why: String,
    ) : CanSay, Ordered {
        override val can: String get() = "no"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<CanSay> =
            tagged<CanSay>(
                "can",
                listOf(
                Variant("yes", CanSay.Yes::class.java, obj(
                        listOf(
                        Field("can", { tag("yes") }, optional = false, nullable = false),
                        Field("hint", { Str }, optional = false, nullable = false),
                        ),
                        { a -> CanSay.Yes(a[1] as String) },
                        { v -> arrayOf("yes", v.hint) },
                    )),
                Variant("no", CanSay.No::class.java, obj(
                        listOf(
                        Field("can", { tag("no") }, optional = false, nullable = false),
                        Field("why", { Str }, optional = false, nullable = false),
                        ),
                        { a -> CanSay.No(a[1] as String) },
                        { v -> arrayOf("no", v.why) },
                    )),
                ),
            )
    }
}

sealed interface CanTerm {
    /** 判别格（`can`）那个词。 */
    val can: String

    @Suppress("UNCHECKED_CAST")
    data class Yes(
        val host: Run,
    ) : CanTerm, Ordered {
        override val can: String get() = "yes"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class No(
        val why: String,
    ) : CanTerm, Ordered {
        override val can: String get() = "no"
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
                        Field("host", { Run.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> CanTerm.Yes(a[1] as Run) },
                        { v -> arrayOf("yes", v.host) },
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
data class CancelReq(
    val target: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<CancelReq> = obj(
            listOf(
            Field("target", { Str }, optional = false, nullable = false),
            ),
            { a -> CancelReq(a[0] as String) },
            { v -> arrayOf(v.target) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class CaptureOpts(
    val maxBytes: Long,
    val abortMarker: String?,
    val stdin: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<CaptureOpts> = obj(
            listOf(
            Field("max_bytes", { Lng }, optional = false, nullable = false),
            Field("abort_marker", { opt(Str) }, optional = false, nullable = true),
            Field("stdin", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> CaptureOpts(a[0] as Long, a[1] as String?, a[2] as String?) },
            { v -> arrayOf(v.maxBytes, v.abortMarker, v.stdin) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Captured(
    val stdout: String,
    val stderr: String,
    val exitStatus: Int?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Captured> = obj(
            listOf(
            Field("stdout", { Str }, optional = false, nullable = false),
            Field("stderr", { Str }, optional = false, nullable = false),
            Field("exit_status", { opt(Int32) }, optional = false, nullable = true),
            ),
            { a -> Captured(a[0] as String, a[1] as String, a[2] as Int?) },
            { v -> arrayOf(v.stdout, v.stderr, v.exitStatus) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class CatalogAsset(
    val kind: ExtKind,
    val name: String,
    val project: String? = null,
    val dir: String? = null,
    val digest: String,
    val summary: Raw,
    val off: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<CatalogAsset> = obj(
            listOf(
            Field("kind", { ExtKind.ADAPTER }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("project", { opt(Str) }, optional = true, nullable = true),
            Field("dir", { opt(Str) }, optional = true, nullable = true),
            Field("digest", { Str }, optional = false, nullable = false),
            Field("summary", { RawJson }, optional = false, nullable = false),
            Field("off", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> CatalogAsset(a[0] as ExtKind, a[1] as String, a[2] as String?, a[3] as String?, a[4] as String, a[5] as Raw, a[6] as Boolean?) },
            { v -> arrayOf(v.kind, v.name, v.project, v.dir, v.digest, v.summary, v.off) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class CatalogFrom(
    val machine: String,
    val project: String?,
    val digest: String,
    val summary: Raw,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<CatalogFrom> = obj(
            listOf(
            Field("machine", { Str }, optional = false, nullable = false),
            Field("project", { opt(Str) }, optional = false, nullable = true),
            Field("digest", { Str }, optional = false, nullable = false),
            Field("summary", { RawJson }, optional = false, nullable = false),
            ),
            { a -> CatalogFrom(a[0] as String, a[1] as String?, a[2] as String, a[3] as Raw) },
            { v -> arrayOf(v.machine, v.project, v.digest, v.summary) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class CatalogIn(
    val machines: List<CatalogMachine>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<CatalogIn> = obj(
            listOf(
            Field("machines", { arr(CatalogMachine.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> CatalogIn(a[0] as List<CatalogMachine>) },
            { v -> arrayOf(v.machines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class CatalogMachine(
    val id: String,
    val label: String,
    val gen: Long,
    val seenAt: Long,
    val assets: List<CatalogAsset>,
    val projectDirs: List<String>,
    val notes: Map<String, CatalogNote>,
    val sharedMcp: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<CatalogMachine> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("label", { Str }, optional = false, nullable = false),
            Field("gen", { U64 }, optional = false, nullable = false),
            Field("seenAt", { U64 }, optional = false, nullable = false),
            Field("assets", { arr(CatalogAsset.ADAPTER) }, optional = false, nullable = false),
            Field("projectDirs", { arr(Str) }, optional = false, nullable = false),
            Field("notes", { dict(CatalogNote.ADAPTER) }, optional = false, nullable = false),
            Field("sharedMcp", { Bool }, optional = false, nullable = false),
            ),
            { a -> CatalogMachine(a[0] as String, a[1] as String, a[2] as Long, a[3] as Long, a[4] as List<CatalogAsset>, a[5] as List<String>, a[6] as Map<String, CatalogNote>, a[7] as Boolean) },
            { v -> arrayOf(v.id, v.label, v.gen, v.seenAt, v.assets, v.projectDirs, v.notes, v.sharedMcp) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class CatalogMergeReq(
    val catalog: CatalogIn,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<CatalogMergeReq> = obj(
            listOf(
            Field("catalog", { CatalogIn.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> CatalogMergeReq(a[0] as CatalogIn) },
            { v -> arrayOf(v.catalog) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class CatalogNote(
    val text: String,
    val rev: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<CatalogNote> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            Field("rev", { U64 }, optional = false, nullable = false),
            ),
            { a -> CatalogNote(a[0] as String, a[1] as Long) },
            { v -> arrayOf(v.text, v.rev) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class CatalogReply(
    val self: String,
    val machines: List<CatalogMachine>,
    val rows: List<CatalogRow>,
    val problems: List<String>,
    val changed: Boolean,
    val path: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<CatalogReply> = obj(
            listOf(
            Field("self", { Str }, optional = false, nullable = false),
            Field("machines", { arr(CatalogMachine.ADAPTER) }, optional = false, nullable = false),
            Field("rows", { arr(CatalogRow.ADAPTER) }, optional = false, nullable = false),
            Field("problems", { arr(Str) }, optional = false, nullable = false),
            Field("changed", { Bool }, optional = false, nullable = false),
            Field("path", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> CatalogReply(a[0] as String, a[1] as List<CatalogMachine>, a[2] as List<CatalogRow>, a[3] as List<String>, a[4] as Boolean, a[5] as String?) },
            { v -> arrayOf(v.self, v.machines, v.rows, v.problems, v.changed, v.path) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class CatalogRow(
    val kind: ExtKind,
    val name: String,
    val state: String,
    val from: List<CatalogFrom>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<CatalogRow> = obj(
            listOf(
            Field("kind", { ExtKind.ADAPTER }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("state", { Str }, optional = false, nullable = false),
            Field("from", { arr(CatalogFrom.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> CatalogRow(a[0] as ExtKind, a[1] as String, a[2] as String, a[3] as List<CatalogFrom>) },
            { v -> arrayOf(v.kind, v.name, v.state, v.from) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class CcBusInstalled(
    val dest: String,
    val written: List<String>,
    val backup: String?,
    val recordFailed: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<CcBusInstalled> = obj(
            listOf(
            Field("dest", { Str }, optional = false, nullable = false),
            Field("written", { arr(Str) }, optional = false, nullable = false),
            Field("backup", { opt(Str) }, optional = false, nullable = true),
            Field("recordFailed", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> CcBusInstalled(a[0] as String, a[1] as List<String>, a[2] as String?, a[3] as String?) },
            { v -> arrayOf(v.dest, v.written, v.backup, v.recordFailed) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class CcBusState(
    val dest: String,
    val writes: List<String>,
    val existing: Boolean,
    val version: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<CcBusState> = obj(
            listOf(
            Field("dest", { Str }, optional = false, nullable = false),
            Field("writes", { arr(Str) }, optional = false, nullable = false),
            Field("existing", { Bool }, optional = false, nullable = false),
            Field("version", { Str }, optional = false, nullable = false),
            ),
            { a -> CcBusState(a[0] as String, a[1] as List<String>, a[2] as Boolean, a[3] as String) },
            { v -> arrayOf(v.dest, v.writes, v.existing, v.version) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class CcmCard(
    val version: String,
    val capabilities: List<String>,
    val agents: List<String>,
    val build: String,
    val onPath: OnPath,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<CcmCard> = obj(
            listOf(
            Field("version", { Str }, optional = false, nullable = false),
            Field("capabilities", { arr(Str) }, optional = false, nullable = false),
            Field("agents", { arr(Str) }, optional = false, nullable = false),
            Field("build", { Str }, optional = false, nullable = false),
            Field("onPath", { OnPath.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> CcmCard(a[0] as String, a[1] as List<String>, a[2] as List<String>, a[3] as String, a[4] as OnPath) },
            { v -> arrayOf(v.version, v.capabilities, v.agents, v.build, v.onPath) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class CcmLine(
    val line: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<CcmLine> = obj(
            listOf(
            Field("line", { Str }, optional = false, nullable = false),
            ),
            { a -> CcmLine(a[0] as String) },
            { v -> arrayOf(v.line) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class CcmPrintReq(
    val args: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<CcmPrintReq> = obj(
            listOf(
            Field("args", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> CcmPrintReq(a[0] as List<String>) },
            { v -> arrayOf(v.args) },
        )
    }
}

sealed interface Changed {
    /** 判别格（`topic`）那个词。 */
    val topic: String

    data object Accounts : Changed {
        override val topic: String get() = "accounts"
    }
    data object Profiles : Changed {
        override val topic: String get() = "profiles"
    }
    @Suppress("UNCHECKED_CAST")
    data class Quota(
        val body: QuotaRead? = null,
    ) : Changed, Ordered {
        override val topic: String get() = "quota"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Rotation(
        val key: String,
    ) : Changed, Ordered {
        override val topic: String get() = "rotation"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class RotationStrategies(
        val body: RotationStrategiesReply? = null,
    ) : Changed, Ordered {
        override val topic: String get() = "rotation_strategies"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Plan(
        val key: String,
        val rev: String? = null,
        val body: PlanNeeds? = null,
    ) : Changed, Ordered {
        override val topic: String get() = "plan"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Tasks(
        val key: String,
        val body: TasksList? = null,
    ) : Changed, Ordered {
        override val topic: String get() = "tasks"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Needs(
        val key: String? = null,
    ) : Changed, Ordered {
        override val topic: String get() = "needs"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Login(
        val key: String,
        val body: SignInState? = null,
    ) : Changed, Ordered {
        override val topic: String get() = "login"
        override var wireOrder: IntArray? = null
    }
    data object Machines : Changed {
        override val topic: String get() = "machines"
    }
    @Suppress("UNCHECKED_CAST")
    data class Machine(
        val key: String,
        val body: MachineState? = null,
    ) : Changed, Ordered {
        override val topic: String get() = "machine"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Empty(
        val body: EmptyFace? = null,
    ) : Changed, Ordered {
        override val topic: String get() = "empty"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class SessionHidden(
        val key: String,
        val body: HiddenNow? = null,
    ) : Changed, Ordered {
        override val topic: String get() = "session_hidden"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class FilesDir(
        val key: String,
    ) : Changed, Ordered {
        override val topic: String get() = "files_dir"
        override var wireOrder: IntArray? = null
    }
    data object HistoryList : Changed {
        override val topic: String get() = "history_list"
    }
    data object Groups : Changed {
        override val topic: String get() = "groups"
    }
    @Suppress("UNCHECKED_CAST")
    data class SessionQueue(
        val key: String,
        val body: QueueList? = null,
    ) : Changed, Ordered {
        override val topic: String get() = "session_queue"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<Changed> =
            tagged<Changed>(
                "topic",
                listOf(
                Variant("accounts", Changed.Accounts::class.java, obj(
                        listOf(
                        Field("topic", { tag("accounts") }, optional = false, nullable = false),
                        ),
                        { Changed.Accounts },
                        { arrayOf("accounts") },
                    )),
                Variant("profiles", Changed.Profiles::class.java, obj(
                        listOf(
                        Field("topic", { tag("profiles") }, optional = false, nullable = false),
                        ),
                        { Changed.Profiles },
                        { arrayOf("profiles") },
                    )),
                Variant("quota", Changed.Quota::class.java, obj(
                        listOf(
                        Field("topic", { tag("quota") }, optional = false, nullable = false),
                        Field("body", { opt(QuotaRead.ADAPTER) }, optional = true, nullable = true),
                        ),
                        { a -> Changed.Quota(a[1] as QuotaRead?) },
                        { v -> arrayOf("quota", v.body) },
                    )),
                Variant("rotation", Changed.Rotation::class.java, obj(
                        listOf(
                        Field("topic", { tag("rotation") }, optional = false, nullable = false),
                        Field("key", { Str }, optional = false, nullable = false),
                        ),
                        { a -> Changed.Rotation(a[1] as String) },
                        { v -> arrayOf("rotation", v.key) },
                    )),
                Variant("rotation_strategies", Changed.RotationStrategies::class.java, obj(
                        listOf(
                        Field("topic", { tag("rotation_strategies") }, optional = false, nullable = false),
                        Field("body", { opt(RotationStrategiesReply.ADAPTER) }, optional = true, nullable = true),
                        ),
                        { a -> Changed.RotationStrategies(a[1] as RotationStrategiesReply?) },
                        { v -> arrayOf("rotation_strategies", v.body) },
                    )),
                Variant("plan", Changed.Plan::class.java, obj(
                        listOf(
                        Field("topic", { tag("plan") }, optional = false, nullable = false),
                        Field("key", { Str }, optional = false, nullable = false),
                        Field("rev", { opt(Str) }, optional = true, nullable = true),
                        Field("body", { opt(PlanNeeds.ADAPTER) }, optional = true, nullable = true),
                        ),
                        { a -> Changed.Plan(a[1] as String, a[2] as String?, a[3] as PlanNeeds?) },
                        { v -> arrayOf("plan", v.key, v.rev, v.body) },
                    )),
                Variant("tasks", Changed.Tasks::class.java, obj(
                        listOf(
                        Field("topic", { tag("tasks") }, optional = false, nullable = false),
                        Field("key", { Str }, optional = false, nullable = false),
                        Field("body", { opt(TasksList.ADAPTER) }, optional = true, nullable = true),
                        ),
                        { a -> Changed.Tasks(a[1] as String, a[2] as TasksList?) },
                        { v -> arrayOf("tasks", v.key, v.body) },
                    )),
                Variant("needs", Changed.Needs::class.java, obj(
                        listOf(
                        Field("topic", { tag("needs") }, optional = false, nullable = false),
                        Field("key", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> Changed.Needs(a[1] as String?) },
                        { v -> arrayOf("needs", v.key) },
                    )),
                Variant("login", Changed.Login::class.java, obj(
                        listOf(
                        Field("topic", { tag("login") }, optional = false, nullable = false),
                        Field("key", { Str }, optional = false, nullable = false),
                        Field("body", { opt(SignInState.ADAPTER) }, optional = true, nullable = true),
                        ),
                        { a -> Changed.Login(a[1] as String, a[2] as SignInState?) },
                        { v -> arrayOf("login", v.key, v.body) },
                    )),
                Variant("machines", Changed.Machines::class.java, obj(
                        listOf(
                        Field("topic", { tag("machines") }, optional = false, nullable = false),
                        ),
                        { Changed.Machines },
                        { arrayOf("machines") },
                    )),
                Variant("machine", Changed.Machine::class.java, obj(
                        listOf(
                        Field("topic", { tag("machine") }, optional = false, nullable = false),
                        Field("key", { Str }, optional = false, nullable = false),
                        Field("body", { opt(MachineState.ADAPTER) }, optional = true, nullable = true),
                        ),
                        { a -> Changed.Machine(a[1] as String, a[2] as MachineState?) },
                        { v -> arrayOf("machine", v.key, v.body) },
                    )),
                Variant("empty", Changed.Empty::class.java, obj(
                        listOf(
                        Field("topic", { tag("empty") }, optional = false, nullable = false),
                        Field("body", { opt(EmptyFace.ADAPTER) }, optional = true, nullable = true),
                        ),
                        { a -> Changed.Empty(a[1] as EmptyFace?) },
                        { v -> arrayOf("empty", v.body) },
                    )),
                Variant("session_hidden", Changed.SessionHidden::class.java, obj(
                        listOf(
                        Field("topic", { tag("session_hidden") }, optional = false, nullable = false),
                        Field("key", { Str }, optional = false, nullable = false),
                        Field("body", { opt(HiddenNow.ADAPTER) }, optional = true, nullable = true),
                        ),
                        { a -> Changed.SessionHidden(a[1] as String, a[2] as HiddenNow?) },
                        { v -> arrayOf("session_hidden", v.key, v.body) },
                    )),
                Variant("files_dir", Changed.FilesDir::class.java, obj(
                        listOf(
                        Field("topic", { tag("files_dir") }, optional = false, nullable = false),
                        Field("key", { Str }, optional = false, nullable = false),
                        ),
                        { a -> Changed.FilesDir(a[1] as String) },
                        { v -> arrayOf("files_dir", v.key) },
                    )),
                Variant("history_list", Changed.HistoryList::class.java, obj(
                        listOf(
                        Field("topic", { tag("history_list") }, optional = false, nullable = false),
                        ),
                        { Changed.HistoryList },
                        { arrayOf("history_list") },
                    )),
                Variant("groups", Changed.Groups::class.java, obj(
                        listOf(
                        Field("topic", { tag("groups") }, optional = false, nullable = false),
                        ),
                        { Changed.Groups },
                        { arrayOf("groups") },
                    )),
                Variant("session_queue", Changed.SessionQueue::class.java, obj(
                        listOf(
                        Field("topic", { tag("session_queue") }, optional = false, nullable = false),
                        Field("key", { Str }, optional = false, nullable = false),
                        Field("body", { opt(QueueList.ADAPTER) }, optional = true, nullable = true),
                        ),
                        { a -> Changed.SessionQueue(a[1] as String, a[2] as QueueList?) },
                        { v -> arrayOf("session_queue", v.key, v.body) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class ChangedFile(
    val path: String,
    val what: String,
    val undo: UndoAt?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ChangedFile> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("what", { Str }, optional = false, nullable = false),
            Field("undo", { opt(UndoAt.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> ChangedFile(a[0] as String, a[1] as String, a[2] as UndoAt?) },
            { v -> arrayOf(v.path, v.what, v.undo) },
        )
    }
}

enum class CheckLevel(val wire: String) {
    OK("ok"),
    WARN("warn"),
    FAIL("fail"),
    SKIP("skip"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<CheckLevel> = enumOf(entries) { it.wire }
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

@Suppress("UNCHECKED_CAST")
data class ChmodItem(
    val rel: WirePath,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ChmodItem> = obj(
            listOf(
            Field("rel", { WirePath.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> ChmodItem(a[0] as WirePath) },
            { v -> arrayOf(v.rel) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Chore(
    val id: String,
    val kind: ChoreKind,
    val state: ChoreState,
    val name: String,
    val loc: String,
    val said: String,
    val why: String,
    val steps: List<String>,
    val diff: List<DiffRow>,
    val copy: String?,
    val whole: String?,
    val wholeCovers: List<String>,
    val file: String?,
    val go: UndoAt?,
    val howUrl: String?,
    val action: ChoreAction,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Chore> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("kind", { ChoreKind.ADAPTER }, optional = false, nullable = false),
            Field("state", { ChoreState.ADAPTER }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("loc", { Str }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            Field("why", { Str }, optional = false, nullable = false),
            Field("steps", { arr(Str) }, optional = false, nullable = false),
            Field("diff", { arr(DiffRow.ADAPTER) }, optional = false, nullable = false),
            Field("copy", { opt(Str) }, optional = false, nullable = true),
            Field("whole", { opt(Str) }, optional = false, nullable = true),
            Field("wholeCovers", { arr(Str) }, optional = false, nullable = false),
            Field("file", { opt(Str) }, optional = false, nullable = true),
            Field("go", { opt(UndoAt.ADAPTER) }, optional = false, nullable = true),
            Field("howUrl", { opt(Str) }, optional = false, nullable = true),
            Field("action", { ChoreAction.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> Chore(a[0] as String, a[1] as ChoreKind, a[2] as ChoreState, a[3] as String, a[4] as String, a[5] as String, a[6] as String, a[7] as List<String>, a[8] as List<DiffRow>, a[9] as String?, a[10] as String?, a[11] as List<String>, a[12] as String?, a[13] as UndoAt?, a[14] as String?, a[15] as ChoreAction) },
            { v -> arrayOf(v.id, v.kind, v.state, v.name, v.loc, v.said, v.why, v.steps, v.diff, v.copy, v.whole, v.wholeCovers, v.file, v.go, v.howUrl, v.action) },
        )
    }
}

enum class ChoreAction(val wire: String) {
    COPY_COMMAND("copyCommand"),
    COPY_SNIPPET("copySnippet"),
    DECIDE("decide"),
    LOCATE("locate"),
    HOW("how"),
    INSTALL_FIRST("installFirst"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ChoreAction> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ChoreCounts(
    val badge: Long,
    val open: Long,
    val must: Long,
    val decide: Long,
    val install: Long,
    val optional: Long,
    val done: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ChoreCounts> = obj(
            listOf(
            Field("badge", { Lng }, optional = false, nullable = false),
            Field("open", { Lng }, optional = false, nullable = false),
            Field("must", { Lng }, optional = false, nullable = false),
            Field("decide", { Lng }, optional = false, nullable = false),
            Field("install", { Lng }, optional = false, nullable = false),
            Field("optional", { Lng }, optional = false, nullable = false),
            Field("done", { Lng }, optional = false, nullable = false),
            ),
            { a -> ChoreCounts(a[0] as Long, a[1] as Long, a[2] as Long, a[3] as Long, a[4] as Long, a[5] as Long, a[6] as Long) },
            { v -> arrayOf(v.badge, v.open, v.must, v.decide, v.install, v.optional, v.done) },
        )
    }
}

enum class ChoreKind(val wire: String) {
    MUST("must"),
    DECIDE("decide"),
    INSTALL("install"),
    INSTALL_OPTIONAL("installOptional"),
    OPTIONAL("optional"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ChoreKind> = enumOf(entries) { it.wire }
    }
}

enum class ChoreState(val wire: String) {
    TODO("todo"),
    DONE("done"),
    EXPIRED("expired"),
    BLOCKED("blocked"),
    DECLINED("declined"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ChoreState> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ChoresAt(
    val origin: String,
    val report: DataReport? = null,
    val said: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ChoresAt> = obj(
            listOf(
            Field("origin", { Str }, optional = false, nullable = false),
            Field("report", { opt(DataReport.ADAPTER) }, optional = true, nullable = true),
            Field("said", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> ChoresAt(a[0] as String, a[1] as DataReport?, a[2] as String?) },
            { v -> arrayOf(v.origin, v.report, v.said) },
        )
    }
}

sealed interface ChoresMarkReq {
    /** 判别格（`op`）那个词。 */
    val op: String

    @Suppress("UNCHECKED_CAST")
    data class Decline(
        val id: String,
    ) : ChoresMarkReq, Ordered {
        override val op: String get() = "decline"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Undecline(
        val id: String,
    ) : ChoresMarkReq, Ordered {
        override val op: String get() = "undecline"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class SelfPaste(
        val rc: String,
    ) : ChoresMarkReq, Ordered {
        override val op: String get() = "selfPaste"
        override var wireOrder: IntArray? = null
    }
    data object UnselfPaste : ChoresMarkReq {
        override val op: String get() = "unselfPaste"
    }
    data object SkipStart : ChoresMarkReq {
        override val op: String get() = "skipStart"
    }
    data object UnskipStart : ChoresMarkReq {
        override val op: String get() = "unskipStart"
    }

    companion object {
        val ADAPTER: JsonAdapter<ChoresMarkReq> =
            tagged<ChoresMarkReq>(
                "op",
                listOf(
                Variant("decline", ChoresMarkReq.Decline::class.java, obj(
                        listOf(
                        Field("op", { tag("decline") }, optional = false, nullable = false),
                        Field("id", { Str }, optional = false, nullable = false),
                        ),
                        { a -> ChoresMarkReq.Decline(a[1] as String) },
                        { v -> arrayOf("decline", v.id) },
                    )),
                Variant("undecline", ChoresMarkReq.Undecline::class.java, obj(
                        listOf(
                        Field("op", { tag("undecline") }, optional = false, nullable = false),
                        Field("id", { Str }, optional = false, nullable = false),
                        ),
                        { a -> ChoresMarkReq.Undecline(a[1] as String) },
                        { v -> arrayOf("undecline", v.id) },
                    )),
                Variant("selfPaste", ChoresMarkReq.SelfPaste::class.java, obj(
                        listOf(
                        Field("op", { tag("selfPaste") }, optional = false, nullable = false),
                        Field("rc", { Str }, optional = false, nullable = false),
                        ),
                        { a -> ChoresMarkReq.SelfPaste(a[1] as String) },
                        { v -> arrayOf("selfPaste", v.rc) },
                    )),
                Variant("unselfPaste", ChoresMarkReq.UnselfPaste::class.java, obj(
                        listOf(
                        Field("op", { tag("unselfPaste") }, optional = false, nullable = false),
                        ),
                        { ChoresMarkReq.UnselfPaste },
                        { arrayOf("unselfPaste") },
                    )),
                Variant("skipStart", ChoresMarkReq.SkipStart::class.java, obj(
                        listOf(
                        Field("op", { tag("skipStart") }, optional = false, nullable = false),
                        ),
                        { ChoresMarkReq.SkipStart },
                        { arrayOf("skipStart") },
                    )),
                Variant("unskipStart", ChoresMarkReq.UnskipStart::class.java, obj(
                        listOf(
                        Field("op", { tag("unskipStart") }, optional = false, nullable = false),
                        ),
                        { ChoresMarkReq.UnskipStart },
                        { arrayOf("unskipStart") },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class ChoresMarks(
    val declined: List<String>,
    val selfPaste: String?,
    val startSkipped: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ChoresMarks> = obj(
            listOf(
            Field("declined", { arr(Str) }, optional = false, nullable = false),
            Field("selfPaste", { opt(Str) }, optional = false, nullable = true),
            Field("startSkipped", { Bool }, optional = false, nullable = false),
            ),
            { a -> ChoresMarks(a[0] as List<String>, a[1] as String?, a[2] as Boolean) },
            { v -> arrayOf(v.declined, v.selfPaste, v.startSkipped) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ChoresMergeReq(
    val machines: List<String>,
    val client: FootprintClient? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ChoresMergeReq> = obj(
            listOf(
            Field("machines", { arr(Str) }, optional = false, nullable = false),
            Field("client", { opt(FootprintClient.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> ChoresMergeReq(a[0] as List<String>, a[1] as FootprintClient?) },
            { v -> arrayOf(v.machines, v.client) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ChoresMerged(
    val total: ChoreCounts,
    val machines: List<ChoresAt>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ChoresMerged> = obj(
            listOf(
            Field("total", { ChoreCounts.ADAPTER }, optional = false, nullable = false),
            Field("machines", { arr(ChoresAt.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> ChoresMerged(a[0] as ChoreCounts, a[1] as List<ChoresAt>) },
            { v -> arrayOf(v.total, v.machines) },
        )
    }
}

sealed interface ChunkBytes {
    data class Text(
        val value: String,
    ) : ChunkBytes
    data class Obj(
        val value: ChunkBytesObjValue,
    ) : ChunkBytes

    companion object {
        val ADAPTER: JsonAdapter<ChunkBytes> =
            byToken<ChunkBytes>(
                listOf(
                Tok(JsonReader.Token.STRING, ChunkBytes.Text::class.java, { Str }, { ChunkBytes.Text(it as String) }, { (it as ChunkBytes.Text).value }),
                Tok(JsonReader.Token.BEGIN_OBJECT, ChunkBytes.Obj::class.java, { ChunkBytesObjValue.ADAPTER }, { ChunkBytes.Obj(it as ChunkBytesObjValue) }, { (it as ChunkBytes.Obj).value }),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class ChunkBytesObjValue(
    val b16: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ChunkBytesObjValue> = obj(
            listOf(
            Field("b16", { Str }, optional = false, nullable = false),
            ),
            { a -> ChunkBytesObjValue(a[0] as String) },
            { v -> arrayOf(v.b16) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ClashRow(
    val key: String,
    val name: String,
    val meta: String,
    val answers: List<Decide>,
    val there: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ClashRow> = obj(
            listOf(
            Field("key", { Str }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("meta", { Str }, optional = false, nullable = false),
            Field("answers", { arr(Decide.ADAPTER) }, optional = false, nullable = false),
            Field("there", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ClashRow(a[0] as String, a[1] as String, a[2] as String, a[3] as List<Decide>, a[4] as String?) },
            { v -> arrayOf(v.key, v.name, v.meta, v.answers, v.there) },
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

enum class Compact(val wire: String) {
    DONE("done"),
    TIMED_OUT("timed_out"),
    SKIPPED("skipped"),
    UNSUPPORTED("unsupported"),
    FAILED("failed"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<Compact> = enumOf(entries) { it.wire }
    }
}

sealed interface ConfigEdit {
    /** 判别格（`op`）那个词。 */
    val op: String

    val path: List<String>

    @Suppress("UNCHECKED_CAST")
    data class Set(
        override val path: List<String>,
        val value: Raw,
    ) : ConfigEdit, Ordered {
        override val op: String get() = "set"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Remove(
        override val path: List<String>,
    ) : ConfigEdit, Ordered {
        override val op: String get() = "remove"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<ConfigEdit> =
            tagged<ConfigEdit>(
                "op",
                listOf(
                Variant("set", ConfigEdit.Set::class.java, obj(
                        listOf(
                        Field("op", { tag("set") }, optional = false, nullable = false),
                        Field("path", { arr(Str) }, optional = false, nullable = false),
                        Field("value", { RawJson }, optional = false, nullable = false),
                        ),
                        { a -> ConfigEdit.Set(a[1] as List<String>, a[2] as Raw) },
                        { v -> arrayOf("set", v.path, v.value) },
                    )),
                Variant("remove", ConfigEdit.Remove::class.java, obj(
                        listOf(
                        Field("op", { tag("remove") }, optional = false, nullable = false),
                        Field("path", { arr(Str) }, optional = false, nullable = false),
                        ),
                        { a -> ConfigEdit.Remove(a[1] as List<String>) },
                        { v -> arrayOf("remove", v.path) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class ConfigHost(
    val alias: String,
    val hostName: String,
    val port: Int,
    val user: String?,
    val identityFile: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ConfigHost> = obj(
            listOf(
            Field("alias", { Str }, optional = false, nullable = false),
            Field("host_name", { Str }, optional = false, nullable = false),
            Field("port", { Int32 }, optional = false, nullable = false),
            Field("user", { opt(Str) }, optional = false, nullable = true),
            Field("identity_file", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ConfigHost(a[0] as String, a[1] as String, a[2] as Int, a[3] as String?, a[4] as String?) },
            { v -> arrayOf(v.alias, v.hostName, v.port, v.user, v.identityFile) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ConfigSurfaceReport(
    val rows: List<SurfaceRow>,
    val settingsScopes: List<SettingsScope>,
    val agentHome: String,
    val home: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ConfigSurfaceReport> = obj(
            listOf(
            Field("rows", { arr(SurfaceRow.ADAPTER) }, optional = false, nullable = false),
            Field("settings_scopes", { arr(SettingsScope.ADAPTER) }, optional = false, nullable = false),
            Field("agent_home", { Str }, optional = false, nullable = false),
            Field("home", { Str }, optional = false, nullable = false),
            ),
            { a -> ConfigSurfaceReport(a[0] as List<SurfaceRow>, a[1] as List<SettingsScope>, a[2] as String, a[3] as String) },
            { v -> arrayOf(v.rows, v.settingsScopes, v.agentHome, v.home) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Conflict(
    val key: String,
    val name: WirePath,
    val kind: FileKind,
    val size: Long? = null,
    val sizeText: String? = null,
    val mtimeText: String? = null,
    val there: FileKind,
    val thereSize: Long? = null,
    val thereSizeText: String? = null,
    val thereMtimeText: String? = null,
    val answers: List<Decide>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Conflict> = obj(
            listOf(
            Field("key", { Str }, optional = false, nullable = false),
            Field("name", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("kind", { FileKind.ADAPTER }, optional = false, nullable = false),
            Field("size", { opt(U64) }, optional = true, nullable = true),
            Field("size_text", { opt(Str) }, optional = true, nullable = true),
            Field("mtime_text", { opt(Str) }, optional = true, nullable = true),
            Field("there", { FileKind.ADAPTER }, optional = false, nullable = false),
            Field("there_size", { opt(U64) }, optional = true, nullable = true),
            Field("there_size_text", { opt(Str) }, optional = true, nullable = true),
            Field("there_mtime_text", { opt(Str) }, optional = true, nullable = true),
            Field("answers", { arr(Decide.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> Conflict(a[0] as String, a[1] as WirePath, a[2] as FileKind, a[3] as Long?, a[4] as String?, a[5] as String?, a[6] as FileKind, a[7] as Long?, a[8] as String?, a[9] as String?, a[10] as List<Decide>) },
            { v -> arrayOf(v.key, v.name, v.kind, v.size, v.sizeText, v.mtimeText, v.there, v.thereSize, v.thereSizeText, v.thereMtimeText, v.answers) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Conflicts(
    val conflicts: List<Conflict>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Conflicts> = obj(
            listOf(
            Field("conflicts", { arr(Conflict.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> Conflicts(a[0] as List<Conflict>) },
            { v -> arrayOf(v.conflicts) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ConnTestResult(
    val backendHello: String?,
    val backendGaps: List<GapCount>,
    val detail: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ConnTestResult> = obj(
            listOf(
            Field("backendHello", { opt(Str) }, optional = false, nullable = true),
            Field("backendGaps", { arr(GapCount.ADAPTER) }, optional = false, nullable = false),
            Field("detail", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ConnTestResult(a[0] as String?, a[1] as List<GapCount>, a[2] as String?) },
            { v -> arrayOf(v.backendHello, v.backendGaps, v.detail) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class CopyFrom(
    val remotePath: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<CopyFrom> = obj(
            listOf(
            Field("remote_path", { Str }, optional = false, nullable = false),
            ),
            { a -> CopyFrom(a[0] as String) },
            { v -> arrayOf(v.remotePath) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class CopyItem(
    val from: WirePath,
    val to: WirePath,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<CopyItem> = obj(
            listOf(
            Field("from", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("to", { WirePath.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> CopyItem(a[0] as WirePath, a[1] as WirePath) },
            { v -> arrayOf(v.from, v.to) },
        )
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
    val x: Int,
    val y: Int,
    val visible: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Cursor> = obj(
            listOf(
            Field("x", { Int32 }, optional = false, nullable = false),
            Field("y", { Int32 }, optional = false, nullable = false),
            Field("visible", { Bool }, optional = false, nullable = false),
            ),
            { a -> Cursor(a[0] as Int, a[1] as Int, a[2] as Boolean) },
            { v -> arrayOf(v.x, v.y, v.visible) },
        )
    }
}

enum class DataClass(val wire: String) {
    TRUTH("truth"),
    CACHE("cache"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<DataClass> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class DataPathInfo(
    val label: String,
    val `class`: DataClass,
    val path: String,
    val kind: String,
    val description: String,
    val exists: Boolean,
    val sizeText: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DataPathInfo> = obj(
            listOf(
            Field("label", { Str }, optional = false, nullable = false),
            Field("class", { DataClass.ADAPTER }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            Field("kind", { Str }, optional = false, nullable = false),
            Field("description", { Str }, optional = false, nullable = false),
            Field("exists", { Bool }, optional = false, nullable = false),
            Field("sizeText", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> DataPathInfo(a[0] as String, a[1] as DataClass, a[2] as String, a[3] as String, a[4] as String, a[5] as Boolean, a[6] as String?) },
            { v -> arrayOf(v.label, v.`class`, v.path, v.kind, v.description, v.exists, v.sizeText) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class DataPathsResponse(
    val monitorDataDir: String,
    val entries: List<DataPathInfo>,
    val webviewUserDataDir: DataPathInfo?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DataPathsResponse> = obj(
            listOf(
            Field("monitorDataDir", { Str }, optional = false, nullable = false),
            Field("entries", { arr(DataPathInfo.ADAPTER) }, optional = false, nullable = false),
            Field("webviewUserDataDir", { opt(DataPathInfo.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> DataPathsResponse(a[0] as String, a[1] as List<DataPathInfo>, a[2] as DataPathInfo?) },
            { v -> arrayOf(v.monitorDataDir, v.entries, v.webviewUserDataDir) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class DataReport(
    val home: String,
    val changedFiles: List<ChangedFile>,
    val todo: List<Chore>,
    val tmux: Boolean?,
    val counts: ChoreCounts,
    val rank: Long,
    val own: List<OwnItem>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DataReport> = obj(
            listOf(
            Field("home", { Str }, optional = false, nullable = false),
            Field("changedFiles", { arr(ChangedFile.ADAPTER) }, optional = false, nullable = false),
            Field("todo", { arr(Chore.ADAPTER) }, optional = false, nullable = false),
            Field("tmux", { opt(Bool) }, optional = false, nullable = true),
            Field("counts", { ChoreCounts.ADAPTER }, optional = false, nullable = false),
            Field("rank", { Lng }, optional = false, nullable = false),
            Field("own", { arr(OwnItem.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> DataReport(a[0] as String, a[1] as List<ChangedFile>, a[2] as List<Chore>, a[3] as Boolean?, a[4] as ChoreCounts, a[5] as Long, a[6] as List<OwnItem>) },
            { v -> arrayOf(v.home, v.changedFiles, v.todo, v.tmux, v.counts, v.rank, v.own) },
        )
    }
}

enum class Decide(val wire: String) {
    OVERWRITE("overwrite"),
    SKIP("skip"),
    KEEP_BOTH("keep_both"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<Decide> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class DeleteItem(
    val rel: WirePath,
    val recursive: Boolean? = null,
    val expect: Raw? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DeleteItem> = obj(
            listOf(
            Field("rel", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("recursive", { opt(Bool) }, optional = true, nullable = true),
            Field("expect", { opt(RawJson) }, optional = true, nullable = true),
            ),
            { a -> DeleteItem(a[0] as WirePath, a[1] as Boolean?, a[2] as Raw?) },
            { v -> arrayOf(v.rel, v.recursive, v.expect) },
        )
    }
}

enum class DeployAct(val wire: String) {
    SKIP("skip"),
    DEPLOY("deploy"),
    KEEP("keep"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<DeployAct> = enumOf(entries) { it.wire }
    }
}

sealed interface DeployEnd {
    /** 判别格（`end`）那个词。 */
    val end: String

    val text: String

    @Suppress("UNCHECKED_CAST")
    data class Done(
        val act: DeployAct,
        override val text: String,
    ) : DeployEnd, Ordered {
        override val end: String get() = "done"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Failed(
        val code: String,
        override val text: String,
        val detail: String,
    ) : DeployEnd, Ordered {
        override val end: String get() = "failed"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<DeployEnd> =
            tagged<DeployEnd>(
                "end",
                listOf(
                Variant("done", DeployEnd.Done::class.java, obj(
                        listOf(
                        Field("end", { tag("done") }, optional = false, nullable = false),
                        Field("act", { DeployAct.ADAPTER }, optional = false, nullable = false),
                        Field("text", { Str }, optional = false, nullable = false),
                        ),
                        { a -> DeployEnd.Done(a[1] as DeployAct, a[2] as String) },
                        { v -> arrayOf("done", v.act, v.text) },
                    )),
                Variant("failed", DeployEnd.Failed::class.java, obj(
                        listOf(
                        Field("end", { tag("failed") }, optional = false, nullable = false),
                        Field("code", { Str }, optional = false, nullable = false),
                        Field("text", { Str }, optional = false, nullable = false),
                        Field("detail", { Str }, optional = false, nullable = false),
                        ),
                        { a -> DeployEnd.Failed(a[1] as String, a[2] as String, a[3] as String) },
                        { v -> arrayOf("failed", v.code, v.text, v.detail) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class DeployInstallReq(
    val slots: List<SlotRow>,
    val machine: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DeployInstallReq> = obj(
            listOf(
            Field("slots", { arr(SlotRow.ADAPTER) }, optional = false, nullable = false),
            Field("machine", { Str }, optional = false, nullable = false),
            ),
            { a -> DeployInstallReq(a[0] as List<SlotRow>, a[1] as String) },
            { v -> arrayOf(v.slots, v.machine) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class DeployUninstallReq(
    val slots: List<SlotRow>,
    val machine: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DeployUninstallReq> = obj(
            listOf(
            Field("slots", { arr(SlotRow.ADAPTER) }, optional = false, nullable = false),
            Field("machine", { Str }, optional = false, nullable = false),
            ),
            { a -> DeployUninstallReq(a[0] as List<SlotRow>, a[1] as String) },
            { v -> arrayOf(v.slots, v.machine) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class DiagnosticsConfig(
    val logEnabled: Boolean,
    val logLevel: String,
    val errorToast: Boolean,
    val maxFiles: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DiagnosticsConfig> = obj(
            listOf(
            Field("log_enabled", { Bool }, optional = false, nullable = false),
            Field("log_level", { Str }, optional = false, nullable = false),
            Field("error_toast", { Bool }, optional = false, nullable = false),
            Field("max_files", { Lng }, optional = false, nullable = false),
            ),
            { a -> DiagnosticsConfig(a[0] as Boolean, a[1] as String, a[2] as Boolean, a[3] as Long) },
            { v -> arrayOf(v.logEnabled, v.logLevel, v.errorToast, v.maxFiles) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class DiagnosticsReport(
    val text: String,
    val unknown: List<UnknownOnMachine>,
    val configUnknown: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DiagnosticsReport> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            Field("unknown", { arr(UnknownOnMachine.ADAPTER) }, optional = false, nullable = false),
            Field("configUnknown", { Lng }, optional = false, nullable = false),
            ),
            { a -> DiagnosticsReport(a[0] as String, a[1] as List<UnknownOnMachine>, a[2] as Long) },
            { v -> arrayOf(v.text, v.unknown, v.configUnknown) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class DialAck(
    val ok: Boolean,
    val error: String?,
    val detail: String? = null,
    val fingerprint: String?,
    val endpoint: String?,
    val winner: Endpoint?,
    val reason: DialWhy?,
    val keyType: String? = null,
    val expectedFingerprint: String? = null,
    val v: Int,
    val uses: List<String>,
    val unreached: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DialAck> = obj(
            listOf(
            Field("ok", { Bool }, optional = false, nullable = false),
            Field("error", { opt(Str) }, optional = false, nullable = true),
            Field("detail", { opt(Str) }, optional = true, nullable = true),
            Field("fingerprint", { opt(Str) }, optional = false, nullable = true),
            Field("endpoint", { opt(Str) }, optional = false, nullable = true),
            Field("winner", { opt(Endpoint.ADAPTER) }, optional = false, nullable = true),
            Field("reason", { opt(DialWhy.ADAPTER) }, optional = false, nullable = true),
            Field("key_type", { opt(Str) }, optional = true, nullable = true),
            Field("expected_fingerprint", { opt(Str) }, optional = true, nullable = true),
            Field("v", { Int32 }, optional = false, nullable = false),
            Field("uses", { arr(Str) }, optional = false, nullable = false),
            Field("unreached", { Bool }, optional = false, nullable = false),
            ),
            { a -> DialAck(a[0] as Boolean, a[1] as String?, a[2] as String?, a[3] as String?, a[4] as String?, a[5] as Endpoint?, a[6] as DialWhy?, a[7] as String?, a[8] as String?, a[9] as Int, a[10] as List<String>, a[11] as Boolean) },
            { v -> arrayOf(v.ok, v.error, v.detail, v.fingerprint, v.endpoint, v.winner, v.reason, v.keyType, v.expectedFingerprint, v.v, v.uses, v.unreached) },
        )
    }
}

sealed interface DialPlan {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    @Suppress("UNCHECKED_CAST")
    data class Ready(
        val request: DialRequest,
    ) : DialPlan, Ordered {
        override val kind: String get() = "ready"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Refused(
        val said: String,
    ) : DialPlan, Ordered {
        override val kind: String get() = "refused"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<DialPlan> =
            tagged<DialPlan>(
                "kind",
                listOf(
                Variant("ready", DialPlan.Ready::class.java, obj(
                        listOf(
                        Field("kind", { tag("ready") }, optional = false, nullable = false),
                        Field("request", { DialRequest.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> DialPlan.Ready(a[1] as DialRequest) },
                        { v -> arrayOf("ready", v.request) },
                    )),
                Variant("refused", DialPlan.Refused::class.java, obj(
                        listOf(
                        Field("kind", { tag("refused") }, optional = false, nullable = false),
                        Field("said", { Str }, optional = false, nullable = false),
                        ),
                        { a -> DialPlan.Refused(a[1] as String) },
                        { v -> arrayOf("refused", v.said) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class DialRequest(
    val host: String,
    val port: Int,
    val user: String,
    val keyPath: String?,
    val command: String,
    val endpoints: List<Endpoint>,
    val jump: JumpHop?,
    val use: DialUse,
    val capture: CaptureOpts?,
    val forward: ForwardSpec?,
    val pty: PtyOpts?,
    val stages: Boolean,
    val probe: Boolean,
    val agentSock: String?,
    val progress: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DialRequest> = obj(
            listOf(
            Field("host", { Str }, optional = false, nullable = false),
            Field("port", { Int32 }, optional = false, nullable = false),
            Field("user", { Str }, optional = false, nullable = false),
            Field("key_path", { opt(Str) }, optional = false, nullable = true),
            Field("command", { Str }, optional = false, nullable = false),
            Field("endpoints", { arr(Endpoint.ADAPTER) }, optional = false, nullable = false),
            Field("jump", { opt(JumpHop.ADAPTER) }, optional = false, nullable = true),
            Field("use", { DialUse.ADAPTER }, optional = false, nullable = false),
            Field("capture", { opt(CaptureOpts.ADAPTER) }, optional = false, nullable = true),
            Field("forward", { opt(ForwardSpec.ADAPTER) }, optional = false, nullable = true),
            Field("pty", { opt(PtyOpts.ADAPTER) }, optional = false, nullable = true),
            Field("stages", { Bool }, optional = false, nullable = false),
            Field("probe", { Bool }, optional = false, nullable = false),
            Field("agent_sock", { opt(Str) }, optional = false, nullable = true),
            Field("progress", { Bool }, optional = false, nullable = false),
            ),
            { a -> DialRequest(a[0] as String, a[1] as Int, a[2] as String, a[3] as String?, a[4] as String, a[5] as List<Endpoint>, a[6] as JumpHop?, a[7] as DialUse, a[8] as CaptureOpts?, a[9] as ForwardSpec?, a[10] as PtyOpts?, a[11] as Boolean, a[12] as Boolean, a[13] as String?, a[14] as Boolean) },
            { v -> arrayOf(v.host, v.port, v.user, v.keyPath, v.command, v.endpoints, v.jump, v.use, v.capture, v.forward, v.pty, v.stages, v.probe, v.agentSock, v.progress) },
        )
    }
}

enum class DialUse(val wire: String) {
    STREAM("stream"),
    CAPTURE("capture"),
    FORWARD("forward"),
    SHELL("shell"),
    HOLD("hold"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<DialUse> = enumOf(entries) { it.wire }
    }
}

enum class DialWhy(val wire: String) {
    RESOLVE("resolve"),
    UNREACHABLE("unreachable"),
    TIMEOUT("timeout"),
    HOST_KEY("host_key"),
    AUTH("auth"),
    PASSWORD("password"),
    KEY_UNREADABLE("key_unreadable"),
    JUMP("jump"),
    SESSION_DENIED("session_denied"),
    OTHER("other"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<DialWhy> = enumOf(entries) { it.wire }
    }
}

enum class DiffOp(val wire: String) {
    SAME("same"),
    DEL("del"),
    ADD("add"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<DiffOp> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class DiffRow(
    val n: Long?,
    val op: DiffOp,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DiffRow> = obj(
            listOf(
            Field("n", { opt(Lng) }, optional = false, nullable = true),
            Field("op", { DiffOp.ADAPTER }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> DiffRow(a[0] as Long?, a[1] as DiffOp, a[2] as String) },
            { v -> arrayOf(v.n, v.op, v.text) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class DirChecked(
    val exists: Boolean,
    val shown: String,
    val tmuxName: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DirChecked> = obj(
            listOf(
            Field("exists", { Bool }, optional = false, nullable = false),
            Field("shown", { Str }, optional = false, nullable = false),
            Field("tmuxName", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> DirChecked(a[0] as Boolean, a[1] as String, a[2] as String?) },
            { v -> arrayOf(v.exists, v.shown, v.tmuxName) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class DownloadItem(
    val remotePath: String,
    val name: WirePath? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DownloadItem> = obj(
            listOf(
            Field("remote_path", { Str }, optional = false, nullable = false),
            Field("name", { opt(WirePath.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> DownloadItem(a[0] as String, a[1] as WirePath?) },
            { v -> arrayOf(v.remotePath, v.name) },
        )
    }
}

enum class Draw(val wire: String) {
    CARD("card"),
    STEPS("steps"),
    STRIP("strip"),
    NONE("none"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<Draw> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class DriftEntry(
    val key: String,
    val count: Long,
    val firstSample: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DriftEntry> = obj(
            listOf(
            Field("key", { Str }, optional = false, nullable = false),
            Field("count", { Lng }, optional = false, nullable = false),
            Field("first_sample", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> DriftEntry(a[0] as String, a[1] as Long, a[2] as String?) },
            { v -> arrayOf(v.key, v.count, v.firstSample) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class DriftFaceReport(
    val face: String,
    val consequence: String,
    val entries: List<DriftEntry>,
    val overflowed: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DriftFaceReport> = obj(
            listOf(
            Field("face", { Str }, optional = false, nullable = false),
            Field("consequence", { Str }, optional = false, nullable = false),
            Field("entries", { arr(DriftEntry.ADAPTER) }, optional = false, nullable = false),
            Field("overflowed", { Bool }, optional = false, nullable = false),
            ),
            { a -> DriftFaceReport(a[0] as String, a[1] as String, a[2] as List<DriftEntry>, a[3] as Boolean) },
            { v -> arrayOf(v.face, v.consequence, v.entries, v.overflowed) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class DriftMerge(
    val machines: List<DriftOn>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DriftMerge> = obj(
            listOf(
            Field("machines", { arr(DriftOn.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> DriftMerge(a[0] as List<DriftOn>) },
            { v -> arrayOf(v.machines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class DriftOn(
    val origin: String,
    val report: DriftReport?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DriftOn> = obj(
            listOf(
            Field("origin", { Str }, optional = false, nullable = false),
            Field("report", { opt(DriftReport.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> DriftOn(a[0] as String, a[1] as DriftReport?) },
            { v -> arrayOf(v.origin, v.report) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class DriftReport(
    val faces: List<DriftFaceReport>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<DriftReport> = obj(
            listOf(
            Field("faces", { arr(DriftFaceReport.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> DriftReport(a[0] as List<DriftFaceReport>) },
            { v -> arrayOf(v.faces) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ElemKey(
    val fields: List<String>,
    val equals: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ElemKey> = obj(
            listOf(
            Field("fields", { arr(Str) }, optional = false, nullable = false),
            Field("equals", { Str }, optional = false, nullable = false),
            ),
            { a -> ElemKey(a[0] as List<String>, a[1] as String) },
            { v -> arrayOf(v.fields, v.equals) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class EmptyFace(
    val line: String,
    val hint: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<EmptyFace> = obj(
            listOf(
            Field("line", { Str }, optional = false, nullable = false),
            Field("hint", { Str }, optional = false, nullable = false),
            ),
            { a -> EmptyFace(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.line, v.hint) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Endpoint(
    val host: String,
    val port: Int,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Endpoint> = obj(
            listOf(
            Field("host", { Str }, optional = false, nullable = false),
            Field("port", { Int32 }, optional = false, nullable = false),
            ),
            { a -> Endpoint(a[0] as String, a[1] as Int) },
            { v -> arrayOf(v.host, v.port) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Entry(
    val starred: Boolean,
    val customTitle: String?,
    val hide: Hide? = null,
    val autoAnswer: AutoAnswer? = null,
    val resumeWith: ResumeWith? = null,
    val groups: List<String>? = null,
    val pinned: Boolean? = null,
    val updatedAt: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Entry> = obj(
            listOf(
            Field("starred", { Bool }, optional = false, nullable = false),
            Field("customTitle", { opt(Str) }, optional = false, nullable = true),
            Field("hide", { opt(Hide.ADAPTER) }, optional = true, nullable = true),
            Field("autoAnswer", { opt(AutoAnswer.ADAPTER) }, optional = true, nullable = true),
            Field("resumeWith", { opt(ResumeWith.ADAPTER) }, optional = true, nullable = true),
            Field("groups", { opt(arr(Str)) }, optional = true, nullable = true),
            Field("pinned", { opt(Bool) }, optional = true, nullable = true),
            Field("updatedAt", { Lng }, optional = false, nullable = false),
            ),
            { a -> Entry(a[0] as Boolean, a[1] as String?, a[2] as Hide?, a[3] as AutoAnswer?, a[4] as ResumeWith?, a[5] as List<String>?, a[6] as Boolean?, a[7] as Long) },
            { v -> arrayOf(v.starred, v.customTitle, v.hide, v.autoAnswer, v.resumeWith, v.groups, v.pinned, v.updatedAt) },
        )
    }
}

enum class EnvTier(val wire: String) {
    APP_INSTALLS("AppInstalls"),
    APP_SHIPS_NO_INSTALLER_YET("AppShipsNoInstallerYet"),
    USER_INSTALLS_WE_PROMPT("UserInstallsWePrompt"),
    APP_ONLY_CHECKS("AppOnlyChecks"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<EnvTier> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ExitPolicy(
    val state: ExitPolicyState,
    val killOnExit: Boolean,
    val reason: String?,
    val detail: String?,
    val path: String?,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExitPolicy> = obj(
            listOf(
            Field("state", { ExitPolicyState.ADAPTER }, optional = false, nullable = false),
            Field("killOnExit", { Bool }, optional = false, nullable = false),
            Field("reason", { opt(Str) }, optional = false, nullable = true),
            Field("detail", { opt(Str) }, optional = false, nullable = true),
            Field("path", { opt(Str) }, optional = false, nullable = true),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> ExitPolicy(a[0] as ExitPolicyState, a[1] as Boolean, a[2] as String?, a[3] as String?, a[4] as String?, a[5] as String) },
            { v -> arrayOf(v.state, v.killOnExit, v.reason, v.detail, v.path, v.said) },
        )
    }
}

data object ExitPolicyReadReq {
    val ADAPTER: JsonAdapter<ExitPolicyReadReq> = obj(emptyList(), { ExitPolicyReadReq }, { emptyArray() })
}

@Suppress("UNCHECKED_CAST")
data class ExitPolicySet(
    val killOnExit: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExitPolicySet> = obj(
            listOf(
            Field("killOnExit", { Bool }, optional = false, nullable = false),
            ),
            { a -> ExitPolicySet(a[0] as Boolean) },
            { v -> arrayOf(v.killOnExit) },
        )
    }
}

enum class ExitPolicyState(val wire: String) {
    ABSENT("absent"),
    CHOSEN("chosen"),
    UNREADABLE("unreadable"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ExitPolicyState> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtBring(
    val from: String?,
    val fromName: String,
    val scope: ExtScope,
    val targets: List<ExtTarget>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtBring> = obj(
            listOf(
            Field("from", { opt(Str) }, optional = false, nullable = true),
            Field("fromName", { Str }, optional = false, nullable = false),
            Field("scope", { ExtScope.ADAPTER }, optional = false, nullable = false),
            Field("targets", { arr(ExtTarget.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> ExtBring(a[0] as String?, a[1] as String, a[2] as ExtScope, a[3] as List<ExtTarget>) },
            { v -> arrayOf(v.from, v.fromName, v.scope, v.targets) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtCard(
    val kind: ExtKind,
    val name: String,
    val path: String,
    val writes: List<String>,
    val unchanged: Boolean,
    val suspects: List<String>,
    val stop: String?,
    val config: String?,
    val slots: List<ExtSlot>,
    val tokens: ExtTokens,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtCard> = obj(
            listOf(
            Field("kind", { ExtKind.ADAPTER }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            Field("writes", { arr(Str) }, optional = false, nullable = false),
            Field("unchanged", { Bool }, optional = false, nullable = false),
            Field("suspects", { arr(Str) }, optional = false, nullable = false),
            Field("stop", { opt(Str) }, optional = false, nullable = true),
            Field("config", { opt(Str) }, optional = false, nullable = true),
            Field("slots", { arr(ExtSlot.ADAPTER) }, optional = false, nullable = false),
            Field("tokens", { ExtTokens.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> ExtCard(a[0] as ExtKind, a[1] as String, a[2] as String, a[3] as List<String>, a[4] as Boolean, a[5] as List<String>, a[6] as String?, a[7] as String?, a[8] as List<ExtSlot>, a[9] as ExtTokens) },
            { v -> arrayOf(v.kind, v.name, v.path, v.writes, v.unchanged, v.suspects, v.stop, v.config, v.slots, v.tokens) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtCell(
    val state: ExtState,
    val said: String,
    val places: List<ExtPlace>,
    val bring: ExtBring?,
    val note: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtCell> = obj(
            listOf(
            Field("state", { ExtState.ADAPTER }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            Field("places", { arr(ExtPlace.ADAPTER) }, optional = false, nullable = false),
            Field("bring", { opt(ExtBring.ADAPTER) }, optional = false, nullable = true),
            Field("note", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ExtCell(a[0] as ExtState, a[1] as String, a[2] as List<ExtPlace>, a[3] as ExtBring?, a[4] as String?) },
            { v -> arrayOf(v.state, v.said, v.places, v.bring, v.note) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtDetail(
    val label: String,
    val value: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtDetail> = obj(
            listOf(
            Field("label", { Str }, optional = false, nullable = false),
            Field("value", { Str }, optional = false, nullable = false),
            ),
            { a -> ExtDetail(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.label, v.value) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtDone(
    val path: String,
    val changed: List<String>,
    val note: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtDone> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("changed", { arr(Str) }, optional = false, nullable = false),
            Field("note", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ExtDone(a[0] as String, a[1] as List<String>, a[2] as String?) },
            { v -> arrayOf(v.path, v.changed, v.note) },
        )
    }
}

enum class ExtKind(val wire: String) {
    SKILL("skill"),
    MCP("mcp"),
    PLUGIN("plugin"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ExtKind> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtKnown(
    val from: String,
    val note: String?,
    val hooks: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtKnown> = obj(
            listOf(
            Field("from", { Str }, optional = false, nullable = false),
            Field("note", { opt(Str) }, optional = false, nullable = true),
            Field("hooks", { Bool }, optional = false, nullable = false),
            ),
            { a -> ExtKnown(a[0] as String, a[1] as String?, a[2] as Boolean) },
            { v -> arrayOf(v.from, v.note, v.hooks) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtLegend(
    val state: ExtState,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtLegend> = obj(
            listOf(
            Field("state", { ExtState.ADAPTER }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> ExtLegend(a[0] as ExtState, a[1] as String) },
            { v -> arrayOf(v.state, v.said) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtList(
    val machines: List<ExtMachine>,
    val rows: List<ExtRow>,
    val problems: List<String>,
    val legend: List<ExtLegend>,
    val knownTitle: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtList> = obj(
            listOf(
            Field("machines", { arr(ExtMachine.ADAPTER) }, optional = false, nullable = false),
            Field("rows", { arr(ExtRow.ADAPTER) }, optional = false, nullable = false),
            Field("problems", { arr(Str) }, optional = false, nullable = false),
            Field("legend", { arr(ExtLegend.ADAPTER) }, optional = false, nullable = false),
            Field("knownTitle", { Str }, optional = false, nullable = false),
            ),
            { a -> ExtList(a[0] as List<ExtMachine>, a[1] as List<ExtRow>, a[2] as List<String>, a[3] as List<ExtLegend>, a[4] as String) },
            { v -> arrayOf(v.machines, v.rows, v.problems, v.legend, v.knownTitle) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtListReq(
    val visit: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtListReq> = obj(
            listOf(
            Field("visit", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> ExtListReq(a[0] as Boolean?) },
            { v -> arrayOf(v.visit) },
        )
    }
}

sealed interface ExtLoc {
    /** 判别格（`level`）那个词。 */
    val level: String

    data object User : ExtLoc {
        override val level: String get() = "user"
    }
    @Suppress("UNCHECKED_CAST")
    data class Project(
        val dir: String,
    ) : ExtLoc, Ordered {
        override val level: String get() = "project"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<ExtLoc> =
            tagged<ExtLoc>(
                "level",
                listOf(
                Variant("user", ExtLoc.User::class.java, obj(
                        listOf(
                        Field("level", { tag("user") }, optional = false, nullable = false),
                        ),
                        { ExtLoc.User },
                        { arrayOf("user") },
                    )),
                Variant("project", ExtLoc.Project::class.java, obj(
                        listOf(
                        Field("level", { tag("project") }, optional = false, nullable = false),
                        Field("dir", { Str }, optional = false, nullable = false),
                        ),
                        { a -> ExtLoc.Project(a[1] as String) },
                        { v -> arrayOf("project", v.dir) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtMachine(
    val key: String?,
    val here: Boolean,
    val reachable: Boolean,
    val name: String,
    val projects: List<String>,
    val mcp: Raw? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtMachine> = obj(
            listOf(
            Field("key", { opt(Str) }, optional = false, nullable = true),
            Field("here", { Bool }, optional = false, nullable = false),
            Field("reachable", { Bool }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("projects", { arr(Str) }, optional = false, nullable = false),
            Field("mcp", { opt(RawJson) }, optional = true, nullable = true),
            ),
            { a -> ExtMachine(a[0] as String?, a[1] as Boolean, a[2] as Boolean, a[3] as String, a[4] as List<String>, a[5] as Raw?) },
            { v -> arrayOf(v.key, v.here, v.reachable, v.name, v.projects, v.mcp) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtNote(
    val note: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtNote> = obj(
            listOf(
            Field("note", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ExtNote(a[0] as String?) },
            { v -> arrayOf(v.note) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtNoteReq(
    val kind: ExtKind,
    val name: String,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtNoteReq> = obj(
            listOf(
            Field("kind", { ExtKind.ADAPTER }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> ExtNoteReq(a[0] as ExtKind, a[1] as String, a[2] as String) },
            { v -> arrayOf(v.kind, v.name, v.text) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtPlace(
    val at: ExtLoc,
    val state: ExtState,
    val said: String,
    val dir: String?,
    val uninstall: Boolean,
    val note: String?,
    val line: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtPlace> = obj(
            listOf(
            Field("at", { ExtLoc.ADAPTER }, optional = false, nullable = false),
            Field("state", { ExtState.ADAPTER }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            Field("dir", { opt(Str) }, optional = false, nullable = true),
            Field("uninstall", { Bool }, optional = false, nullable = false),
            Field("note", { opt(Str) }, optional = false, nullable = true),
            Field("line", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> ExtPlace(a[0] as ExtLoc, a[1] as ExtState, a[2] as String, a[3] as String?, a[4] as Boolean, a[5] as String?, a[6] as String?) },
            { v -> arrayOf(v.at, v.state, v.said, v.dir, v.uninstall, v.note, v.line) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtRow(
    val kind: ExtKind,
    val kindSaid: String,
    val name: String,
    val about: String?,
    val detail: List<ExtDetail>,
    val new: Boolean,
    val known: ExtKnown?,
    val note: String?,
    val cells: List<ExtCell>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtRow> = obj(
            listOf(
            Field("kind", { ExtKind.ADAPTER }, optional = false, nullable = false),
            Field("kindSaid", { Str }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("about", { opt(Str) }, optional = false, nullable = true),
            Field("detail", { arr(ExtDetail.ADAPTER) }, optional = false, nullable = false),
            Field("new", { Bool }, optional = false, nullable = false),
            Field("known", { opt(ExtKnown.ADAPTER) }, optional = false, nullable = true),
            Field("note", { opt(Str) }, optional = false, nullable = true),
            Field("cells", { arr(ExtCell.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> ExtRow(a[0] as ExtKind, a[1] as String, a[2] as String, a[3] as String?, a[4] as List<ExtDetail>, a[5] as Boolean, a[6] as ExtKnown?, a[7] as String?, a[8] as List<ExtCell>) },
            { v -> arrayOf(v.kind, v.kindSaid, v.name, v.about, v.detail, v.new, v.known, v.note, v.cells) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtScope(
    val from: ExtLoc,
    val to: ExtLoc,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtScope> = obj(
            listOf(
            Field("from", { ExtLoc.ADAPTER }, optional = false, nullable = false),
            Field("to", { ExtLoc.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> ExtScope(a[0] as ExtLoc, a[1] as ExtLoc) },
            { v -> arrayOf(v.from, v.to) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtSlot(
    val field: String,
    val key: String,
    val kept: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtSlot> = obj(
            listOf(
            Field("field", { Str }, optional = false, nullable = false),
            Field("key", { Str }, optional = false, nullable = false),
            Field("kept", { Bool }, optional = false, nullable = false),
            ),
            { a -> ExtSlot(a[0] as String, a[1] as String, a[2] as Boolean) },
            { v -> arrayOf(v.field, v.key, v.kept) },
        )
    }
}

enum class ExtState(val wire: String) {
    SAME("same"),
    DIFFERS("differs"),
    MISSING("missing"),
    PROJECT("project"),
    OFF("off"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ExtState> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtTarget(
    val at: ExtLoc,
    val ok: Boolean,
    val note: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtTarget> = obj(
            listOf(
            Field("at", { ExtLoc.ADAPTER }, optional = false, nullable = false),
            Field("ok", { Bool }, optional = false, nullable = false),
            Field("note", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ExtTarget(a[0] as ExtLoc, a[1] as Boolean, a[2] as String?) },
            { v -> arrayOf(v.at, v.ok, v.note) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtTokens(
    val source: String,
    val target: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtTokens> = obj(
            listOf(
            Field("source", { Str }, optional = false, nullable = false),
            Field("target", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ExtTokens(a[0] as String, a[1] as String?) },
            { v -> arrayOf(v.source, v.target) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtUninstallApplyReq(
    val kind: ExtKind,
    val name: String,
    val at: ExtLoc,
    val token: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtUninstallApplyReq> = obj(
            listOf(
            Field("kind", { ExtKind.ADAPTER }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("at", { ExtLoc.ADAPTER }, optional = false, nullable = false),
            Field("token", { Str }, optional = false, nullable = false),
            ),
            { a -> ExtUninstallApplyReq(a[0] as ExtKind, a[1] as String, a[2] as ExtLoc, a[3] as String) },
            { v -> arrayOf(v.kind, v.name, v.at, v.token) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtUninstallCard(
    val kind: ExtKind,
    val name: String,
    val path: String,
    val recorded: Boolean,
    val files: List<String>,
    val backup: String?,
    val said: String,
    val token: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtUninstallCard> = obj(
            listOf(
            Field("kind", { ExtKind.ADAPTER }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            Field("recorded", { Bool }, optional = false, nullable = false),
            Field("files", { arr(Str) }, optional = false, nullable = false),
            Field("backup", { opt(Str) }, optional = false, nullable = true),
            Field("said", { Str }, optional = false, nullable = false),
            Field("token", { Str }, optional = false, nullable = false),
            ),
            { a -> ExtUninstallCard(a[0] as ExtKind, a[1] as String, a[2] as String, a[3] as Boolean, a[4] as List<String>, a[5] as String?, a[6] as String, a[7] as String) },
            { v -> arrayOf(v.kind, v.name, v.path, v.recorded, v.files, v.backup, v.said, v.token) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ExtUninstallReq(
    val kind: ExtKind,
    val name: String,
    val at: ExtLoc,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ExtUninstallReq> = obj(
            listOf(
            Field("kind", { ExtKind.ADAPTER }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("at", { ExtLoc.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> ExtUninstallReq(a[0] as ExtKind, a[1] as String, a[2] as ExtLoc) },
            { v -> arrayOf(v.kind, v.name, v.at) },
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
            Field("until", { opt(U64) }, optional = true, nullable = true),
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

@Suppress("UNCHECKED_CAST")
data class FactsBackground(
    val text: String,
    val clock: FactsClock?,
    val what: String?,
    val count: Long,
    val tone: Tone,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FactsBackground> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            Field("clock", { opt(FactsClock.ADAPTER) }, optional = false, nullable = true),
            Field("what", { opt(Str) }, optional = false, nullable = true),
            Field("count", { Lng }, optional = false, nullable = false),
            Field("tone", { Tone.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> FactsBackground(a[0] as String, a[1] as FactsClock?, a[2] as String?, a[3] as Long, a[4] as Tone) },
            { v -> arrayOf(v.text, v.clock, v.what, v.count, v.tone) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FactsByPath(
    val path: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FactsByPath> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            ),
            { a -> FactsByPath(a[0] as String) },
            { v -> arrayOf(v.path) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FactsBySid(
    val sid: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FactsBySid> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            ),
            { a -> FactsBySid(a[0] as String) },
            { v -> arrayOf(v.sid) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FactsClock(
    val text: String,
    val from: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FactsClock> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            Field("from", { Lng }, optional = false, nullable = false),
            ),
            { a -> FactsClock(a[0] as String, a[1] as Long) },
            { v -> arrayOf(v.text, v.from) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FactsCost(
    val micros: Long,
    val partial: Boolean,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FactsCost> = obj(
            listOf(
            Field("micros", { Lng }, optional = false, nullable = false),
            Field("partial", { Bool }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> FactsCost(a[0] as Long, a[1] as Boolean, a[2] as String) },
            { v -> arrayOf(v.micros, v.partial, v.text) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FactsLine(
    val `for`: WireSessionActivity,
    val text: String,
    val clock: FactsClock?,
    val peek: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FactsLine> = obj(
            listOf(
            Field("for", { WireSessionActivity.ADAPTER }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("clock", { opt(FactsClock.ADAPTER) }, optional = false, nullable = true),
            Field("peek", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> FactsLine(a[0] as WireSessionActivity, a[1] as String, a[2] as FactsClock?, a[3] as String?) },
            { v -> arrayOf(v.`for`, v.text, v.clock, v.peek) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FactsReply(
    val path: String? = null,
    val forkedFrom: String?,
    val touchedFiles: List<String>,
    val usage: UsageFact?,
    val modelText: String?,
    val modelPick: String?,
    val projectDir: String?,
    val agent: String?,
    val writers: List<Long>,
    val pending: List<PendingCall>,
    val lastSay: LastSay?,
    val needs: Needs?,
    val handedBack: List<String>,
    val retries: List<RetryRun>,
    val permissionMode: String?,
    val permissionModeText: String?,
    val tokens: TokenUse?,
    val cost: FactsCost?,
    val bgTasks: List<BgTask>,
    val background: FactsBackground?,
    val answerOff: String?,
    val autoAnswer: Long?,
    val autoAnswered: String?,
    val mcp: List<McpTrouble>,
    val startedBy: Kin?,
    val children: List<Kin>,
    val line: FactsLine?,
    val quietUnread: QuietUnread?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FactsReply> = obj(
            listOf(
            Field("path", { opt(Str) }, optional = true, nullable = true),
            Field("forkedFrom", { opt(Str) }, optional = false, nullable = true),
            Field("touchedFiles", { arr(Str) }, optional = false, nullable = false),
            Field("usage", { opt(UsageFact.ADAPTER) }, optional = false, nullable = true),
            Field("modelText", { opt(Str) }, optional = false, nullable = true),
            Field("modelPick", { opt(Str) }, optional = false, nullable = true),
            Field("projectDir", { opt(Str) }, optional = false, nullable = true),
            Field("agent", { opt(Str) }, optional = false, nullable = true),
            Field("writers", { arr(Lng) }, optional = false, nullable = false),
            Field("pending", { arr(PendingCall.ADAPTER) }, optional = false, nullable = false),
            Field("lastSay", { opt(LastSay.ADAPTER) }, optional = false, nullable = true),
            Field("needs", { opt(Needs.ADAPTER) }, optional = false, nullable = true),
            Field("handedBack", { arr(Str) }, optional = false, nullable = false),
            Field("retries", { arr(RetryRun.ADAPTER) }, optional = false, nullable = false),
            Field("permissionMode", { opt(Str) }, optional = false, nullable = true),
            Field("permissionModeText", { opt(Str) }, optional = false, nullable = true),
            Field("tokens", { opt(TokenUse.ADAPTER) }, optional = false, nullable = true),
            Field("cost", { opt(FactsCost.ADAPTER) }, optional = false, nullable = true),
            Field("bgTasks", { arr(BgTask.ADAPTER) }, optional = false, nullable = false),
            Field("background", { opt(FactsBackground.ADAPTER) }, optional = false, nullable = true),
            Field("answerOff", { opt(Str) }, optional = false, nullable = true),
            Field("autoAnswer", { opt(Lng) }, optional = false, nullable = true),
            Field("autoAnswered", { opt(Str) }, optional = false, nullable = true),
            Field("mcp", { arr(McpTrouble.ADAPTER) }, optional = false, nullable = false),
            Field("startedBy", { opt(Kin.ADAPTER) }, optional = false, nullable = true),
            Field("children", { arr(Kin.ADAPTER) }, optional = false, nullable = false),
            Field("line", { opt(FactsLine.ADAPTER) }, optional = false, nullable = true),
            Field("quietUnread", { opt(QuietUnread.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> FactsReply(a[0] as String?, a[1] as String?, a[2] as List<String>, a[3] as UsageFact?, a[4] as String?, a[5] as String?, a[6] as String?, a[7] as String?, a[8] as List<Long>, a[9] as List<PendingCall>, a[10] as LastSay?, a[11] as Needs?, a[12] as List<String>, a[13] as List<RetryRun>, a[14] as String?, a[15] as String?, a[16] as TokenUse?, a[17] as FactsCost?, a[18] as List<BgTask>, a[19] as FactsBackground?, a[20] as String?, a[21] as Long?, a[22] as String?, a[23] as List<McpTrouble>, a[24] as Kin?, a[25] as List<Kin>, a[26] as FactsLine?, a[27] as QuietUnread?) },
            { v -> arrayOf(v.path, v.forkedFrom, v.touchedFiles, v.usage, v.modelText, v.modelPick, v.projectDir, v.agent, v.writers, v.pending, v.lastSay, v.needs, v.handedBack, v.retries, v.permissionMode, v.permissionModeText, v.tokens, v.cost, v.bgTasks, v.background, v.answerOff, v.autoAnswer, v.autoAnswered, v.mcp, v.startedBy, v.children, v.line, v.quietUnread) },
        )
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

enum class FileState(val wire: String) {
    ABSENT("absent"),
    OK("ok"),
    UNREADABLE("unreadable"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<FileState> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesBatchStopReply(
    val stopping: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesBatchStopReply> = obj(
            listOf(
            Field("stopping", { Bool }, optional = false, nullable = false),
            ),
            { a -> FilesBatchStopReply(a[0] as Boolean) },
            { v -> arrayOf(v.stopping) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesBatchStopReq(
    val ticket: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesBatchStopReq> = obj(
            listOf(
            Field("ticket", { Str }, optional = false, nullable = false),
            ),
            { a -> FilesBatchStopReq(a[0] as String) },
            { v -> arrayOf(v.ticket) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesChmodReq(
    val root: WirePath,
    val items: List<ChmodItem>,
    val mode: String,
    val ticket: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesChmodReq> = obj(
            listOf(
            Field("root", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("items", { arr(ChmodItem.ADAPTER) }, optional = false, nullable = false),
            Field("mode", { Str }, optional = false, nullable = false),
            Field("ticket", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> FilesChmodReq(a[0] as WirePath, a[1] as List<ChmodItem>, a[2] as String, a[3] as String?) },
            { v -> arrayOf(v.root, v.items, v.mode, v.ticket) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesCommitTextReq(
    val root: WirePath,
    val rel: WirePath,
    val key: String,
    val chunks: Long,
    val bytes: Long,
    val expect: Sha256Expect,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesCommitTextReq> = obj(
            listOf(
            Field("root", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("rel", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("key", { Str }, optional = false, nullable = false),
            Field("chunks", { U64 }, optional = false, nullable = false),
            Field("bytes", { U64 }, optional = false, nullable = false),
            Field("expect", { Sha256Expect.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> FilesCommitTextReq(a[0] as WirePath, a[1] as WirePath, a[2] as String, a[3] as Long, a[4] as Long, a[5] as Sha256Expect) },
            { v -> arrayOf(v.root, v.rel, v.key, v.chunks, v.bytes, v.expect) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesCommitUploadReq(
    val root: WirePath,
    val rel: WirePath,
    val key: String,
    val onExists: OnExists? = null,
    val expect: Sha256Expect,
    val chunks: Long? = null,
    val bytes: Long? = null,
    val single: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesCommitUploadReq> = obj(
            listOf(
            Field("root", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("rel", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("key", { Str }, optional = false, nullable = false),
            Field("on_exists", { opt(OnExists.ADAPTER) }, optional = true, nullable = true),
            Field("expect", { Sha256Expect.ADAPTER }, optional = false, nullable = false),
            Field("chunks", { opt(U64) }, optional = true, nullable = true),
            Field("bytes", { opt(U64) }, optional = true, nullable = true),
            Field("single", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> FilesCommitUploadReq(a[0] as WirePath, a[1] as WirePath, a[2] as String, a[3] as OnExists?, a[4] as Sha256Expect, a[5] as Long?, a[6] as Long?, a[7] as Boolean?) },
            { v -> arrayOf(v.root, v.rel, v.key, v.onExists, v.expect, v.chunks, v.bytes, v.single) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesCopyReq(
    val root: WirePath,
    val items: List<CopyItem>,
    val onExists: OnExists? = null,
    val decide: Map<String, Decide>? = null,
    val ticket: String? = null,
    val single: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesCopyReq> = obj(
            listOf(
            Field("root", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("items", { arr(CopyItem.ADAPTER) }, optional = false, nullable = false),
            Field("on_exists", { opt(OnExists.ADAPTER) }, optional = true, nullable = true),
            Field("decide", { opt(dict(Decide.ADAPTER)) }, optional = true, nullable = true),
            Field("ticket", { opt(Str) }, optional = true, nullable = true),
            Field("single", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> FilesCopyReq(a[0] as WirePath, a[1] as List<CopyItem>, a[2] as OnExists?, a[3] as Map<String, Decide>?, a[4] as String?, a[5] as Boolean?) },
            { v -> arrayOf(v.root, v.items, v.onExists, v.decide, v.ticket, v.single) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesCreateReq(
    val root: WirePath,
    val rel: WirePath,
    val content: WirePath? = null,
    val single: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesCreateReq> = obj(
            listOf(
            Field("root", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("rel", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("content", { opt(WirePath.ADAPTER) }, optional = true, nullable = true),
            Field("single", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> FilesCreateReq(a[0] as WirePath, a[1] as WirePath, a[2] as WirePath?, a[3] as Boolean?) },
            { v -> arrayOf(v.root, v.rel, v.content, v.single) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesDeleteReq(
    val root: WirePath,
    val items: List<DeleteItem>,
    val ticket: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesDeleteReq> = obj(
            listOf(
            Field("root", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("items", { arr(DeleteItem.ADAPTER) }, optional = false, nullable = false),
            Field("ticket", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> FilesDeleteReq(a[0] as WirePath, a[1] as List<DeleteItem>, a[2] as String?) },
            { v -> arrayOf(v.root, v.items, v.ticket) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesDeleteSessionReq(
    val sid: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesDeleteSessionReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            ),
            { a -> FilesDeleteSessionReq(a[0] as String) },
            { v -> arrayOf(v.sid) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesExtractReq(
    val root: WirePath,
    val rel: WirePath,
    val fresh: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesExtractReq> = obj(
            listOf(
            Field("root", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("rel", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("fresh", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> FilesExtractReq(a[0] as WirePath, a[1] as WirePath, a[2] as Boolean?) },
            { v -> arrayOf(v.root, v.rel, v.fresh) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesFindHit(
    val path: WirePath,
    val kind: FileKind,
    val `class`: FileClass,
    val location: WirePath,
    val size: Long?,
    val mtimeSecs: Long?,
    val mtimeText: String?,
    val marks: List<FilesFindHitMarks>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesFindHit> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("kind", { FileKind.ADAPTER }, optional = false, nullable = false),
            Field("class", { FileClass.ADAPTER }, optional = false, nullable = false),
            Field("location", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("size", { opt(U64) }, optional = false, nullable = true),
            Field("mtime_secs", { opt(U64) }, optional = false, nullable = true),
            Field("mtime_text", { opt(Str) }, optional = false, nullable = true),
            Field("marks", { arr(FilesFindHitMarks.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> FilesFindHit(a[0] as WirePath, a[1] as FileKind, a[2] as FileClass, a[3] as WirePath, a[4] as Long?, a[5] as Long?, a[6] as String?, a[7] as List<FilesFindHitMarks>) },
            { v -> arrayOf(v.path, v.kind, v.`class`, v.location, v.size, v.mtimeSecs, v.mtimeText, v.marks) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesFindHitMarks(
    val p0: Long,
    val p1: Long,
) {
    companion object {
        val ADAPTER: JsonAdapter<FilesFindHitMarks> = tuple(
            listOf({ Lng }, { Lng }),
            { a -> FilesFindHitMarks(a[0] as Long, a[1] as Long) },
            { v -> arrayOf(v.p0, v.p1) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesMkdirReq(
    val root: WirePath,
    val rel: WirePath,
    val single: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesMkdirReq> = obj(
            listOf(
            Field("root", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("rel", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("single", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> FilesMkdirReq(a[0] as WirePath, a[1] as WirePath, a[2] as Boolean?) },
            { v -> arrayOf(v.root, v.rel, v.single) },
        )
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
            Field("bytes", { U64 }, optional = false, nullable = false),
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
data class FilesRenameReq(
    val root: WirePath,
    val from: WirePath,
    val to: WirePath,
    val single: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesRenameReq> = obj(
            listOf(
            Field("root", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("from", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("to", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("single", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> FilesRenameReq(a[0] as WirePath, a[1] as WirePath, a[2] as WirePath, a[3] as Boolean?) },
            { v -> arrayOf(v.root, v.from, v.to, v.single) },
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
            Field("bytes", { U64 }, optional = false, nullable = false),
            ),
            { a -> FilesStageChunkReply(a[0] as Long) },
            { v -> arrayOf(v.bytes) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesStageChunkReq(
    val key: String,
    val seq: Long,
    val content: ChunkBytes,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesStageChunkReq> = obj(
            listOf(
            Field("key", { Str }, optional = false, nullable = false),
            Field("seq", { U64 }, optional = false, nullable = false),
            Field("content", { ChunkBytes.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> FilesStageChunkReq(a[0] as String, a[1] as Long, a[2] as ChunkBytes) },
            { v -> arrayOf(v.key, v.seq, v.content) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesTreeReply(
    val path: WirePath,
    val bytes: Long,
    val files: Long,
    val dirs: Long,
    val links: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesTreeReply> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("bytes", { U64 }, optional = false, nullable = false),
            Field("files", { U64 }, optional = false, nullable = false),
            Field("dirs", { U64 }, optional = false, nullable = false),
            Field("links", { U64 }, optional = false, nullable = false),
            ),
            { a -> FilesTreeReply(a[0] as WirePath, a[1] as Long, a[2] as Long, a[3] as Long, a[4] as Long) },
            { v -> arrayOf(v.path, v.bytes, v.files, v.dirs, v.links) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilesWriteTextReq(
    val root: WirePath,
    val rel: WirePath,
    val content: WirePath,
    val expect: Sha256Expect,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilesWriteTextReq> = obj(
            listOf(
            Field("root", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("rel", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("content", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("expect", { Sha256Expect.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> FilesWriteTextReq(a[0] as WirePath, a[1] as WirePath, a[2] as WirePath, a[3] as Sha256Expect) },
            { v -> arrayOf(v.root, v.rel, v.content, v.expect) },
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
            Field("bytes", { U64 }, optional = false, nullable = false),
            Field("sha256", { Str }, optional = false, nullable = false),
            ),
            { a -> FilesWrittenReply(a[0] as WirePath, a[1] as Long, a[2] as String) },
            { v -> arrayOf(v.path, v.bytes, v.sha256) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FilewinTotals(
    val windows: Long,
    val unsaved: Long,
    val running: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FilewinTotals> = obj(
            listOf(
            Field("windows", { Lng }, optional = false, nullable = false),
            Field("unsaved", { Lng }, optional = false, nullable = false),
            Field("running", { Lng }, optional = false, nullable = false),
            ),
            { a -> FilewinTotals(a[0] as Long, a[1] as Long, a[2] as Long) },
            { v -> arrayOf(v.windows, v.unsaved, v.running) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FindHit(
    val uuid: String,
    val line: Long,
    val kind: String,
    val before: String,
    val matched: String,
    val after: String,
    val turn: Long,
    val tsMs: Long,
    val tsText: String,
    val tool: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FindHit> = obj(
            listOf(
            Field("uuid", { Str }, optional = false, nullable = false),
            Field("line", { Lng }, optional = false, nullable = false),
            Field("kind", { Str }, optional = false, nullable = false),
            Field("before", { Str }, optional = false, nullable = false),
            Field("matched", { Str }, optional = false, nullable = false),
            Field("after", { Str }, optional = false, nullable = false),
            Field("turn", { Lng }, optional = false, nullable = false),
            Field("tsMs", { Lng }, optional = false, nullable = false),
            Field("tsText", { Str }, optional = false, nullable = false),
            Field("tool", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> FindHit(a[0] as String, a[1] as Long, a[2] as String, a[3] as String, a[4] as String, a[5] as String, a[6] as Long, a[7] as Long, a[8] as String, a[9] as String?) },
            { v -> arrayOf(v.uuid, v.line, v.kind, v.before, v.matched, v.after, v.turn, v.tsMs, v.tsText, v.tool) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FindReply(
    val hits: List<FilesFindHit>,
    val start: WirePath?,
    val sort: FindSort,
    val desc: Boolean,
    val totalHits: Long,
    val truncated: Boolean,
    val scanned: Long,
    val indexAgeSecs: Long,
    val indexMissing: Boolean,
    val stale: Boolean,
    val indexRoot: WirePath?,
    val outOfIndex: Boolean,
    val coverRoot: WirePath?,
    val seq: Long?,
    val offset: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FindReply> = obj(
            listOf(
            Field("hits", { arr(FilesFindHit.ADAPTER) }, optional = false, nullable = false),
            Field("start", { opt(WirePath.ADAPTER) }, optional = false, nullable = true),
            Field("sort", { FindSort.ADAPTER }, optional = false, nullable = false),
            Field("desc", { Bool }, optional = false, nullable = false),
            Field("total_hits", { U64 }, optional = false, nullable = false),
            Field("truncated", { Bool }, optional = false, nullable = false),
            Field("scanned", { U64 }, optional = false, nullable = false),
            Field("index_age_secs", { U64 }, optional = false, nullable = false),
            Field("index_missing", { Bool }, optional = false, nullable = false),
            Field("stale", { Bool }, optional = false, nullable = false),
            Field("index_root", { opt(WirePath.ADAPTER) }, optional = false, nullable = true),
            Field("out_of_index", { Bool }, optional = false, nullable = false),
            Field("cover_root", { opt(WirePath.ADAPTER) }, optional = false, nullable = true),
            Field("seq", { opt(U64) }, optional = false, nullable = true),
            Field("offset", { U64 }, optional = false, nullable = false),
            ),
            { a -> FindReply(a[0] as List<FilesFindHit>, a[1] as WirePath?, a[2] as FindSort, a[3] as Boolean, a[4] as Long, a[5] as Boolean, a[6] as Long, a[7] as Long, a[8] as Boolean, a[9] as Boolean, a[10] as WirePath?, a[11] as Boolean, a[12] as WirePath?, a[13] as Long?, a[14] as Long) },
            { v -> arrayOf(v.hits, v.start, v.sort, v.desc, v.totalHits, v.truncated, v.scanned, v.indexAgeSecs, v.indexMissing, v.stale, v.indexRoot, v.outOfIndex, v.coverRoot, v.seq, v.offset) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FindReq(
    val query: String,
    val under: WirePath? = null,
    val scope: FindScope? = null,
    val seq: Long? = null,
    val stream: String? = null,
    val offset: Long? = null,
    val limit: Long? = null,
    val sort: FindSort? = null,
    val desc: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FindReq> = obj(
            listOf(
            Field("query", { Str }, optional = false, nullable = false),
            Field("under", { opt(WirePath.ADAPTER) }, optional = true, nullable = true),
            Field("scope", { opt(FindScope.ADAPTER) }, optional = true, nullable = true),
            Field("seq", { opt(U64) }, optional = true, nullable = true),
            Field("stream", { opt(Str) }, optional = true, nullable = true),
            Field("offset", { opt(U64) }, optional = true, nullable = true),
            Field("limit", { opt(U64) }, optional = true, nullable = true),
            Field("sort", { opt(FindSort.ADAPTER) }, optional = true, nullable = true),
            Field("desc", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> FindReq(a[0] as String, a[1] as WirePath?, a[2] as FindScope?, a[3] as Long?, a[4] as String?, a[5] as Long?, a[6] as Long?, a[7] as FindSort?, a[8] as Boolean?) },
            { v -> arrayOf(v.query, v.under, v.scope, v.seq, v.stream, v.offset, v.limit, v.sort, v.desc) },
        )
    }
}

enum class FindScope(val wire: String) {
    UNDER("under"),
    MACHINE("machine"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<FindScope> = enumOf(entries) { it.wire }
    }
}

enum class FindSort(val wire: String) {
    RELEVANCE("relevance"),
    NAME("name"),
    LOCATION("location"),
    MTIME("mtime"),
    SIZE("size"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<FindSort> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class FirstRun(
    val steps: List<FirstRunStep>,
    val left: Long,
    val skipped: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FirstRun> = obj(
            listOf(
            Field("steps", { arr(FirstRunStep.ADAPTER) }, optional = false, nullable = false),
            Field("left", { Lng }, optional = false, nullable = false),
            Field("skipped", { Bool }, optional = false, nullable = false),
            ),
            { a -> FirstRun(a[0] as List<FirstRunStep>, a[1] as Long, a[2] as Boolean) },
            { v -> arrayOf(v.steps, v.left, v.skipped) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FirstRunReq(
    val remotes: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FirstRunReq> = obj(
            listOf(
            Field("remotes", { Lng }, optional = false, nullable = false),
            ),
            { a -> FirstRunReq(a[0] as Long) },
            { v -> arrayOf(v.remotes) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FirstRunStep(
    val id: FirstRunStepId,
    val done: Boolean,
    val required: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FirstRunStep> = obj(
            listOf(
            Field("id", { FirstRunStepId.ADAPTER }, optional = false, nullable = false),
            Field("done", { Bool }, optional = false, nullable = false),
            Field("required", { Bool }, optional = false, nullable = false),
            ),
            { a -> FirstRunStep(a[0] as FirstRunStepId, a[1] as Boolean, a[2] as Boolean) },
            { v -> arrayOf(v.id, v.done, v.required) },
        )
    }
}

enum class FirstRunStepId(val wire: String) {
    TERMINAL("terminal"),
    NAMED("named"),
    REMOTE("remote"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<FirstRunStepId> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class FoldFail(
    val uuid: String,
    val tool: String?,
    val line: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FoldFail> = obj(
            listOf(
            Field("uuid", { Str }, optional = false, nullable = false),
            Field("tool", { opt(Str) }, optional = false, nullable = true),
            Field("line", { Lng }, optional = false, nullable = false),
            ),
            { a -> FoldFail(a[0] as String, a[1] as String?, a[2] as Long) },
            { v -> arrayOf(v.uuid, v.tool, v.line) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FoldSegment(
    val after: String,
    val afterLine: Long,
    val parts: List<TurnPart>,
    val span: TurnSpan,
    val until: String?,
    val untilLine: Long?,
    val phase: TurnPhase,
    val firstFail: FoldFail?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FoldSegment> = obj(
            listOf(
            Field("after", { Str }, optional = false, nullable = false),
            Field("afterLine", { Lng }, optional = false, nullable = false),
            Field("parts", { arr(TurnPart.ADAPTER) }, optional = false, nullable = false),
            Field("span", { TurnSpan.ADAPTER }, optional = false, nullable = false),
            Field("until", { opt(Str) }, optional = false, nullable = true),
            Field("untilLine", { opt(Lng) }, optional = false, nullable = true),
            Field("phase", { TurnPhase.ADAPTER }, optional = false, nullable = false),
            Field("firstFail", { opt(FoldFail.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> FoldSegment(a[0] as String, a[1] as Long, a[2] as List<TurnPart>, a[3] as TurnSpan, a[4] as String?, a[5] as Long?, a[6] as TurnPhase, a[7] as FoldFail?) },
            { v -> arrayOf(v.after, v.afterLine, v.parts, v.span, v.until, v.untilLine, v.phase, v.firstFail) },
        )
    }
}

enum class FollowEnd(val wire: String) {
    GONE("gone"),
    LOST("lost"),
    TOO_BIG("too_big"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<FollowEnd> = enumOf(entries) { it.wire }
    }
}

enum class FollowLive(val wire: String) {
    SNAPSHOT_ONLY("snapshot_only"),
    STOPPED("stopped"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<FollowLive> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class FollowRefused(
    val live: FollowLive,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FollowRefused> = obj(
            listOf(
            Field("live", { FollowLive.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> FollowRefused(a[0] as FollowLive) },
            { v -> arrayOf(v.live) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FollowReq(
    val ticket: String,
    val terminal: String? = null,
    val sid: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FollowReq> = obj(
            listOf(
            Field("ticket", { Str }, optional = false, nullable = false),
            Field("terminal", { opt(Str) }, optional = true, nullable = true),
            Field("sid", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> FollowReq(a[0] as String, a[1] as String?, a[2] as String?) },
            { v -> arrayOf(v.ticket, v.terminal, v.sid) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FootprintClient(
    val home: String,
    val path: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FootprintClient> = obj(
            listOf(
            Field("home", { Str }, optional = false, nullable = false),
            Field("path", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> FootprintClient(a[0] as String, a[1] as String?) },
            { v -> arrayOf(v.home, v.path) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class FootprintReq(
    val client: FootprintClient? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FootprintReq> = obj(
            listOf(
            Field("client", { opt(FootprintClient.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> FootprintReq(a[0] as FootprintClient?) },
            { v -> arrayOf(v.client) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ForgetReq(
    val sid: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ForgetReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            ),
            { a -> ForgetReq(a[0] as String) },
            { v -> arrayOf(v.sid) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Forgotten(
    val removed: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Forgotten> = obj(
            listOf(
            Field("removed", { Bool }, optional = false, nullable = false),
            ),
            { a -> Forgotten(a[0] as Boolean) },
            { v -> arrayOf(v.removed) },
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
            Field("turn", { opt(U64) }, optional = false, nullable = true),
            Field("startText", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ForkFacts(a[0] as String, a[1] as ForkLaunch, a[2] as Long?, a[3] as String?) },
            { v -> arrayOf(v.agent, v.launch, v.turn, v.startText) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ForkFrom(
    val sid: String,
    val uuid: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ForkFrom> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("uuid", { Str }, optional = false, nullable = false),
            ),
            { a -> ForkFrom(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.sid, v.uuid) },
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

@Suppress("UNCHECKED_CAST")
data class ForkReq(
    val sid: String,
    val uuid: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ForkReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("uuid", { Str }, optional = false, nullable = false),
            ),
            { a -> ForkReq(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.sid, v.uuid) },
        )
    }
}

sealed interface ForkSlot<T> {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    @Suppress("UNCHECKED_CAST")
    data class Known<T>(
        val value: T,
        val from: ForkWhence,
    ) : ForkSlot<T>, Ordered {
        override val kind: String get() = "known"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Unknown<T>(
        val why: ForkWhy,
    ) : ForkSlot<T>, Ordered {
        override val kind: String get() = "unknown"
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
    /** 判别格（`host`）那个词。 */
    val host: String

    @Suppress("UNCHECKED_CAST")
    data class Tmux(
        val terminal: String,
    ) : ForkTerminal, Ordered {
        override val host: String get() = "tmux"
        override var wireOrder: IntArray? = null
    }
    data object None : ForkTerminal {
        override val host: String get() = "none"
    }

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
data class Forked(
    val sessionId: String,
    val jsonlPath: String,
    val launch: ForkLaunch,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Forked> = obj(
            listOf(
            Field("sessionId", { Str }, optional = false, nullable = false),
            Field("jsonlPath", { Str }, optional = false, nullable = false),
            Field("launch", { ForkLaunch.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> Forked(a[0] as String, a[1] as String, a[2] as ForkLaunch) },
            { v -> arrayOf(v.sessionId, v.jsonlPath, v.launch) },
        )
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

@Suppress("UNCHECKED_CAST")
data class ForwardId(
    val id: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ForwardId> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            ),
            { a -> ForwardId(a[0] as String) },
            { v -> arrayOf(v.id) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ForwardRow(
    val id: String,
    val origin: String,
    val localPort: Long,
    val remoteHost: String,
    val remotePort: Long,
    val state: ForwardRowState,
    val connCount: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ForwardRow> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("origin", { Str }, optional = false, nullable = false),
            Field("localPort", { Lng }, optional = false, nullable = false),
            Field("remoteHost", { Str }, optional = false, nullable = false),
            Field("remotePort", { Lng }, optional = false, nullable = false),
            Field("state", { ForwardRowState.ADAPTER }, optional = false, nullable = false),
            Field("connCount", { Lng }, optional = false, nullable = false),
            ),
            { a -> ForwardRow(a[0] as String, a[1] as String, a[2] as Long, a[3] as String, a[4] as Long, a[5] as ForwardRowState, a[6] as Long) },
            { v -> arrayOf(v.id, v.origin, v.localPort, v.remoteHost, v.remotePort, v.state, v.connCount) },
        )
    }
}

enum class ForwardRowState(val wire: String) {
    RUNNING("running"),
    ERROR("error"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ForwardRowState> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ForwardSpec(
    val localPort: Int,
    val remoteHost: String,
    val remotePort: Int,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ForwardSpec> = obj(
            listOf(
            Field("local_port", { Int32 }, optional = false, nullable = false),
            Field("remote_host", { Str }, optional = false, nullable = false),
            Field("remote_port", { Int32 }, optional = false, nullable = false),
            ),
            { a -> ForwardSpec(a[0] as Int, a[1] as String, a[2] as Int) },
            { v -> arrayOf(v.localPort, v.remoteHost, v.remotePort) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ForwardStartReq(
    val origin: String,
    val localPort: String,
    val remoteHost: String,
    val remotePort: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ForwardStartReq> = obj(
            listOf(
            Field("origin", { Str }, optional = false, nullable = false),
            Field("localPort", { Str }, optional = false, nullable = false),
            Field("remoteHost", { Str }, optional = false, nullable = false),
            Field("remotePort", { Str }, optional = false, nullable = false),
            ),
            { a -> ForwardStartReq(a[0] as String, a[1] as String, a[2] as String, a[3] as String) },
            { v -> arrayOf(v.origin, v.localPort, v.remoteHost, v.remotePort) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ForwardStarted(
    val id: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ForwardStarted> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            ),
            { a -> ForwardStarted(a[0] as String) },
            { v -> arrayOf(v.id) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Forwards(
    val forwards: List<ForwardRow>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Forwards> = obj(
            listOf(
            Field("forwards", { arr(ForwardRow.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> Forwards(a[0] as List<ForwardRow>) },
            { v -> arrayOf(v.forwards) },
        )
    }
}

sealed interface FrontAct {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    data object Connect : FrontAct {
        override val kind: String get() = "connect"
    }
    data object OpenInTerminal : FrontAct {
        override val kind: String get() = "open-in-terminal"
    }
    data object OpenHere : FrontAct {
        override val kind: String get() = "open-here"
    }
    @Suppress("UNCHECKED_CAST")
    data class Copy(
        val detail: String,
    ) : FrontAct, Ordered {
        override val kind: String get() = "copy"
        override var wireOrder: IntArray? = null
    }
    data object Update : FrontAct {
        override val kind: String get() = "update"
    }
    data object Retry : FrontAct {
        override val kind: String get() = "retry"
    }
    data object Reconnect : FrontAct {
        override val kind: String get() = "reconnect"
    }

    companion object {
        val ADAPTER: JsonAdapter<FrontAct> =
            tagged<FrontAct>(
                "kind",
                listOf(
                Variant("connect", FrontAct.Connect::class.java, obj(
                        listOf(
                        Field("kind", { tag("connect") }, optional = false, nullable = false),
                        ),
                        { FrontAct.Connect },
                        { arrayOf("connect") },
                    )),
                Variant("open-in-terminal", FrontAct.OpenInTerminal::class.java, obj(
                        listOf(
                        Field("kind", { tag("open-in-terminal") }, optional = false, nullable = false),
                        ),
                        { FrontAct.OpenInTerminal },
                        { arrayOf("open-in-terminal") },
                    )),
                Variant("open-here", FrontAct.OpenHere::class.java, obj(
                        listOf(
                        Field("kind", { tag("open-here") }, optional = false, nullable = false),
                        ),
                        { FrontAct.OpenHere },
                        { arrayOf("open-here") },
                    )),
                Variant("copy", FrontAct.Copy::class.java, obj(
                        listOf(
                        Field("kind", { tag("copy") }, optional = false, nullable = false),
                        Field("detail", { Str }, optional = false, nullable = false),
                        ),
                        { a -> FrontAct.Copy(a[1] as String) },
                        { v -> arrayOf("copy", v.detail) },
                    )),
                Variant("update", FrontAct.Update::class.java, obj(
                        listOf(
                        Field("kind", { tag("update") }, optional = false, nullable = false),
                        ),
                        { FrontAct.Update },
                        { arrayOf("update") },
                    )),
                Variant("retry", FrontAct.Retry::class.java, obj(
                        listOf(
                        Field("kind", { tag("retry") }, optional = false, nullable = false),
                        ),
                        { FrontAct.Retry },
                        { arrayOf("retry") },
                    )),
                Variant("reconnect", FrontAct.Reconnect::class.java, obj(
                        listOf(
                        Field("kind", { tag("reconnect") }, optional = false, nullable = false),
                        ),
                        { FrontAct.Reconnect },
                        { arrayOf("reconnect") },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class FrontFace(
    val title: String,
    val body: String,
    val hint: String? = null,
    val tone: Tone,
    val acts: List<FrontAct>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FrontFace> = obj(
            listOf(
            Field("title", { Str }, optional = false, nullable = false),
            Field("body", { Str }, optional = false, nullable = false),
            Field("hint", { opt(Str) }, optional = true, nullable = true),
            Field("tone", { Tone.ADAPTER }, optional = false, nullable = false),
            Field("acts", { arr(FrontAct.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> FrontFace(a[0] as String, a[1] as String, a[2] as String?, a[3] as Tone, a[4] as List<FrontAct>) },
            { v -> arrayOf(v.title, v.body, v.hint, v.tone, v.acts) },
        )
    }
}

sealed interface FrontTold {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    data object Switched : FrontTold {
        override val kind: String get() = "switched"
    }
    @Suppress("UNCHECKED_CAST")
    data class Said(
        val title: String,
        val body: String,
        val hint: String? = null,
        val tone: Tone,
        val acts: List<FrontAct>,
    ) : FrontTold, Ordered {
        override val kind: String get() = "said"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<FrontTold> =
            tagged<FrontTold>(
                "kind",
                listOf(
                Variant("switched", FrontTold.Switched::class.java, obj(
                        listOf(
                        Field("kind", { tag("switched") }, optional = false, nullable = false),
                        ),
                        { FrontTold.Switched },
                        { arrayOf("switched") },
                    )),
                Variant("said", FrontTold.Said::class.java, obj(
                        listOf(
                        Field("kind", { tag("said") }, optional = false, nullable = false),
                        Field("title", { Str }, optional = false, nullable = false),
                        Field("body", { Str }, optional = false, nullable = false),
                        Field("hint", { opt(Str) }, optional = true, nullable = true),
                        Field("tone", { Tone.ADAPTER }, optional = false, nullable = false),
                        Field("acts", { arr(FrontAct.ADAPTER) }, optional = false, nullable = false),
                        ),
                        { a -> FrontTold.Said(a[1] as String, a[2] as String, a[3] as String?, a[4] as Tone, a[5] as List<FrontAct>) },
                        { v -> arrayOf("said", v.title, v.body, v.hint, v.tone, v.acts) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class FrontendReadyPayload(
    val prioritySid: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<FrontendReadyPayload> = obj(
            listOf(
            Field("prioritySid", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> FrontendReadyPayload(a[0] as String?) },
            { v -> arrayOf(v.prioritySid) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class GapCount(
    val code: String,
    val count: Long,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<GapCount> = obj(
            listOf(
            Field("code", { Str }, optional = false, nullable = false),
            Field("count", { Lng }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> GapCount(a[0] as String, a[1] as Long, a[2] as String) },
            { v -> arrayOf(v.code, v.count, v.said) },
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
data class Gone(
    val ambiguous: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Gone> = obj(
            listOf(
            Field("ambiguous", { Bool }, optional = false, nullable = false),
            ),
            { a -> Gone(a[0] as Boolean) },
            { v -> arrayOf(v.ambiguous) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class GrepHit(
    val path: WirePath,
    val rel: WirePath,
    val line: Long,
    val text: WirePath,
    val matches: Long,
    val lines: List<GrepLine>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<GrepHit> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("rel", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("line", { U64 }, optional = false, nullable = false),
            Field("text", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("matches", { U64 }, optional = false, nullable = false),
            Field("lines", { arr(GrepLine.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> GrepHit(a[0] as WirePath, a[1] as WirePath, a[2] as Long, a[3] as WirePath, a[4] as Long, a[5] as List<GrepLine>) },
            { v -> arrayOf(v.path, v.rel, v.line, v.text, v.matches, v.lines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class GrepLine(
    val line: Long,
    val text: WirePath,
    val marks: List<GrepLineMarks>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<GrepLine> = obj(
            listOf(
            Field("line", { U64 }, optional = false, nullable = false),
            Field("text", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("marks", { arr(GrepLineMarks.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> GrepLine(a[0] as Long, a[1] as WirePath, a[2] as List<GrepLineMarks>) },
            { v -> arrayOf(v.line, v.text, v.marks) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class GrepLineMarks(
    val p0: Long,
    val p1: Long,
) {
    companion object {
        val ADAPTER: JsonAdapter<GrepLineMarks> = tuple(
            listOf({ Lng }, { Lng }),
            { a -> GrepLineMarks(a[0] as Long, a[1] as Long) },
            { v -> arrayOf(v.p0, v.p1) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class GrepReply(
    val path: WirePath,
    val hits: List<GrepHit>,
    val files: Long,
    val bytes: Long,
    val links: Long,
    val skippedBinary: Long,
    val skippedLarge: Long,
    val skippedMounts: Long,
    val unreadable: Long,
    val limit: Long,
    val truncated: Boolean,
    val stopped: GrepStopped?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<GrepReply> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("hits", { arr(GrepHit.ADAPTER) }, optional = false, nullable = false),
            Field("files", { U64 }, optional = false, nullable = false),
            Field("bytes", { U64 }, optional = false, nullable = false),
            Field("links", { U64 }, optional = false, nullable = false),
            Field("skipped_binary", { U64 }, optional = false, nullable = false),
            Field("skipped_large", { U64 }, optional = false, nullable = false),
            Field("skipped_mounts", { U64 }, optional = false, nullable = false),
            Field("unreadable", { U64 }, optional = false, nullable = false),
            Field("limit", { U64 }, optional = false, nullable = false),
            Field("truncated", { Bool }, optional = false, nullable = false),
            Field("stopped", { opt(GrepStopped.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> GrepReply(a[0] as WirePath, a[1] as List<GrepHit>, a[2] as Long, a[3] as Long, a[4] as Long, a[5] as Long, a[6] as Long, a[7] as Long, a[8] as Long, a[9] as Long, a[10] as Boolean, a[11] as GrepStopped?) },
            { v -> arrayOf(v.path, v.hits, v.files, v.bytes, v.links, v.skippedBinary, v.skippedLarge, v.skippedMounts, v.unreadable, v.limit, v.truncated, v.stopped) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class GrepReq(
    val path: WirePath,
    val needle: WirePath,
    val ignoreAsciiCase: Boolean? = null,
    val limit: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<GrepReq> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("needle", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("ignore_ascii_case", { opt(Bool) }, optional = true, nullable = true),
            Field("limit", { opt(U64) }, optional = true, nullable = true),
            ),
            { a -> GrepReq(a[0] as WirePath, a[1] as WirePath, a[2] as Boolean?, a[3] as Long?) },
            { v -> arrayOf(v.path, v.needle, v.ignoreAsciiCase, v.limit) },
        )
    }
}

enum class GrepStopped(val wire: String) {
    HITS("hits"),
    BYTES("bytes"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<GrepStopped> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class GroupCount(
    val dot: SessionDot,
    val n: Long,
    val word: String,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<GroupCount> = obj(
            listOf(
            Field("dot", { SessionDot.ADAPTER }, optional = false, nullable = false),
            Field("n", { Lng }, optional = false, nullable = false),
            Field("word", { Str }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> GroupCount(a[0] as SessionDot, a[1] as Long, a[2] as String, a[3] as String) },
            { v -> arrayOf(v.dot, v.n, v.word, v.text) },
        )
    }
}

sealed interface GroupKnown {
    /** 判别格（`is`）那个词。 */
    val `is`: String

    data object Now : GroupKnown {
        override val `is`: String get() = "now"
    }
    @Suppress("UNCHECKED_CAST")
    data class Before(
        val at: String,
    ) : GroupKnown, Ordered {
        override val `is`: String get() = "before"
        override var wireOrder: IntArray? = null
    }
    data object Unknown : GroupKnown {
        override val `is`: String get() = "unknown"
    }

    companion object {
        val ADAPTER: JsonAdapter<GroupKnown> =
            tagged<GroupKnown>(
                "is",
                listOf(
                Variant("now", GroupKnown.Now::class.java, obj(
                        listOf(
                        Field("is", { tag("now") }, optional = false, nullable = false),
                        ),
                        { GroupKnown.Now },
                        { arrayOf("now") },
                    )),
                Variant("before", GroupKnown.Before::class.java, obj(
                        listOf(
                        Field("is", { tag("before") }, optional = false, nullable = false),
                        Field("at", { Str }, optional = false, nullable = false),
                        ),
                        { a -> GroupKnown.Before(a[1] as String) },
                        { v -> arrayOf("before", v.at) },
                    )),
                Variant("unknown", GroupKnown.Unknown::class.java, obj(
                        listOf(
                        Field("is", { tag("unknown") }, optional = false, nullable = false),
                        ),
                        { GroupKnown.Unknown },
                        { arrayOf("unknown") },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class GroupMember(
    val origin: String,
    val sid: String,
    val pinned: Boolean,
    val face: SessionFace,
    val facts: MemberFacts,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<GroupMember> = obj(
            listOf(
            Field("origin", { Str }, optional = false, nullable = false),
            Field("sid", { Str }, optional = false, nullable = false),
            Field("pinned", { Bool }, optional = false, nullable = false),
            Field("face", { SessionFace.ADAPTER }, optional = false, nullable = false),
            Field("facts", { MemberFacts.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> GroupMember(a[0] as String, a[1] as String, a[2] as Boolean, a[3] as SessionFace, a[4] as MemberFacts) },
            { v -> arrayOf(v.origin, v.sid, v.pinned, v.face, v.facts) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class GroupMissed(
    val origin: String,
    val why: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<GroupMissed> = obj(
            listOf(
            Field("origin", { Str }, optional = false, nullable = false),
            Field("why", { Str }, optional = false, nullable = false),
            ),
            { a -> GroupMissed(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.origin, v.why) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class GroupRead(
    val id: String,
    val name: String?,
    val title: String,
    val made: Long?,
    val named: Long?,
    val known: GroupKnown,
    val members: List<GroupMember>,
    val counts: List<GroupCount>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<GroupRead> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("name", { opt(Str) }, optional = false, nullable = true),
            Field("title", { Str }, optional = false, nullable = false),
            Field("made", { opt(Lng) }, optional = false, nullable = true),
            Field("named", { opt(Lng) }, optional = false, nullable = true),
            Field("known", { GroupKnown.ADAPTER }, optional = false, nullable = false),
            Field("members", { arr(GroupMember.ADAPTER) }, optional = false, nullable = false),
            Field("counts", { arr(GroupCount.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> GroupRead(a[0] as String, a[1] as String?, a[2] as String, a[3] as Long?, a[4] as Long?, a[5] as GroupKnown, a[6] as List<GroupMember>, a[7] as List<GroupCount>) },
            { v -> arrayOf(v.id, v.name, v.title, v.made, v.named, v.known, v.members, v.counts) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class GroupRef(
    val id: String,
    val name: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<GroupRef> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            ),
            { a -> GroupRef(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.id, v.name) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class GroupsHere(
    val groups: List<HereGroup>,
    val members: List<HereMember>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<GroupsHere> = obj(
            listOf(
            Field("groups", { arr(HereGroup.ADAPTER) }, optional = false, nullable = false),
            Field("members", { arr(HereMember.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> GroupsHere(a[0] as List<HereGroup>, a[1] as List<HereMember>) },
            { v -> arrayOf(v.groups, v.members) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class GroupsMachine(
    val origin: String,
    val read: HistoryMachineRead,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<GroupsMachine> = obj(
            listOf(
            Field("origin", { Str }, optional = false, nullable = false),
            Field("read", { HistoryMachineRead.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> GroupsMachine(a[0] as String, a[1] as HistoryMachineRead) },
            { v -> arrayOf(v.origin, v.read) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class GroupsRead(
    val groups: List<GroupRead>,
    val pinned: List<GroupMember>,
    val machines: List<GroupsMachine>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<GroupsRead> = obj(
            listOf(
            Field("groups", { arr(GroupRead.ADAPTER) }, optional = false, nullable = false),
            Field("pinned", { arr(GroupMember.ADAPTER) }, optional = false, nullable = false),
            Field("machines", { arr(GroupsMachine.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> GroupsRead(a[0] as List<GroupRead>, a[1] as List<GroupMember>, a[2] as List<GroupsMachine>) },
            { v -> arrayOf(v.groups, v.pinned, v.machines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class GroupsReadReq(
    val machines: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<GroupsReadReq> = obj(
            listOf(
            Field("machines", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> GroupsReadReq(a[0] as List<String>) },
            { v -> arrayOf(v.machines) },
        )
    }
}

sealed interface GroupsWriteReq {
    /** 判别格（`op`）那个词。 */
    val op: String

    @Suppress("UNCHECKED_CAST")
    data class Make(
        val name: String,
    ) : GroupsWriteReq, Ordered {
        override val op: String get() = "make"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Move(
        val sid: String,
        val join: HereGroup? = null,
        val leave: String? = null,
    ) : GroupsWriteReq, Ordered {
        override val op: String get() = "move"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Rename(
        val id: String,
        val name: String,
        val machines: List<String>,
    ) : GroupsWriteReq, Ordered {
        override val op: String get() = "rename"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Dissolve(
        val id: String,
        val machines: List<String>,
    ) : GroupsWriteReq, Ordered {
        override val op: String get() = "dissolve"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<GroupsWriteReq> =
            tagged<GroupsWriteReq>(
                "op",
                listOf(
                Variant("make", GroupsWriteReq.Make::class.java, obj(
                        listOf(
                        Field("op", { tag("make") }, optional = false, nullable = false),
                        Field("name", { Str }, optional = false, nullable = false),
                        ),
                        { a -> GroupsWriteReq.Make(a[1] as String) },
                        { v -> arrayOf("make", v.name) },
                    )),
                Variant("move", GroupsWriteReq.Move::class.java, obj(
                        listOf(
                        Field("op", { tag("move") }, optional = false, nullable = false),
                        Field("sid", { Str }, optional = false, nullable = false),
                        Field("join", { opt(HereGroup.ADAPTER) }, optional = true, nullable = true),
                        Field("leave", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> GroupsWriteReq.Move(a[1] as String, a[2] as HereGroup?, a[3] as String?) },
                        { v -> arrayOf("move", v.sid, v.join, v.leave) },
                    )),
                Variant("rename", GroupsWriteReq.Rename::class.java, obj(
                        listOf(
                        Field("op", { tag("rename") }, optional = false, nullable = false),
                        Field("id", { Str }, optional = false, nullable = false),
                        Field("name", { Str }, optional = false, nullable = false),
                        Field("machines", { arr(Str) }, optional = false, nullable = false),
                        ),
                        { a -> GroupsWriteReq.Rename(a[1] as String, a[2] as String, a[3] as List<String>) },
                        { v -> arrayOf("rename", v.id, v.name, v.machines) },
                    )),
                Variant("dissolve", GroupsWriteReq.Dissolve::class.java, obj(
                        listOf(
                        Field("op", { tag("dissolve") }, optional = false, nullable = false),
                        Field("id", { Str }, optional = false, nullable = false),
                        Field("machines", { arr(Str) }, optional = false, nullable = false),
                        ),
                        { a -> GroupsWriteReq.Dissolve(a[1] as String, a[2] as List<String>) },
                        { v -> arrayOf("dissolve", v.id, v.machines) },
                    )),
                ),
            )
    }
}

sealed interface GroupsWritten {
    /** 判别格（`is`）那个词。 */
    val `is`: String

    @Suppress("UNCHECKED_CAST")
    data class Made(
        val id: String,
        val name: String,
        val made: Long,
        val named: Long,
    ) : GroupsWritten, Ordered {
        override val `is`: String get() = "made"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Moved(
        val sid: String,
        val groups: List<String>,
    ) : GroupsWritten, Ordered {
        override val `is`: String get() = "moved"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Sent(
        val id: String,
        val missed: List<GroupMissed>,
    ) : GroupsWritten, Ordered {
        override val `is`: String get() = "sent"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<GroupsWritten> =
            tagged<GroupsWritten>(
                "is",
                listOf(
                Variant("made", GroupsWritten.Made::class.java, obj(
                        listOf(
                        Field("is", { tag("made") }, optional = false, nullable = false),
                        Field("id", { Str }, optional = false, nullable = false),
                        Field("name", { Str }, optional = false, nullable = false),
                        Field("made", { Lng }, optional = false, nullable = false),
                        Field("named", { Lng }, optional = false, nullable = false),
                        ),
                        { a -> GroupsWritten.Made(a[1] as String, a[2] as String, a[3] as Long, a[4] as Long) },
                        { v -> arrayOf("made", v.id, v.name, v.made, v.named) },
                    )),
                Variant("moved", GroupsWritten.Moved::class.java, obj(
                        listOf(
                        Field("is", { tag("moved") }, optional = false, nullable = false),
                        Field("sid", { Str }, optional = false, nullable = false),
                        Field("groups", { arr(Str) }, optional = false, nullable = false),
                        ),
                        { a -> GroupsWritten.Moved(a[1] as String, a[2] as List<String>) },
                        { v -> arrayOf("moved", v.sid, v.groups) },
                    )),
                Variant("sent", GroupsWritten.Sent::class.java, obj(
                        listOf(
                        Field("is", { tag("sent") }, optional = false, nullable = false),
                        Field("id", { Str }, optional = false, nullable = false),
                        Field("missed", { arr(GroupMissed.ADAPTER) }, optional = false, nullable = false),
                        ),
                        { a -> GroupsWritten.Sent(a[1] as String, a[2] as List<GroupMissed>) },
                        { v -> arrayOf("sent", v.id, v.missed) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class HealthFace(
    val state: HealthState,
    val summary: String,
    val why: String?,
    val detail: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HealthFace> = obj(
            listOf(
            Field("state", { HealthState.ADAPTER }, optional = false, nullable = false),
            Field("summary", { Str }, optional = false, nullable = false),
            Field("why", { opt(Str) }, optional = false, nullable = true),
            Field("detail", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> HealthFace(a[0] as HealthState, a[1] as String, a[2] as String?, a[3] as String?) },
            { v -> arrayOf(v.state, v.summary, v.why, v.detail) },
        )
    }
}

enum class HealthState(val wire: String) {
    UNKNOWN("unknown"),
    CLEAN("clean"),
    CRASHED("crashed"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<HealthState> = enumOf(entries) { it.wire }
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
data class HereGroup(
    val id: String,
    val name: String,
    val made: Long,
    val named: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HereGroup> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("made", { Lng }, optional = false, nullable = false),
            Field("named", { Lng }, optional = false, nullable = false),
            ),
            { a -> HereGroup(a[0] as String, a[1] as String, a[2] as Long, a[3] as Long) },
            { v -> arrayOf(v.id, v.name, v.made, v.named) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HereMember(
    val sid: String,
    val groups: List<String>,
    val pinned: Boolean,
    val face: SessionFace,
    val facts: MemberFacts,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HereMember> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("groups", { arr(Str) }, optional = false, nullable = false),
            Field("pinned", { Bool }, optional = false, nullable = false),
            Field("face", { SessionFace.ADAPTER }, optional = false, nullable = false),
            Field("facts", { MemberFacts.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> HereMember(a[0] as String, a[1] as List<String>, a[2] as Boolean, a[3] as SessionFace, a[4] as MemberFacts) },
            { v -> arrayOf(v.sid, v.groups, v.pinned, v.face, v.facts) },
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
data class HiddenNow(
    val hidden: Hidden?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HiddenNow> = obj(
            listOf(
            Field("hidden", { opt(Hidden.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> HiddenNow(a[0] as Hidden?) },
            { v -> arrayOf(v.hidden) },
        )
    }
}

enum class Hide(val wire: String) {
    ON("on"),
    OFF("off"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<Hide> = enumOf(entries) { it.wire }
    }
}

enum class HideAsk(val wire: String) {
    ON("on"),
    OFF("off"),
    RULE("rule"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<HideAsk> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryBrowseHead(
    val machines: List<HistoryMachine>,
    val projects: Long,
    val grouped: Long,
    val sections: List<HistorySection>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryBrowseHead> = obj(
            listOf(
            Field("machines", { arr(HistoryMachine.ADAPTER) }, optional = false, nullable = false),
            Field("projects", { Lng }, optional = false, nullable = false),
            Field("grouped", { Lng }, optional = false, nullable = false),
            Field("sections", { arr(HistorySection.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> HistoryBrowseHead(a[0] as List<HistoryMachine>, a[1] as Long, a[2] as Long, a[3] as List<HistorySection>) },
            { v -> arrayOf(v.machines, v.projects, v.grouped, v.sections) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryBrowseReq(
    val here: Boolean,
    val remotes: List<String>,
    val fresh: List<String>,
    val query: String? = null,
    val sort: HistorySort? = null,
    val withinDays: Long? = null,
    val hidden: Boolean? = null,
    val by: HistoryBy,
    val groups: HistoryFold,
    val forks: List<HistoryRowId>,
    val range: SyncRange,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryBrowseReq> = obj(
            listOf(
            Field("here", { Bool }, optional = false, nullable = false),
            Field("remotes", { arr(Str) }, optional = false, nullable = false),
            Field("fresh", { arr(Str) }, optional = false, nullable = false),
            Field("query", { opt(Str) }, optional = true, nullable = true),
            Field("sort", { opt(HistorySort.ADAPTER) }, optional = true, nullable = true),
            Field("within_days", { opt(Lng) }, optional = true, nullable = true),
            Field("hidden", { opt(Bool) }, optional = true, nullable = true),
            Field("by", { HistoryBy.ADAPTER }, optional = false, nullable = false),
            Field("groups", { HistoryFold.ADAPTER }, optional = false, nullable = false),
            Field("forks", { arr(HistoryRowId.ADAPTER) }, optional = false, nullable = false),
            Field("range", { SyncRange.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> HistoryBrowseReq(a[0] as Boolean, a[1] as List<String>, a[2] as List<String>, a[3] as String?, a[4] as HistorySort?, a[5] as Long?, a[6] as Boolean?, a[7] as HistoryBy, a[8] as HistoryFold, a[9] as List<HistoryRowId>, a[10] as SyncRange) },
            { v -> arrayOf(v.here, v.remotes, v.fresh, v.query, v.sort, v.withinDays, v.hidden, v.by, v.groups, v.forks, v.range) },
        )
    }
}

enum class HistoryBy(val wire: String) {
    TIME("time"),
    PROJECT("project"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<HistoryBy> = enumOf(entries) { it.wire }
    }
}

sealed interface HistoryFactsReq {
    data class Path(
        val value: String,
    ) : HistoryFactsReq
    data class Sid(
        val value: String,
    ) : HistoryFactsReq

    companion object {
        val ADAPTER: JsonAdapter<HistoryFactsReq> =
            external<HistoryFactsReq>(
                listOf(
                Ext("path", HistoryFactsReq.Path::class.java, { Str }, { HistoryFactsReq.Path(it as String) }, { (it as HistoryFactsReq.Path).value }),
                Ext("sid", HistoryFactsReq.Sid::class.java, { Str }, { HistoryFactsReq.Sid(it as String) }, { (it as HistoryFactsReq.Sid).value }),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryFailedDir(
    val projectDir: String,
    val error: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryFailedDir> = obj(
            listOf(
            Field("projectDir", { Str }, optional = false, nullable = false),
            Field("error", { Str }, optional = false, nullable = false),
            ),
            { a -> HistoryFailedDir(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.projectDir, v.error) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryFind(
    val total: Long,
    val hits: List<FindHit>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryFind> = obj(
            listOf(
            Field("total", { Lng }, optional = false, nullable = false),
            Field("hits", { arr(FindHit.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> HistoryFind(a[0] as Long, a[1] as List<FindHit>) },
            { v -> arrayOf(v.total, v.hits) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryFindReq(
    val path: String,
    val query: String,
    val includeTools: Boolean? = null,
    val scope: SearchScope? = null,
    val limit: Long? = null,
    val skip: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryFindReq> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("query", { Str }, optional = false, nullable = false),
            Field("include_tools", { opt(Bool) }, optional = true, nullable = true),
            Field("scope", { opt(SearchScope.ADAPTER) }, optional = true, nullable = true),
            Field("limit", { opt(Lng) }, optional = true, nullable = true),
            Field("skip", { opt(Lng) }, optional = true, nullable = true),
            ),
            { a -> HistoryFindReq(a[0] as String, a[1] as String, a[2] as Boolean?, a[3] as SearchScope?, a[4] as Long?, a[5] as Long?) },
            { v -> arrayOf(v.path, v.query, v.includeTools, v.scope, v.limit, v.skip) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryFold(
    val open: Boolean,
    val except: List<HistoryGroupId>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryFold> = obj(
            listOf(
            Field("open", { Bool }, optional = false, nullable = false),
            Field("except", { arr(HistoryGroupId.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> HistoryFold(a[0] as Boolean, a[1] as List<HistoryGroupId>) },
            { v -> arrayOf(v.open, v.except) },
        )
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
            Field("count", { U64 }, optional = false, nullable = false),
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
data class HistoryGroupId(
    val origin: String? = null,
    val key: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryGroupId> = obj(
            listOf(
            Field("origin", { opt(Str) }, optional = true, nullable = true),
            Field("key", { Str }, optional = false, nullable = false),
            ),
            { a -> HistoryGroupId(a[0] as String?, a[1] as String) },
            { v -> arrayOf(v.origin, v.key) },
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
data class HistoryImage(
    val mediaType: String,
    val data: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryImage> = obj(
            listOf(
            Field("mediaType", { Str }, optional = false, nullable = false),
            Field("data", { Str }, optional = false, nullable = false),
            ),
            { a -> HistoryImage(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.mediaType, v.data) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryImageReq(
    val path: String,
    val at: Long,
    val n: Long,
    val id: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryImageReq> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("at", { Lng }, optional = false, nullable = false),
            Field("n", { Lng }, optional = false, nullable = false),
            Field("id", { Str }, optional = false, nullable = false),
            ),
            { a -> HistoryImageReq(a[0] as String, a[1] as Long, a[2] as Long, a[3] as String) },
            { v -> arrayOf(v.path, v.at, v.n, v.id) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryIndex(
    val from: Long,
    val end: Long,
    val next: Long?,
    val count: Long,
    val classes: List<RecordClass>,
    val draws: List<Draw>,
    val o: List<Long>,
    val n: List<Long>,
    val t: List<Long>,
    val dr: List<Long>,
    val fd: List<Long>,
    val body: IndexBody,
    val hu: List<Long>,
    val inputs: IndexInputs,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryIndex> = obj(
            listOf(
            Field("from", { Lng }, optional = false, nullable = false),
            Field("end", { Lng }, optional = false, nullable = false),
            Field("next", { opt(Lng) }, optional = false, nullable = true),
            Field("count", { Lng }, optional = false, nullable = false),
            Field("classes", { arr(RecordClass.ADAPTER) }, optional = false, nullable = false),
            Field("draws", { arr(Draw.ADAPTER) }, optional = false, nullable = false),
            Field("o", { arr(Lng) }, optional = false, nullable = false),
            Field("n", { arr(Lng) }, optional = false, nullable = false),
            Field("t", { arr(Lng) }, optional = false, nullable = false),
            Field("dr", { arr(Lng) }, optional = false, nullable = false),
            Field("fd", { arr(Lng) }, optional = false, nullable = false),
            Field("body", { IndexBody.ADAPTER }, optional = false, nullable = false),
            Field("hu", { arr(Lng) }, optional = false, nullable = false),
            Field("inputs", { IndexInputs.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> HistoryIndex(a[0] as Long, a[1] as Long, a[2] as Long?, a[3] as Long, a[4] as List<RecordClass>, a[5] as List<Draw>, a[6] as List<Long>, a[7] as List<Long>, a[8] as List<Long>, a[9] as List<Long>, a[10] as List<Long>, a[11] as IndexBody, a[12] as List<Long>, a[13] as IndexInputs) },
            { v -> arrayOf(v.from, v.end, v.next, v.count, v.classes, v.draws, v.o, v.n, v.t, v.dr, v.fd, v.body, v.hu, v.inputs) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryIndexReq(
    val path: String,
    val offset: Long? = null,
    val until: Long? = null,
    val limit: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryIndexReq> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("offset", { opt(Lng) }, optional = true, nullable = true),
            Field("until", { opt(Lng) }, optional = true, nullable = true),
            Field("limit", { opt(Lng) }, optional = true, nullable = true),
            ),
            { a -> HistoryIndexReq(a[0] as String, a[1] as Long?, a[2] as Long?, a[3] as Long?) },
            { v -> arrayOf(v.path, v.offset, v.until, v.limit) },
        )
    }
}

sealed interface HistoryLine {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    @Suppress("UNCHECKED_CAST")
    data class Group(
        val group: HistoryGroup,
        val open: Boolean,
    ) : HistoryLine, Ordered {
        override val kind: String get() = "group"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Row(
        val row: HistoryRow,
        val child: Boolean,
        val forks: Long,
        val forksOpen: Boolean,
        val orphan: Boolean,
    ) : HistoryLine, Ordered {
        override val kind: String get() = "row"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<HistoryLine> =
            tagged<HistoryLine>(
                "kind",
                listOf(
                Variant("group", HistoryLine.Group::class.java, obj(
                        listOf(
                        Field("kind", { tag("group") }, optional = false, nullable = false),
                        Field("group", { HistoryGroup.ADAPTER }, optional = false, nullable = false),
                        Field("open", { Bool }, optional = false, nullable = false),
                        ),
                        { a -> HistoryLine.Group(a[1] as HistoryGroup, a[2] as Boolean) },
                        { v -> arrayOf("group", v.group, v.open) },
                    )),
                Variant("row", HistoryLine.Row::class.java, obj(
                        listOf(
                        Field("kind", { tag("row") }, optional = false, nullable = false),
                        Field("row", { HistoryRow.ADAPTER }, optional = false, nullable = false),
                        Field("child", { Bool }, optional = false, nullable = false),
                        Field("forks", { Lng }, optional = false, nullable = false),
                        Field("forksOpen", { Bool }, optional = false, nullable = false),
                        Field("orphan", { Bool }, optional = false, nullable = false),
                        ),
                        { a -> HistoryLine.Row(a[1] as HistoryRow, a[2] as Boolean, a[3] as Long, a[4] as Boolean, a[5] as Boolean) },
                        { v -> arrayOf("row", v.row, v.child, v.forks, v.forksOpen, v.orphan) },
                    )),
                ),
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
            Field("from", { U64 }, optional = false, nullable = false),
            Field("next", { U64 }, optional = false, nullable = false),
            Field("eof", { Bool }, optional = false, nullable = false),
            Field("end", { U64 }, optional = false, nullable = false),
            Field("lines", { arr(RecordLine.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> HistoryLines(a[0] as Long, a[1] as Long, a[2] as Boolean, a[3] as Long, a[4] as List<RecordLine>) },
            { v -> arrayOf(v.from, v.next, v.eof, v.end, v.lines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryLinesReq(
    val path: String,
    val from: Long? = null,
    val until: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryLinesReq> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("from", { opt(Lng) }, optional = true, nullable = true),
            Field("until", { opt(Lng) }, optional = true, nullable = true),
            ),
            { a -> HistoryLinesReq(a[0] as String, a[1] as Long?, a[2] as Long?) },
            { v -> arrayOf(v.path, v.from, v.until) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryListReq(
    val origin: String? = null,
    val sid: String? = null,
    val query: String? = null,
    val sort: HistorySort? = null,
    val withinDays: Long? = null,
    val hidden: Boolean? = null,
    val held: String? = null,
    val range: SyncRange? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryListReq> = obj(
            listOf(
            Field("origin", { opt(Str) }, optional = true, nullable = true),
            Field("sid", { opt(Str) }, optional = true, nullable = true),
            Field("query", { opt(Str) }, optional = true, nullable = true),
            Field("sort", { opt(HistorySort.ADAPTER) }, optional = true, nullable = true),
            Field("within_days", { opt(U64) }, optional = true, nullable = true),
            Field("hidden", { opt(Bool) }, optional = true, nullable = true),
            Field("held", { opt(Str) }, optional = true, nullable = true),
            Field("range", { opt(SyncRange.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> HistoryListReq(a[0] as String?, a[1] as String?, a[2] as String?, a[3] as HistorySort?, a[4] as Long?, a[5] as Boolean?, a[6] as String?, a[7] as SyncRange?) },
            { v -> arrayOf(v.origin, v.sid, v.query, v.sort, v.withinDays, v.hidden, v.held, v.range) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryListingHead(
    val failed: List<HistoryFailedDir>,
    val annotations: Map<String, Entry>?,
    val annotationsNotice: String?,
    val rules: Rules,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryListingHead> = obj(
            listOf(
            Field("failed", { arr(HistoryFailedDir.ADAPTER) }, optional = false, nullable = false),
            Field("annotations", { opt(dict(Entry.ADAPTER)) }, optional = false, nullable = true),
            Field("annotationsNotice", { opt(Str) }, optional = false, nullable = true),
            Field("rules", { Rules.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> HistoryListingHead(a[0] as List<HistoryFailedDir>, a[1] as Map<String, Entry>?, a[2] as String?, a[3] as Rules) },
            { v -> arrayOf(v.failed, v.annotations, v.annotationsNotice, v.rules) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryListingReq(
    val held: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryListingReq> = obj(
            listOf(
            Field("held", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> HistoryListingReq(a[0] as String?) },
            { v -> arrayOf(v.held) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryMachine(
    val origin: String? = null,
    val total: Long,
    val read: HistoryMachineRead,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryMachine> = obj(
            listOf(
            Field("origin", { opt(Str) }, optional = true, nullable = true),
            Field("total", { Lng }, optional = false, nullable = false),
            Field("read", { HistoryMachineRead.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> HistoryMachine(a[0] as String?, a[1] as Long, a[2] as HistoryMachineRead) },
            { v -> arrayOf(v.origin, v.total, v.read) },
        )
    }
}

sealed interface HistoryMachineRead {
    /** 判别格（`is`）那个词。 */
    val `is`: String

    @Suppress("UNCHECKED_CAST")
    data class Ok(
        val notice: String?,
    ) : HistoryMachineRead, Ordered {
        override val `is`: String get() = "ok"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Stale(
        val why: String,
        val at: String,
    ) : HistoryMachineRead, Ordered {
        override val `is`: String get() = "stale"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Failed(
        val why: String,
    ) : HistoryMachineRead, Ordered {
        override val `is`: String get() = "failed"
        override var wireOrder: IntArray? = null
    }
    data object Unread : HistoryMachineRead {
        override val `is`: String get() = "unread"
    }

    companion object {
        val ADAPTER: JsonAdapter<HistoryMachineRead> =
            tagged<HistoryMachineRead>(
                "is",
                listOf(
                Variant("ok", HistoryMachineRead.Ok::class.java, obj(
                        listOf(
                        Field("is", { tag("ok") }, optional = false, nullable = false),
                        Field("notice", { opt(Str) }, optional = false, nullable = true),
                        ),
                        { a -> HistoryMachineRead.Ok(a[1] as String?) },
                        { v -> arrayOf("ok", v.notice) },
                    )),
                Variant("stale", HistoryMachineRead.Stale::class.java, obj(
                        listOf(
                        Field("is", { tag("stale") }, optional = false, nullable = false),
                        Field("why", { Str }, optional = false, nullable = false),
                        Field("at", { Str }, optional = false, nullable = false),
                        ),
                        { a -> HistoryMachineRead.Stale(a[1] as String, a[2] as String) },
                        { v -> arrayOf("stale", v.why, v.at) },
                    )),
                Variant("failed", HistoryMachineRead.Failed::class.java, obj(
                        listOf(
                        Field("is", { tag("failed") }, optional = false, nullable = false),
                        Field("why", { Str }, optional = false, nullable = false),
                        ),
                        { a -> HistoryMachineRead.Failed(a[1] as String) },
                        { v -> arrayOf("failed", v.why) },
                    )),
                Variant("unread", HistoryMachineRead.Unread::class.java, obj(
                        listOf(
                        Field("is", { tag("unread") }, optional = false, nullable = false),
                        ),
                        { HistoryMachineRead.Unread },
                        { arrayOf("unread") },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryPage(
    val lines: List<RecordLine>,
    val next: Long,
    val nextSeq: Long,
    val eof: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryPage> = obj(
            listOf(
            Field("lines", { arr(RecordLine.ADAPTER) }, optional = false, nullable = false),
            Field("next", { Lng }, optional = false, nullable = false),
            Field("nextSeq", { Lng }, optional = false, nullable = false),
            Field("eof", { Bool }, optional = false, nullable = false),
            ),
            { a -> HistoryPage(a[0] as List<RecordLine>, a[1] as Long, a[2] as Long, a[3] as Boolean) },
            { v -> arrayOf(v.lines, v.next, v.nextSeq, v.eof) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryPageReq(
    val path: String,
    val offset: Long? = null,
    val until: Long? = null,
    val seq: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryPageReq> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("offset", { opt(Lng) }, optional = true, nullable = true),
            Field("until", { opt(Lng) }, optional = true, nullable = true),
            Field("seq", { opt(Lng) }, optional = true, nullable = true),
            ),
            { a -> HistoryPageReq(a[0] as String, a[1] as Long?, a[2] as Long?, a[3] as Long?) },
            { v -> arrayOf(v.path, v.offset, v.until, v.seq) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryRawRow(
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
    val life: Life,
    val messageCountApprox: Int,
    val isBg: Boolean,
    val starred: Boolean,
    val customTitle: String?,
    val launchTab: LaunchTab? = null,
    val parent: String? = null,
    val forkedFromSessionId: String? = null,
    val forkedFromMessageUuid: String? = null,
    val projectDir: String,
    val group: String,
    val lastAccount: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryRawRow> = obj(
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
            Field("life", { Life.ADAPTER }, optional = false, nullable = false),
            Field("messageCountApprox", { Int32 }, optional = false, nullable = false),
            Field("isBg", { Bool }, optional = false, nullable = false),
            Field("starred", { Bool }, optional = false, nullable = false),
            Field("customTitle", { opt(Str) }, optional = false, nullable = true),
            Field("launchTab", { opt(LaunchTab.ADAPTER) }, optional = true, nullable = true),
            Field("parent", { opt(Str) }, optional = true, nullable = true),
            Field("forkedFromSessionId", { opt(Str) }, optional = true, nullable = true),
            Field("forkedFromMessageUuid", { opt(Str) }, optional = true, nullable = true),
            Field("projectDir", { Str }, optional = false, nullable = false),
            Field("group", { Str }, optional = false, nullable = false),
            Field("lastAccount", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> HistoryRawRow(a[0] as String, a[1] as String, a[2] as String, a[3] as String, a[4] as String?, a[5] as String, a[6] as String, a[7] as Long, a[8] as Long, a[9] as String, a[10] as Life, a[11] as Int, a[12] as Boolean, a[13] as Boolean, a[14] as String?, a[15] as LaunchTab?, a[16] as String?, a[17] as String?, a[18] as String?, a[19] as String, a[20] as String, a[21] as String?) },
            { v -> arrayOf(v.agent, v.sessionId, v.projectPath, v.projectName, v.aiTitle, v.firstUserExcerpt, v.title, v.startedAt, v.updatedAt, v.jsonlPath, v.life, v.messageCountApprox, v.isBg, v.starred, v.customTitle, v.launchTab, v.parent, v.forkedFromSessionId, v.forkedFromMessageUuid, v.projectDir, v.group, v.lastAccount) },
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
            Field("next", { U64 }, optional = false, nullable = false),
            Field("eof", { Bool }, optional = false, nullable = false),
            ),
            { a -> HistoryRead(a[0] as List<ReadRow>, a[1] as Long, a[2] as Boolean) },
            { v -> arrayOf(v.rows, v.next, v.eof) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryReadReq(
    val path: String,
    val offset: Long? = null,
    val until: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryReadReq> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("offset", { opt(U64) }, optional = true, nullable = true),
            Field("until", { opt(U64) }, optional = true, nullable = true),
            ),
            { a -> HistoryReadReq(a[0] as String, a[1] as Long?, a[2] as Long?) },
            { v -> arrayOf(v.path, v.offset, v.until) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryRecordReq(
    val sid: String,
    val configDir: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryRecordReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("configDir", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> HistoryRecordReq(a[0] as String, a[1] as String?) },
            { v -> arrayOf(v.sid, v.configDir) },
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
    val messageCountApprox: Int,
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
            Field("messageCountApprox", { Int32 }, optional = false, nullable = false),
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
            { a -> HistoryRow(a[0] as String, a[1] as String, a[2] as String, a[3] as String, a[4] as String?, a[5] as String, a[6] as String, a[7] as Long, a[8] as Long, a[9] as String, a[10] as Int, a[11] as Boolean, a[12] as Boolean, a[13] as String?, a[14] as Hidden?, a[15] as String?, a[16] as String?, a[17] as ForkedFrom?, a[18] as String, a[19] as String, a[20] as String?, a[21] as String, a[22] as Boolean, a[23] as SessionFace, a[24] as Can, a[25] as String?, a[26] as Boolean?, a[27] as Long, a[28] as String, a[29] as String, a[30] as String, a[31] as String?) },
            { v -> arrayOf(v.agent, v.sessionId, v.projectPath, v.projectName, v.aiTitle, v.firstUserExcerpt, v.title, v.startedAt, v.updatedAt, v.jsonlPath, v.messageCountApprox, v.isBg, v.starred, v.customTitle, v.hidden, v.forkedFromSessionId, v.forkedFromMessageUuid, v.forkedFrom, v.projectDir, v.group, v.lastAccount, v.label, v.untitled, v.face, v.can, v.agentTag, v.context, v.at, v.atText, v.sectionText, v.spanText, v.origin) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryRowId(
    val origin: String? = null,
    val sid: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryRowId> = obj(
            listOf(
            Field("origin", { opt(Str) }, optional = true, nullable = true),
            Field("sid", { Str }, optional = false, nullable = false),
            ),
            { a -> HistoryRowId(a[0] as String?, a[1] as String) },
            { v -> arrayOf(v.origin, v.sid) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryRun(
    val run: String,
    val path: String,
    val rows: List<RunRow>,
    val from: Long,
    val end: Long,
    val more: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryRun> = obj(
            listOf(
            Field("run", { Str }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            Field("rows", { arr(RunRow.ADAPTER) }, optional = false, nullable = false),
            Field("from", { Lng }, optional = false, nullable = false),
            Field("end", { Lng }, optional = false, nullable = false),
            Field("more", { Bool }, optional = false, nullable = false),
            ),
            { a -> HistoryRun(a[0] as String, a[1] as String, a[2] as List<RunRow>, a[3] as Long, a[4] as Long, a[5] as Boolean) },
            { v -> arrayOf(v.run, v.path, v.rows, v.from, v.end, v.more) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryRunReq(
    val parent: String,
    val run: String? = null,
    val tool: String? = null,
    val from: Long? = null,
    val tail: Boolean? = null,
    val before: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryRunReq> = obj(
            listOf(
            Field("parent", { Str }, optional = false, nullable = false),
            Field("run", { opt(Str) }, optional = true, nullable = true),
            Field("tool", { opt(Str) }, optional = true, nullable = true),
            Field("from", { opt(Lng) }, optional = true, nullable = true),
            Field("tail", { opt(Bool) }, optional = true, nullable = true),
            Field("before", { opt(Lng) }, optional = true, nullable = true),
            ),
            { a -> HistoryRunReq(a[0] as String, a[1] as String?, a[2] as String?, a[3] as Long?, a[4] as Boolean?, a[5] as Long?) },
            { v -> arrayOf(v.parent, v.run, v.tool, v.from, v.tail, v.before) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistorySearchMergeReq(
    val machines: List<String>,
    val search: HistorySearchReq,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistorySearchMergeReq> = obj(
            listOf(
            Field("machines", { arr(Str) }, optional = false, nullable = false),
            Field("search", { HistorySearchReq.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> HistorySearchMergeReq(a[0] as List<String>, a[1] as HistorySearchReq) },
            { v -> arrayOf(v.machines, v.search) },
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
data class HistorySearchReq(
    val query: String,
    val includeTools: Boolean? = null,
    val scope: SearchScope? = null,
    val afterMs: Long? = null,
    val limit: Long? = null,
    val titles: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistorySearchReq> = obj(
            listOf(
            Field("query", { Str }, optional = false, nullable = false),
            Field("include_tools", { opt(Bool) }, optional = true, nullable = true),
            Field("scope", { opt(SearchScope.ADAPTER) }, optional = true, nullable = true),
            Field("after_ms", { opt(U64) }, optional = true, nullable = true),
            Field("limit", { opt(U64) }, optional = true, nullable = true),
            Field("titles", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> HistorySearchReq(a[0] as String, a[1] as Boolean?, a[2] as SearchScope?, a[3] as Long?, a[4] as Long?, a[5] as Boolean?) },
            { v -> arrayOf(v.query, v.includeTools, v.scope, v.afterMs, v.limit, v.titles) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistorySection(
    val at: Long,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistorySection> = obj(
            listOf(
            Field("at", { Lng }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> HistorySection(a[0] as Long, a[1] as String) },
            { v -> arrayOf(v.at, v.text) },
        )
    }
}

enum class HistorySort(val wire: String) {
    ACTIVITY("activity"),
    CREATED("created"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<HistorySort> = enumOf(entries) { it.wire }
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
            Field("from", { U64 }, optional = false, nullable = false),
            Field("rows", { arr(ReadRow.ADAPTER) }, optional = false, nullable = false),
            Field("next", { U64 }, optional = false, nullable = false),
            Field("eof", { Bool }, optional = false, nullable = false),
            Field("held", { Held.ADAPTER }, optional = false, nullable = false),
            Field("total", { U64 }, optional = false, nullable = false),
            Field("tail_from", { U64 }, optional = false, nullable = false),
            Field("split_at", { U64 }, optional = false, nullable = false),
            Field("end", { U64 }, optional = false, nullable = false),
            ),
            { a -> HistoryTail(a[0] as TailHow, a[1] as Long, a[2] as List<ReadRow>, a[3] as Long, a[4] as Boolean, a[5] as Held, a[6] as Long, a[7] as Long, a[8] as Long, a[9] as Long) },
            { v -> arrayOf(v.how, v.from, v.rows, v.next, v.eof, v.held, v.total, v.tailFrom, v.splitAt, v.end) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryTailReq(
    val path: String,
    val n: Long,
    val held: Held? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryTailReq> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("n", { U64 }, optional = false, nullable = false),
            Field("held", { opt(Held.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> HistoryTailReq(a[0] as String, a[1] as Long, a[2] as Held?) },
            { v -> arrayOf(v.path, v.n, v.held) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryTurns(
    val from: Long,
    val end: Long,
    val next: Long?,
    val turns: List<TurnRow>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryTurns> = obj(
            listOf(
            Field("from", { Lng }, optional = false, nullable = false),
            Field("end", { Lng }, optional = false, nullable = false),
            Field("next", { opt(Lng) }, optional = false, nullable = true),
            Field("turns", { arr(TurnRow.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> HistoryTurns(a[0] as Long, a[1] as Long, a[2] as Long?, a[3] as List<TurnRow>) },
            { v -> arrayOf(v.from, v.end, v.next, v.turns) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HistoryTurnsReq(
    val path: String,
    val from: Long? = null,
    val cover: Long? = null,
    val to: Long? = null,
    val limit: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HistoryTurnsReq> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("from", { opt(Lng) }, optional = true, nullable = true),
            Field("cover", { opt(Lng) }, optional = true, nullable = true),
            Field("to", { opt(Lng) }, optional = true, nullable = true),
            Field("limit", { opt(Lng) }, optional = true, nullable = true),
            ),
            { a -> HistoryTurnsReq(a[0] as String, a[1] as Long?, a[2] as Long?, a[3] as Long?, a[4] as Long?) },
            { v -> arrayOf(v.path, v.from, v.cover, v.to, v.limit) },
        )
    }
}

enum class HolderWho(val wire: String) {
    TERMINAL_WINDOW("terminal-window"),
    DESKTOP("desktop"),
    MOBILE("mobile"),
    NONE("none"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<HolderWho> = enumOf(entries) { it.wire }
    }
}

data object HomeReq {
    val ADAPTER: JsonAdapter<HomeReq> = obj(emptyList(), { HomeReq }, { emptyArray() })
}

sealed interface HookState {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    data object NotInstalled : HookState {
        override val kind: String get() = "not-installed"
    }
    @Suppress("UNCHECKED_CAST")
    data class InstalledViaPath(
        val command: String,
    ) : HookState, Ordered {
        override val kind: String get() = "installed-via-path"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class InstalledAtPath(
        val command: String,
        val path: String,
    ) : HookState, Ordered {
        override val kind: String get() = "installed-at-path"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class PathMissing(
        val command: String,
        val path: String,
    ) : HookState, Ordered {
        override val kind: String get() = "path-missing"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Unknown(
        val command: String,
    ) : HookState, Ordered {
        override val kind: String get() = "unknown"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<HookState> =
            tagged<HookState>(
                "kind",
                listOf(
                Variant("not-installed", HookState.NotInstalled::class.java, obj(
                        listOf(
                        Field("kind", { tag("not-installed") }, optional = false, nullable = false),
                        ),
                        { HookState.NotInstalled },
                        { arrayOf("not-installed") },
                    )),
                Variant("installed-via-path", HookState.InstalledViaPath::class.java, obj(
                        listOf(
                        Field("kind", { tag("installed-via-path") }, optional = false, nullable = false),
                        Field("command", { Str }, optional = false, nullable = false),
                        ),
                        { a -> HookState.InstalledViaPath(a[1] as String) },
                        { v -> arrayOf("installed-via-path", v.command) },
                    )),
                Variant("installed-at-path", HookState.InstalledAtPath::class.java, obj(
                        listOf(
                        Field("kind", { tag("installed-at-path") }, optional = false, nullable = false),
                        Field("command", { Str }, optional = false, nullable = false),
                        Field("path", { Str }, optional = false, nullable = false),
                        ),
                        { a -> HookState.InstalledAtPath(a[1] as String, a[2] as String) },
                        { v -> arrayOf("installed-at-path", v.command, v.path) },
                    )),
                Variant("path-missing", HookState.PathMissing::class.java, obj(
                        listOf(
                        Field("kind", { tag("path-missing") }, optional = false, nullable = false),
                        Field("command", { Str }, optional = false, nullable = false),
                        Field("path", { Str }, optional = false, nullable = false),
                        ),
                        { a -> HookState.PathMissing(a[1] as String, a[2] as String) },
                        { v -> arrayOf("path-missing", v.command, v.path) },
                    )),
                Variant("unknown", HookState.Unknown::class.java, obj(
                        listOf(
                        Field("kind", { tag("unknown") }, optional = false, nullable = false),
                        Field("command", { Str }, optional = false, nullable = false),
                        ),
                        { a -> HookState.Unknown(a[1] as String) },
                        { v -> arrayOf("unknown", v.command) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class HooksDiagnosis(
    val stop: HookState,
    val note: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HooksDiagnosis> = obj(
            listOf(
            Field("stop", { HookState.ADAPTER }, optional = false, nullable = false),
            Field("note", { Str }, optional = false, nullable = false),
            ),
            { a -> HooksDiagnosis(a[0] as HookState, a[1] as String) },
            { v -> arrayOf(v.stop, v.note) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HooksReport(
    val supported: Boolean,
    val diagnosis: HooksDiagnosis,
    val snippet: String?,
    val source: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HooksReport> = obj(
            listOf(
            Field("supported", { Bool }, optional = false, nullable = false),
            Field("diagnosis", { HooksDiagnosis.ADAPTER }, optional = false, nullable = false),
            Field("snippet", { opt(Str) }, optional = false, nullable = true),
            Field("source", { Str }, optional = false, nullable = false),
            ),
            { a -> HooksReport(a[0] as Boolean, a[1] as HooksDiagnosis, a[2] as String?, a[3] as String) },
            { v -> arrayOf(v.supported, v.diagnosis, v.snippet, v.source) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Host(
    val label: String,
    val host: String,
    val port: Long,
    val user: String,
    val keyPath: String,
    val addresses: List<String>,
    val jump: String,
    val connect: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Host> = obj(
            listOf(
            Field("label", { Str }, optional = false, nullable = false),
            Field("host", { Str }, optional = false, nullable = false),
            Field("port", { U64 }, optional = false, nullable = false),
            Field("user", { Str }, optional = false, nullable = false),
            Field("keyPath", { Str }, optional = false, nullable = false),
            Field("addresses", { arr(Str) }, optional = false, nullable = false),
            Field("jump", { Str }, optional = false, nullable = false),
            Field("connect", { Bool }, optional = false, nullable = false),
            ),
            { a -> Host(a[0] as String, a[1] as String, a[2] as Long, a[3] as String, a[4] as String, a[5] as List<String>, a[6] as String, a[7] as Boolean) },
            { v -> arrayOf(v.label, v.host, v.port, v.user, v.keyPath, v.addresses, v.jump, v.connect) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HostFacts(
    val os: String,
    val terminalFront: Boolean,
    val shellDialect: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HostFacts> = obj(
            listOf(
            Field("os", { Str }, optional = false, nullable = false),
            Field("terminalFront", { Bool }, optional = false, nullable = false),
            Field("shellDialect", { Str }, optional = false, nullable = false),
            ),
            { a -> HostFacts(a[0] as String, a[1] as Boolean, a[2] as String) },
            { v -> arrayOf(v.os, v.terminalFront, v.shellDialect) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HostKeysForgetReq(
    val machine: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HostKeysForgetReq> = obj(
            listOf(
            Field("machine", { Str }, optional = false, nullable = false),
            ),
            { a -> HostKeysForgetReq(a[0] as String) },
            { v -> arrayOf(v.machine) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HubApplied(
    val machines: List<HubMachineDone>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HubApplied> = obj(
            listOf(
            Field("machines", { arr(HubMachineDone.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> HubApplied(a[0] as List<HubMachineDone>) },
            { v -> arrayOf(v.machines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HubApplyReq(
    val kind: ExtKind,
    val name: String,
    val to: List<String?>,
    val place: ExtLoc?,
    val tokens: Map<String, ExtTokens>,
    val fill: Map<String, Map<String, String>>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HubApplyReq> = obj(
            listOf(
            Field("kind", { ExtKind.ADAPTER }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("to", { arr(opt(Str)) }, optional = false, nullable = false),
            Field("place", { opt(ExtLoc.ADAPTER) }, optional = false, nullable = true),
            Field("tokens", { dict(ExtTokens.ADAPTER) }, optional = false, nullable = false),
            Field("fill", { dict(dict(Str)) }, optional = false, nullable = false),
            ),
            { a -> HubApplyReq(a[0] as ExtKind, a[1] as String, a[2] as List<String?>, a[3] as ExtLoc?, a[4] as Map<String, ExtTokens>, a[5] as Map<String, Map<String, String>>) },
            { v -> arrayOf(v.kind, v.name, v.to, v.place, v.tokens, v.fill) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HubMachineCard(
    val to: String?,
    val name: String,
    val card: ExtCard?,
    val files: List<String>,
    val error: String?,
    val detail: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HubMachineCard> = obj(
            listOf(
            Field("to", { opt(Str) }, optional = false, nullable = true),
            Field("name", { Str }, optional = false, nullable = false),
            Field("card", { opt(ExtCard.ADAPTER) }, optional = false, nullable = true),
            Field("files", { arr(Str) }, optional = false, nullable = false),
            Field("error", { opt(Str) }, optional = false, nullable = true),
            Field("detail", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> HubMachineCard(a[0] as String?, a[1] as String, a[2] as ExtCard?, a[3] as List<String>, a[4] as String?, a[5] as String?) },
            { v -> arrayOf(v.to, v.name, v.card, v.files, v.error, v.detail) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HubMachineDone(
    val to: String?,
    val name: String,
    val done: ExtDone?,
    val error: String?,
    val detail: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HubMachineDone> = obj(
            listOf(
            Field("to", { opt(Str) }, optional = false, nullable = true),
            Field("name", { Str }, optional = false, nullable = false),
            Field("done", { opt(ExtDone.ADAPTER) }, optional = false, nullable = true),
            Field("error", { opt(Str) }, optional = false, nullable = true),
            Field("detail", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> HubMachineDone(a[0] as String?, a[1] as String, a[2] as ExtDone?, a[3] as String?, a[4] as String?) },
            { v -> arrayOf(v.to, v.name, v.done, v.error, v.detail) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HubPreview(
    val places: List<ExtTarget>,
    val place: ExtLoc?,
    val slots: List<HubSlot>,
    val machines: List<HubMachineCard>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HubPreview> = obj(
            listOf(
            Field("places", { arr(ExtTarget.ADAPTER) }, optional = false, nullable = false),
            Field("place", { opt(ExtLoc.ADAPTER) }, optional = false, nullable = true),
            Field("slots", { arr(HubSlot.ADAPTER) }, optional = false, nullable = false),
            Field("machines", { arr(HubMachineCard.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> HubPreview(a[0] as List<ExtTarget>, a[1] as ExtLoc?, a[2] as List<HubSlot>, a[3] as List<HubMachineCard>) },
            { v -> arrayOf(v.places, v.place, v.slots, v.machines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HubReq(
    val kind: ExtKind,
    val name: String,
    val to: List<String?>,
    val place: ExtLoc?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HubReq> = obj(
            listOf(
            Field("kind", { ExtKind.ADAPTER }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("to", { arr(opt(Str)) }, optional = false, nullable = false),
            Field("place", { opt(ExtLoc.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> HubReq(a[0] as ExtKind, a[1] as String, a[2] as List<String?>, a[3] as ExtLoc?) },
            { v -> arrayOf(v.kind, v.name, v.to, v.place) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class HubSlot(
    val field: String,
    val key: String,
    val kept: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<HubSlot> = obj(
            listOf(
            Field("field", { Str }, optional = false, nullable = false),
            Field("key", { Str }, optional = false, nullable = false),
            Field("kept", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> HubSlot(a[0] as String, a[1] as String, a[2] as List<String>) },
            { v -> arrayOf(v.field, v.key, v.kept) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class IndexBody(
    val at: List<Long>,
    val ch: List<Long>,
    val cj: List<Long>,
    val pl: List<Long>,
    val cb: List<Long>,
    val cl: List<Long>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<IndexBody> = obj(
            listOf(
            Field("at", { arr(Lng) }, optional = false, nullable = false),
            Field("ch", { arr(Lng) }, optional = false, nullable = false),
            Field("cj", { arr(Lng) }, optional = false, nullable = false),
            Field("pl", { arr(Lng) }, optional = false, nullable = false),
            Field("cb", { arr(Lng) }, optional = false, nullable = false),
            Field("cl", { arr(Lng) }, optional = false, nullable = false),
            ),
            { a -> IndexBody(a[0] as List<Long>, a[1] as List<Long>, a[2] as List<Long>, a[3] as List<Long>, a[4] as List<Long>, a[5] as List<Long>) },
            { v -> arrayOf(v.at, v.ch, v.cj, v.pl, v.cb, v.cl) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class IndexInputs(
    val at: List<Long>,
    val u: List<String>,
    val x: List<String>,
    val ts: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<IndexInputs> = obj(
            listOf(
            Field("at", { arr(Lng) }, optional = false, nullable = false),
            Field("u", { arr(Str) }, optional = false, nullable = false),
            Field("x", { arr(Str) }, optional = false, nullable = false),
            Field("ts", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> IndexInputs(a[0] as List<Long>, a[1] as List<String>, a[2] as List<String>, a[3] as List<String>) },
            { v -> arrayOf(v.at, v.u, v.x, v.ts) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class IndexStatusReply(
    val indexMissing: Boolean,
    val entries: Long,
    val residentBytes: Long,
    val unreadableDirs: Long,
    val unreadablePaths: List<WirePath>,
    val truncated: Boolean,
    val skippedMounts: Long,
    val ageSecs: Long,
    val rewalkIntervalSecs: Long,
    val stale: Boolean,
    val browseWatches: Long,
    val browseWatchCap: Long,
    val coldFirstBuildSecs: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<IndexStatusReply> = obj(
            listOf(
            Field("index_missing", { Bool }, optional = false, nullable = false),
            Field("entries", { U64 }, optional = false, nullable = false),
            Field("resident_bytes", { U64 }, optional = false, nullable = false),
            Field("unreadable_dirs", { U64 }, optional = false, nullable = false),
            Field("unreadable_paths", { arr(WirePath.ADAPTER) }, optional = false, nullable = false),
            Field("truncated", { Bool }, optional = false, nullable = false),
            Field("skipped_mounts", { U64 }, optional = false, nullable = false),
            Field("age_secs", { U64 }, optional = false, nullable = false),
            Field("rewalk_interval_secs", { U64 }, optional = false, nullable = false),
            Field("stale", { Bool }, optional = false, nullable = false),
            Field("browse_watches", { U64 }, optional = false, nullable = false),
            Field("browse_watch_cap", { U64 }, optional = false, nullable = false),
            Field("cold_first_build_secs", { U64 }, optional = false, nullable = false),
            ),
            { a -> IndexStatusReply(a[0] as Boolean, a[1] as Long, a[2] as Long, a[3] as Long, a[4] as List<WirePath>, a[5] as Boolean, a[6] as Long, a[7] as Long, a[8] as Long, a[9] as Boolean, a[10] as Long, a[11] as Long, a[12] as Long) },
            { v -> arrayOf(v.indexMissing, v.entries, v.residentBytes, v.unreadableDirs, v.unreadablePaths, v.truncated, v.skippedMounts, v.ageSecs, v.rewalkIntervalSecs, v.stale, v.browseWatches, v.browseWatchCap, v.coldFirstBuildSecs) },
        )
    }
}

data object IndexStatusReq {
    val ADAPTER: JsonAdapter<IndexStatusReq> = obj(emptyList(), { IndexStatusReq }, { emptyArray() })
}

@Suppress("UNCHECKED_CAST")
data class InputHolder(
    val who: HolderWho,
    val mine: Boolean,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<InputHolder> = obj(
            listOf(
            Field("who", { HolderWho.ADAPTER }, optional = false, nullable = false),
            Field("mine", { Bool }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> InputHolder(a[0] as HolderWho, a[1] as Boolean, a[2] as String) },
            { v -> arrayOf(v.who, v.mine, v.said) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class InputNow(
    val input: TermInput,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<InputNow> = obj(
            listOf(
            Field("input", { TermInput.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> InputNow(a[0] as TermInput) },
            { v -> arrayOf(v.input) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Installed(
    val action: ActionTag,
    val said: String,
    val build: String,
    val os: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Installed> = obj(
            listOf(
            Field("action", { ActionTag.ADAPTER }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            Field("build", { Str }, optional = false, nullable = false),
            Field("os", { Str }, optional = false, nullable = false),
            ),
            { a -> Installed(a[0] as ActionTag, a[1] as String, a[2] as String, a[3] as String) },
            { v -> arrayOf(v.action, v.said, v.build, v.os) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Interrupts(
    val relayedSessions: Long,
    val relayedMaybe: Long,
    val liveStreams: Long,
    val forwards: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Interrupts> = obj(
            listOf(
            Field("relayedSessions", { Lng }, optional = false, nullable = false),
            Field("relayedMaybe", { Lng }, optional = false, nullable = false),
            Field("liveStreams", { Lng }, optional = false, nullable = false),
            Field("forwards", { Lng }, optional = false, nullable = false),
            ),
            { a -> Interrupts(a[0] as Long, a[1] as Long, a[2] as Long, a[3] as Long) },
            { v -> arrayOf(v.relayedSessions, v.relayedMaybe, v.liveStreams, v.forwards) },
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
data class JumpHop(
    val host: String,
    val port: Int,
    val user: String,
    val keyPath: String?,
    val label: String,
    val jump: JumpHop?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<JumpHop> = obj(
            listOf(
            Field("host", { Str }, optional = false, nullable = false),
            Field("port", { Int32 }, optional = false, nullable = false),
            Field("user", { Str }, optional = false, nullable = false),
            Field("key_path", { opt(Str) }, optional = false, nullable = true),
            Field("label", { Str }, optional = false, nullable = false),
            Field("jump", { opt(JumpHop.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> JumpHop(a[0] as String, a[1] as Int, a[2] as String, a[3] as String?, a[4] as String, a[5] as JumpHop?) },
            { v -> arrayOf(v.host, v.port, v.user, v.keyPath, v.label, v.jump) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class KeyAddReq(
    val key: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<KeyAddReq> = obj(
            listOf(
            Field("key", { Str }, optional = false, nullable = false),
            ),
            { a -> KeyAddReq(a[0] as String) },
            { v -> arrayOf(v.key) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class KeyAdded(
    val outcome: PushOutcome,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<KeyAdded> = obj(
            listOf(
            Field("outcome", { PushOutcome.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> KeyAdded(a[0] as PushOutcome) },
            { v -> arrayOf(v.outcome) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class KeyPair(
    val pem: String,
    val public: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<KeyPair> = obj(
            listOf(
            Field("pem", { Str }, optional = false, nullable = false),
            Field("public", { Str }, optional = false, nullable = false),
            ),
            { a -> KeyPair(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.pem, v.public) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class KeySet(
    val account: String,
    val path: String,
    val masked: String,
    val baseUrl: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<KeySet> = obj(
            listOf(
            Field("account", { Str }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            Field("masked", { Str }, optional = false, nullable = false),
            Field("baseUrl", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> KeySet(a[0] as String, a[1] as String, a[2] as String, a[3] as String?) },
            { v -> arrayOf(v.account, v.path, v.masked, v.baseUrl) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class KeySetReq(
    val configDir: String,
    val key: String,
    val baseUrl: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<KeySetReq> = obj(
            listOf(
            Field("configDir", { Str }, optional = false, nullable = false),
            Field("key", { Str }, optional = false, nullable = false),
            Field("baseUrl", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> KeySetReq(a[0] as String, a[1] as String, a[2] as String?) },
            { v -> arrayOf(v.configDir, v.key, v.baseUrl) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Kin(
    val sid: String,
    val title: String,
    val face: SessionFace,
    val hidden: Hidden?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Kin> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("title", { Str }, optional = false, nullable = false),
            Field("face", { SessionFace.ADAPTER }, optional = false, nullable = false),
            Field("hidden", { opt(Hidden.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> Kin(a[0] as String, a[1] as String, a[2] as SessionFace, a[3] as Hidden?) },
            { v -> arrayOf(v.sid, v.title, v.face, v.hidden) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class KnownHost(
    val host: String,
    val user: String,
    val port: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<KnownHost> = obj(
            listOf(
            Field("host", { Str }, optional = false, nullable = false),
            Field("user", { Str }, optional = false, nullable = false),
            Field("port", { Lng }, optional = false, nullable = false),
            ),
            { a -> KnownHost(a[0] as String, a[1] as String, a[2] as Long) },
            { v -> arrayOf(v.host, v.user, v.port) },
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
data class LastSay(
    val text: String,
    val at: String?,
    val atMs: Long?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LastSay> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            Field("at", { opt(Str) }, optional = false, nullable = true),
            Field("atMs", { opt(Lng) }, optional = false, nullable = true),
            ),
            { a -> LastSay(a[0] as String, a[1] as String?, a[2] as Long?) },
            { v -> arrayOf(v.text, v.at, v.atMs) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LastSeen(
    val accounts: Seen<AccountsList>?,
    val data: Seen<DataReport>?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LastSeen> = obj(
            listOf(
            Field("accounts", { opt(Seen.adapter(AccountsList.ADAPTER)) }, optional = false, nullable = true),
            Field("data", { opt(Seen.adapter(DataReport.ADAPTER)) }, optional = false, nullable = true),
            ),
            { a -> LastSeen(a[0] as Seen<AccountsList>?, a[1] as Seen<DataReport>?) },
            { v -> arrayOf(v.accounts, v.data) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LastSeenReadReq(
    val origin: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LastSeenReadReq> = obj(
            listOf(
            Field("origin", { Str }, optional = false, nullable = false),
            ),
            { a -> LastSeenReadReq(a[0] as String) },
            { v -> arrayOf(v.origin) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LastSeenWriteReq(
    val origin: String,
    val kind: SeenKind? = null,
    val value: Map<String, Raw>? = null,
    val forget: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LastSeenWriteReq> = obj(
            listOf(
            Field("origin", { Str }, optional = false, nullable = false),
            Field("kind", { opt(SeenKind.ADAPTER) }, optional = true, nullable = true),
            Field("value", { opt(dict(RawJson)) }, optional = true, nullable = true),
            Field("forget", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> LastSeenWriteReq(a[0] as String, a[1] as SeenKind?, a[2] as Map<String, Raw>?, a[3] as Boolean?) },
            { v -> arrayOf(v.origin, v.kind, v.value, v.forget) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LastSeenWritten(
    val atMs: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LastSeenWritten> = obj(
            listOf(
            Field("atMs", { Lng }, optional = false, nullable = false),
            ),
            { a -> LastSeenWritten(a[0] as Long) },
            { v -> arrayOf(v.atMs) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LaunchFail(
    val code: String,
    val message: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LaunchFail> = obj(
            listOf(
            Field("code", { Str }, optional = false, nullable = false),
            Field("message", { Str }, optional = false, nullable = false),
            ),
            { a -> LaunchFail(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.code, v.message) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LaunchTab(
    val tab: Tab,
    val alias: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LaunchTab> = obj(
            listOf(
            Field("tab", { Tab.ADAPTER }, optional = false, nullable = false),
            Field("alias", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> LaunchTab(a[0] as Tab, a[1] as String?) },
            { v -> arrayOf(v.tab, v.alias) },
        )
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

@Suppress("UNCHECKED_CAST")
data class LedgerFile(
    val digest: String,
    val created: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LedgerFile> = obj(
            listOf(
            Field("digest", { Str }, optional = false, nullable = false),
            Field("created", { Bool }, optional = false, nullable = false),
            ),
            { a -> LedgerFile(a[0] as String, a[1] as Boolean) },
            { v -> arrayOf(v.digest, v.created) },
        )
    }
}

sealed interface Life {
    /** 判别格（`is`）那个词。 */
    val `is`: String

    @Suppress("UNCHECKED_CAST")
    data class Live(
        val activity: WireSessionActivity? = null,
    ) : Life, Ordered {
        override val `is`: String get() = "live"
        override var wireOrder: IntArray? = null
    }
    data object Reconnectable : Life {
        override val `is`: String get() = "reconnectable"
    }
    @Suppress("UNCHECKED_CAST")
    data class Ended(
        val by: String? = null,
    ) : Life, Ordered {
        override val `is`: String get() = "ended"
        override var wireOrder: IntArray? = null
    }
    data object Gone : Life {
        override val `is`: String get() = "gone"
    }
    data object Unclear : Life {
        override val `is`: String get() = "unclear"
    }

    companion object {
        val ADAPTER: JsonAdapter<Life> =
            tagged<Life>(
                "is",
                listOf(
                Variant("live", Life.Live::class.java, obj(
                        listOf(
                        Field("is", { tag("live") }, optional = false, nullable = false),
                        Field("activity", { opt(WireSessionActivity.ADAPTER) }, optional = true, nullable = true),
                        ),
                        { a -> Life.Live(a[1] as WireSessionActivity?) },
                        { v -> arrayOf("live", v.activity) },
                    )),
                Variant("reconnectable", Life.Reconnectable::class.java, obj(
                        listOf(
                        Field("is", { tag("reconnectable") }, optional = false, nullable = false),
                        ),
                        { Life.Reconnectable },
                        { arrayOf("reconnectable") },
                    )),
                Variant("ended", Life.Ended::class.java, obj(
                        listOf(
                        Field("is", { tag("ended") }, optional = false, nullable = false),
                        Field("by", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> Life.Ended(a[1] as String?) },
                        { v -> arrayOf("ended", v.by) },
                    )),
                Variant("gone", Life.Gone::class.java, obj(
                        listOf(
                        Field("is", { tag("gone") }, optional = false, nullable = false),
                        ),
                        { Life.Gone },
                        { arrayOf("gone") },
                    )),
                Variant("unclear", Life.Unclear::class.java, obj(
                        listOf(
                        Field("is", { tag("unclear") }, optional = false, nullable = false),
                        ),
                        { Life.Unclear },
                        { arrayOf("unclear") },
                    )),
                ),
            )
    }
}

enum class LimitFrom(val wire: String) {
    RELAY("relay"),
    SETTING("setting"),
    MODEL("model"),
    OBSERVED("observed"),
    ASSUMED("assumed"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<LimitFrom> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class Line<R>(
    val sessionId: String,
    val path: String,
    val seq: Long,
    val record: R? = null,
    val cwd: String? = null,
    val byteOffset: Long,
    val rid: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        fun <R> adapter(adR: JsonAdapter<R>): JsonAdapter<Line<R>> = obj(
            listOf(
            Field("session_id", { Str }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            Field("seq", { U64 }, optional = false, nullable = false),
            Field("record", { opt(adR) }, optional = true, nullable = true),
            Field("cwd", { opt(Str) }, optional = true, nullable = true),
            Field("byte_offset", { U64 }, optional = false, nullable = false),
            Field("rid", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> Line<R>(a[0] as String, a[1] as String, a[2] as Long, a[3] as R?, a[4] as String?, a[5] as Long, a[6] as String?) },
            { v -> arrayOf(v.sessionId, v.path, v.seq, v.record, v.cwd, v.byteOffset, v.rid) },
        )
    }
}

sealed interface LineRecord {
    /** 判别格（`t`）那个词。 */
    val t: String

    val agent: String

    val id: String

    val at: String?

    val timeText: String?

    val atMs: Long?

    val draw: Draw

    val human: Boolean

    @Suppress("UNCHECKED_CAST")
    data class Said(
        override val agent: String,
        override val id: String,
        override val at: String? = null,
        override val timeText: String? = null,
        override val atMs: Long? = null,
        override val draw: Draw,
        override val human: Boolean,
        val who: UserText,
        val blocks: List<Block>,
        val results: Map<String, StepResult>? = null,
        val cwd: String? = null,
        val attachments: List<Attachment>? = null,
    ) : LineRecord, Ordered {
        override val t: String get() = "said"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Reply(
        override val agent: String,
        override val id: String,
        override val at: String? = null,
        override val timeText: String? = null,
        override val atMs: Long? = null,
        override val draw: Draw,
        override val human: Boolean,
        val blocks: List<Block>,
        val model: String? = null,
        val autoReply: Boolean,
        val cards: Map<String, ToolCard>? = null,
        val steps: Map<String, ToolStep>? = null,
        val runs: Map<String, ChildRunTag>? = null,
        val error: ReplyError? = null,
    ) : LineRecord, Ordered {
        override val t: String get() = "reply"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Retry(
        override val agent: String,
        override val id: String,
        override val at: String? = null,
        override val timeText: String? = null,
        override val atMs: Long? = null,
        override val draw: Draw,
        override val human: Boolean,
        val reason: ApiReason,
        val attempt: Int? = null,
        val max: Int? = null,
    ) : LineRecord, Ordered {
        override val t: String get() = "retry"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Title(
        override val agent: String,
        override val id: String,
        override val at: String? = null,
        override val timeText: String? = null,
        override val atMs: Long? = null,
        override val draw: Draw,
        override val human: Boolean,
        val text: String,
        val by: TitleBy,
    ) : LineRecord, Ordered {
        override val t: String get() = "title"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Unread(
        override val agent: String,
        override val id: String,
        override val at: String? = null,
        override val timeText: String? = null,
        override val atMs: Long? = null,
        override val draw: Draw,
        override val human: Boolean,
        val why: UnreadWhy,
        val `type`: String?,
        val text: String,
        val tone: LineRecordUnreadTone,
        val excerpt: String,
        val count: Int,
    ) : LineRecord, Ordered {
        override val t: String get() = "unread"
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
                        Field("draw", { Draw.ADAPTER }, optional = false, nullable = false),
                        Field("human", { Bool }, optional = false, nullable = false),
                        Field("t", { tag("said") }, optional = false, nullable = false),
                        Field("who", { UserText.ADAPTER }, optional = false, nullable = false),
                        Field("blocks", { arr(Block.ADAPTER) }, optional = false, nullable = false),
                        Field("results", { opt(dict(StepResult.ADAPTER)) }, optional = true, nullable = true),
                        Field("cwd", { opt(Str) }, optional = true, nullable = true),
                        Field("attachments", { opt(arr(Attachment.ADAPTER)) }, optional = true, nullable = true),
                        ),
                        { a -> LineRecord.Said(a[0] as String, a[1] as String, a[2] as String?, a[3] as String?, a[4] as Long?, a[5] as Draw, a[6] as Boolean, a[8] as UserText, a[9] as List<Block>, a[10] as Map<String, StepResult>?, a[11] as String?, a[12] as List<Attachment>?) },
                        { v -> arrayOf(v.agent, v.id, v.at, v.timeText, v.atMs, v.draw, v.human, "said", v.who, v.blocks, v.results, v.cwd, v.attachments) },
                    )),
                Variant("reply", LineRecord.Reply::class.java, obj(
                        listOf(
                        Field("agent", { Str }, optional = false, nullable = false),
                        Field("id", { Str }, optional = false, nullable = false),
                        Field("at", { opt(Str) }, optional = true, nullable = true),
                        Field("timeText", { opt(Str) }, optional = true, nullable = true),
                        Field("atMs", { opt(Lng) }, optional = true, nullable = true),
                        Field("draw", { Draw.ADAPTER }, optional = false, nullable = false),
                        Field("human", { Bool }, optional = false, nullable = false),
                        Field("t", { tag("reply") }, optional = false, nullable = false),
                        Field("blocks", { arr(Block.ADAPTER) }, optional = false, nullable = false),
                        Field("model", { opt(Str) }, optional = true, nullable = true),
                        Field("autoReply", { Bool }, optional = false, nullable = false),
                        Field("cards", { opt(dict(ToolCard.ADAPTER)) }, optional = true, nullable = true),
                        Field("steps", { opt(dict(ToolStep.ADAPTER)) }, optional = true, nullable = true),
                        Field("runs", { opt(dict(ChildRunTag.ADAPTER)) }, optional = true, nullable = true),
                        Field("error", { opt(ReplyError.ADAPTER) }, optional = true, nullable = true),
                        ),
                        { a -> LineRecord.Reply(a[0] as String, a[1] as String, a[2] as String?, a[3] as String?, a[4] as Long?, a[5] as Draw, a[6] as Boolean, a[8] as List<Block>, a[9] as String?, a[10] as Boolean, a[11] as Map<String, ToolCard>?, a[12] as Map<String, ToolStep>?, a[13] as Map<String, ChildRunTag>?, a[14] as ReplyError?) },
                        { v -> arrayOf(v.agent, v.id, v.at, v.timeText, v.atMs, v.draw, v.human, "reply", v.blocks, v.model, v.autoReply, v.cards, v.steps, v.runs, v.error) },
                    )),
                Variant("retry", LineRecord.Retry::class.java, obj(
                        listOf(
                        Field("agent", { Str }, optional = false, nullable = false),
                        Field("id", { Str }, optional = false, nullable = false),
                        Field("at", { opt(Str) }, optional = true, nullable = true),
                        Field("timeText", { opt(Str) }, optional = true, nullable = true),
                        Field("atMs", { opt(Lng) }, optional = true, nullable = true),
                        Field("draw", { Draw.ADAPTER }, optional = false, nullable = false),
                        Field("human", { Bool }, optional = false, nullable = false),
                        Field("t", { tag("retry") }, optional = false, nullable = false),
                        Field("reason", { ApiReason.ADAPTER }, optional = false, nullable = false),
                        Field("attempt", { opt(Int32) }, optional = true, nullable = true),
                        Field("max", { opt(Int32) }, optional = true, nullable = true),
                        ),
                        { a -> LineRecord.Retry(a[0] as String, a[1] as String, a[2] as String?, a[3] as String?, a[4] as Long?, a[5] as Draw, a[6] as Boolean, a[8] as ApiReason, a[9] as Int?, a[10] as Int?) },
                        { v -> arrayOf(v.agent, v.id, v.at, v.timeText, v.atMs, v.draw, v.human, "retry", v.reason, v.attempt, v.max) },
                    )),
                Variant("title", LineRecord.Title::class.java, obj(
                        listOf(
                        Field("agent", { Str }, optional = false, nullable = false),
                        Field("id", { Str }, optional = false, nullable = false),
                        Field("at", { opt(Str) }, optional = true, nullable = true),
                        Field("timeText", { opt(Str) }, optional = true, nullable = true),
                        Field("atMs", { opt(Lng) }, optional = true, nullable = true),
                        Field("draw", { Draw.ADAPTER }, optional = false, nullable = false),
                        Field("human", { Bool }, optional = false, nullable = false),
                        Field("t", { tag("title") }, optional = false, nullable = false),
                        Field("text", { Str }, optional = false, nullable = false),
                        Field("by", { TitleBy.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> LineRecord.Title(a[0] as String, a[1] as String, a[2] as String?, a[3] as String?, a[4] as Long?, a[5] as Draw, a[6] as Boolean, a[8] as String, a[9] as TitleBy) },
                        { v -> arrayOf(v.agent, v.id, v.at, v.timeText, v.atMs, v.draw, v.human, "title", v.text, v.by) },
                    )),
                Variant("unread", LineRecord.Unread::class.java, obj(
                        listOf(
                        Field("agent", { Str }, optional = false, nullable = false),
                        Field("id", { Str }, optional = false, nullable = false),
                        Field("at", { opt(Str) }, optional = true, nullable = true),
                        Field("timeText", { opt(Str) }, optional = true, nullable = true),
                        Field("atMs", { opt(Lng) }, optional = true, nullable = true),
                        Field("draw", { Draw.ADAPTER }, optional = false, nullable = false),
                        Field("human", { Bool }, optional = false, nullable = false),
                        Field("t", { tag("unread") }, optional = false, nullable = false),
                        Field("why", { UnreadWhy.ADAPTER }, optional = false, nullable = false),
                        Field("type", { opt(Str) }, optional = false, nullable = true),
                        Field("text", { Str }, optional = false, nullable = false),
                        Field("tone", { LineRecordUnreadTone.ADAPTER }, optional = false, nullable = false),
                        Field("excerpt", { Str }, optional = false, nullable = false),
                        Field("count", { Int32 }, optional = false, nullable = false),
                        ),
                        { a -> LineRecord.Unread(a[0] as String, a[1] as String, a[2] as String?, a[3] as String?, a[4] as Long?, a[5] as Draw, a[6] as Boolean, a[8] as UnreadWhy, a[9] as String?, a[10] as String, a[11] as LineRecordUnreadTone, a[12] as String, a[13] as Int) },
                        { v -> arrayOf(v.agent, v.id, v.at, v.timeText, v.atMs, v.draw, v.human, "unread", v.why, v.`type`, v.text, v.tone, v.excerpt, v.count) },
                    )),
                ),
            )
    }
}

sealed interface LineRecordBody {
    /** 判别格（`t`）那个词。 */
    val t: String

    @Suppress("UNCHECKED_CAST")
    data class Said(
        val who: UserText,
        val blocks: List<Block>,
        val results: Map<String, StepResult>? = null,
        val cwd: String? = null,
        val attachments: List<Attachment>? = null,
    ) : LineRecordBody, Ordered {
        override val t: String get() = "said"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Reply(
        val blocks: List<Block>,
        val model: String? = null,
        val autoReply: Boolean,
        val cards: Map<String, ToolCard>? = null,
        val steps: Map<String, ToolStep>? = null,
        val runs: Map<String, ChildRunTag>? = null,
        val error: ReplyError? = null,
    ) : LineRecordBody, Ordered {
        override val t: String get() = "reply"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Retry(
        val reason: ApiReason,
        val attempt: Int? = null,
        val max: Int? = null,
    ) : LineRecordBody, Ordered {
        override val t: String get() = "retry"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Title(
        val text: String,
        val by: TitleBy,
    ) : LineRecordBody, Ordered {
        override val t: String get() = "title"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Unread(
        val why: UnreadWhy,
        val `type`: String?,
        val text: String,
        val tone: LineRecordBodyUnreadTone,
        val excerpt: String,
        val count: Int,
    ) : LineRecordBody, Ordered {
        override val t: String get() = "unread"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<LineRecordBody> =
            tagged<LineRecordBody>(
                "t",
                listOf(
                Variant("said", LineRecordBody.Said::class.java, obj(
                        listOf(
                        Field("t", { tag("said") }, optional = false, nullable = false),
                        Field("who", { UserText.ADAPTER }, optional = false, nullable = false),
                        Field("blocks", { arr(Block.ADAPTER) }, optional = false, nullable = false),
                        Field("results", { opt(dict(StepResult.ADAPTER)) }, optional = true, nullable = true),
                        Field("cwd", { opt(Str) }, optional = true, nullable = true),
                        Field("attachments", { opt(arr(Attachment.ADAPTER)) }, optional = true, nullable = true),
                        ),
                        { a -> LineRecordBody.Said(a[1] as UserText, a[2] as List<Block>, a[3] as Map<String, StepResult>?, a[4] as String?, a[5] as List<Attachment>?) },
                        { v -> arrayOf("said", v.who, v.blocks, v.results, v.cwd, v.attachments) },
                    )),
                Variant("reply", LineRecordBody.Reply::class.java, obj(
                        listOf(
                        Field("t", { tag("reply") }, optional = false, nullable = false),
                        Field("blocks", { arr(Block.ADAPTER) }, optional = false, nullable = false),
                        Field("model", { opt(Str) }, optional = true, nullable = true),
                        Field("autoReply", { Bool }, optional = false, nullable = false),
                        Field("cards", { opt(dict(ToolCard.ADAPTER)) }, optional = true, nullable = true),
                        Field("steps", { opt(dict(ToolStep.ADAPTER)) }, optional = true, nullable = true),
                        Field("runs", { opt(dict(ChildRunTag.ADAPTER)) }, optional = true, nullable = true),
                        Field("error", { opt(ReplyError.ADAPTER) }, optional = true, nullable = true),
                        ),
                        { a -> LineRecordBody.Reply(a[1] as List<Block>, a[2] as String?, a[3] as Boolean, a[4] as Map<String, ToolCard>?, a[5] as Map<String, ToolStep>?, a[6] as Map<String, ChildRunTag>?, a[7] as ReplyError?) },
                        { v -> arrayOf("reply", v.blocks, v.model, v.autoReply, v.cards, v.steps, v.runs, v.error) },
                    )),
                Variant("retry", LineRecordBody.Retry::class.java, obj(
                        listOf(
                        Field("t", { tag("retry") }, optional = false, nullable = false),
                        Field("reason", { ApiReason.ADAPTER }, optional = false, nullable = false),
                        Field("attempt", { opt(Int32) }, optional = true, nullable = true),
                        Field("max", { opt(Int32) }, optional = true, nullable = true),
                        ),
                        { a -> LineRecordBody.Retry(a[1] as ApiReason, a[2] as Int?, a[3] as Int?) },
                        { v -> arrayOf("retry", v.reason, v.attempt, v.max) },
                    )),
                Variant("title", LineRecordBody.Title::class.java, obj(
                        listOf(
                        Field("t", { tag("title") }, optional = false, nullable = false),
                        Field("text", { Str }, optional = false, nullable = false),
                        Field("by", { TitleBy.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> LineRecordBody.Title(a[1] as String, a[2] as TitleBy) },
                        { v -> arrayOf("title", v.text, v.by) },
                    )),
                Variant("unread", LineRecordBody.Unread::class.java, obj(
                        listOf(
                        Field("t", { tag("unread") }, optional = false, nullable = false),
                        Field("why", { UnreadWhy.ADAPTER }, optional = false, nullable = false),
                        Field("type", { opt(Str) }, optional = false, nullable = true),
                        Field("text", { Str }, optional = false, nullable = false),
                        Field("tone", { LineRecordBodyUnreadTone.ADAPTER }, optional = false, nullable = false),
                        Field("excerpt", { Str }, optional = false, nullable = false),
                        Field("count", { Int32 }, optional = false, nullable = false),
                        ),
                        { a -> LineRecordBody.Unread(a[1] as UnreadWhy, a[2] as String?, a[3] as String, a[4] as LineRecordBodyUnreadTone, a[5] as String, a[6] as Int) },
                        { v -> arrayOf("unread", v.why, v.`type`, v.text, v.tone, v.excerpt, v.count) },
                    )),
                ),
            )
    }
}

enum class LineRecordBodyUnreadTone(val wire: String) {
    WARN("warn"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<LineRecordBodyUnreadTone> = enumOf(entries) { it.wire }
    }
}

enum class LineRecordUnreadTone(val wire: String) {
    WARN("warn"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<LineRecordUnreadTone> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class Link(
    val pid: Long,
    val name: String,
    val start: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Link> = obj(
            listOf(
            Field("pid", { Lng }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("start", { Lng }, optional = false, nullable = false),
            ),
            { a -> Link(a[0] as Long, a[1] as String, a[2] as Long) },
            { v -> arrayOf(v.pid, v.name, v.start) },
        )
    }
}

sealed interface LinkLine {
    /** 判别格（`state`）那个词。 */
    val state: String

    val machine: MachineState

    @Suppress("UNCHECKED_CAST")
    data class Connecting(
        override val machine: MachineState,
    ) : LinkLine, Ordered {
        override val state: String get() = "connecting"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Up(
        val epoch: Long,
        override val machine: MachineState,
    ) : LinkLine, Ordered {
        override val state: String get() = "up"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Down(
        override val machine: MachineState,
    ) : LinkLine, Ordered {
        override val state: String get() = "down"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<LinkLine> =
            tagged<LinkLine>(
                "state",
                listOf(
                Variant("connecting", LinkLine.Connecting::class.java, obj(
                        listOf(
                        Field("state", { tag("connecting") }, optional = false, nullable = false),
                        Field("machine", { MachineState.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> LinkLine.Connecting(a[1] as MachineState) },
                        { v -> arrayOf("connecting", v.machine) },
                    )),
                Variant("up", LinkLine.Up::class.java, obj(
                        listOf(
                        Field("state", { tag("up") }, optional = false, nullable = false),
                        Field("epoch", { U64 }, optional = false, nullable = false),
                        Field("machine", { MachineState.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> LinkLine.Up(a[1] as Long, a[2] as MachineState) },
                        { v -> arrayOf("up", v.epoch, v.machine) },
                    )),
                Variant("down", LinkLine.Down::class.java, obj(
                        listOf(
                        Field("state", { tag("down") }, optional = false, nullable = false),
                        Field("machine", { MachineState.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> LinkLine.Down(a[1] as MachineState) },
                        { v -> arrayOf("down", v.machine) },
                    )),
                ),
            )
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
            ),
            { a -> ListMeta(a[0] as Boolean, a[1] as String, a[2] as String, a[3] as String?, a[4] as String?, a[5] as Long, a[6] as String?, a[7] as String?, a[8] as String?, a[9] as String?) },
            { v -> arrayOf(v.enabled, v.acctsDir, v.manifestPath, v.updatedAt, v.sharedStore, v.count, v.error, v.unsupported, v.nextDefault, v.effectiveDefault) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LiveBlock(
    val kind: BlockKind,
    val text: String? = null,
    val tool: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LiveBlock> = obj(
            listOf(
            Field("kind", { BlockKind.ADAPTER }, optional = false, nullable = false),
            Field("text", { opt(Str) }, optional = true, nullable = true),
            Field("tool", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> LiveBlock(a[0] as BlockKind, a[1] as String?, a[2] as String?) },
            { v -> arrayOf(v.kind, v.text, v.tool) },
        )
    }
}

sealed interface LiveCard {
    /** 判别格（`op`）那个词。 */
    val op: String

    val sid: String

    val run: String?

    val card: Long

    val n: Long

    @Suppress("UNCHECKED_CAST")
    data class Open(
        override val sid: String,
        override val run: String? = null,
        override val card: Long,
        override val n: Long,
        val rid: String,
        val phase: LivePhase,
        val clipped: Boolean,
        val blocks: List<LiveBlock>,
    ) : LiveCard, Ordered {
        override val op: String get() = "open"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Block(
        override val sid: String,
        override val run: String? = null,
        override val card: Long,
        override val n: Long,
        val i: Long,
        val block: BlockKind,
        val tool: String? = null,
    ) : LiveCard, Ordered {
        override val op: String get() = "block"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Text(
        override val sid: String,
        override val run: String? = null,
        override val card: Long,
        override val n: Long,
        val i: Long,
        val s: String,
    ) : LiveCard, Ordered {
        override val op: String get() = "text"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Awaiting(
        override val sid: String,
        override val run: String? = null,
        override val card: Long,
        override val n: Long,
    ) : LiveCard, Ordered {
        override val op: String get() = "awaiting"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Close(
        override val sid: String,
        override val run: String? = null,
        override val card: Long,
        override val n: Long,
        val why: LiveWhy,
    ) : LiveCard, Ordered {
        override val op: String get() = "close"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<LiveCard> =
            tagged<LiveCard>(
                "op",
                listOf(
                Variant("open", LiveCard.Open::class.java, obj(
                        listOf(
                        Field("sid", { Str }, optional = false, nullable = false),
                        Field("run", { opt(Str) }, optional = true, nullable = true),
                        Field("card", { U64 }, optional = false, nullable = false),
                        Field("n", { U64 }, optional = false, nullable = false),
                        Field("op", { tag("open") }, optional = false, nullable = false),
                        Field("rid", { Str }, optional = false, nullable = false),
                        Field("phase", { LivePhase.ADAPTER }, optional = false, nullable = false),
                        Field("clipped", { Bool }, optional = false, nullable = false),
                        Field("blocks", { arr(LiveBlock.ADAPTER) }, optional = false, nullable = false),
                        ),
                        { a -> LiveCard.Open(a[0] as String, a[1] as String?, a[2] as Long, a[3] as Long, a[5] as String, a[6] as LivePhase, a[7] as Boolean, a[8] as List<LiveBlock>) },
                        { v -> arrayOf(v.sid, v.run, v.card, v.n, "open", v.rid, v.phase, v.clipped, v.blocks) },
                    )),
                Variant("block", LiveCard.Block::class.java, obj(
                        listOf(
                        Field("sid", { Str }, optional = false, nullable = false),
                        Field("run", { opt(Str) }, optional = true, nullable = true),
                        Field("card", { U64 }, optional = false, nullable = false),
                        Field("n", { U64 }, optional = false, nullable = false),
                        Field("op", { tag("block") }, optional = false, nullable = false),
                        Field("i", { U64 }, optional = false, nullable = false),
                        Field("block", { BlockKind.ADAPTER }, optional = false, nullable = false),
                        Field("tool", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> LiveCard.Block(a[0] as String, a[1] as String?, a[2] as Long, a[3] as Long, a[5] as Long, a[6] as BlockKind, a[7] as String?) },
                        { v -> arrayOf(v.sid, v.run, v.card, v.n, "block", v.i, v.block, v.tool) },
                    )),
                Variant("text", LiveCard.Text::class.java, obj(
                        listOf(
                        Field("sid", { Str }, optional = false, nullable = false),
                        Field("run", { opt(Str) }, optional = true, nullable = true),
                        Field("card", { U64 }, optional = false, nullable = false),
                        Field("n", { U64 }, optional = false, nullable = false),
                        Field("op", { tag("text") }, optional = false, nullable = false),
                        Field("i", { U64 }, optional = false, nullable = false),
                        Field("s", { Str }, optional = false, nullable = false),
                        ),
                        { a -> LiveCard.Text(a[0] as String, a[1] as String?, a[2] as Long, a[3] as Long, a[5] as Long, a[6] as String) },
                        { v -> arrayOf(v.sid, v.run, v.card, v.n, "text", v.i, v.s) },
                    )),
                Variant("awaiting", LiveCard.Awaiting::class.java, obj(
                        listOf(
                        Field("sid", { Str }, optional = false, nullable = false),
                        Field("run", { opt(Str) }, optional = true, nullable = true),
                        Field("card", { U64 }, optional = false, nullable = false),
                        Field("n", { U64 }, optional = false, nullable = false),
                        Field("op", { tag("awaiting") }, optional = false, nullable = false),
                        ),
                        { a -> LiveCard.Awaiting(a[0] as String, a[1] as String?, a[2] as Long, a[3] as Long) },
                        { v -> arrayOf(v.sid, v.run, v.card, v.n, "awaiting") },
                    )),
                Variant("close", LiveCard.Close::class.java, obj(
                        listOf(
                        Field("sid", { Str }, optional = false, nullable = false),
                        Field("run", { opt(Str) }, optional = true, nullable = true),
                        Field("card", { U64 }, optional = false, nullable = false),
                        Field("n", { U64 }, optional = false, nullable = false),
                        Field("op", { tag("close") }, optional = false, nullable = false),
                        Field("why", { LiveWhy.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> LiveCard.Close(a[0] as String, a[1] as String?, a[2] as Long, a[3] as Long, a[5] as LiveWhy) },
                        { v -> arrayOf(v.sid, v.run, v.card, v.n, "close", v.why) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class LiveCardNow(
    val card: Long,
    val rid: String,
    val phase: LivePhase,
    val clipped: Boolean,
    val blocks: List<LiveBlock>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LiveCardNow> = obj(
            listOf(
            Field("card", { Lng }, optional = false, nullable = false),
            Field("rid", { Str }, optional = false, nullable = false),
            Field("phase", { LivePhase.ADAPTER }, optional = false, nullable = false),
            Field("clipped", { Bool }, optional = false, nullable = false),
            Field("blocks", { arr(LiveBlock.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> LiveCardNow(a[0] as Long, a[1] as String, a[2] as LivePhase, a[3] as Boolean, a[4] as List<LiveBlock>) },
            { v -> arrayOf(v.card, v.rid, v.phase, v.clipped, v.blocks) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LiveCards(
    val sid: String,
    val run: String? = null,
    val cards: List<LiveCardNow>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LiveCards> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("run", { opt(Str) }, optional = true, nullable = true),
            Field("cards", { arr(LiveCardNow.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> LiveCards(a[0] as String, a[1] as String?, a[2] as List<LiveCardNow>) },
            { v -> arrayOf(v.sid, v.run, v.cards) },
        )
    }
}

sealed interface LiveOp {
    /** 判别格（`op`）那个词。 */
    val op: String

    @Suppress("UNCHECKED_CAST")
    data class Open(
        val rid: String,
        val phase: LivePhase,
        val clipped: Boolean,
        val blocks: List<LiveBlock>,
    ) : LiveOp, Ordered {
        override val op: String get() = "open"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Block(
        val i: Long,
        val block: BlockKind,
        val tool: String? = null,
    ) : LiveOp, Ordered {
        override val op: String get() = "block"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Text(
        val i: Long,
        val s: String,
    ) : LiveOp, Ordered {
        override val op: String get() = "text"
        override var wireOrder: IntArray? = null
    }
    data object Awaiting : LiveOp {
        override val op: String get() = "awaiting"
    }
    @Suppress("UNCHECKED_CAST")
    data class Close(
        val why: LiveWhy,
    ) : LiveOp, Ordered {
        override val op: String get() = "close"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<LiveOp> =
            tagged<LiveOp>(
                "op",
                listOf(
                Variant("open", LiveOp.Open::class.java, obj(
                        listOf(
                        Field("op", { tag("open") }, optional = false, nullable = false),
                        Field("rid", { Str }, optional = false, nullable = false),
                        Field("phase", { LivePhase.ADAPTER }, optional = false, nullable = false),
                        Field("clipped", { Bool }, optional = false, nullable = false),
                        Field("blocks", { arr(LiveBlock.ADAPTER) }, optional = false, nullable = false),
                        ),
                        { a -> LiveOp.Open(a[1] as String, a[2] as LivePhase, a[3] as Boolean, a[4] as List<LiveBlock>) },
                        { v -> arrayOf("open", v.rid, v.phase, v.clipped, v.blocks) },
                    )),
                Variant("block", LiveOp.Block::class.java, obj(
                        listOf(
                        Field("op", { tag("block") }, optional = false, nullable = false),
                        Field("i", { U64 }, optional = false, nullable = false),
                        Field("block", { BlockKind.ADAPTER }, optional = false, nullable = false),
                        Field("tool", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> LiveOp.Block(a[1] as Long, a[2] as BlockKind, a[3] as String?) },
                        { v -> arrayOf("block", v.i, v.block, v.tool) },
                    )),
                Variant("text", LiveOp.Text::class.java, obj(
                        listOf(
                        Field("op", { tag("text") }, optional = false, nullable = false),
                        Field("i", { U64 }, optional = false, nullable = false),
                        Field("s", { Str }, optional = false, nullable = false),
                        ),
                        { a -> LiveOp.Text(a[1] as Long, a[2] as String) },
                        { v -> arrayOf("text", v.i, v.s) },
                    )),
                Variant("awaiting", LiveOp.Awaiting::class.java, obj(
                        listOf(
                        Field("op", { tag("awaiting") }, optional = false, nullable = false),
                        ),
                        { LiveOp.Awaiting },
                        { arrayOf("awaiting") },
                    )),
                Variant("close", LiveOp.Close::class.java, obj(
                        listOf(
                        Field("op", { tag("close") }, optional = false, nullable = false),
                        Field("why", { LiveWhy.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> LiveOp.Close(a[1] as LiveWhy) },
                        { v -> arrayOf("close", v.why) },
                    )),
                ),
            )
    }
}

enum class LivePhase(val wire: String) {
    STREAMING("streaming"),
    AWAITING("awaiting"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<LivePhase> = enumOf(entries) { it.wire }
    }
}

enum class LiveWhy(val wire: String) {
    RECORD("record"),
    BROKEN("broken"),
    ERROR("error"),
    EVICTED("evicted"),
    GONE("gone"),
    UNWATCHED("unwatched"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<LiveWhy> = enumOf(entries) { it.wire }
    }
}

sealed interface LocalBackend {
    /** 判别格（`state`）那个词。 */
    val state: String

    data object Up : LocalBackend {
        override val state: String get() = "up"
    }
    @Suppress("UNCHECKED_CAST")
    data class Starting(
        val word: String,
        val said: String,
    ) : LocalBackend, Ordered {
        override val state: String get() = "starting"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Down(
        val word: String,
        val said: String,
        val fixes: List<MachineFixButton>,
        val next: String?,
    ) : LocalBackend, Ordered {
        override val state: String get() = "down"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Older(
        val word: String,
        val said: String,
        val fixes: List<MachineFixButton>,
        val next: String?,
    ) : LocalBackend, Ordered {
        override val state: String get() = "older"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Newer(
        val word: String,
        val said: String,
        val fixes: List<MachineFixButton>,
        val next: String?,
    ) : LocalBackend, Ordered {
        override val state: String get() = "newer"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<LocalBackend> =
            tagged<LocalBackend>(
                "state",
                listOf(
                Variant("up", LocalBackend.Up::class.java, obj(
                        listOf(
                        Field("state", { tag("up") }, optional = false, nullable = false),
                        ),
                        { LocalBackend.Up },
                        { arrayOf("up") },
                    )),
                Variant("starting", LocalBackend.Starting::class.java, obj(
                        listOf(
                        Field("state", { tag("starting") }, optional = false, nullable = false),
                        Field("word", { Str }, optional = false, nullable = false),
                        Field("said", { Str }, optional = false, nullable = false),
                        ),
                        { a -> LocalBackend.Starting(a[1] as String, a[2] as String) },
                        { v -> arrayOf("starting", v.word, v.said) },
                    )),
                Variant("down", LocalBackend.Down::class.java, obj(
                        listOf(
                        Field("state", { tag("down") }, optional = false, nullable = false),
                        Field("word", { Str }, optional = false, nullable = false),
                        Field("said", { Str }, optional = false, nullable = false),
                        Field("fixes", { arr(MachineFixButton.ADAPTER) }, optional = false, nullable = false),
                        Field("next", { opt(Str) }, optional = false, nullable = true),
                        ),
                        { a -> LocalBackend.Down(a[1] as String, a[2] as String, a[3] as List<MachineFixButton>, a[4] as String?) },
                        { v -> arrayOf("down", v.word, v.said, v.fixes, v.next) },
                    )),
                Variant("older", LocalBackend.Older::class.java, obj(
                        listOf(
                        Field("state", { tag("older") }, optional = false, nullable = false),
                        Field("word", { Str }, optional = false, nullable = false),
                        Field("said", { Str }, optional = false, nullable = false),
                        Field("fixes", { arr(MachineFixButton.ADAPTER) }, optional = false, nullable = false),
                        Field("next", { opt(Str) }, optional = false, nullable = true),
                        ),
                        { a -> LocalBackend.Older(a[1] as String, a[2] as String, a[3] as List<MachineFixButton>, a[4] as String?) },
                        { v -> arrayOf("older", v.word, v.said, v.fixes, v.next) },
                    )),
                Variant("newer", LocalBackend.Newer::class.java, obj(
                        listOf(
                        Field("state", { tag("newer") }, optional = false, nullable = false),
                        Field("word", { Str }, optional = false, nullable = false),
                        Field("said", { Str }, optional = false, nullable = false),
                        Field("fixes", { arr(MachineFixButton.ADAPTER) }, optional = false, nullable = false),
                        Field("next", { opt(Str) }, optional = false, nullable = true),
                        ),
                        { a -> LocalBackend.Newer(a[1] as String, a[2] as String, a[3] as List<MachineFixButton>, a[4] as String?) },
                        { v -> arrayOf("newer", v.word, v.said, v.fixes, v.next) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class LogFileEntry(
    val path: String,
    val shown: String,
    val sizeText: String?,
    val modifiedMs: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LogFileEntry> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("shown", { Str }, optional = false, nullable = false),
            Field("size_text", { opt(Str) }, optional = false, nullable = true),
            Field("modified_ms", { Lng }, optional = false, nullable = false),
            ),
            { a -> LogFileEntry(a[0] as String, a[1] as String, a[2] as String?, a[3] as Long) },
            { v -> arrayOf(v.path, v.shown, v.sizeText, v.modifiedMs) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LogFileInfo(
    val dir: String,
    val dirShown: String,
    val currentFile: String?,
    val currentFileShown: String?,
    val currentSizeText: String?,
    val allFiles: List<LogFileEntry>,
    val backendStderr: List<LogFileEntry>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LogFileInfo> = obj(
            listOf(
            Field("dir", { Str }, optional = false, nullable = false),
            Field("dir_shown", { Str }, optional = false, nullable = false),
            Field("current_file", { opt(Str) }, optional = false, nullable = true),
            Field("current_file_shown", { opt(Str) }, optional = false, nullable = true),
            Field("current_size_text", { opt(Str) }, optional = false, nullable = true),
            Field("all_files", { arr(LogFileEntry.ADAPTER) }, optional = false, nullable = false),
            Field("backend_stderr", { arr(LogFileEntry.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> LogFileInfo(a[0] as String, a[1] as String, a[2] as String?, a[3] as String?, a[4] as String?, a[5] as List<LogFileEntry>, a[6] as List<LogFileEntry>) },
            { v -> arrayOf(v.dir, v.dirShown, v.currentFile, v.currentFileShown, v.currentSizeText, v.allFiles, v.backendStderr) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LogTail(
    val path: String?,
    val sizeText: String?,
    val text: String,
    val truncated: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LogTail> = obj(
            listOf(
            Field("path", { opt(Str) }, optional = false, nullable = true),
            Field("size_text", { opt(Str) }, optional = false, nullable = true),
            Field("text", { Str }, optional = false, nullable = false),
            Field("truncated", { Bool }, optional = false, nullable = false),
            ),
            { a -> LogTail(a[0] as String?, a[1] as String?, a[2] as String, a[3] as Boolean) },
            { v -> arrayOf(v.path, v.sizeText, v.text, v.truncated) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LoginAgain(
    val again: LoginAgainHow,
    val terminal: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LoginAgain> = obj(
            listOf(
            Field("again", { LoginAgainHow.ADAPTER }, optional = false, nullable = false),
            Field("terminal", { Bool }, optional = false, nullable = false),
            ),
            { a -> LoginAgain(a[0] as LoginAgainHow, a[1] as Boolean) },
            { v -> arrayOf(v.again, v.terminal) },
        )
    }
}

enum class LoginAgainHow(val wire: String) {
    PASTE("paste"),
    RESTART("restart"),
    NONE("none"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<LoginAgainHow> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class LoginCancelReq(
    val job: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LoginCancelReq> = obj(
            listOf(
            Field("job", { Str }, optional = false, nullable = false),
            ),
            { a -> LoginCancelReq(a[0] as String) },
            { v -> arrayOf(v.job) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LoginCancelled(
    val was: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LoginCancelled> = obj(
            listOf(
            Field("was", { Bool }, optional = false, nullable = false),
            ),
            { a -> LoginCancelled(a[0] as Boolean) },
            { v -> arrayOf(v.was) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LoginDone(
    val target: LoginStartReq,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LoginDone> = obj(
            listOf(
            Field("target", { LoginStartReq.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> LoginDone(a[0] as LoginStartReq) },
            { v -> arrayOf(v.target) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LoginFinishReq(
    val job: String,
    val pasted: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LoginFinishReq> = obj(
            listOf(
            Field("job", { Str }, optional = false, nullable = false),
            Field("pasted", { Str }, optional = false, nullable = false),
            ),
            { a -> LoginFinishReq(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.job, v.pasted) },
        )
    }
}

sealed interface LoginStartArgs {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    val browserHere: Boolean?

    val agent: String

    val account: String?

    @Suppress("UNCHECKED_CAST")
    data class Account(
        override val browserHere: Boolean? = null,
        override val agent: String,
        override val account: String,
    ) : LoginStartArgs, Ordered {
        override val kind: String get() = "account"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Mcp(
        override val browserHere: Boolean? = null,
        override val agent: String,
        override val account: String?,
        val server: String,
        val projectDir: String?,
    ) : LoginStartArgs, Ordered {
        override val kind: String get() = "mcp"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<LoginStartArgs> =
            tagged<LoginStartArgs>(
                "kind",
                listOf(
                Variant("account", LoginStartArgs.Account::class.java, obj(
                        listOf(
                        Field("browserHere", { opt(Bool) }, optional = true, nullable = true),
                        Field("kind", { tag("account") }, optional = false, nullable = false),
                        Field("agent", { Str }, optional = false, nullable = false),
                        Field("account", { Str }, optional = false, nullable = false),
                        ),
                        { a -> LoginStartArgs.Account(a[0] as Boolean?, a[2] as String, a[3] as String) },
                        { v -> arrayOf(v.browserHere, "account", v.agent, v.account) },
                    )),
                Variant("mcp", LoginStartArgs.Mcp::class.java, obj(
                        listOf(
                        Field("browserHere", { opt(Bool) }, optional = true, nullable = true),
                        Field("kind", { tag("mcp") }, optional = false, nullable = false),
                        Field("agent", { Str }, optional = false, nullable = false),
                        Field("account", { opt(Str) }, optional = false, nullable = true),
                        Field("server", { Str }, optional = false, nullable = false),
                        Field("projectDir", { opt(Str) }, optional = false, nullable = true),
                        ),
                        { a -> LoginStartArgs.Mcp(a[0] as Boolean?, a[2] as String, a[3] as String?, a[4] as String, a[5] as String?) },
                        { v -> arrayOf(v.browserHere, "mcp", v.agent, v.account, v.server, v.projectDir) },
                    )),
                ),
            )
    }
}

sealed interface LoginStartReq {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    val agent: String

    val account: String?

    @Suppress("UNCHECKED_CAST")
    data class Account(
        override val agent: String,
        override val account: String,
    ) : LoginStartReq, Ordered {
        override val kind: String get() = "account"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Mcp(
        override val agent: String,
        override val account: String?,
        val server: String,
        val projectDir: String?,
    ) : LoginStartReq, Ordered {
        override val kind: String get() = "mcp"
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

@Suppress("UNCHECKED_CAST")
data class LoginStarted(
    val job: String,
    val url: String,
    val paste: PasteKind,
    val auto: Boolean,
    val opened: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LoginStarted> = obj(
            listOf(
            Field("job", { Str }, optional = false, nullable = false),
            Field("url", { Str }, optional = false, nullable = false),
            Field("paste", { PasteKind.ADAPTER }, optional = false, nullable = false),
            Field("auto", { Bool }, optional = false, nullable = false),
            Field("opened", { Bool }, optional = false, nullable = false),
            ),
            { a -> LoginStarted(a[0] as String, a[1] as String, a[2] as PasteKind, a[3] as Boolean, a[4] as Boolean) },
            { v -> arrayOf(v.job, v.url, v.paste, v.auto, v.opened) },
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

enum class Looked(val wire: String) {
    EVERY("every"),
    THIS_MACHINE("this_machine"),
    NOTHING("nothing"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<Looked> = enumOf(entries) { it.wire }
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
    val shown: WirePath,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LsEntry> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("name", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("kind", { FileKind.ADAPTER }, optional = false, nullable = false),
            Field("size", { opt(U64) }, optional = true, nullable = true),
            Field("size_text", { opt(Str) }, optional = true, nullable = true),
            Field("mtime_secs", { opt(U64) }, optional = true, nullable = true),
            Field("mtime_text", { opt(Str) }, optional = true, nullable = true),
            Field("mtime_full", { opt(Str) }, optional = true, nullable = true),
            Field("link_dir", { opt(Bool) }, optional = true, nullable = true),
            Field("link_to", { opt(LinkTo.ADAPTER) }, optional = true, nullable = true),
            Field("class", { FileClass.ADAPTER }, optional = false, nullable = false),
            Field("type_text", { Str }, optional = false, nullable = false),
            Field("gates", { opt(Gates.ADAPTER) }, optional = true, nullable = true),
            Field("shown", { WirePath.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> LsEntry(a[0] as WirePath, a[1] as WirePath, a[2] as FileKind, a[3] as Long?, a[4] as String?, a[5] as Long?, a[6] as String?, a[7] as String?, a[8] as Boolean?, a[9] as LinkTo?, a[10] as FileClass, a[11] as String, a[12] as Gates?, a[13] as WirePath) },
            { v -> arrayOf(v.path, v.name, v.kind, v.size, v.sizeText, v.mtimeSecs, v.mtimeText, v.mtimeFull, v.linkDir, v.linkTo, v.`class`, v.typeText, v.gates, v.shown) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LsHead(
    val shown: WirePath,
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
            Field("shown", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("up", { opt(WirePath.ADAPTER) }, optional = false, nullable = true),
            Field("crumbs", { arr(Crumb.ADAPTER) }, optional = false, nullable = false),
            Field("truncated", { Bool }, optional = false, nullable = false),
            Field("total", { U64 }, optional = false, nullable = false),
            Field("unreadable", { U64 }, optional = false, nullable = false),
            ),
            { a -> LsHead(a[0] as WirePath, a[1] as WirePath?, a[2] as List<Crumb>, a[3] as Boolean, a[4] as Long, a[5] as Long) },
            { v -> arrayOf(v.shown, v.up, v.crumbs, v.truncated, v.total, v.unreadable) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LsReq(
    val path: WirePath,
    val limit: Long? = null,
    val dirsOnly: Boolean? = null,
    val sort: LsSort? = null,
    val held: String? = null,
    val range: SyncRange? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LsReq> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("limit", { opt(U64) }, optional = true, nullable = true),
            Field("dirs_only", { opt(Bool) }, optional = true, nullable = true),
            Field("sort", { opt(LsSort.ADAPTER) }, optional = true, nullable = true),
            Field("held", { opt(Str) }, optional = true, nullable = true),
            Field("range", { opt(SyncRange.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> LsReq(a[0] as WirePath, a[1] as Long?, a[2] as Boolean?, a[3] as LsSort?, a[4] as String?, a[5] as SyncRange?) },
            { v -> arrayOf(v.path, v.limit, v.dirsOnly, v.sort, v.held, v.range) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class LsSort(
    val by: LsSortBy,
    val reverse: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<LsSort> = obj(
            listOf(
            Field("by", { LsSortBy.ADAPTER }, optional = false, nullable = false),
            Field("reverse", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> LsSort(a[0] as LsSortBy, a[1] as Boolean?) },
            { v -> arrayOf(v.by, v.reverse) },
        )
    }
}

enum class LsSortBy(val wire: String) {
    NAME("name"),
    SIZE("size"),
    MTIME("mtime"),
    TYPE("type"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<LsSortBy> = enumOf(entries) { it.wire }
    }
}

enum class MachineBlocker(val wire: String) {
    NOT_UNIX("not_unix"),
    NO_BACKEND("no_backend"),
    OLD_BACKEND("old_backend"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<MachineBlocker> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class MachineCheck(
    val fault: MachineFault?,
    val endpoints: List<Endpoint>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachineCheck> = obj(
            listOf(
            Field("fault", { opt(MachineFault.ADAPTER) }, optional = false, nullable = true),
            Field("endpoints", { arr(Endpoint.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> MachineCheck(a[0] as MachineFault?, a[1] as List<Endpoint>) },
            { v -> arrayOf(v.fault, v.endpoints) },
        )
    }
}

sealed interface MachineEdit {
    /** 判别格（`op`）那个词。 */
    val op: String

    val where: List<ElemKey>

    @Suppress("UNCHECKED_CAST")
    data class Setin(
        override val where: List<ElemKey>,
        val field: String,
        val value: Raw,
    ) : MachineEdit, Ordered {
        override val op: String get() = "setin"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Insertin(
        override val where: List<ElemKey>,
        val value: Raw,
    ) : MachineEdit, Ordered {
        override val op: String get() = "insertin"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Removein(
        override val where: List<ElemKey>,
    ) : MachineEdit, Ordered {
        override val op: String get() = "removein"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<MachineEdit> =
            tagged<MachineEdit>(
                "op",
                listOf(
                Variant("setin", MachineEdit.Setin::class.java, obj(
                        listOf(
                        Field("op", { tag("setin") }, optional = false, nullable = false),
                        Field("where", { arr(ElemKey.ADAPTER) }, optional = false, nullable = false),
                        Field("field", { Str }, optional = false, nullable = false),
                        Field("value", { RawJson }, optional = false, nullable = false),
                        ),
                        { a -> MachineEdit.Setin(a[1] as List<ElemKey>, a[2] as String, a[3] as Raw) },
                        { v -> arrayOf("setin", v.where, v.field, v.value) },
                    )),
                Variant("insertin", MachineEdit.Insertin::class.java, obj(
                        listOf(
                        Field("op", { tag("insertin") }, optional = false, nullable = false),
                        Field("where", { arr(ElemKey.ADAPTER) }, optional = false, nullable = false),
                        Field("value", { RawJson }, optional = false, nullable = false),
                        ),
                        { a -> MachineEdit.Insertin(a[1] as List<ElemKey>, a[2] as Raw) },
                        { v -> arrayOf("insertin", v.where, v.value) },
                    )),
                Variant("removein", MachineEdit.Removein::class.java, obj(
                        listOf(
                        Field("op", { tag("removein") }, optional = false, nullable = false),
                        Field("where", { arr(ElemKey.ADAPTER) }, optional = false, nullable = false),
                        ),
                        { a -> MachineEdit.Removein(a[1] as List<ElemKey>) },
                        { v -> arrayOf("removein", v.where) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class MachineFault(
    val code: MachineFaultCode,
    val name: String,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachineFault> = obj(
            listOf(
            Field("code", { MachineFaultCode.ADAPTER }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> MachineFault(a[0] as MachineFaultCode, a[1] as String, a[2] as String) },
            { v -> arrayOf(v.code, v.name, v.said) },
        )
    }
}

enum class MachineFaultCode(val wire: String) {
    NO_HOST("no_host"),
    NO_USER("no_user"),
    PORT("port"),
    NAME_TAKEN("name_taken"),
    LEADING_DASH("leading_dash"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<MachineFaultCode> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class MachineFind(
    val hit: MachineHit?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachineFind> = obj(
            listOf(
            Field("hit", { opt(MachineHit.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> MachineFind(a[0] as MachineHit?) },
            { v -> arrayOf(v.hit) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class MachineFindReq(
    val query: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachineFindReq> = obj(
            listOf(
            Field("query", { Str }, optional = false, nullable = false),
            ),
            { a -> MachineFindReq(a[0] as String) },
            { v -> arrayOf(v.query) },
        )
    }
}

enum class MachineFix(val wire: String) {
    RETRY("retry"),
    CONN_SETTINGS("conn_settings"),
    PUSH_KEY("push_key"),
    COMPARE_FINGERPRINT("compare_fingerprint"),
    UPDATE("update"),
    CONNECT("connect"),
    INSTALL("install"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<MachineFix> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class MachineFixButton(
    val fix: MachineFix,
    val label: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachineFixButton> = obj(
            listOf(
            Field("fix", { MachineFix.ADAPTER }, optional = false, nullable = false),
            Field("label", { Str }, optional = false, nullable = false),
            ),
            { a -> MachineFixButton(a[0] as MachineFix, a[1] as String) },
            { v -> arrayOf(v.fix, v.label) },
        )
    }
}

enum class MachineFrom(val wire: String) {
    LOCAL("local"),
    TABLE("table"),
    SSH_CONFIG("ssh_config"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<MachineFrom> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class MachineHit(
    val machine: String,
    val from: MachineFrom,
    val nth: Long?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachineHit> = obj(
            listOf(
            Field("machine", { Str }, optional = false, nullable = false),
            Field("from", { MachineFrom.ADAPTER }, optional = false, nullable = false),
            Field("nth", { opt(Lng) }, optional = false, nullable = true),
            ),
            { a -> MachineHit(a[0] as String, a[1] as MachineFrom, a[2] as Long?) },
            { v -> arrayOf(v.machine, v.from, v.nth) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class MachineInterruptsReply(
    val relayedSessions: Long?,
    val relayedMaybe: Long,
    val liveStreams: Long,
    val forwards: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachineInterruptsReply> = obj(
            listOf(
            Field("relayedSessions", { opt(Lng) }, optional = false, nullable = true),
            Field("relayedMaybe", { Lng }, optional = false, nullable = false),
            Field("liveStreams", { Lng }, optional = false, nullable = false),
            Field("forwards", { Lng }, optional = false, nullable = false),
            ),
            { a -> MachineInterruptsReply(a[0] as Long?, a[1] as Long, a[2] as Long, a[3] as Long) },
            { v -> arrayOf(v.relayedSessions, v.relayedMaybe, v.liveStreams, v.forwards) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class MachineInterruptsReq(
    val machine: String? = null,
    val appExit: Boolean? = null,
    val machines: List<String>? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachineInterruptsReq> = obj(
            listOf(
            Field("machine", { opt(Str) }, optional = true, nullable = true),
            Field("appExit", { opt(Bool) }, optional = true, nullable = true),
            Field("machines", { opt(arr(Str)) }, optional = true, nullable = true),
            ),
            { a -> MachineInterruptsReq(a[0] as String?, a[1] as Boolean?, a[2] as List<String>?) },
            { v -> arrayOf(v.machine, v.appExit, v.machines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class MachineMenu(
    val from: MachineFrom,
    val head: MachineState?,
    val actions: List<MenuAction>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachineMenu> = obj(
            listOf(
            Field("from", { MachineFrom.ADAPTER }, optional = false, nullable = false),
            Field("head", { opt(MachineState.ADAPTER) }, optional = false, nullable = true),
            Field("actions", { arr(MenuAction.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> MachineMenu(a[0] as MachineFrom, a[1] as MachineState?, a[2] as List<MenuAction>) },
            { v -> arrayOf(v.from, v.head, v.actions) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class MachineMenuReq(
    val machine: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachineMenuReq> = obj(
            listOf(
            Field("machine", { Str }, optional = false, nullable = false),
            ),
            { a -> MachineMenuReq(a[0] as String) },
            { v -> arrayOf(v.machine) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class MachineName(
    val machine: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachineName> = obj(
            listOf(
            Field("machine", { Str }, optional = false, nullable = false),
            ),
            { a -> MachineName(a[0] as String) },
            { v -> arrayOf(v.machine) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class MachineRead(
    val machines: Map<String, MachineState>,
    val empty: EmptyFace,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachineRead> = obj(
            listOf(
            Field("machines", { dict(MachineState.ADAPTER) }, optional = false, nullable = false),
            Field("empty", { EmptyFace.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> MachineRead(a[0] as Map<String, MachineState>, a[1] as EmptyFace) },
            { v -> arrayOf(v.machines, v.empty) },
        )
    }
}

sealed interface MachineRound {
    /** 判别格（`state`）那个词。 */
    val state: String

    data object Connecting : MachineRound {
        override val state: String get() = "connecting"
    }
    @Suppress("UNCHECKED_CAST")
    data class Up(
        val relation: VersionRelation,
        val differs: List<String>,
    ) : MachineRound, Ordered {
        override val state: String get() = "up"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Down(
        val problem: MachineRoundProblem,
        val blocker: MachineBlocker?,
    ) : MachineRound, Ordered {
        override val state: String get() = "down"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<MachineRound> =
            tagged<MachineRound>(
                "state",
                listOf(
                Variant("connecting", MachineRound.Connecting::class.java, obj(
                        listOf(
                        Field("state", { tag("connecting") }, optional = false, nullable = false),
                        ),
                        { MachineRound.Connecting },
                        { arrayOf("connecting") },
                    )),
                Variant("up", MachineRound.Up::class.java, obj(
                        listOf(
                        Field("state", { tag("up") }, optional = false, nullable = false),
                        Field("relation", { VersionRelation.ADAPTER }, optional = false, nullable = false),
                        Field("differs", { arr(Str) }, optional = false, nullable = false),
                        ),
                        { a -> MachineRound.Up(a[1] as VersionRelation, a[2] as List<String>) },
                        { v -> arrayOf("up", v.relation, v.differs) },
                    )),
                Variant("down", MachineRound.Down::class.java, obj(
                        listOf(
                        Field("state", { tag("down") }, optional = false, nullable = false),
                        Field("problem", { MachineRoundProblem.ADAPTER }, optional = false, nullable = false),
                        Field("blocker", { opt(MachineBlocker.ADAPTER) }, optional = false, nullable = true),
                        ),
                        { a -> MachineRound.Down(a[1] as MachineRoundProblem, a[2] as MachineBlocker?) },
                        { v -> arrayOf("down", v.problem, v.blocker) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class MachineRoundProblem(
    val text: String,
    val detail: String,
    val why: DialWhy?,
    val seenKey: String?,
    val expectedKey: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachineRoundProblem> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            Field("detail", { Str }, optional = false, nullable = false),
            Field("why", { opt(DialWhy.ADAPTER) }, optional = false, nullable = true),
            Field("seen_key", { opt(Str) }, optional = false, nullable = true),
            Field("expected_key", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> MachineRoundProblem(a[0] as String, a[1] as String, a[2] as DialWhy?, a[3] as String?, a[4] as String?) },
            { v -> arrayOf(v.text, v.detail, v.why, v.seenKey, v.expectedKey) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class MachineRow(
    val name: String,
    val host: Host,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachineRow> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("host", { Host.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> MachineRow(a[0] as String, a[1] as Host) },
            { v -> arrayOf(v.name, v.host) },
        )
    }
}

enum class MachineStage(val wire: String) {
    DEPLOY("deploy"),
    ATTACH("attach"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<MachineStage> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class MachineState(
    val name: String,
    val state: MachineStateKind,
    val offline: Boolean,
    val reason: DialWhy?,
    val stage: MachineStage?,
    val version: String?,
    val versionRelation: VersionRelation?,
    val os: String?,
    val word: String,
    val said: String,
    val offlineSaid: String,
    val fixes: List<MachineFixButton>,
    val seenHostKey: String?,
    val expectedHostKey: String?,
    val detail: String?,
    val route: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachineState> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("state", { MachineStateKind.ADAPTER }, optional = false, nullable = false),
            Field("offline", { Bool }, optional = false, nullable = false),
            Field("reason", { opt(DialWhy.ADAPTER) }, optional = false, nullable = true),
            Field("stage", { opt(MachineStage.ADAPTER) }, optional = false, nullable = true),
            Field("version", { opt(Str) }, optional = false, nullable = true),
            Field("versionRelation", { opt(VersionRelation.ADAPTER) }, optional = false, nullable = true),
            Field("os", { opt(Str) }, optional = false, nullable = true),
            Field("word", { Str }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            Field("offlineSaid", { Str }, optional = false, nullable = false),
            Field("fixes", { arr(MachineFixButton.ADAPTER) }, optional = false, nullable = false),
            Field("seenHostKey", { opt(Str) }, optional = false, nullable = true),
            Field("expectedHostKey", { opt(Str) }, optional = false, nullable = true),
            Field("detail", { opt(Str) }, optional = false, nullable = true),
            Field("route", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> MachineState(a[0] as String, a[1] as MachineStateKind, a[2] as Boolean, a[3] as DialWhy?, a[4] as MachineStage?, a[5] as String?, a[6] as VersionRelation?, a[7] as String?, a[8] as String, a[9] as String, a[10] as String, a[11] as List<MachineFixButton>, a[12] as String?, a[13] as String?, a[14] as String?, a[15] as String?) },
            { v -> arrayOf(v.name, v.state, v.offline, v.reason, v.stage, v.version, v.versionRelation, v.os, v.word, v.said, v.offlineSaid, v.fixes, v.seenHostKey, v.expectedHostKey, v.detail, v.route) },
        )
    }
}

enum class MachineStateKind(val wire: String) {
    UP("up"),
    CONNECTING("connecting"),
    INSTALLING("installing"),
    UPDATING("updating"),
    DOWN("down"),
    HOST_KEY_CHANGED("host_key_changed"),
    NEEDS_UPDATE("needs_update"),
    NEWER("newer"),
    DISABLED("disabled"),
    UNSUPPORTED("unsupported"),
    UNKNOWN("unknown"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<MachineStateKind> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class MachineStopped(
    val stopped: String,
    val pid: Long?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachineStopped> = obj(
            listOf(
            Field("stopped", { Str }, optional = false, nullable = false),
            Field("pid", { opt(Lng) }, optional = false, nullable = true),
            ),
            { a -> MachineStopped(a[0] as String, a[1] as Long?) },
            { v -> arrayOf(v.stopped, v.pid) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class MachinesEdits(
    val edits: List<MachineEdit>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachinesEdits> = obj(
            listOf(
            Field("edits", { arr(MachineEdit.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> MachinesEdits(a[0] as List<MachineEdit>) },
            { v -> arrayOf(v.edits) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class MachinesReply(
    val state: FileState,
    val reason: String?,
    val detail: String?,
    val path: String?,
    val hosts: List<MachineRow>,
    val hostKeys: Map<String, List<String>>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachinesReply> = obj(
            listOf(
            Field("state", { FileState.ADAPTER }, optional = false, nullable = false),
            Field("reason", { opt(Str) }, optional = false, nullable = true),
            Field("detail", { opt(Str) }, optional = false, nullable = true),
            Field("path", { opt(Str) }, optional = false, nullable = true),
            Field("hosts", { arr(MachineRow.ADAPTER) }, optional = false, nullable = false),
            Field("hostKeys", { dict(arr(Str)) }, optional = false, nullable = false),
            ),
            { a -> MachinesReply(a[0] as FileState, a[1] as String?, a[2] as String?, a[3] as String?, a[4] as List<MachineRow>, a[5] as Map<String, List<String>>) },
            { v -> arrayOf(v.state, v.reason, v.detail, v.path, v.hosts, v.hostKeys) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class MachinesSet(
    val machines: MachinesReply,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachinesSet> = obj(
            listOf(
            Field("machines", { MachinesReply.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> MachinesSet(a[0] as MachinesReply) },
            { v -> arrayOf(v.machines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class MachinesTried(
    val fault: MachineFault?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MachinesTried> = obj(
            listOf(
            Field("fault", { opt(MachineFault.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> MachinesTried(a[0] as MachineFault?) },
            { v -> arrayOf(v.fault) },
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
    /** 判别格（`kind`）那个词。 */
    val kind: String

    val said: String

    val fix: ManualFix?

    @Suppress("UNCHECKED_CAST")
    data class AccountLogin(
        val agent: String,
        val account: String,
        override val said: String,
        override val fix: ManualFix,
    ) : ManualItem, Ordered {
        override val kind: String get() = "accountLogin"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class McpLogin(
        val account: String?,
        val server: String,
        val scope: String,
        override val said: String,
        override val fix: ManualFix?,
        val why: String? = null,
    ) : ManualItem, Ordered {
        override val kind: String get() = "mcpLogin"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class SecretFile(
        override val said: String,
        override val fix: ManualFix?,
        val why: String? = null,
    ) : ManualItem, Ordered {
        override val kind: String get() = "secretFile"
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

@Suppress("UNCHECKED_CAST")
data class McpApplied(
    val path: String,
    val written: Boolean,
    val recordFailed: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<McpApplied> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("written", { Bool }, optional = false, nullable = false),
            Field("recordFailed", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> McpApplied(a[0] as String, a[1] as Boolean, a[2] as String?) },
            { v -> arrayOf(v.path, v.written, v.recordFailed) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class McpApplyReq(
    val name: String,
    val at: ExtLoc,
    val def: Raw,
    val fill: Map<String, Map<String, String>>?,
    val target: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<McpApplyReq> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("at", { ExtLoc.ADAPTER }, optional = false, nullable = false),
            Field("def", { RawJson }, optional = false, nullable = false),
            Field("fill", { opt(dict(dict(Str))) }, optional = false, nullable = true),
            Field("target", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> McpApplyReq(a[0] as String, a[1] as ExtLoc, a[2] as Raw, a[3] as Map<String, Map<String, String>>?, a[4] as String?) },
            { v -> arrayOf(v.name, v.at, v.def, v.fill, v.target) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class McpFailedMark(
    val said: String,
    val tip: String,
    val detail: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<McpFailedMark> = obj(
            listOf(
            Field("said", { Str }, optional = false, nullable = false),
            Field("tip", { Str }, optional = false, nullable = false),
            Field("detail", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> McpFailedMark(a[0] as String, a[1] as String, a[2] as String?) },
            { v -> arrayOf(v.said, v.tip, v.detail) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class McpLoginFix(
    val account: String?,
    val fix: ManualFix?,
    val why: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<McpLoginFix> = obj(
            listOf(
            Field("account", { opt(Str) }, optional = false, nullable = true),
            Field("fix", { opt(ManualFix.ADAPTER) }, optional = false, nullable = true),
            Field("why", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> McpLoginFix(a[0] as String?, a[1] as ManualFix?, a[2] as String?) },
            { v -> arrayOf(v.account, v.fix, v.why) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class McpLoginMark(
    val said: String,
    val tip: String,
    val fix: List<McpLoginFix>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<McpLoginMark> = obj(
            listOf(
            Field("said", { Str }, optional = false, nullable = false),
            Field("tip", { Str }, optional = false, nullable = false),
            Field("fix", { arr(McpLoginFix.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> McpLoginMark(a[0] as String, a[1] as String, a[2] as List<McpLoginFix>) },
            { v -> arrayOf(v.said, v.tip, v.fix) },
        )
    }
}

enum class McpMark(val wire: String) {
    NEEDS_LOGIN("needsLogin"),
    FAILED("failed"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<McpMark> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class McpPreview(
    val path: String,
    val state: String,
    val def: Raw,
    val slots: List<ExtSlot>,
    val suspects: List<McpSuspect>,
    val target: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<McpPreview> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("state", { Str }, optional = false, nullable = false),
            Field("def", { RawJson }, optional = false, nullable = false),
            Field("slots", { arr(ExtSlot.ADAPTER) }, optional = false, nullable = false),
            Field("suspects", { arr(McpSuspect.ADAPTER) }, optional = false, nullable = false),
            Field("target", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> McpPreview(a[0] as String, a[1] as String, a[2] as Raw, a[3] as List<ExtSlot>, a[4] as List<McpSuspect>, a[5] as String?) },
            { v -> arrayOf(v.path, v.state, v.def, v.slots, v.suspects, v.target) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class McpPreviewReq(
    val name: String,
    val at: ExtLoc,
    val def: Raw,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<McpPreviewReq> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("at", { ExtLoc.ADAPTER }, optional = false, nullable = false),
            Field("def", { RawJson }, optional = false, nullable = false),
            ),
            { a -> McpPreviewReq(a[0] as String, a[1] as ExtLoc, a[2] as Raw) },
            { v -> arrayOf(v.name, v.at, v.def) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class McpReadEntry(
    val scope: McpReadEntryScope,
    val name: String,
    val server: Raw,
    val sourcePath: String,
    val status: McpReadEntryStatus,
    val loginIn: List<String>,
    val seenAt: Long?,
    val mark: McpMark?,
    val login: McpLoginMark?,
    val failed: McpFailedMark?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<McpReadEntry> = obj(
            listOf(
            Field("scope", { McpReadEntryScope.ADAPTER }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("server", { RawJson }, optional = false, nullable = false),
            Field("sourcePath", { Str }, optional = false, nullable = false),
            Field("status", { McpReadEntryStatus.ADAPTER }, optional = false, nullable = false),
            Field("loginIn", { arr(Str) }, optional = false, nullable = false),
            Field("seenAt", { opt(Lng) }, optional = false, nullable = true),
            Field("mark", { opt(McpMark.ADAPTER) }, optional = false, nullable = true),
            Field("login", { opt(McpLoginMark.ADAPTER) }, optional = false, nullable = true),
            Field("failed", { opt(McpFailedMark.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> McpReadEntry(a[0] as McpReadEntryScope, a[1] as String, a[2] as Raw, a[3] as String, a[4] as McpReadEntryStatus, a[5] as List<String>, a[6] as Long?, a[7] as McpMark?, a[8] as McpLoginMark?, a[9] as McpFailedMark?) },
            { v -> arrayOf(v.scope, v.name, v.server, v.sourcePath, v.status, v.loginIn, v.seenAt, v.mark, v.login, v.failed) },
        )
    }
}

enum class McpReadEntryScope(val wire: String) {
    USER("user"),
    LOCAL("local"),
    PROJECT("project"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<McpReadEntryScope> = enumOf(entries) { it.wire }
    }
}

enum class McpReadEntryStatus(val wire: String) {
    NEEDS_LOGIN("needsLogin"),
    DISABLED("disabled"),
    UNKNOWN("unknown"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<McpReadEntryStatus> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class McpReadReply(
    val entries: List<McpReadEntry>,
    val dirs: List<String>,
    val problems: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<McpReadReply> = obj(
            listOf(
            Field("entries", { arr(McpReadEntry.ADAPTER) }, optional = false, nullable = false),
            Field("dirs", { arr(Str) }, optional = false, nullable = false),
            Field("problems", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> McpReadReply(a[0] as List<McpReadEntry>, a[1] as List<String>, a[2] as List<String>) },
            { v -> arrayOf(v.entries, v.dirs, v.problems) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class McpReadReq(
    val projectDir: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<McpReadReq> = obj(
            listOf(
            Field("projectDir", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> McpReadReq(a[0] as String?) },
            { v -> arrayOf(v.projectDir) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class McpSource(
    val path: String,
    val def: Raw,
    val slots: List<McpSourceSlot>,
    val token: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<McpSource> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("def", { RawJson }, optional = false, nullable = false),
            Field("slots", { arr(McpSourceSlot.ADAPTER) }, optional = false, nullable = false),
            Field("token", { Str }, optional = false, nullable = false),
            ),
            { a -> McpSource(a[0] as String, a[1] as Raw, a[2] as List<McpSourceSlot>, a[3] as String) },
            { v -> arrayOf(v.path, v.def, v.slots, v.token) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class McpSourceReq(
    val name: String,
    val at: ExtLoc,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<McpSourceReq> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("at", { ExtLoc.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> McpSourceReq(a[0] as String, a[1] as ExtLoc) },
            { v -> arrayOf(v.name, v.at) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class McpSourceSlot(
    val field: String,
    val key: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<McpSourceSlot> = obj(
            listOf(
            Field("field", { Str }, optional = false, nullable = false),
            Field("key", { Str }, optional = false, nullable = false),
            ),
            { a -> McpSourceSlot(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.field, v.key) },
        )
    }
}

enum class McpStatus(val wire: String) {
    NEEDS_LOGIN("needsLogin"),
    FAILED("failed"),
    PENDING("pending"),
    DISABLED("disabled"),
    UNKNOWN("unknown"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<McpStatus> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class McpSuspect(
    val kind: String,
    val field: String,
    val value: String,
    val there: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<McpSuspect> = obj(
            listOf(
            Field("kind", { Str }, optional = false, nullable = false),
            Field("field", { Str }, optional = false, nullable = false),
            Field("value", { Str }, optional = false, nullable = false),
            Field("there", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> McpSuspect(a[0] as String, a[1] as String, a[2] as String, a[3] as String?) },
            { v -> arrayOf(v.kind, v.field, v.value, v.there) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class McpTrouble(
    val name: String,
    val status: McpStatus,
    val detail: String?,
    val at: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<McpTrouble> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("status", { McpStatus.ADAPTER }, optional = false, nullable = false),
            Field("detail", { opt(Str) }, optional = false, nullable = true),
            Field("at", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> McpTrouble(a[0] as String, a[1] as McpStatus, a[2] as String?, a[3] as String?) },
            { v -> arrayOf(v.name, v.status, v.detail, v.at) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class MemberFacts(
    val title: String,
    val cwd: String?,
    val record: String?,
    val account: String?,
    val bg: Boolean,
    val at: Long?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MemberFacts> = obj(
            listOf(
            Field("title", { Str }, optional = false, nullable = false),
            Field("cwd", { opt(Str) }, optional = false, nullable = true),
            Field("record", { opt(Str) }, optional = false, nullable = true),
            Field("account", { opt(Str) }, optional = false, nullable = true),
            Field("bg", { Bool }, optional = false, nullable = false),
            Field("at", { opt(Lng) }, optional = false, nullable = true),
            ),
            { a -> MemberFacts(a[0] as String, a[1] as String?, a[2] as String?, a[3] as String?, a[4] as Boolean, a[5] as Long?) },
            { v -> arrayOf(v.title, v.cwd, v.record, v.account, v.bg, v.at) },
        )
    }
}

enum class MenuAct(val wire: String) {
    OPEN_TERMINAL("open_terminal"),
    NEW_SESSION("new_session"),
    FILES("files"),
    MACHINE_STATUS("machine_status"),
    ADD_MACHINE("add_machine"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<MenuAct> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class MenuAction(
    val act: MenuAct,
    val sub: String?,
    val blocked: String?,
    val note: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MenuAction> = obj(
            listOf(
            Field("act", { MenuAct.ADAPTER }, optional = false, nullable = false),
            Field("sub", { opt(Str) }, optional = false, nullable = true),
            Field("blocked", { opt(Str) }, optional = false, nullable = true),
            Field("note", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> MenuAction(a[0] as MenuAct, a[1] as String?, a[2] as String?, a[3] as String?) },
            { v -> arrayOf(v.act, v.sub, v.blocked, v.note) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class MenuEntry(
    val id: String,
    val text: String,
    val enabled: Boolean,
    val why: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MenuEntry> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("enabled", { Bool }, optional = false, nullable = false),
            Field("why", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> MenuEntry(a[0] as String, a[1] as String, a[2] as Boolean, a[3] as String?) },
            { v -> arrayOf(v.id, v.text, v.enabled, v.why) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Merged(
    val totalHits: Long,
    val sessionCount: Long,
    val truncated: Boolean,
    val sessions: List<MergedRow>,
    val unreadable: Long,
    val skipped: List<String>,
    val down: List<SearchDown>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Merged> = obj(
            listOf(
            Field("totalHits", { U64 }, optional = false, nullable = false),
            Field("sessionCount", { Lng }, optional = false, nullable = false),
            Field("truncated", { Bool }, optional = false, nullable = false),
            Field("sessions", { arr(MergedRow.ADAPTER) }, optional = false, nullable = false),
            Field("unreadable", { Lng }, optional = false, nullable = false),
            Field("skipped", { arr(Str) }, optional = false, nullable = false),
            Field("down", { arr(SearchDown.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> Merged(a[0] as Long, a[1] as Long, a[2] as Boolean, a[3] as List<MergedRow>, a[4] as Long, a[5] as List<String>, a[6] as List<SearchDown>) },
            { v -> arrayOf(v.totalHits, v.sessionCount, v.truncated, v.sessions, v.unreadable, v.skipped, v.down) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class MergedRow(
    val origin: String? = null,
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
    val moreHits: Int,
    val hitsTruncated: Boolean,
    val isBg: Boolean,
    val can: Can,
    val forkedFrom: ForkedFrom? = null,
    val face: SessionFace,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<MergedRow> = obj(
            listOf(
            Field("origin", { opt(Str) }, optional = true, nullable = true),
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
            Field("moreHits", { Int32 }, optional = false, nullable = false),
            Field("hitsTruncated", { Bool }, optional = false, nullable = false),
            Field("isBg", { Bool }, optional = false, nullable = false),
            Field("can", { Can.ADAPTER }, optional = false, nullable = false),
            Field("forkedFrom", { opt(ForkedFrom.ADAPTER) }, optional = true, nullable = true),
            Field("face", { SessionFace.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> MergedRow(a[0] as String?, a[1] as String, a[2] as String, a[3] as String, a[4] as Boolean, a[5] as Hidden?, a[6] as String, a[7] as String, a[8] as String, a[9] as String, a[10] as String, a[11] as String, a[12] as Long, a[13] as List<SearchHit>, a[14] as Int, a[15] as Boolean, a[16] as Boolean, a[17] as Can, a[18] as ForkedFrom?, a[19] as SessionFace) },
            { v -> arrayOf(v.origin, v.atText, v.spanText, v.label, v.starred, v.hidden, v.agent, v.sessionId, v.projectPath, v.projectName, v.jsonlPath, v.title, v.updatedAt, v.hits, v.moreHits, v.hitsTruncated, v.isBg, v.can, v.forkedFrom, v.face) },
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
data class Moved(
    val call: Long,
    val done: Long,
    val total: Long?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Moved> = obj(
            listOf(
            Field("call", { Lng }, optional = false, nullable = false),
            Field("done", { U64 }, optional = false, nullable = false),
            Field("total", { opt(U64) }, optional = false, nullable = true),
            ),
            { a -> Moved(a[0] as Long, a[1] as Long, a[2] as Long?) },
            { v -> arrayOf(v.call, v.done, v.total) },
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
            Field("sinceMs", { opt(U64) }, optional = false, nullable = true),
            Field("text", { Str }, optional = false, nullable = false),
            Field("head", { Str }, optional = false, nullable = false),
            Field("headCode", { opt(Str) }, optional = false, nullable = true),
            Field("tone", { Tone.ADAPTER }, optional = false, nullable = false),
            Field("rank", { U64 }, optional = false, nullable = false),
            Field("waitedMs", { opt(U64) }, optional = false, nullable = true),
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
            Field("sinceMs", { U64 }, optional = false, nullable = false),
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
data class NeedsAt(
    val origin: String,
    val item: NeedsItem,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<NeedsAt> = obj(
            listOf(
            Field("origin", { Str }, optional = false, nullable = false),
            Field("item", { NeedsItem.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> NeedsAt(a[0] as String, a[1] as NeedsItem) },
            { v -> arrayOf(v.origin, v.item) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class NeedsCell(
    val id: String,
    val label: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<NeedsCell> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("label", { Str }, optional = false, nullable = false),
            ),
            { a -> NeedsCell(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.id, v.label) },
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

@Suppress("UNCHECKED_CAST")
data class NeedsDown(
    val origin: String,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<NeedsDown> = obj(
            listOf(
            Field("origin", { Str }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> NeedsDown(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.origin, v.said) },
        )
    }
}

sealed interface NeedsItem {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    val rank: Long

    @Suppress("UNCHECKED_CAST")
    data class Session(
        override val rank: Long,
        val session: NeedsRow,
    ) : NeedsItem, Ordered {
        override val kind: String get() = "session"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Plan(
        override val rank: Long,
        val workspace: String,
        val slice: String,
        val cell: NeedsCell? = null,
    ) : NeedsItem, Ordered {
        override val kind: String get() = "plan"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Manual(
        override val rank: Long,
        val manual: ManualItem,
    ) : NeedsItem, Ordered {
        override val kind: String get() = "manual"
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
                        Field("rank", { U64 }, optional = false, nullable = false),
                        Field("session", { NeedsRow.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> NeedsItem.Session(a[1] as Long, a[2] as NeedsRow) },
                        { v -> arrayOf("session", v.rank, v.session) },
                    )),
                Variant("plan", NeedsItem.Plan::class.java, obj(
                        listOf(
                        Field("kind", { tag("plan") }, optional = false, nullable = false),
                        Field("rank", { U64 }, optional = false, nullable = false),
                        Field("workspace", { Str }, optional = false, nullable = false),
                        Field("slice", { Str }, optional = false, nullable = false),
                        Field("cell", { opt(NeedsCell.ADAPTER) }, optional = true, nullable = true),
                        ),
                        { a -> NeedsItem.Plan(a[1] as Long, a[2] as String, a[3] as String, a[4] as NeedsCell?) },
                        { v -> arrayOf("plan", v.rank, v.workspace, v.slice, v.cell) },
                    )),
                Variant("manual", NeedsItem.Manual::class.java, obj(
                        listOf(
                        Field("kind", { tag("manual") }, optional = false, nullable = false),
                        Field("rank", { U64 }, optional = false, nullable = false),
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

@Suppress("UNCHECKED_CAST")
data class NeedsMergeReq(
    val machines: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<NeedsMergeReq> = obj(
            listOf(
            Field("machines", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> NeedsMergeReq(a[0] as List<String>) },
            { v -> arrayOf(v.machines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class NeedsMerged(
    val items: List<NeedsAt>,
    val total: Long,
    val unreadable: List<NeedsUnreadable>,
    val down: List<NeedsDown>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<NeedsMerged> = obj(
            listOf(
            Field("items", { arr(NeedsAt.ADAPTER) }, optional = false, nullable = false),
            Field("total", { Lng }, optional = false, nullable = false),
            Field("unreadable", { arr(NeedsUnreadable.ADAPTER) }, optional = false, nullable = false),
            Field("down", { arr(NeedsDown.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> NeedsMerged(a[0] as List<NeedsAt>, a[1] as Long, a[2] as List<NeedsUnreadable>, a[3] as List<NeedsDown>) },
            { v -> arrayOf(v.items, v.total, v.unreadable, v.down) },
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
data class NeedsUnreadable(
    val origin: String,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<NeedsUnreadable> = obj(
            listOf(
            Field("origin", { Str }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> NeedsUnreadable(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.origin, v.said) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class NewOpen(
    val agent: String,
    val cwd: String,
    val tmuxName: String? = null,
    val command: String? = null,
    val fork: ForkFrom? = null,
    val prompt: String? = null,
    val model: String? = null,
    val permissionMode: String? = null,
    val rotation: RotationPick? = null,
    val alias: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<NewOpen> = obj(
            listOf(
            Field("agent", { Str }, optional = false, nullable = false),
            Field("cwd", { Str }, optional = false, nullable = false),
            Field("tmuxName", { opt(Str) }, optional = true, nullable = true),
            Field("command", { opt(Str) }, optional = true, nullable = true),
            Field("fork", { opt(ForkFrom.ADAPTER) }, optional = true, nullable = true),
            Field("prompt", { opt(Str) }, optional = true, nullable = true),
            Field("model", { opt(Str) }, optional = true, nullable = true),
            Field("permissionMode", { opt(Str) }, optional = true, nullable = true),
            Field("rotation", { opt(RotationPick.ADAPTER) }, optional = true, nullable = true),
            Field("alias", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> NewOpen(a[0] as String, a[1] as String, a[2] as String?, a[3] as String?, a[4] as ForkFrom?, a[5] as String?, a[6] as String?, a[7] as String?, a[8] as RotationPick?, a[9] as String?) },
            { v -> arrayOf(v.agent, v.cwd, v.tmuxName, v.command, v.fork, v.prompt, v.model, v.permissionMode, v.rotation, v.alias) },
        )
    }
}

data object NoArgs {
    val ADAPTER: JsonAdapter<NoArgs> = obj(emptyList(), { NoArgs }, { emptyArray() })
}

data object NoReq {
    val ADAPTER: JsonAdapter<NoReq> = obj(emptyList(), { NoReq }, { emptyArray() })
}

enum class OnExists(val wire: String) {
    ASK("ask"),
    OVERWRITE("overwrite"),
    SKIP("skip"),
    KEEP_BOTH("keep_both"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<OnExists> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class OnPath(
    val verdict: OnPathVerdict,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<OnPath> = obj(
            listOf(
            Field("verdict", { OnPathVerdict.ADAPTER }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> OnPath(a[0] as OnPathVerdict, a[1] as String) },
            { v -> arrayOf(v.verdict, v.said) },
        )
    }
}

enum class OnPathVerdict(val wire: String) {
    OURS("ours"),
    OTHER("other"),
    ABSENT("absent"),
    UNKNOWN("unknown"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<OnPathVerdict> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class OpenDirReq(
    val cwd: String,
    val forkOf: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<OpenDirReq> = obj(
            listOf(
            Field("cwd", { Str }, optional = false, nullable = false),
            Field("forkOf", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> OpenDirReq(a[0] as String, a[1] as String?) },
            { v -> arrayOf(v.cwd, v.forkOf) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class OpenFacts(
    val recent: List<RecentDir>,
    val hosts: List<Run>,
    val run: Run,
    val agents: List<AgentFacts>,
    val fork: ForkFacts?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<OpenFacts> = obj(
            listOf(
            Field("recent", { arr(RecentDir.ADAPTER) }, optional = false, nullable = false),
            Field("hosts", { arr(Run.ADAPTER) }, optional = false, nullable = false),
            Field("run", { Run.ADAPTER }, optional = false, nullable = false),
            Field("agents", { arr(AgentFacts.ADAPTER) }, optional = false, nullable = false),
            Field("fork", { opt(ForkFacts.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> OpenFacts(a[0] as List<RecentDir>, a[1] as List<Run>, a[2] as Run, a[3] as List<AgentFacts>, a[4] as ForkFacts?) },
            { v -> arrayOf(v.recent, v.hosts, v.run, v.agents, v.fork) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class OpenFactsReq(
    val forkOf: String? = null,
    val at: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<OpenFactsReq> = obj(
            listOf(
            Field("forkOf", { opt(Str) }, optional = true, nullable = true),
            Field("at", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> OpenFactsReq(a[0] as String?, a[1] as String?) },
            { v -> arrayOf(v.forkOf, v.at) },
        )
    }
}

enum class OpenField(val wire: String) {
    AGENT("agent"),
    COMMAND("command"),
    CWD("cwd"),
    ACCOUNT("account"),
    PLACE("place"),
    TMUX_NAME("tmuxName"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<OpenField> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class OpenRefusal(
    val field: OpenField?,
    val unavailable: AccountUnavailable?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<OpenRefusal> = obj(
            listOf(
            Field("field", { opt(OpenField.ADAPTER) }, optional = false, nullable = true),
            Field("unavailable", { opt(AccountUnavailable.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> OpenRefusal(a[0] as OpenField?, a[1] as AccountUnavailable?) },
            { v -> arrayOf(v.field, v.unavailable) },
        )
    }
}

enum class OpenState(val wire: String) {
    OPENED("opened"),
    SKIPPED("skipped"),
    FAILED("failed"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<OpenState> = enumOf(entries) { it.wire }
    }
}

sealed interface OpenWhat {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    @Suppress("UNCHECKED_CAST")
    data class New(
        val agent: String,
        val cwd: String,
        val tmuxName: String? = null,
        val command: String? = null,
        val fork: ForkFrom? = null,
        val prompt: String? = null,
        val model: String? = null,
        val permissionMode: String? = null,
        val rotation: RotationPick? = null,
        val alias: String? = null,
    ) : OpenWhat, Ordered {
        override val kind: String get() = "new"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Resume(
        val sid: String,
    ) : OpenWhat, Ordered {
        override val kind: String get() = "resume"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Look(
        val sid: String? = null,
        val session: String? = null,
    ) : OpenWhat, Ordered {
        override val kind: String get() = "look"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Shell(
        val session: String? = null,
        val cwd: String? = null,
    ) : OpenWhat, Ordered {
        override val kind: String get() = "shell"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<OpenWhat> =
            tagged<OpenWhat>(
                "kind",
                listOf(
                Variant("new", OpenWhat.New::class.java, obj(
                        listOf(
                        Field("kind", { tag("new") }, optional = false, nullable = false),
                        Field("agent", { Str }, optional = false, nullable = false),
                        Field("cwd", { Str }, optional = false, nullable = false),
                        Field("tmuxName", { opt(Str) }, optional = true, nullable = true),
                        Field("command", { opt(Str) }, optional = true, nullable = true),
                        Field("fork", { opt(ForkFrom.ADAPTER) }, optional = true, nullable = true),
                        Field("prompt", { opt(Str) }, optional = true, nullable = true),
                        Field("model", { opt(Str) }, optional = true, nullable = true),
                        Field("permissionMode", { opt(Str) }, optional = true, nullable = true),
                        Field("rotation", { opt(RotationPick.ADAPTER) }, optional = true, nullable = true),
                        Field("alias", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> OpenWhat.New(a[1] as String, a[2] as String, a[3] as String?, a[4] as String?, a[5] as ForkFrom?, a[6] as String?, a[7] as String?, a[8] as String?, a[9] as RotationPick?, a[10] as String?) },
                        { v -> arrayOf("new", v.agent, v.cwd, v.tmuxName, v.command, v.fork, v.prompt, v.model, v.permissionMode, v.rotation, v.alias) },
                    )),
                Variant("resume", OpenWhat.Resume::class.java, obj(
                        listOf(
                        Field("kind", { tag("resume") }, optional = false, nullable = false),
                        Field("sid", { Str }, optional = false, nullable = false),
                        ),
                        { a -> OpenWhat.Resume(a[1] as String) },
                        { v -> arrayOf("resume", v.sid) },
                    )),
                Variant("look", OpenWhat.Look::class.java, obj(
                        listOf(
                        Field("kind", { tag("look") }, optional = false, nullable = false),
                        Field("sid", { opt(Str) }, optional = true, nullable = true),
                        Field("session", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> OpenWhat.Look(a[1] as String?, a[2] as String?) },
                        { v -> arrayOf("look", v.sid, v.session) },
                    )),
                Variant("shell", OpenWhat.Shell::class.java, obj(
                        listOf(
                        Field("kind", { tag("shell") }, optional = false, nullable = false),
                        Field("session", { opt(Str) }, optional = true, nullable = true),
                        Field("cwd", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> OpenWhat.Shell(a[1] as String?, a[2] as String?) },
                        { v -> arrayOf("shell", v.session, v.cwd) },
                    )),
                ),
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
    val prompt: Sent?,
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
            Field("prompt", { opt(Sent.ADAPTER) }, optional = false, nullable = true),
            Field("arrival", { opt(ArriveAsk.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> Opened(a[0] as Landed, a[1] as String?, a[2] as String?, a[3] as String?, a[4] as String?, a[5] as String?, a[6] as LaunchedAccount?, a[7] as String?, a[8] as Sent?, a[9] as ArriveAsk?) },
            { v -> arrayOf(v.landed, v.session, v.line, v.sid, v.agent, v.cwd, v.account, v.note, v.prompt, v.arrival) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class OpenedItem(
    val key: String,
    val state: OpenState,
    val id: String? = null,
    val staged: String? = null,
    val path: WirePath? = null,
    val newName: WirePath? = null,
    val overwrite: Boolean? = null,
    val code: String? = null,
    val reason: String? = null,
    val detail: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<OpenedItem> = obj(
            listOf(
            Field("key", { Str }, optional = false, nullable = false),
            Field("state", { OpenState.ADAPTER }, optional = false, nullable = false),
            Field("id", { opt(Str) }, optional = true, nullable = true),
            Field("staged", { opt(Str) }, optional = true, nullable = true),
            Field("path", { opt(WirePath.ADAPTER) }, optional = true, nullable = true),
            Field("new_name", { opt(WirePath.ADAPTER) }, optional = true, nullable = true),
            Field("overwrite", { opt(Bool) }, optional = true, nullable = true),
            Field("code", { opt(Str) }, optional = true, nullable = true),
            Field("reason", { opt(Str) }, optional = true, nullable = true),
            Field("detail", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> OpenedItem(a[0] as String, a[1] as OpenState, a[2] as String?, a[3] as String?, a[4] as WirePath?, a[5] as WirePath?, a[6] as Boolean?, a[7] as String?, a[8] as String?, a[9] as String?) },
            { v -> arrayOf(v.key, v.state, v.id, v.staged, v.path, v.newName, v.overwrite, v.code, v.reason, v.detail) },
        )
    }
}

typealias Origin = String

@Suppress("UNCHECKED_CAST")
data class OriginSessionsListedPayload(
    val origin: String,
    val all: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<OriginSessionsListedPayload> = obj(
            listOf(
            Field("origin", { Str }, optional = false, nullable = false),
            Field("all", { Bool }, optional = false, nullable = false),
            ),
            { a -> OriginSessionsListedPayload(a[0] as String, a[1] as Boolean) },
            { v -> arrayOf(v.origin, v.all) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Outcome(
    val sid: String,
    val outcome: OutcomeOutcome,
    val why: String?,
    val said: String?,
    val copyDetail: String,
    val session: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Outcome> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("outcome", { OutcomeOutcome.ADAPTER }, optional = false, nullable = false),
            Field("why", { opt(Str) }, optional = false, nullable = true),
            Field("said", { opt(Str) }, optional = false, nullable = true),
            Field("copyDetail", { Str }, optional = false, nullable = false),
            Field("session", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> Outcome(a[0] as String, a[1] as OutcomeOutcome, a[2] as String?, a[3] as String?, a[4] as String, a[5] as String?) },
            { v -> arrayOf(v.sid, v.outcome, v.why, v.said, v.copyDetail, v.session) },
        )
    }
}

enum class OutcomeOutcome(val wire: String) {
    DONE("done"),
    SKIPPED("skipped"),
    FAILED("failed"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<OutcomeOutcome> = enumOf(entries) { it.wire }
    }
}

enum class OwnClass(val wire: String) {
    TRUTH("truth"),
    CACHE("cache"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<OwnClass> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class OwnItem(
    val id: String,
    val what: String,
    val path: String,
    val dir: Boolean,
    val `class`: OwnClass,
    val exists: Boolean,
    val sizeText: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<OwnItem> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("what", { Str }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            Field("dir", { Bool }, optional = false, nullable = false),
            Field("class", { OwnClass.ADAPTER }, optional = false, nullable = false),
            Field("exists", { Bool }, optional = false, nullable = false),
            Field("size_text", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> OwnItem(a[0] as String, a[1] as String, a[2] as String, a[3] as Boolean, a[4] as OwnClass, a[5] as Boolean, a[6] as String?) },
            { v -> arrayOf(v.id, v.what, v.path, v.dir, v.`class`, v.exists, v.sizeText) },
        )
    }
}

enum class PasteKind(val wire: String) {
    CODE("code"),
    REDIRECT_URL("redirectUrl"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PasteKind> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class Pasted(
    val id: String? = null,
    val start: Int,
    val end: Int,
    val bodyStart: Int,
    val bodyEnd: Int,
    val lines: Int,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Pasted> = obj(
            listOf(
            Field("id", { opt(Str) }, optional = true, nullable = true),
            Field("start", { Int32 }, optional = false, nullable = false),
            Field("end", { Int32 }, optional = false, nullable = false),
            Field("bodyStart", { Int32 }, optional = false, nullable = false),
            Field("bodyEnd", { Int32 }, optional = false, nullable = false),
            Field("lines", { Int32 }, optional = false, nullable = false),
            ),
            { a -> Pasted(a[0] as String?, a[1] as Int, a[2] as Int, a[3] as Int, a[4] as Int, a[5] as Int) },
            { v -> arrayOf(v.id, v.start, v.end, v.bodyStart, v.bodyEnd, v.lines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Patch(
    val starred: Boolean? = null,
    val customTitle: String? = null,
    val hide: HideAsk? = null,
    val autoAnswer: AutoAnswer? = null,
    val pinned: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Patch> = obj(
            listOf(
            Field("starred", { opt(Bool) }, optional = true, nullable = true),
            Field("customTitle", { opt(Str) }, optional = true, nullable = true),
            Field("hide", { opt(HideAsk.ADAPTER) }, optional = true, nullable = true),
            Field("autoAnswer", { opt(AutoAnswer.ADAPTER) }, optional = true, nullable = true),
            Field("pinned", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> Patch(a[0] as Boolean?, a[1] as String?, a[2] as HideAsk?, a[3] as AutoAnswer?, a[4] as Boolean?) },
            { v -> arrayOf(v.starred, v.customTitle, v.hide, v.autoAnswer, v.pinned) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PatchHunk(
    val oldStart: Int,
    val oldLines: Int,
    val newStart: Int,
    val newLines: Int,
    val lines: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PatchHunk> = obj(
            listOf(
            Field("oldStart", { Int32 }, optional = false, nullable = false),
            Field("oldLines", { Int32 }, optional = false, nullable = false),
            Field("newStart", { Int32 }, optional = false, nullable = false),
            Field("newLines", { Int32 }, optional = false, nullable = false),
            Field("lines", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> PatchHunk(a[0] as Int, a[1] as Int, a[2] as Int, a[3] as Int, a[4] as List<String>) },
            { v -> arrayOf(v.oldStart, v.oldLines, v.newStart, v.newLines, v.lines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PendingCall(
    val id: String,
    val name: String,
    val what: String?,
    val at: String?,
    val atMs: Long?,
    val state: StepWait,
    val why: UnclearWhy?,
    val text: String?,
    val whyText: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PendingCall> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("what", { opt(Str) }, optional = false, nullable = true),
            Field("at", { opt(Str) }, optional = false, nullable = true),
            Field("atMs", { opt(Lng) }, optional = false, nullable = true),
            Field("state", { StepWait.ADAPTER }, optional = false, nullable = false),
            Field("why", { opt(UnclearWhy.ADAPTER) }, optional = false, nullable = true),
            Field("text", { opt(Str) }, optional = false, nullable = true),
            Field("whyText", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PendingCall(a[0] as String, a[1] as String, a[2] as String?, a[3] as String?, a[4] as Long?, a[5] as StepWait, a[6] as UnclearWhy?, a[7] as String?, a[8] as String?) },
            { v -> arrayOf(v.id, v.name, v.what, v.at, v.atMs, v.state, v.why, v.text, v.whyText) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PhoneMenu(
    val bar: List<MenuEntry>,
    val more: List<MenuEntry>,
    val tap: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PhoneMenu> = obj(
            listOf(
            Field("bar", { arr(MenuEntry.ADAPTER) }, optional = false, nullable = false),
            Field("more", { arr(MenuEntry.ADAPTER) }, optional = false, nullable = false),
            Field("tap", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PhoneMenu(a[0] as List<MenuEntry>, a[1] as List<MenuEntry>, a[2] as String?) },
            { v -> arrayOf(v.bar, v.more, v.tap) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlaceAnswer(
    val action: PlaceAnswerAction,
    val why: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlaceAnswer> = obj(
            listOf(
            Field("action", { PlaceAnswerAction.ADAPTER }, optional = false, nullable = false),
            Field("why", { Str }, optional = false, nullable = false),
            ),
            { a -> PlaceAnswer(a[0] as PlaceAnswerAction, a[1] as String) },
            { v -> arrayOf(v.action, v.why) },
        )
    }
}

enum class PlaceAnswerAction(val wire: String) {
    PLACE("place"),
    KEEP("keep"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PlaceAnswerAction> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class PlaceReq(
    val dest: String,
    val machine: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlaceReq> = obj(
            listOf(
            Field("dest", { Str }, optional = false, nullable = false),
            Field("machine", { Str }, optional = false, nullable = false),
            ),
            { a -> PlaceReq(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.dest, v.machine) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanAckReply(
    val acked: Boolean,
    val key: String,
    val needCount: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanAckReply> = obj(
            listOf(
            Field("acked", { Bool }, optional = false, nullable = false),
            Field("key", { Str }, optional = false, nullable = false),
            Field("needCount", { Lng }, optional = false, nullable = false),
            ),
            { a -> PlanAckReply(a[0] as Boolean, a[1] as String, a[2] as Long) },
            { v -> arrayOf(v.acked, v.key, v.needCount) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanAckReq(
    val workspace: String,
    val slice: String,
    val key: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanAckReq> = obj(
            listOf(
            Field("workspace", { Str }, optional = false, nullable = false),
            Field("slice", { Str }, optional = false, nullable = false),
            Field("key", { Str }, optional = false, nullable = false),
            ),
            { a -> PlanAckReq(a[0] as String, a[1] as String, a[2] as String) },
            { v -> arrayOf(v.workspace, v.slice, v.key) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanAdopted(
    val slice: String,
    val path: String,
    val switched: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanAdopted> = obj(
            listOf(
            Field("slice", { Str }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            Field("switched", { Bool }, optional = false, nullable = false),
            ),
            { a -> PlanAdopted(a[0] as String, a[1] as String, a[2] as Boolean) },
            { v -> arrayOf(v.slice, v.path, v.switched) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanArchived(
    val id: String,
    val title: String?,
    val kind: String?,
    val replacedBy: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanArchived> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("title", { opt(Str) }, optional = false, nullable = true),
            Field("kind", { opt(Str) }, optional = false, nullable = true),
            Field("replacedBy", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PlanArchived(a[0] as String, a[1] as String?, a[2] as String?, a[3] as String?) },
            { v -> arrayOf(v.id, v.title, v.kind, v.replacedBy) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanAsk(
    val title: String,
    val alias: String?,
    val line: String,
    val runs: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanAsk> = obj(
            listOf(
            Field("title", { Str }, optional = false, nullable = false),
            Field("alias", { opt(Str) }, optional = false, nullable = true),
            Field("line", { Str }, optional = false, nullable = false),
            Field("runs", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PlanAsk(a[0] as String, a[1] as String?, a[2] as String, a[3] as String?) },
            { v -> arrayOf(v.title, v.alias, v.line, v.runs) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanAskReq(
    val workspace: String,
    val slice: String,
    val id: String,
    val alias: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanAskReq> = obj(
            listOf(
            Field("workspace", { Str }, optional = false, nullable = false),
            Field("slice", { Str }, optional = false, nullable = false),
            Field("id", { Str }, optional = false, nullable = false),
            Field("alias", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> PlanAskReq(a[0] as String, a[1] as String, a[2] as String, a[3] as String?) },
            { v -> arrayOf(v.workspace, v.slice, v.id, v.alias) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanBlock(
    val id: String,
    val cells: List<String>,
    val inside: PlanKids?,
    val dir: String?,
    val owner: PlanWho?,
    val phase: String?,
    val at: String?,
    val listed: Boolean,
    val handoff: String?,
    val handoffError: String?,
    val takeover: PlanTakeover,
    val groups: PlanBlockGroups,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanBlock> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("cells", { arr(Str) }, optional = false, nullable = false),
            Field("inside", { opt(PlanKids.ADAPTER) }, optional = false, nullable = true),
            Field("dir", { opt(Str) }, optional = false, nullable = true),
            Field("owner", { opt(PlanWho.ADAPTER) }, optional = false, nullable = true),
            Field("phase", { opt(Str) }, optional = false, nullable = true),
            Field("at", { opt(Str) }, optional = false, nullable = true),
            Field("listed", { Bool }, optional = false, nullable = false),
            Field("handoff", { opt(Str) }, optional = false, nullable = true),
            Field("handoffError", { opt(Str) }, optional = false, nullable = true),
            Field("takeover", { PlanTakeover.ADAPTER }, optional = false, nullable = false),
            Field("groups", { PlanBlockGroups.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> PlanBlock(a[0] as String, a[1] as List<String>, a[2] as PlanKids?, a[3] as String?, a[4] as PlanWho?, a[5] as String?, a[6] as String?, a[7] as Boolean, a[8] as String?, a[9] as String?, a[10] as PlanTakeover, a[11] as PlanBlockGroups) },
            { v -> arrayOf(v.id, v.cells, v.inside, v.dir, v.owner, v.phase, v.at, v.listed, v.handoff, v.handoffError, v.takeover, v.groups) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanBlockGroups(
    val groups: List<PlanGroup>,
    val text: String,
    val unread: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanBlockGroups> = obj(
            listOf(
            Field("groups", { arr(PlanGroup.ADAPTER) }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("unread", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PlanBlockGroups(a[0] as List<PlanGroup>, a[1] as String, a[2] as String?) },
            { v -> arrayOf(v.groups, v.text, v.unread) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanCell(
    val id: String,
    val title: String?,
    val kind: String?,
    val body: String?,
    val parent: String?,
    val children: List<String>,
    val edges: PlanEdges,
    val pointedBy: PlanEdges,
    val files: List<PlanFile>,
    val state: List<PlanState>,
    val stateLine: String?,
    val evidencedBy: List<String>,
    val statusCode: PlanCellStatusCode?,
    val undecided: Boolean,
    val whyCode: PlanWhyCode?,
    val whyShort: PlanWhyShort?,
    val sets: List<PlanSetsTo>,
    val block: String?,
    val owner: PlanWho?,
    val signer: PlanWho?,
    val covers: List<String>,
    val coveredBy: List<String>,
    val refs: PlanRefs,
    val returned: PlanReturned?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanCell> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("title", { opt(Str) }, optional = false, nullable = true),
            Field("kind", { opt(Str) }, optional = false, nullable = true),
            Field("body", { opt(Str) }, optional = false, nullable = true),
            Field("parent", { opt(Str) }, optional = false, nullable = true),
            Field("children", { arr(Str) }, optional = false, nullable = false),
            Field("edges", { PlanEdges.ADAPTER }, optional = false, nullable = false),
            Field("pointedBy", { PlanEdges.ADAPTER }, optional = false, nullable = false),
            Field("files", { arr(PlanFile.ADAPTER) }, optional = false, nullable = false),
            Field("state", { arr(PlanState.ADAPTER) }, optional = false, nullable = false),
            Field("stateLine", { opt(Str) }, optional = false, nullable = true),
            Field("evidencedBy", { arr(Str) }, optional = false, nullable = false),
            Field("statusCode", { opt(PlanCellStatusCode.ADAPTER) }, optional = false, nullable = true),
            Field("undecided", { Bool }, optional = false, nullable = false),
            Field("whyCode", { opt(PlanWhyCode.ADAPTER) }, optional = false, nullable = true),
            Field("whyShort", { opt(PlanWhyShort.ADAPTER) }, optional = false, nullable = true),
            Field("sets", { arr(PlanSetsTo.ADAPTER) }, optional = false, nullable = false),
            Field("block", { opt(Str) }, optional = false, nullable = true),
            Field("owner", { opt(PlanWho.ADAPTER) }, optional = false, nullable = true),
            Field("signer", { opt(PlanWho.ADAPTER) }, optional = false, nullable = true),
            Field("covers", { arr(Str) }, optional = false, nullable = false),
            Field("coveredBy", { arr(Str) }, optional = false, nullable = false),
            Field("refs", { PlanRefs.ADAPTER }, optional = false, nullable = false),
            Field("returned", { opt(PlanReturned.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> PlanCell(a[0] as String, a[1] as String?, a[2] as String?, a[3] as String?, a[4] as String?, a[5] as List<String>, a[6] as PlanEdges, a[7] as PlanEdges, a[8] as List<PlanFile>, a[9] as List<PlanState>, a[10] as String?, a[11] as List<String>, a[12] as PlanCellStatusCode?, a[13] as Boolean, a[14] as PlanWhyCode?, a[15] as PlanWhyShort?, a[16] as List<PlanSetsTo>, a[17] as String?, a[18] as PlanWho?, a[19] as PlanWho?, a[20] as List<String>, a[21] as List<String>, a[22] as PlanRefs, a[23] as PlanReturned?) },
            { v -> arrayOf(v.id, v.title, v.kind, v.body, v.parent, v.children, v.edges, v.pointedBy, v.files, v.state, v.stateLine, v.evidencedBy, v.statusCode, v.undecided, v.whyCode, v.whyShort, v.sets, v.block, v.owner, v.signer, v.covers, v.coveredBy, v.refs, v.returned) },
        )
    }
}

enum class PlanCellStatusCode(val wire: String) {
    DONE("done"),
    OPEN("open"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PlanCellStatusCode> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanCellView(
    val view: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanCellView> = obj(
            listOf(
            Field("view", { Str }, optional = false, nullable = false),
            ),
            { a -> PlanCellView(a[0] as String) },
            { v -> arrayOf(v.view) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanCellViewReq(
    val workspace: String,
    val slice: String,
    val id: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanCellViewReq> = obj(
            listOf(
            Field("workspace", { Str }, optional = false, nullable = false),
            Field("slice", { Str }, optional = false, nullable = false),
            Field("id", { Str }, optional = false, nullable = false),
            ),
            { a -> PlanCellViewReq(a[0] as String, a[1] as String, a[2] as String) },
            { v -> arrayOf(v.workspace, v.slice, v.id) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanCheck(
    val unreadable: List<PlanReadError>,
    val unparsed: List<String>,
    val red: List<PlanRed>,
    val undecidable: List<PlanUndecidable>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanCheck> = obj(
            listOf(
            Field("unreadable", { arr(PlanReadError.ADAPTER) }, optional = false, nullable = false),
            Field("unparsed", { arr(Str) }, optional = false, nullable = false),
            Field("red", { arr(PlanRed.ADAPTER) }, optional = false, nullable = false),
            Field("undecidable", { arr(PlanUndecidable.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> PlanCheck(a[0] as List<PlanReadError>, a[1] as List<String>, a[2] as List<PlanRed>, a[3] as List<PlanUndecidable>) },
            { v -> arrayOf(v.unreadable, v.unparsed, v.red, v.undecidable) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanCmdReply(
    val rc: Long,
    val said: String?,
    val path: String?,
    val adopted: PlanAdopted?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanCmdReply> = obj(
            listOf(
            Field("rc", { Lng }, optional = false, nullable = false),
            Field("said", { opt(Str) }, optional = false, nullable = true),
            Field("path", { opt(Str) }, optional = false, nullable = true),
            Field("adopted", { opt(PlanAdopted.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> PlanCmdReply(a[0] as Long, a[1] as String?, a[2] as String?, a[3] as PlanAdopted?) },
            { v -> arrayOf(v.rc, v.said, v.path, v.adopted) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanCommandReq(
    val workspace: String,
    val cmd: PlanCommandReqCmd,
    val slice: String? = null,
    val path: String? = null,
    val domain: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanCommandReq> = obj(
            listOf(
            Field("workspace", { Str }, optional = false, nullable = false),
            Field("cmd", { PlanCommandReqCmd.ADAPTER }, optional = false, nullable = false),
            Field("slice", { opt(Str) }, optional = true, nullable = true),
            Field("path", { opt(Str) }, optional = true, nullable = true),
            Field("domain", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> PlanCommandReq(a[0] as String, a[1] as PlanCommandReqCmd, a[2] as String?, a[3] as String?, a[4] as String?) },
            { v -> arrayOf(v.workspace, v.cmd, v.slice, v.path, v.domain) },
        )
    }
}

enum class PlanCommandReqCmd(val wire: String) {
    CONTINUE("continue"),
    PAUSE("pause"),
    ADOPT("adopt"),
    RENDER("render"),
    REPO("repo"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PlanCommandReqCmd> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanDomain(
    val name: String?,
    val error: String?,
    val kinds: List<PlanKind>,
    val phases: List<PlanPhase>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanDomain> = obj(
            listOf(
            Field("name", { opt(Str) }, optional = false, nullable = true),
            Field("error", { opt(Str) }, optional = false, nullable = true),
            Field("kinds", { arr(PlanKind.ADAPTER) }, optional = false, nullable = false),
            Field("phases", { arr(PlanPhase.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> PlanDomain(a[0] as String?, a[1] as String?, a[2] as List<PlanKind>, a[3] as List<PlanPhase>) },
            { v -> arrayOf(v.name, v.error, v.kinds, v.phases) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanEdgeWords(
    val to: String?,
    val with: String?,
    val after: String?,
    val replaces: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanEdgeWords> = obj(
            listOf(
            Field("to", { opt(Str) }, optional = false, nullable = true),
            Field("with", { opt(Str) }, optional = false, nullable = true),
            Field("after", { opt(Str) }, optional = false, nullable = true),
            Field("replaces", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PlanEdgeWords(a[0] as String?, a[1] as String?, a[2] as String?, a[3] as String?) },
            { v -> arrayOf(v.to, v.with, v.after, v.replaces) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanEdges(
    val to: List<String>,
    val with: List<String>,
    val after: List<String>,
    val replaces: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanEdges> = obj(
            listOf(
            Field("to", { arr(Str) }, optional = false, nullable = false),
            Field("with", { arr(Str) }, optional = false, nullable = false),
            Field("after", { arr(Str) }, optional = false, nullable = false),
            Field("replaces", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> PlanEdges(a[0] as List<String>, a[1] as List<String>, a[2] as List<String>, a[3] as List<String>) },
            { v -> arrayOf(v.to, v.with, v.after, v.replaces) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanFile(
    val path: String,
    val stateCode: PlanFileStateCode?,
    val note: String?,
    val checkText: String?,
    val alsoBy: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanFile> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("stateCode", { opt(PlanFileStateCode.ADAPTER) }, optional = false, nullable = true),
            Field("note", { opt(Str) }, optional = false, nullable = true),
            Field("checkText", { opt(Str) }, optional = false, nullable = true),
            Field("alsoBy", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> PlanFile(a[0] as String, a[1] as PlanFileStateCode?, a[2] as String?, a[3] as String?, a[4] as List<String>) },
            { v -> arrayOf(v.path, v.stateCode, v.note, v.checkText, v.alsoBy) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanFileDup(
    val id: String,
    val label: String,
    val title: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanFileDup> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("label", { Str }, optional = false, nullable = false),
            Field("title", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PlanFileDup(a[0] as String, a[1] as String, a[2] as String?) },
            { v -> arrayOf(v.id, v.label, v.title) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanFileOwner(
    val name: String,
    val id: String,
    val label: String,
    val title: String?,
    val statusCode: PlanFileOwnerStatusCode?,
    val status: String?,
    val block: String?,
    val fileState: PlanFileOwnerFileState?,
    val fileNote: String?,
    val dup: List<PlanFileDup>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanFileOwner> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("id", { Str }, optional = false, nullable = false),
            Field("label", { Str }, optional = false, nullable = false),
            Field("title", { opt(Str) }, optional = false, nullable = true),
            Field("statusCode", { opt(PlanFileOwnerStatusCode.ADAPTER) }, optional = false, nullable = true),
            Field("status", { opt(Str) }, optional = false, nullable = true),
            Field("block", { opt(Str) }, optional = false, nullable = true),
            Field("fileState", { opt(PlanFileOwnerFileState.ADAPTER) }, optional = false, nullable = true),
            Field("fileNote", { opt(Str) }, optional = false, nullable = true),
            Field("dup", { arr(PlanFileDup.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> PlanFileOwner(a[0] as String, a[1] as String, a[2] as String, a[3] as String?, a[4] as PlanFileOwnerStatusCode?, a[5] as String?, a[6] as String?, a[7] as PlanFileOwnerFileState?, a[8] as String?, a[9] as List<PlanFileDup>) },
            { v -> arrayOf(v.name, v.id, v.label, v.title, v.statusCode, v.status, v.block, v.fileState, v.fileNote, v.dup) },
        )
    }
}

enum class PlanFileOwnerFileState(val wire: String) {
    OK("ok"),
    MISSING("missing"),
    EMPTY("empty"),
    BROKEN("broken"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PlanFileOwnerFileState> = enumOf(entries) { it.wire }
    }
}

enum class PlanFileOwnerStatusCode(val wire: String) {
    DONE("done"),
    OPEN("open"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PlanFileOwnerStatusCode> = enumOf(entries) { it.wire }
    }
}

enum class PlanFileStateCode(val wire: String) {
    OK("ok"),
    MISSING("missing"),
    EMPTY("empty"),
    BROKEN("broken"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PlanFileStateCode> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanFiles(
    val workspace: String?,
    val slice: String?,
    val unreadable: String?,
    val entries: List<PlanFileOwner>,
    val unowned: List<String>?,
    val unownedWhy: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanFiles> = obj(
            listOf(
            Field("workspace", { opt(Str) }, optional = false, nullable = true),
            Field("slice", { opt(Str) }, optional = false, nullable = true),
            Field("unreadable", { opt(Str) }, optional = false, nullable = true),
            Field("entries", { arr(PlanFileOwner.ADAPTER) }, optional = false, nullable = false),
            Field("unowned", { opt(arr(Str)) }, optional = false, nullable = true),
            Field("unownedWhy", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PlanFiles(a[0] as String?, a[1] as String?, a[2] as String?, a[3] as List<PlanFileOwner>, a[4] as List<String>?, a[5] as String?) },
            { v -> arrayOf(v.workspace, v.slice, v.unreadable, v.entries, v.unowned, v.unownedWhy) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanFilesReq(
    val dir: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanFilesReq> = obj(
            listOf(
            Field("dir", { Str }, optional = false, nullable = false),
            ),
            { a -> PlanFilesReq(a[0] as String) },
            { v -> arrayOf(v.dir) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanGroup(
    val id: String,
    val title: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanGroup> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("title", { Str }, optional = false, nullable = false),
            ),
            { a -> PlanGroup(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.id, v.title) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanKids(
    val done: Long,
    val of: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanKids> = obj(
            listOf(
            Field("done", { Lng }, optional = false, nullable = false),
            Field("of", { Lng }, optional = false, nullable = false),
            ),
            { a -> PlanKids(a[0] as Long, a[1] as Long) },
            { v -> arrayOf(v.done, v.of) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanKind(
    val name: String?,
    val edgeWords: PlanEdgeWords,
    val values: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanKind> = obj(
            listOf(
            Field("name", { opt(Str) }, optional = false, nullable = true),
            Field("edgeWords", { PlanEdgeWords.ADAPTER }, optional = false, nullable = false),
            Field("values", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> PlanKind(a[0] as String?, a[1] as PlanEdgeWords, a[2] as List<String>) },
            { v -> arrayOf(v.name, v.edgeWords, v.values) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanList(
    val pb: PlanPb,
    val workspaces: List<PlanListWorkspace>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanList> = obj(
            listOf(
            Field("pb", { PlanPb.ADAPTER }, optional = false, nullable = false),
            Field("workspaces", { arr(PlanListWorkspace.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> PlanList(a[0] as PlanPb, a[1] as List<PlanListWorkspace>) },
            { v -> arrayOf(v.pb, v.workspaces) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanListReq(
    val dirs: List<String>? = null,
    val fresh: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanListReq> = obj(
            listOf(
            Field("dirs", { opt(arr(Str)) }, optional = true, nullable = true),
            Field("fresh", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> PlanListReq(a[0] as List<String>?, a[1] as Boolean?) },
            { v -> arrayOf(v.dirs, v.fresh) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanListSlice(
    val name: String,
    val domain: String?,
    val current: Boolean,
    val progress: PlanProgress?,
    val needCount: Long,
    val error: String?,
    val stale: PlanStale?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanListSlice> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("domain", { opt(Str) }, optional = false, nullable = true),
            Field("current", { Bool }, optional = false, nullable = false),
            Field("progress", { opt(PlanProgress.ADAPTER) }, optional = false, nullable = true),
            Field("needCount", { Lng }, optional = false, nullable = false),
            Field("error", { opt(Str) }, optional = false, nullable = true),
            Field("stale", { opt(PlanStale.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> PlanListSlice(a[0] as String, a[1] as String?, a[2] as Boolean, a[3] as PlanProgress?, a[4] as Long, a[5] as String?, a[6] as PlanStale?) },
            { v -> arrayOf(v.name, v.domain, v.current, v.progress, v.needCount, v.error, v.stale) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanListWorkspace(
    val workspace: String,
    val repo: String?,
    val auto: Boolean,
    val rev: String,
    val stale: PlanStale?,
    val needCount: Long,
    val bySession: Map<String, PlanSessionBlock>,
    val slices: List<PlanListSlice>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanListWorkspace> = obj(
            listOf(
            Field("workspace", { Str }, optional = false, nullable = false),
            Field("repo", { opt(Str) }, optional = false, nullable = true),
            Field("auto", { Bool }, optional = false, nullable = false),
            Field("rev", { Str }, optional = false, nullable = false),
            Field("stale", { opt(PlanStale.ADAPTER) }, optional = false, nullable = true),
            Field("needCount", { Lng }, optional = false, nullable = false),
            Field("bySession", { dict(PlanSessionBlock.ADAPTER) }, optional = false, nullable = false),
            Field("slices", { arr(PlanListSlice.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> PlanListWorkspace(a[0] as String, a[1] as String?, a[2] as Boolean, a[3] as String, a[4] as PlanStale?, a[5] as Long, a[6] as Map<String, PlanSessionBlock>, a[7] as List<PlanListSlice>) },
            { v -> arrayOf(v.workspace, v.repo, v.auto, v.rev, v.stale, v.needCount, v.bySession, v.slices) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanMsgRef(
    val start: Long,
    val end: Long,
    val ids: List<String>,
    val label: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanMsgRef> = obj(
            listOf(
            Field("start", { Lng }, optional = false, nullable = false),
            Field("end", { Lng }, optional = false, nullable = false),
            Field("ids", { arr(Str) }, optional = false, nullable = false),
            Field("label", { Str }, optional = false, nullable = false),
            ),
            { a -> PlanMsgRef(a[0] as Long, a[1] as Long, a[2] as List<String>, a[3] as String) },
            { v -> arrayOf(v.start, v.end, v.ids, v.label) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanMsgRefs(
    val workspace: String?,
    val slice: String?,
    val refs: List<List<PlanMsgRef>>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanMsgRefs> = obj(
            listOf(
            Field("workspace", { opt(Str) }, optional = false, nullable = true),
            Field("slice", { opt(Str) }, optional = false, nullable = true),
            Field("refs", { arr(arr(PlanMsgRef.ADAPTER)) }, optional = false, nullable = false),
            ),
            { a -> PlanMsgRefs(a[0] as String?, a[1] as String?, a[2] as List<List<PlanMsgRef>>) },
            { v -> arrayOf(v.workspace, v.slice, v.refs) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanNeed(
    val key: String,
    val kind: PlanNeedKind,
    val block: String?,
    val cell: String?,
    val sid: String?,
    val red: Long? = null,
    val acked: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanNeed> = obj(
            listOf(
            Field("key", { Str }, optional = false, nullable = false),
            Field("kind", { PlanNeedKind.ADAPTER }, optional = false, nullable = false),
            Field("block", { opt(Str) }, optional = false, nullable = true),
            Field("cell", { opt(Str) }, optional = false, nullable = true),
            Field("sid", { opt(Str) }, optional = false, nullable = true),
            Field("red", { opt(Lng) }, optional = true, nullable = true),
            Field("acked", { Bool }, optional = false, nullable = false),
            ),
            { a -> PlanNeed(a[0] as String, a[1] as PlanNeedKind, a[2] as String?, a[3] as String?, a[4] as String?, a[5] as Long?, a[6] as Boolean) },
            { v -> arrayOf(v.key, v.kind, v.block, v.cell, v.sid, v.red, v.acked) },
        )
    }
}

enum class PlanNeedKind(val wire: String) {
    TOP("top"),
    RED("red"),
    ENDED("ended"),
    ASK("ask"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PlanNeedKind> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanNeeds(
    val needs: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanNeeds> = obj(
            listOf(
            Field("needs", { U64 }, optional = false, nullable = false),
            ),
            { a -> PlanNeeds(a[0] as Long) },
            { v -> arrayOf(v.needs) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanNewExisting(
    val slice: String,
    val domain: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanNewExisting> = obj(
            listOf(
            Field("slice", { Str }, optional = false, nullable = false),
            Field("domain", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PlanNewExisting(a[0] as String, a[1] as String?) },
            { v -> arrayOf(v.slice, v.domain) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanNewForm(
    val pb: PlanPb,
    val workspace: String?,
    val root: String,
    val slice: String?,
    val domains: List<PlanDomain>,
    val existing: PlanNewExisting?,
    val repoDir: String?,
    val note: String?,
    val line: String?,
    val failed: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanNewForm> = obj(
            listOf(
            Field("pb", { PlanPb.ADAPTER }, optional = false, nullable = false),
            Field("workspace", { opt(Str) }, optional = false, nullable = true),
            Field("root", { Str }, optional = false, nullable = false),
            Field("slice", { opt(Str) }, optional = false, nullable = true),
            Field("domains", { arr(PlanDomain.ADAPTER) }, optional = false, nullable = false),
            Field("existing", { opt(PlanNewExisting.ADAPTER) }, optional = false, nullable = true),
            Field("repoDir", { opt(Str) }, optional = false, nullable = true),
            Field("note", { opt(Str) }, optional = false, nullable = true),
            Field("line", { opt(Str) }, optional = false, nullable = true),
            Field("failed", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PlanNewForm(a[0] as PlanPb, a[1] as String?, a[2] as String, a[3] as String?, a[4] as List<PlanDomain>, a[5] as PlanNewExisting?, a[6] as String?, a[7] as String?, a[8] as String?, a[9] as String?) },
            { v -> arrayOf(v.pb, v.workspace, v.root, v.slice, v.domains, v.existing, v.repoDir, v.note, v.line, v.failed) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanNewReq(
    val dir: String,
    val path: String,
    val domain: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanNewReq> = obj(
            listOf(
            Field("dir", { Str }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            Field("domain", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> PlanNewReq(a[0] as String, a[1] as String, a[2] as String?) },
            { v -> arrayOf(v.dir, v.path, v.domain) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanPb(
    val state: PlanPbState,
    val said: String?,
    val version: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanPb> = obj(
            listOf(
            Field("state", { PlanPbState.ADAPTER }, optional = false, nullable = false),
            Field("said", { opt(Str) }, optional = false, nullable = true),
            Field("version", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> PlanPb(a[0] as PlanPbState, a[1] as String?, a[2] as String?) },
            { v -> arrayOf(v.state, v.said, v.version) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanPbCheck(
    val state: PlanPbCheckState,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanPbCheck> = obj(
            listOf(
            Field("state", { PlanPbCheckState.ADAPTER }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> PlanPbCheck(a[0] as PlanPbCheckState, a[1] as String) },
            { v -> arrayOf(v.state, v.said) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanPbCheckReq(
    val dir: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanPbCheckReq> = obj(
            listOf(
            Field("dir", { Str }, optional = false, nullable = false),
            ),
            { a -> PlanPbCheckReq(a[0] as String) },
            { v -> arrayOf(v.dir) },
        )
    }
}

enum class PlanPbCheckState(val wire: String) {
    OK("ok"),
    MISSING("missing"),
    NO_ENTRY("no_entry"),
    NOT_RUNNABLE("not_runnable"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PlanPbCheckState> = enumOf(entries) { it.wire }
    }
}

enum class PlanPbState(val wire: String) {
    OK("ok"),
    MISSING("missing"),
    UNSUPPORTED("unsupported"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PlanPbState> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanPendingSet(
    val source: String?,
    val target: String?,
    val value: String?,
    val op: UserPathOp,
    val blockedReason: String?,
    val blockedCode: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanPendingSet> = obj(
            listOf(
            Field("source", { opt(Str) }, optional = false, nullable = true),
            Field("target", { opt(Str) }, optional = false, nullable = true),
            Field("value", { opt(Str) }, optional = false, nullable = true),
            Field("op", { UserPathOp.ADAPTER }, optional = false, nullable = false),
            Field("blockedReason", { opt(Str) }, optional = false, nullable = true),
            Field("blockedCode", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PlanPendingSet(a[0] as String?, a[1] as String?, a[2] as String?, a[3] as UserPathOp, a[4] as String?, a[5] as String?) },
            { v -> arrayOf(v.source, v.target, v.value, v.op, v.blockedReason, v.blockedCode) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanPhase(
    val name: String?,
    val does: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanPhase> = obj(
            listOf(
            Field("name", { opt(Str) }, optional = false, nullable = true),
            Field("does", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PlanPhase(a[0] as String?, a[1] as String?) },
            { v -> arrayOf(v.name, v.does) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanProgress(
    val done: Long,
    val of: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanProgress> = obj(
            listOf(
            Field("done", { Lng }, optional = false, nullable = false),
            Field("of", { Lng }, optional = false, nullable = false),
            ),
            { a -> PlanProgress(a[0] as Long, a[1] as Long) },
            { v -> arrayOf(v.done, v.of) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanRead(
    val pb: String?,
    val workspace: String,
    val repo: String?,
    val auto: Boolean,
    val domains: List<PlanDomain>,
    val slices: List<PlanSlice>,
    val bySession: Map<String, PlanSessionBlock>,
    val rev: String,
    val readAt: Long,
    val readAtText: String? = null,
    val seen: SeenAt? = null,
    val stale: PlanStale?,
    val needCount: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanRead> = obj(
            listOf(
            Field("pb", { opt(Str) }, optional = false, nullable = true),
            Field("workspace", { Str }, optional = false, nullable = false),
            Field("repo", { opt(Str) }, optional = false, nullable = true),
            Field("auto", { Bool }, optional = false, nullable = false),
            Field("domains", { arr(PlanDomain.ADAPTER) }, optional = false, nullable = false),
            Field("slices", { arr(PlanSlice.ADAPTER) }, optional = false, nullable = false),
            Field("bySession", { dict(PlanSessionBlock.ADAPTER) }, optional = false, nullable = false),
            Field("rev", { Str }, optional = false, nullable = false),
            Field("readAt", { Lng }, optional = false, nullable = false),
            Field("readAtText", { opt(Str) }, optional = true, nullable = true),
            Field("seen", { opt(SeenAt.ADAPTER) }, optional = true, nullable = true),
            Field("stale", { opt(PlanStale.ADAPTER) }, optional = false, nullable = true),
            Field("needCount", { Lng }, optional = false, nullable = false),
            ),
            { a -> PlanRead(a[0] as String?, a[1] as String, a[2] as String?, a[3] as Boolean, a[4] as List<PlanDomain>, a[5] as List<PlanSlice>, a[6] as Map<String, PlanSessionBlock>, a[7] as String, a[8] as Long, a[9] as String?, a[10] as SeenAt?, a[11] as PlanStale?, a[12] as Long) },
            { v -> arrayOf(v.pb, v.workspace, v.repo, v.auto, v.domains, v.slices, v.bySession, v.rev, v.readAt, v.readAtText, v.seen, v.stale, v.needCount) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanReadError(
    val file: String?,
    val line: Long?,
    val message: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanReadError> = obj(
            listOf(
            Field("file", { opt(Str) }, optional = false, nullable = true),
            Field("line", { opt(Lng) }, optional = false, nullable = true),
            Field("message", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PlanReadError(a[0] as String?, a[1] as Long?, a[2] as String?) },
            { v -> arrayOf(v.file, v.line, v.message) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanReadReq(
    val workspace: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanReadReq> = obj(
            listOf(
            Field("workspace", { Str }, optional = false, nullable = false),
            ),
            { a -> PlanReadReq(a[0] as String) },
            { v -> arrayOf(v.workspace) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanRed(
    val rule: String?,
    val what: String?,
    val block: String?,
    val fix: String?,
    val refs: List<PlanRef>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanRed> = obj(
            listOf(
            Field("rule", { opt(Str) }, optional = false, nullable = true),
            Field("what", { opt(Str) }, optional = false, nullable = true),
            Field("block", { opt(Str) }, optional = false, nullable = true),
            Field("fix", { opt(Str) }, optional = false, nullable = true),
            Field("refs", { arr(PlanRef.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> PlanRed(a[0] as String?, a[1] as String?, a[2] as String?, a[3] as String?, a[4] as List<PlanRef>) },
            { v -> arrayOf(v.rule, v.what, v.block, v.fix, v.refs) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanRef(
    val start: Long,
    val end: Long,
    val ids: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanRef> = obj(
            listOf(
            Field("start", { Lng }, optional = false, nullable = false),
            Field("end", { Lng }, optional = false, nullable = false),
            Field("ids", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> PlanRef(a[0] as Long, a[1] as Long, a[2] as List<String>) },
            { v -> arrayOf(v.start, v.end, v.ids) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanRefs(
    val title: List<PlanRef>,
    val body: List<PlanRef>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanRefs> = obj(
            listOf(
            Field("title", { arr(PlanRef.ADAPTER) }, optional = false, nullable = false),
            Field("body", { arr(PlanRef.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> PlanRefs(a[0] as List<PlanRef>, a[1] as List<PlanRef>) },
            { v -> arrayOf(v.title, v.body) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanRefsReq(
    val sid: String,
    val texts: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanRefsReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("texts", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> PlanRefsReq(a[0] as String, a[1] as List<String>) },
            { v -> arrayOf(v.sid, v.texts) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanReturnReply(
    val line: String,
    val to: PlanWho?,
    val result: PlanReturnReplyResult,
    val why: String?,
    val said: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanReturnReply> = obj(
            listOf(
            Field("line", { Str }, optional = false, nullable = false),
            Field("to", { opt(PlanWho.ADAPTER) }, optional = false, nullable = true),
            Field("result", { PlanReturnReplyResult.ADAPTER }, optional = false, nullable = false),
            Field("why", { opt(Str) }, optional = false, nullable = true),
            Field("said", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PlanReturnReply(a[0] as String, a[1] as PlanWho?, a[2] as PlanReturnReplyResult, a[3] as String?, a[4] as String?) },
            { v -> arrayOf(v.line, v.to, v.result, v.why, v.said) },
        )
    }
}

enum class PlanReturnReplyResult(val wire: String) {
    QUEUED("queued"),
    COPY("copy"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PlanReturnReplyResult> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanReturnReq(
    val workspace: String,
    val slice: String,
    val id: String,
    val text: String,
    val to: PlanReturnReqTo? = null,
    val client: String? = null,
    val resume: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanReturnReq> = obj(
            listOf(
            Field("workspace", { Str }, optional = false, nullable = false),
            Field("slice", { Str }, optional = false, nullable = false),
            Field("id", { Str }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("to", { opt(PlanReturnReqTo.ADAPTER) }, optional = true, nullable = true),
            Field("client", { opt(Str) }, optional = true, nullable = true),
            Field("resume", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> PlanReturnReq(a[0] as String, a[1] as String, a[2] as String, a[3] as String, a[4] as PlanReturnReqTo?, a[5] as String?, a[6] as Boolean?) },
            { v -> arrayOf(v.workspace, v.slice, v.id, v.text, v.to, v.client, v.resume) },
        )
    }
}

enum class PlanReturnReqTo(val wire: String) {
    OWNER("owner"),
    SIGNER("signer"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PlanReturnReqTo> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanReturned(
    val at: Long,
    val atText: String? = null,
    val to: PlanWho?,
    val state: PlanReturnedState,
    val by: PlanReturnedBy?,
    val child: PlanReturnedChild?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanReturned> = obj(
            listOf(
            Field("at", { Lng }, optional = false, nullable = false),
            Field("atText", { opt(Str) }, optional = true, nullable = true),
            Field("to", { opt(PlanWho.ADAPTER) }, optional = false, nullable = true),
            Field("state", { PlanReturnedState.ADAPTER }, optional = false, nullable = false),
            Field("by", { opt(PlanReturnedBy.ADAPTER) }, optional = false, nullable = true),
            Field("child", { opt(PlanReturnedChild.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> PlanReturned(a[0] as Long, a[1] as String?, a[2] as PlanWho?, a[3] as PlanReturnedState, a[4] as PlanReturnedBy?, a[5] as PlanReturnedChild?) },
            { v -> arrayOf(v.at, v.atText, v.to, v.state, v.by, v.child) },
        )
    }
}

enum class PlanReturnedBy(val wire: String) {
    CHILD("child"),
    BODY("body"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PlanReturnedBy> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanReturnedChild(
    val id: String,
    val title: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanReturnedChild> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("title", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PlanReturnedChild(a[0] as String, a[1] as String?) },
            { v -> arrayOf(v.id, v.title) },
        )
    }
}

enum class PlanReturnedState(val wire: String) {
    RETURNED("returned"),
    UNSURE("unsure"),
    LANDED("landed"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PlanReturnedState> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanSessionBlock(
    val slice: String,
    val block: String,
    val cell: String?,
    val title: String?,
    val top: Boolean,
    val phase: String?,
    val at: String?,
    val atTitle: String?,
    val via: PlanSessionBlockVia,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanSessionBlock> = obj(
            listOf(
            Field("slice", { Str }, optional = false, nullable = false),
            Field("block", { Str }, optional = false, nullable = false),
            Field("cell", { opt(Str) }, optional = false, nullable = true),
            Field("title", { opt(Str) }, optional = false, nullable = true),
            Field("top", { Bool }, optional = false, nullable = false),
            Field("phase", { opt(Str) }, optional = false, nullable = true),
            Field("at", { opt(Str) }, optional = false, nullable = true),
            Field("atTitle", { opt(Str) }, optional = false, nullable = true),
            Field("via", { PlanSessionBlockVia.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> PlanSessionBlock(a[0] as String, a[1] as String, a[2] as String?, a[3] as String?, a[4] as Boolean, a[5] as String?, a[6] as String?, a[7] as String?, a[8] as PlanSessionBlockVia) },
            { v -> arrayOf(v.slice, v.block, v.cell, v.title, v.top, v.phase, v.at, v.atTitle, v.via) },
        )
    }
}

enum class PlanSessionBlockVia(val wire: String) {
    SESSION("session"),
    SUBAGENT("subagent"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PlanSessionBlockVia> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanSetsTo(
    val target: String?,
    val value: String?,
    val op: UserPathOp,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanSetsTo> = obj(
            listOf(
            Field("target", { opt(Str) }, optional = false, nullable = true),
            Field("value", { opt(Str) }, optional = false, nullable = true),
            Field("op", { UserPathOp.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> PlanSetsTo(a[0] as String?, a[1] as String?, a[2] as UserPathOp) },
            { v -> arrayOf(v.target, v.value, v.op) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanSlice(
    val name: String,
    val domain: String?,
    val path: String?,
    val current: Boolean,
    val docsOnly: Boolean,
    val error: String?,
    val stale: PlanStale?,
    val bare: Boolean,
    val kinds: List<PlanKind>,
    val phases: List<PlanPhase>,
    val doneValue: String?,
    val top: List<String>,
    val progress: PlanProgress?,
    val openCells: Long,
    val blocks: List<PlanBlock>,
    val unbuiltBlocks: List<PlanUnbuilt>,
    val unowned: PlanUnowned,
    val pendingSets: List<PlanPendingSet>,
    val check: PlanCheck,
    val labels: Map<String, String>,
    val cells: List<PlanCell>,
    val archived: List<PlanArchived>,
    val needs: List<PlanNeed>,
    val needCount: Long,
    val returned: PlanReturned?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanSlice> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("domain", { opt(Str) }, optional = false, nullable = true),
            Field("path", { opt(Str) }, optional = false, nullable = true),
            Field("current", { Bool }, optional = false, nullable = false),
            Field("docsOnly", { Bool }, optional = false, nullable = false),
            Field("error", { opt(Str) }, optional = false, nullable = true),
            Field("stale", { opt(PlanStale.ADAPTER) }, optional = false, nullable = true),
            Field("bare", { Bool }, optional = false, nullable = false),
            Field("kinds", { arr(PlanKind.ADAPTER) }, optional = false, nullable = false),
            Field("phases", { arr(PlanPhase.ADAPTER) }, optional = false, nullable = false),
            Field("doneValue", { opt(Str) }, optional = false, nullable = true),
            Field("top", { arr(Str) }, optional = false, nullable = false),
            Field("progress", { opt(PlanProgress.ADAPTER) }, optional = false, nullable = true),
            Field("openCells", { Lng }, optional = false, nullable = false),
            Field("blocks", { arr(PlanBlock.ADAPTER) }, optional = false, nullable = false),
            Field("unbuiltBlocks", { arr(PlanUnbuilt.ADAPTER) }, optional = false, nullable = false),
            Field("unowned", { PlanUnowned.ADAPTER }, optional = false, nullable = false),
            Field("pendingSets", { arr(PlanPendingSet.ADAPTER) }, optional = false, nullable = false),
            Field("check", { PlanCheck.ADAPTER }, optional = false, nullable = false),
            Field("labels", { dict(Str) }, optional = false, nullable = false),
            Field("cells", { arr(PlanCell.ADAPTER) }, optional = false, nullable = false),
            Field("archived", { arr(PlanArchived.ADAPTER) }, optional = false, nullable = false),
            Field("needs", { arr(PlanNeed.ADAPTER) }, optional = false, nullable = false),
            Field("needCount", { Lng }, optional = false, nullable = false),
            Field("returned", { opt(PlanReturned.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> PlanSlice(a[0] as String, a[1] as String?, a[2] as String?, a[3] as Boolean, a[4] as Boolean, a[5] as String?, a[6] as PlanStale?, a[7] as Boolean, a[8] as List<PlanKind>, a[9] as List<PlanPhase>, a[10] as String?, a[11] as List<String>, a[12] as PlanProgress?, a[13] as Long, a[14] as List<PlanBlock>, a[15] as List<PlanUnbuilt>, a[16] as PlanUnowned, a[17] as List<PlanPendingSet>, a[18] as PlanCheck, a[19] as Map<String, String>, a[20] as List<PlanCell>, a[21] as List<PlanArchived>, a[22] as List<PlanNeed>, a[23] as Long, a[24] as PlanReturned?) },
            { v -> arrayOf(v.name, v.domain, v.path, v.current, v.docsOnly, v.error, v.stale, v.bare, v.kinds, v.phases, v.doneValue, v.top, v.progress, v.openCells, v.blocks, v.unbuiltBlocks, v.unowned, v.pendingSets, v.check, v.labels, v.cells, v.archived, v.needs, v.needCount, v.returned) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanStale(
    val said: String?,
    val raw: String?,
    val since: Long,
    val sinceText: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanStale> = obj(
            listOf(
            Field("said", { opt(Str) }, optional = false, nullable = true),
            Field("raw", { opt(Str) }, optional = false, nullable = true),
            Field("since", { Lng }, optional = false, nullable = false),
            Field("sinceText", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> PlanStale(a[0] as String?, a[1] as String?, a[2] as Long, a[3] as String?) },
            { v -> arrayOf(v.said, v.raw, v.since, v.sinceText) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanState(
    val value: String,
    val count: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanState> = obj(
            listOf(
            Field("value", { Str }, optional = false, nullable = false),
            Field("count", { Lng }, optional = false, nullable = false),
            ),
            { a -> PlanState(a[0] as String, a[1] as Long) },
            { v -> arrayOf(v.value, v.count) },
        )
    }
}

enum class PlanTakeover(val wire: String) {
    OPEN("open"),
    NO_LINE("no_line"),
    RUNNING("running"),
    UNSURE("unsure"),
    MERGING("merging"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PlanTakeover> = enumOf(entries) { it.wire }
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
data class PlanUnbuilt(
    val id: String?,
    val cells: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanUnbuilt> = obj(
            listOf(
            Field("id", { opt(Str) }, optional = false, nullable = true),
            Field("cells", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> PlanUnbuilt(a[0] as String?, a[1] as List<String>) },
            { v -> arrayOf(v.id, v.cells) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanUndecidable(
    val rule: String?,
    val why: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanUndecidable> = obj(
            listOf(
            Field("rule", { opt(Str) }, optional = false, nullable = true),
            Field("why", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PlanUndecidable(a[0] as String?, a[1] as String?) },
            { v -> arrayOf(v.rule, v.why) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanUnowned(
    val paths: List<String>?,
    val why: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanUnowned> = obj(
            listOf(
            Field("paths", { opt(arr(Str)) }, optional = false, nullable = true),
            Field("why", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PlanUnowned(a[0] as List<String>?, a[1] as String?) },
            { v -> arrayOf(v.paths, v.why) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanWho(
    val id: String,
    val kind: PlanWhoKind,
    val sid: String?,
    val needs: String?,
    val face: SessionFace,
    val label: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanWho> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("kind", { PlanWhoKind.ADAPTER }, optional = false, nullable = false),
            Field("sid", { opt(Str) }, optional = false, nullable = true),
            Field("needs", { opt(Str) }, optional = false, nullable = true),
            Field("face", { SessionFace.ADAPTER }, optional = false, nullable = false),
            Field("label", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PlanWho(a[0] as String, a[1] as PlanWhoKind, a[2] as String?, a[3] as String?, a[4] as SessionFace, a[5] as String?) },
            { v -> arrayOf(v.id, v.kind, v.sid, v.needs, v.face, v.label) },
        )
    }
}

enum class PlanWhoKind(val wire: String) {
    SESSION("session"),
    SUBAGENT("subagent"),
    UNKNOWN("unknown"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PlanWhoKind> = enumOf(entries) { it.wire }
    }
}

sealed interface PlanWhyCode {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    data object Nosign : PlanWhyCode {
        override val kind: String get() = "nosign"
    }
    @Suppress("UNCHECKED_CAST")
    data class Inside(
        val done: Long,
        val of: Long,
    ) : PlanWhyCode, Ordered {
        override val kind: String get() = "inside"
        override var wireOrder: IntArray? = null
    }
    data object Upper : PlanWhyCode {
        override val kind: String get() = "upper"
    }
    data object Undecided : PlanWhyCode {
        override val kind: String get() = "undecided"
    }

    companion object {
        val ADAPTER: JsonAdapter<PlanWhyCode> =
            tagged<PlanWhyCode>(
                "kind",
                listOf(
                Variant("nosign", PlanWhyCode.Nosign::class.java, obj(
                        listOf(
                        Field("kind", { tag("nosign") }, optional = false, nullable = false),
                        ),
                        { PlanWhyCode.Nosign },
                        { arrayOf("nosign") },
                    )),
                Variant("inside", PlanWhyCode.Inside::class.java, obj(
                        listOf(
                        Field("kind", { tag("inside") }, optional = false, nullable = false),
                        Field("done", { Lng }, optional = false, nullable = false),
                        Field("of", { Lng }, optional = false, nullable = false),
                        ),
                        { a -> PlanWhyCode.Inside(a[1] as Long, a[2] as Long) },
                        { v -> arrayOf("inside", v.done, v.of) },
                    )),
                Variant("upper", PlanWhyCode.Upper::class.java, obj(
                        listOf(
                        Field("kind", { tag("upper") }, optional = false, nullable = false),
                        ),
                        { PlanWhyCode.Upper },
                        { arrayOf("upper") },
                    )),
                Variant("undecided", PlanWhyCode.Undecided::class.java, obj(
                        listOf(
                        Field("kind", { tag("undecided") }, optional = false, nullable = false),
                        ),
                        { PlanWhyCode.Undecided },
                        { arrayOf("undecided") },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class PlanWhyShort(
    val text: String,
    val wait: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PlanWhyShort> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            Field("wait", { Bool }, optional = false, nullable = false),
            ),
            { a -> PlanWhyShort(a[0] as String, a[1] as Boolean) },
            { v -> arrayOf(v.text, v.wait) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PolicySet(
    val policy: PsExecPolicy,
    val setError: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PolicySet> = obj(
            listOf(
            Field("policy", { PsExecPolicy.ADAPTER }, optional = false, nullable = false),
            Field("setError", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PolicySet(a[0] as PsExecPolicy, a[1] as String?) },
            { v -> arrayOf(v.policy, v.setError) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PolicySetReq(
    val host: PsHost,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PolicySetReq> = obj(
            listOf(
            Field("host", { PsHost.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> PolicySetReq(a[0] as PsHost) },
            { v -> arrayOf(v.host) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Prefs(
    val launchers: Map<String, String>,
    val resumeWhere: ResumeWhere,
    val models: Map<String, String>,
    val permissionModes: Map<String, String>,
    val contextLimits: Map<String, Long>,
    val agentHomes: Map<String, String>,
    val pbDir: String?,
    val childTab: Tab,
    val bgTab: Tab,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Prefs> = obj(
            listOf(
            Field("launchers", { dict(Str) }, optional = false, nullable = false),
            Field("resumeWhere", { ResumeWhere.ADAPTER }, optional = false, nullable = false),
            Field("models", { dict(Str) }, optional = false, nullable = false),
            Field("permissionModes", { dict(Str) }, optional = false, nullable = false),
            Field("contextLimits", { dict(Lng) }, optional = false, nullable = false),
            Field("agentHomes", { dict(Str) }, optional = false, nullable = false),
            Field("pbDir", { opt(Str) }, optional = false, nullable = true),
            Field("childTab", { Tab.ADAPTER }, optional = false, nullable = false),
            Field("bgTab", { Tab.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> Prefs(a[0] as Map<String, String>, a[1] as ResumeWhere, a[2] as Map<String, String>, a[3] as Map<String, String>, a[4] as Map<String, Long>, a[5] as Map<String, String>, a[6] as String?, a[7] as Tab, a[8] as Tab) },
            { v -> arrayOf(v.launchers, v.resumeWhere, v.models, v.permissionModes, v.contextLimits, v.agentHomes, v.pbDir, v.childTab, v.bgTab) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PrefsReply(
    val state: FileState,
    val reason: String?,
    val detail: String?,
    val path: String?,
    val prefs: Prefs,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PrefsReply> = obj(
            listOf(
            Field("state", { FileState.ADAPTER }, optional = false, nullable = false),
            Field("reason", { opt(Str) }, optional = false, nullable = true),
            Field("detail", { opt(Str) }, optional = false, nullable = true),
            Field("path", { opt(Str) }, optional = false, nullable = true),
            Field("prefs", { Prefs.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> PrefsReply(a[0] as FileState, a[1] as String?, a[2] as String?, a[3] as String?, a[4] as Prefs) },
            { v -> arrayOf(v.state, v.reason, v.detail, v.path, v.prefs) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PrefsSet(
    val launchers: Map<String, String>? = null,
    val resumeWhere: ResumeWhere? = null,
    val models: Map<String, String?>? = null,
    val permissionModes: Map<String, String?>? = null,
    val contextLimits: Map<String, Long>? = null,
    val agentHomes: Map<String, String>? = null,
    val childTab: Tab? = null,
    val bgTab: Tab? = null,
    val pbDir: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PrefsSet> = obj(
            listOf(
            Field("launchers", { opt(dict(Str)) }, optional = true, nullable = true),
            Field("resumeWhere", { opt(ResumeWhere.ADAPTER) }, optional = true, nullable = true),
            Field("models", { opt(dict(opt(Str))) }, optional = true, nullable = true),
            Field("permissionModes", { opt(dict(opt(Str))) }, optional = true, nullable = true),
            Field("contextLimits", { opt(dict(Lng)) }, optional = true, nullable = true),
            Field("agentHomes", { opt(dict(Str)) }, optional = true, nullable = true),
            Field("childTab", { opt(Tab.ADAPTER) }, optional = true, nullable = true),
            Field("bgTab", { opt(Tab.ADAPTER) }, optional = true, nullable = true),
            Field("pbDir", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> PrefsSet(a[0] as Map<String, String>?, a[1] as ResumeWhere?, a[2] as Map<String, String?>?, a[3] as Map<String, String?>?, a[4] as Map<String, Long>?, a[5] as Map<String, String>?, a[6] as Tab?, a[7] as Tab?, a[8] as String?) },
            { v -> arrayOf(v.launchers, v.resumeWhere, v.models, v.permissionModes, v.contextLimits, v.agentHomes, v.childTab, v.bgTab, v.pbDir) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Preview(
    val screen: String,
    val cols: Int,
    val rows: Int,
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
            Field("cols", { Int32 }, optional = false, nullable = false),
            Field("rows", { Int32 }, optional = false, nullable = false),
            Field("cursor", { Cursor.ADAPTER }, optional = false, nullable = false),
            Field("lines", { arr(ScreenLine.ADAPTER) }, optional = false, nullable = false),
            Field("scrollback_lines", { Lng }, optional = false, nullable = false),
            Field("capped", { Bool }, optional = false, nullable = false),
            Field("captured_at", { U64 }, optional = false, nullable = false),
            Field("captured_at_text", { Str }, optional = false, nullable = false),
            ),
            { a -> Preview(a[0] as String, a[1] as Int, a[2] as Int, a[3] as Cursor, a[4] as List<ScreenLine>, a[5] as Long, a[6] as Boolean, a[7] as Long, a[8] as String) },
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
data class ProbeCell(
    val said: String,
    val tone: Tone,
    val segs: Segs,
    val end: ConnTestResult?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProbeCell> = obj(
            listOf(
            Field("said", { Str }, optional = false, nullable = false),
            Field("tone", { Tone.ADAPTER }, optional = false, nullable = false),
            Field("segs", { Segs.ADAPTER }, optional = false, nullable = false),
            Field("end", { opt(ConnTestResult.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> ProbeCell(a[0] as String, a[1] as Tone, a[2] as Segs, a[3] as ConnTestResult?) },
            { v -> arrayOf(v.said, v.tone, v.segs, v.end) },
        )
    }
}

enum class ProbeFrom(val wire: String) {
    USAGE("usage"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ProbeFrom> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ProbeReq(
    val ticket: String,
    val machine: Host,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProbeReq> = obj(
            listOf(
            Field("ticket", { Str }, optional = false, nullable = false),
            Field("machine", { Host.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> ProbeReq(a[0] as String, a[1] as Host) },
            { v -> arrayOf(v.ticket, v.machine) },
        )
    }
}

enum class ProbeState(val wire: String) {
    READ("read"),
    UNREADABLE("unreadable"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ProbeState> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class Probed(
    val agent: String,
    val account: String,
    val from: ProbeFrom,
    val now: Long,
    val state: ProbeState,
    val reason: String?,
    val windows: List<QuotaWindow>,
    val path: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Probed> = obj(
            listOf(
            Field("agent", { Str }, optional = false, nullable = false),
            Field("account", { Str }, optional = false, nullable = false),
            Field("from", { ProbeFrom.ADAPTER }, optional = false, nullable = false),
            Field("now", { Lng }, optional = false, nullable = false),
            Field("state", { ProbeState.ADAPTER }, optional = false, nullable = false),
            Field("reason", { opt(Str) }, optional = false, nullable = true),
            Field("windows", { arr(QuotaWindow.ADAPTER) }, optional = false, nullable = false),
            Field("path", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> Probed(a[0] as String, a[1] as String, a[2] as ProbeFrom, a[3] as Long, a[4] as ProbeState, a[5] as String?, a[6] as List<QuotaWindow>, a[7] as String?) },
            { v -> arrayOf(v.agent, v.account, v.from, v.now, v.state, v.reason, v.windows, v.path) },
        )
    }
}

sealed interface ProfileAccountPick {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    @Suppress("UNCHECKED_CAST")
    data class Account(
        val name: String,
    ) : ProfileAccountPick, Ordered {
        override val kind: String get() = "account"
        override var wireOrder: IntArray? = null
    }
    data object Base : ProfileAccountPick {
        override val kind: String get() = "base"
    }

    companion object {
        val ADAPTER: JsonAdapter<ProfileAccountPick> =
            tagged<ProfileAccountPick>(
                "kind",
                listOf(
                Variant("account", ProfileAccountPick.Account::class.java, obj(
                        listOf(
                        Field("kind", { tag("account") }, optional = false, nullable = false),
                        Field("name", { Str }, optional = false, nullable = false),
                        ),
                        { a -> ProfileAccountPick.Account(a[1] as String) },
                        { v -> arrayOf("account", v.name) },
                    )),
                Variant("base", ProfileAccountPick.Base::class.java, obj(
                        listOf(
                        Field("kind", { tag("base") }, optional = false, nullable = false),
                        ),
                        { ProfileAccountPick.Base },
                        { arrayOf("base") },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfileAccountShape(
    val account: String,
    val tmux: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfileAccountShape> = obj(
            listOf(
            Field("account", { Str }, optional = false, nullable = false),
            Field("tmux", { Bool }, optional = false, nullable = false),
            ),
            { a -> ProfileAccountShape(a[0] as String, a[1] as Boolean) },
            { v -> arrayOf(v.account, v.tmux) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfileAffected(
    val name: String,
    val changes: List<ProfileSlotChange>,
    val problem: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfileAffected> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("changes", { arr(ProfileSlotChange.ADAPTER) }, optional = false, nullable = false),
            Field("problem", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ProfileAffected(a[0] as String, a[1] as List<ProfileSlotChange>, a[2] as String?) },
            { v -> arrayOf(v.name, v.changes, v.problem) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfileBase(
    val name: String,
    val from: String?,
    val said: String,
    val selectable: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfileBase> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("from", { opt(Str) }, optional = false, nullable = true),
            Field("said", { Str }, optional = false, nullable = false),
            Field("selectable", { Bool }, optional = false, nullable = false),
            ),
            { a -> ProfileBase(a[0] as String, a[1] as String?, a[2] as String, a[3] as Boolean) },
            { v -> arrayOf(v.name, v.from, v.said, v.selectable) },
        )
    }
}

enum class ProfileChildren(val wire: String) {
    REPARENT("reparent"),
    CASCADE("cascade"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ProfileChildren> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfileCwdCase(
    val at: String,
    val to: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfileCwdCase> = obj(
            listOf(
            Field("at", { Str }, optional = false, nullable = false),
            Field("to", { Str }, optional = false, nullable = false),
            ),
            { a -> ProfileCwdCase(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.at, v.to) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfileForm(
    val name: String,
    val from: String?,
    val account: ProfileAccountPick?,
    val tmux: ProfileTmuxPick?,
    val cwdIf: List<ProfileCwdCase>?,
    val cwd: String?,
    val agent: String?,
    val args: String?,
    val launcher: String?,
    val tmuxSize: String?,
    val detach: Boolean,
    val tab: Tab?,
    val tabGroup: String?,
    val say: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfileForm> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("from", { opt(Str) }, optional = false, nullable = true),
            Field("account", { opt(ProfileAccountPick.ADAPTER) }, optional = false, nullable = true),
            Field("tmux", { opt(ProfileTmuxPick.ADAPTER) }, optional = false, nullable = true),
            Field("cwdIf", { opt(arr(ProfileCwdCase.ADAPTER)) }, optional = false, nullable = true),
            Field("cwd", { opt(Str) }, optional = false, nullable = true),
            Field("agent", { opt(Str) }, optional = false, nullable = true),
            Field("args", { opt(Str) }, optional = false, nullable = true),
            Field("launcher", { opt(Str) }, optional = false, nullable = true),
            Field("tmuxSize", { opt(Str) }, optional = false, nullable = true),
            Field("detach", { Bool }, optional = false, nullable = false),
            Field("tab", { opt(Tab.ADAPTER) }, optional = false, nullable = true),
            Field("tabGroup", { opt(Str) }, optional = false, nullable = true),
            Field("say", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ProfileForm(a[0] as String, a[1] as String?, a[2] as ProfileAccountPick?, a[3] as ProfileTmuxPick?, a[4] as List<ProfileCwdCase>?, a[5] as String?, a[6] as String?, a[7] as String?, a[8] as String?, a[9] as String?, a[10] as Boolean, a[11] as Tab?, a[12] as String?, a[13] as String?) },
            { v -> arrayOf(v.name, v.from, v.account, v.tmux, v.cwdIf, v.cwd, v.agent, v.args, v.launcher, v.tmuxSize, v.detach, v.tab, v.tabGroup, v.say) },
        )
    }
}

enum class ProfileKind(val wire: String) {
    LINK("link"),
    FUNCTION("function"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ProfileKind> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfileMergeRow(
    val key: String,
    val slot: ProfileSlotId,
    val label: String,
    val vals: List<String>,
    val said: String,
    val from: String,
    val fromSaid: String,
    val overriddenBy: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfileMergeRow> = obj(
            listOf(
            Field("key", { Str }, optional = false, nullable = false),
            Field("slot", { ProfileSlotId.ADAPTER }, optional = false, nullable = false),
            Field("label", { Str }, optional = false, nullable = false),
            Field("vals", { arr(Str) }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            Field("from", { Str }, optional = false, nullable = false),
            Field("fromSaid", { Str }, optional = false, nullable = false),
            Field("overriddenBy", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ProfileMergeRow(a[0] as String, a[1] as ProfileSlotId, a[2] as String, a[3] as List<String>, a[4] as String, a[5] as String, a[6] as String, a[7] as String?) },
            { v -> arrayOf(v.key, v.slot, v.label, v.vals, v.said, v.from, v.fromSaid, v.overriddenBy) },
        )
    }
}

sealed interface ProfileOp {
    /** 判别格（`op`）那个词。 */
    val op: String

    @Suppress("UNCHECKED_CAST")
    data class Set(
        val was: String?,
        val form: ProfileForm,
    ) : ProfileOp, Ordered {
        override val op: String get() = "set"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Remove(
        val name: String,
        val children: ProfileChildren? = null,
    ) : ProfileOp, Ordered {
        override val op: String get() = "remove"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Init(
        val seed: Boolean,
    ) : ProfileOp, Ordered {
        override val op: String get() = "init"
        override var wireOrder: IntArray? = null
    }
    data object AckMigrated : ProfileOp {
        override val op: String get() = "ackMigrated"
    }
    @Suppress("UNCHECKED_CAST")
    data class SaveSet(
        val was: String?,
        val set: SetEdit,
    ) : ProfileOp, Ordered {
        override val op: String get() = "saveSet"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class RemoveSet(
        val name: String,
    ) : ProfileOp, Ordered {
        override val op: String get() = "removeSet"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<ProfileOp> =
            tagged<ProfileOp>(
                "op",
                listOf(
                Variant("set", ProfileOp.Set::class.java, obj(
                        listOf(
                        Field("op", { tag("set") }, optional = false, nullable = false),
                        Field("was", { opt(Str) }, optional = false, nullable = true),
                        Field("form", { ProfileForm.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> ProfileOp.Set(a[1] as String?, a[2] as ProfileForm) },
                        { v -> arrayOf("set", v.was, v.form) },
                    )),
                Variant("remove", ProfileOp.Remove::class.java, obj(
                        listOf(
                        Field("op", { tag("remove") }, optional = false, nullable = false),
                        Field("name", { Str }, optional = false, nullable = false),
                        Field("children", { opt(ProfileChildren.ADAPTER) }, optional = true, nullable = true),
                        ),
                        { a -> ProfileOp.Remove(a[1] as String, a[2] as ProfileChildren?) },
                        { v -> arrayOf("remove", v.name, v.children) },
                    )),
                Variant("init", ProfileOp.Init::class.java, obj(
                        listOf(
                        Field("op", { tag("init") }, optional = false, nullable = false),
                        Field("seed", { Bool }, optional = false, nullable = false),
                        ),
                        { a -> ProfileOp.Init(a[1] as Boolean) },
                        { v -> arrayOf("init", v.seed) },
                    )),
                Variant("ackMigrated", ProfileOp.AckMigrated::class.java, obj(
                        listOf(
                        Field("op", { tag("ackMigrated") }, optional = false, nullable = false),
                        ),
                        { ProfileOp.AckMigrated },
                        { arrayOf("ackMigrated") },
                    )),
                Variant("saveSet", ProfileOp.SaveSet::class.java, obj(
                        listOf(
                        Field("op", { tag("saveSet") }, optional = false, nullable = false),
                        Field("was", { opt(Str) }, optional = false, nullable = true),
                        Field("set", { SetEdit.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> ProfileOp.SaveSet(a[1] as String?, a[2] as SetEdit) },
                        { v -> arrayOf("saveSet", v.was, v.set) },
                    )),
                Variant("removeSet", ProfileOp.RemoveSet::class.java, obj(
                        listOf(
                        Field("op", { tag("removeSet") }, optional = false, nullable = false),
                        Field("name", { Str }, optional = false, nullable = false),
                        ),
                        { a -> ProfileOp.RemoveSet(a[1] as String) },
                        { v -> arrayOf("removeSet", v.name) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfileOwnItem(
    val key: String,
    val slot: ProfileSlotId,
    val vals: List<String>,
    val line: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfileOwnItem> = obj(
            listOf(
            Field("key", { Str }, optional = false, nullable = false),
            Field("slot", { ProfileSlotId.ADAPTER }, optional = false, nullable = false),
            Field("vals", { arr(Str) }, optional = false, nullable = false),
            Field("line", { Lng }, optional = false, nullable = false),
            ),
            { a -> ProfileOwnItem(a[0] as String, a[1] as ProfileSlotId, a[2] as List<String>, a[3] as Long) },
            { v -> arrayOf(v.key, v.slot, v.vals, v.line) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfileProblem(
    val line: Long?,
    val message: String,
    val detail: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfileProblem> = obj(
            listOf(
            Field("line", { opt(Lng) }, optional = false, nullable = true),
            Field("message", { Str }, optional = false, nullable = false),
            Field("detail", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ProfileProblem(a[0] as Long?, a[1] as String, a[2] as String?) },
            { v -> arrayOf(v.line, v.message, v.detail) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfileResolved(
    val chain: List<String>,
    val rows: List<ProfileMergeRow>,
    val line: String?,
    val lineShort: String?,
    val lineError: String?,
    val problem: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfileResolved> = obj(
            listOf(
            Field("chain", { arr(Str) }, optional = false, nullable = false),
            Field("rows", { arr(ProfileMergeRow.ADAPTER) }, optional = false, nullable = false),
            Field("line", { opt(Str) }, optional = false, nullable = true),
            Field("lineShort", { opt(Str) }, optional = false, nullable = true),
            Field("lineError", { opt(Str) }, optional = false, nullable = true),
            Field("problem", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ProfileResolved(a[0] as List<String>, a[1] as List<ProfileMergeRow>, a[2] as String?, a[3] as String?, a[4] as String?, a[5] as String?) },
            { v -> arrayOf(v.chain, v.rows, v.line, v.lineShort, v.lineError, v.problem) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfileRow(
    val name: String,
    val from: String?,
    val own: List<ProfileOwnItem>,
    val agent: List<String>,
    val usable: Boolean,
    val problem: ProfileProblem?,
    val kind: ProfileKind,
    val functionWhy: String?,
    val functionLine: String?,
    val said: String,
    val form: ProfileForm,
    val accountShape: ProfileAccountShape?,
    val start: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfileRow> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("from", { opt(Str) }, optional = false, nullable = true),
            Field("own", { arr(ProfileOwnItem.ADAPTER) }, optional = false, nullable = false),
            Field("agent", { arr(Str) }, optional = false, nullable = false),
            Field("usable", { Bool }, optional = false, nullable = false),
            Field("problem", { opt(ProfileProblem.ADAPTER) }, optional = false, nullable = true),
            Field("kind", { ProfileKind.ADAPTER }, optional = false, nullable = false),
            Field("functionWhy", { opt(Str) }, optional = false, nullable = true),
            Field("functionLine", { opt(Str) }, optional = false, nullable = true),
            Field("said", { Str }, optional = false, nullable = false),
            Field("form", { ProfileForm.ADAPTER }, optional = false, nullable = false),
            Field("accountShape", { opt(ProfileAccountShape.ADAPTER) }, optional = false, nullable = true),
            Field("start", { Str }, optional = false, nullable = false),
            ),
            { a -> ProfileRow(a[0] as String, a[1] as String?, a[2] as List<ProfileOwnItem>, a[3] as List<String>, a[4] as Boolean, a[5] as ProfileProblem?, a[6] as ProfileKind, a[7] as String?, a[8] as String?, a[9] as String, a[10] as ProfileForm, a[11] as ProfileAccountShape?, a[12] as String) },
            { v -> arrayOf(v.name, v.from, v.own, v.agent, v.usable, v.problem, v.kind, v.functionWhy, v.functionLine, v.said, v.form, v.accountShape, v.start) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfileSlot(
    val id: ProfileSlotId,
    val label: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfileSlot> = obj(
            listOf(
            Field("id", { ProfileSlotId.ADAPTER }, optional = false, nullable = false),
            Field("label", { Str }, optional = false, nullable = false),
            ),
            { a -> ProfileSlot(a[0] as ProfileSlotId, a[1] as String) },
            { v -> arrayOf(v.id, v.label) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfileSlotChange(
    val slot: ProfileSlotId,
    val label: String,
    val before: String,
    val after: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfileSlotChange> = obj(
            listOf(
            Field("slot", { ProfileSlotId.ADAPTER }, optional = false, nullable = false),
            Field("label", { Str }, optional = false, nullable = false),
            Field("before", { Str }, optional = false, nullable = false),
            Field("after", { Str }, optional = false, nullable = false),
            ),
            { a -> ProfileSlotChange(a[0] as ProfileSlotId, a[1] as String, a[2] as String, a[3] as String) },
            { v -> arrayOf(v.slot, v.label, v.before, v.after) },
        )
    }
}

enum class ProfileSlotId(val wire: String) {
    ACCOUNT("account"),
    TMUX("tmux"),
    CWD_IF("cwdIf"),
    CWD("cwd"),
    AGENT("agent"),
    ARGS("args"),
    LAUNCHER("launcher"),
    TMUX_SIZE("tmuxSize"),
    DETACH("detach"),
    TAB("tab"),
    TAB_GROUP("tabGroup"),
    SAY("say"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ProfileSlotId> = enumOf(entries) { it.wire }
    }
}

enum class ProfileTmuxMode(val wire: String) {
    AUTO("auto"),
    FIXED("fixed"),
    BASE("base"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ProfileTmuxMode> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfileTmuxNaming(
    val mode: ProfileTmuxMode,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfileTmuxNaming> = obj(
            listOf(
            Field("mode", { ProfileTmuxMode.ADAPTER }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> ProfileTmuxNaming(a[0] as ProfileTmuxMode, a[1] as String) },
            { v -> arrayOf(v.mode, v.text) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfileTmuxPick(
    val mode: ProfileTmuxMode,
    val name: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfileTmuxPick> = obj(
            listOf(
            Field("mode", { ProfileTmuxMode.ADAPTER }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            ),
            { a -> ProfileTmuxPick(a[0] as ProfileTmuxMode, a[1] as String) },
            { v -> arrayOf(v.mode, v.name) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfilesBases(
    val bases: List<ProfileBase>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfilesBases> = obj(
            listOf(
            Field("bases", { arr(ProfileBase.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> ProfilesBases(a[0] as List<ProfileBase>) },
            { v -> arrayOf(v.bases) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfilesBasesReq(
    val name: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfilesBasesReq> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            ),
            { a -> ProfilesBasesReq(a[0] as String) },
            { v -> arrayOf(v.name) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfilesBook(
    val path: String,
    val pathShown: String,
    val exists: Boolean,
    val rev: String?,
    val editedAt: String?,
    val fileProblem: ProfileProblem?,
    val profiles: List<ProfileRow>,
    val sets: List<SetOut>,
    val seed: List<ProfileRow>,
    val seedHint: String,
    val seedCreate: String,
    val migrated: ProfilesMigrated?,
    val binDir: String,
    val accounts: List<String>,
    val tmux: Boolean?,
    val slots: List<ProfileSlot>,
    val tmuxNaming: List<ProfileTmuxNaming>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfilesBook> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("pathShown", { Str }, optional = false, nullable = false),
            Field("exists", { Bool }, optional = false, nullable = false),
            Field("rev", { opt(Str) }, optional = false, nullable = true),
            Field("editedAt", { opt(Str) }, optional = false, nullable = true),
            Field("fileProblem", { opt(ProfileProblem.ADAPTER) }, optional = false, nullable = true),
            Field("profiles", { arr(ProfileRow.ADAPTER) }, optional = false, nullable = false),
            Field("sets", { arr(SetOut.ADAPTER) }, optional = false, nullable = false),
            Field("seed", { arr(ProfileRow.ADAPTER) }, optional = false, nullable = false),
            Field("seedHint", { Str }, optional = false, nullable = false),
            Field("seedCreate", { Str }, optional = false, nullable = false),
            Field("migrated", { opt(ProfilesMigrated.ADAPTER) }, optional = false, nullable = true),
            Field("binDir", { Str }, optional = false, nullable = false),
            Field("accounts", { arr(Str) }, optional = false, nullable = false),
            Field("tmux", { opt(Bool) }, optional = false, nullable = true),
            Field("slots", { arr(ProfileSlot.ADAPTER) }, optional = false, nullable = false),
            Field("tmuxNaming", { arr(ProfileTmuxNaming.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> ProfilesBook(a[0] as String, a[1] as String, a[2] as Boolean, a[3] as String?, a[4] as String?, a[5] as ProfileProblem?, a[6] as List<ProfileRow>, a[7] as List<SetOut>, a[8] as List<ProfileRow>, a[9] as String, a[10] as String, a[11] as ProfilesMigrated?, a[12] as String, a[13] as List<String>, a[14] as Boolean?, a[15] as List<ProfileSlot>, a[16] as List<ProfileTmuxNaming>) },
            { v -> arrayOf(v.path, v.pathShown, v.exists, v.rev, v.editedAt, v.fileProblem, v.profiles, v.sets, v.seed, v.seedHint, v.seedCreate, v.migrated, v.binDir, v.accounts, v.tmux, v.slots, v.tmuxNaming) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfilesFromLineReq(
    val name: String,
    val line: String,
    val cwd: String? = null,
    val sessionName: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfilesFromLineReq> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("line", { Str }, optional = false, nullable = false),
            Field("cwd", { opt(Str) }, optional = true, nullable = true),
            Field("sessionName", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> ProfilesFromLineReq(a[0] as String, a[1] as String, a[2] as String?, a[3] as String?) },
            { v -> arrayOf(v.name, v.line, v.cwd, v.sessionName) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfilesImpact(
    val affected: List<ProfileAffected>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfilesImpact> = obj(
            listOf(
            Field("affected", { arr(ProfileAffected.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> ProfilesImpact(a[0] as List<ProfileAffected>) },
            { v -> arrayOf(v.affected) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfilesImpactReq(
    val changes: List<ProfileOp>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfilesImpactReq> = obj(
            listOf(
            Field("changes", { arr(ProfileOp.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> ProfilesImpactReq(a[0] as List<ProfileOp>) },
            { v -> arrayOf(v.changes) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfilesMigrated(
    val count: Long,
    val path: String,
    val shown: String,
    val skipped: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfilesMigrated> = obj(
            listOf(
            Field("count", { Lng }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            Field("shown", { Str }, optional = false, nullable = false),
            Field("skipped", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> ProfilesMigrated(a[0] as Long, a[1] as String, a[2] as String, a[3] as List<String>) },
            { v -> arrayOf(v.count, v.path, v.shown, v.skipped) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfilesResolveReq(
    val name: String,
    val edit: ProfileForm? = null,
    val at: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfilesResolveReq> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("edit", { opt(ProfileForm.ADAPTER) }, optional = true, nullable = true),
            Field("at", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> ProfilesResolveReq(a[0] as String, a[1] as ProfileForm?, a[2] as String?) },
            { v -> arrayOf(v.name, v.edit, v.at) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfilesWriteDone(
    val wrote: Boolean,
    val rev: String?,
    val reload: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfilesWriteDone> = obj(
            listOf(
            Field("wrote", { Bool }, optional = false, nullable = false),
            Field("rev", { opt(Str) }, optional = false, nullable = true),
            Field("reload", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ProfilesWriteDone(a[0] as Boolean, a[1] as String?, a[2] as String?) },
            { v -> arrayOf(v.wrote, v.rev, v.reload) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ProfilesWriteReq(
    val changes: List<ProfileOp>,
    val ifRev: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ProfilesWriteReq> = obj(
            listOf(
            Field("changes", { arr(ProfileOp.ADAPTER) }, optional = false, nullable = false),
            Field("ifRev", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ProfilesWriteReq(a[0] as List<ProfileOp>, a[1] as String?) },
            { v -> arrayOf(v.changes, v.ifRev) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PsExecPolicy(
    val host: PsHost,
    val effective: String?,
    val loads: Boolean?,
    val groupPolicy: Boolean,
    val error: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PsExecPolicy> = obj(
            listOf(
            Field("host", { PsHost.ADAPTER }, optional = false, nullable = false),
            Field("effective", { opt(Str) }, optional = false, nullable = true),
            Field("loads", { opt(Bool) }, optional = false, nullable = true),
            Field("groupPolicy", { Bool }, optional = false, nullable = false),
            Field("error", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> PsExecPolicy(a[0] as PsHost, a[1] as String?, a[2] as Boolean?, a[3] as Boolean, a[4] as String?) },
            { v -> arrayOf(v.host, v.effective, v.loads, v.groupPolicy, v.error) },
        )
    }
}

enum class PsHost(val wire: String) {
    POWERSHELL("powershell"),
    PWSH("pwsh"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PsHost> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class PtyOpts(
    val term: String,
    val cols: Int,
    val rows: Int,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PtyOpts> = obj(
            listOf(
            Field("term", { Str }, optional = false, nullable = false),
            Field("cols", { Int32 }, optional = false, nullable = false),
            Field("rows", { Int32 }, optional = false, nullable = false),
            ),
            { a -> PtyOpts(a[0] as String, a[1] as Int, a[2] as Int) },
            { v -> arrayOf(v.term, v.cols, v.rows) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PubkeyPushReq(
    val machine: Host,
    val pubKeyPath: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PubkeyPushReq> = obj(
            listOf(
            Field("machine", { Host.ADAPTER }, optional = false, nullable = false),
            Field("pubKeyPath", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> PubkeyPushReq(a[0] as Host, a[1] as String?) },
            { v -> arrayOf(v.machine, v.pubKeyPath) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class PubkeyPushed(
    val outcome: PushOutcome,
    val pubPath: String,
    val via: PushVia,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<PubkeyPushed> = obj(
            listOf(
            Field("outcome", { PushOutcome.ADAPTER }, optional = false, nullable = false),
            Field("pubPath", { Str }, optional = false, nullable = false),
            Field("via", { PushVia.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> PubkeyPushed(a[0] as PushOutcome, a[1] as String, a[2] as PushVia) },
            { v -> arrayOf(v.outcome, v.pubPath, v.via) },
        )
    }
}

enum class PushOutcome(val wire: String) {
    ADDED("added"),
    ALREADY("already"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PushOutcome> = enumOf(entries) { it.wire }
    }
}

enum class PushVia(val wire: String) {
    BACKEND("backend"),
    EXEC("exec"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<PushVia> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class QueueCancelled(
    val cancelled: Boolean,
    val queue: QueueList,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QueueCancelled> = obj(
            listOf(
            Field("cancelled", { Bool }, optional = false, nullable = false),
            Field("queue", { QueueList.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> QueueCancelled(a[0] as Boolean, a[1] as QueueList) },
            { v -> arrayOf(v.cancelled, v.queue) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class QueueItem(
    val seq: Long? = null,
    val id: String,
    val end: String,
    val atMs: Long,
    val text: String,
    val attachments: List<String>? = null,
    val rewind: Rewind? = null,
    val state: QueueState,
    val said: String,
    val from: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QueueItem> = obj(
            listOf(
            Field("seq", { opt(U64) }, optional = true, nullable = true),
            Field("id", { Str }, optional = false, nullable = false),
            Field("end", { Str }, optional = false, nullable = false),
            Field("atMs", { Lng }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("attachments", { opt(arr(Str)) }, optional = true, nullable = true),
            Field("rewind", { opt(Rewind.ADAPTER) }, optional = true, nullable = true),
            Field("state", { QueueState.ADAPTER }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            Field("from", { Str }, optional = false, nullable = false),
            ),
            { a -> QueueItem(a[0] as Long?, a[1] as String, a[2] as String, a[3] as Long, a[4] as String, a[5] as List<String>?, a[6] as Rewind?, a[7] as QueueState, a[8] as String, a[9] as String) },
            { v -> arrayOf(v.seq, v.id, v.end, v.atMs, v.text, v.attachments, v.rewind, v.state, v.said, v.from) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class QueueList(
    val items: List<QueueItem>,
    val sent: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QueueList> = obj(
            listOf(
            Field("items", { arr(QueueItem.ADAPTER) }, optional = false, nullable = false),
            Field("sent", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> QueueList(a[0] as List<QueueItem>, a[1] as List<String>) },
            { v -> arrayOf(v.items, v.sent) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class QueuePut(
    val outcome: QueuePutOutcome,
    val seq: Long? = null,
    val place: Int? = null,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QueuePut> = obj(
            listOf(
            Field("outcome", { QueuePutOutcome.ADAPTER }, optional = false, nullable = false),
            Field("seq", { opt(U64) }, optional = true, nullable = true),
            Field("place", { opt(Int32) }, optional = true, nullable = true),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> QueuePut(a[0] as QueuePutOutcome, a[1] as Long?, a[2] as Int?, a[3] as String) },
            { v -> arrayOf(v.outcome, v.seq, v.place, v.said) },
        )
    }
}

enum class QueuePutOutcome(val wire: String) {
    QUEUED("queued"),
    HELD("held"),
    SEEN("seen"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<QueuePutOutcome> = enumOf(entries) { it.wire }
    }
}

enum class QueueState(val wire: String) {
    QUEUED("queued"),
    HELD("held"),
    UNDELIVERED("undelivered"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<QueueState> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class QuietUnread(
    val text: String,
    val count: Long,
    val kinds: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuietUnread> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            Field("count", { Lng }, optional = false, nullable = false),
            Field("kinds", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> QuietUnread(a[0] as String, a[1] as Long, a[2] as List<String>) },
            { v -> arrayOf(v.text, v.count, v.kinds) },
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
            Field("seenAt", { U64 }, optional = false, nullable = false),
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
data class QuotaAccountHead(
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
        val ADAPTER: JsonAdapter<QuotaAccountHead> = obj(
            listOf(
            Field("agent", { Str }, optional = false, nullable = false),
            Field("account", { Str }, optional = false, nullable = false),
            Field("seenAt", { U64 }, optional = false, nullable = false),
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
            { a -> QuotaAccountHead(a[0] as String, a[1] as String, a[2] as Long, a[3] as String, a[4] as String?, a[5] as QuotaReadingShown, a[6] as Map<String, QuotaWindowSeen>?, a[7] as QuotaKind, a[8] as QuotaState, a[9] as Boolean, a[10] as String?, a[11] as List<SlotShow>, a[12] as LoginState, a[13] as String?, a[14] as List<WindowShow>?, a[15] as Usage) },
            { v -> arrayOf(v.agent, v.account, v.seenAt, v.seenAtText, v.seenAtRelText, v.reading, v.windowsSeen, v.kind, v.state, v.stale, v.limiting, v.slots, v.login, v.subId, v.windows, v.usage) },
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
            Field("resetsAt", { opt(U64) }, optional = true, nullable = true),
            Field("resetsAtText", { opt(Str) }, optional = true, nullable = true),
            Field("resetsAtRelText", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> QuotaOverageShown(a[0] as QuotaStatus?, a[1] as String?, a[2] as Boolean, a[3] as Long?, a[4] as String?, a[5] as String?) },
            { v -> arrayOf(v.status, v.disabled, v.inUse, v.resetsAt, v.resetsAtText, v.resetsAtRelText) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaProbeReq(
    val agent: String,
    val account: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaProbeReq> = obj(
            listOf(
            Field("agent", { Str }, optional = false, nullable = false),
            Field("account", { Str }, optional = false, nullable = false),
            ),
            { a -> QuotaProbeReq(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.agent, v.account) },
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
            Field("now", { U64 }, optional = false, nullable = false),
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

data object QuotaReadReq {
    val ADAPTER: JsonAdapter<QuotaReadReq> = obj(emptyList(), { QuotaReadReq }, { emptyArray() })
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
            Field("resetsAt", { opt(U64) }, optional = true, nullable = true),
            Field("resetsAtText", { opt(Str) }, optional = true, nullable = true),
            Field("resetsAtRelText", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> QuotaReadingShown(a[0] as QuotaStatus?, a[1] as Boolean, a[2] as String?, a[3] as List<QuotaWindowRead>, a[4] as QuotaOverageShown?, a[5] as Long?, a[6] as String?, a[7] as String?) },
            { v -> arrayOf(v.status, v.refused, v.limiting, v.windows, v.overage, v.resetsAt, v.resetsAtText, v.resetsAtRelText) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaResetsAt(
    val resetsAt: Long? = null,
    val resetsAtText: String? = null,
    val resetsAtRelText: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaResetsAt> = obj(
            listOf(
            Field("resetsAt", { opt(U64) }, optional = true, nullable = true),
            Field("resetsAtText", { opt(Str) }, optional = true, nullable = true),
            Field("resetsAtRelText", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> QuotaResetsAt(a[0] as Long?, a[1] as String?, a[2] as String?) },
            { v -> arrayOf(v.resetsAt, v.resetsAtText, v.resetsAtRelText) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaShow(
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
        val ADAPTER: JsonAdapter<QuotaShow> = obj(
            listOf(
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
            { a -> QuotaShow(a[0] as QuotaKind, a[1] as QuotaState, a[2] as Boolean, a[3] as String?, a[4] as List<SlotShow>, a[5] as LoginState, a[6] as String?, a[7] as List<WindowShow>?, a[8] as Usage) },
            { v -> arrayOf(v.kind, v.state, v.stale, v.limiting, v.slots, v.login, v.subId, v.windows, v.usage) },
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
data class QuotaUnseenHead(
    val agent: String,
    val account: String,
    val kind: QuotaKind,
    val login: LoginState,
    val subId: String? = null,
    val usage: QuotaUnseenUsage,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaUnseenHead> = obj(
            listOf(
            Field("agent", { Str }, optional = false, nullable = false),
            Field("account", { Str }, optional = false, nullable = false),
            Field("kind", { QuotaKind.ADAPTER }, optional = false, nullable = false),
            Field("login", { LoginState.ADAPTER }, optional = false, nullable = false),
            Field("subId", { opt(Str) }, optional = true, nullable = true),
            Field("usage", { QuotaUnseenUsage.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> QuotaUnseenHead(a[0] as String, a[1] as String, a[2] as QuotaKind, a[3] as LoginState, a[4] as String?, a[5] as QuotaUnseenUsage) },
            { v -> arrayOf(v.agent, v.account, v.kind, v.login, v.subId, v.usage) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaUnseenUsage(
    val slot: String? = null,
    val window: String? = null,
    val value: String,
    val text: String,
    val tone: Tone,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaUnseenUsage> = obj(
            listOf(
            Field("slot", { opt(Str) }, optional = true, nullable = true),
            Field("window", { opt(Str) }, optional = true, nullable = true),
            Field("value", { Str }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("tone", { Tone.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> QuotaUnseenUsage(a[0] as String?, a[1] as String?, a[2] as String, a[3] as String, a[4] as Tone) },
            { v -> arrayOf(v.slot, v.window, v.value, v.text, v.tone) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class QuotaVerdict(
    val text: String,
    val sub: String?,
    val tone: Tone,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaVerdict> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            Field("sub", { opt(Str) }, optional = false, nullable = true),
            Field("tone", { Tone.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> QuotaVerdict(a[0] as String, a[1] as String?, a[2] as Tone) },
            { v -> arrayOf(v.text, v.sub, v.tone) },
        )
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
data class QuotaWindow(
    val name: String,
    val used: Long? = null,
    val resetsAt: Long? = null,
    val warnedAt: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<QuotaWindow> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("used", { opt(Lng) }, optional = true, nullable = true),
            Field("resetsAt", { opt(Lng) }, optional = true, nullable = true),
            Field("warnedAt", { opt(Lng) }, optional = true, nullable = true),
            ),
            { a -> QuotaWindow(a[0] as String, a[1] as Long?, a[2] as Long?, a[3] as Long?) },
            { v -> arrayOf(v.name, v.used, v.resetsAt, v.warnedAt) },
        )
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
            Field("resetsAt", { opt(U64) }, optional = true, nullable = true),
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
            Field("at", { U64 }, optional = false, nullable = false),
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
data class ReachRow(
    val origin: String,
    val machine: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ReachRow> = obj(
            listOf(
            Field("origin", { Str }, optional = false, nullable = false),
            Field("machine", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> ReachRow(a[0] as String, a[1] as String?) },
            { v -> arrayOf(v.origin, v.machine) },
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
            Field("offset", { U64 }, optional = false, nullable = false),
            Field("size", { U64 }, optional = false, nullable = false),
            Field("eof", { Bool }, optional = false, nullable = false),
            Field("content", { HexBytes.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> ReadChunkReply(a[0] as WirePath, a[1] as Long, a[2] as Long, a[3] as Boolean, a[4] as HexBytes) },
            { v -> arrayOf(v.path, v.offset, v.size, v.eof, v.content) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ReadChunkReq(
    val path: WirePath,
    val offset: Long,
    val len: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ReadChunkReq> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("offset", { U64 }, optional = false, nullable = false),
            Field("len", { U64 }, optional = false, nullable = false),
            ),
            { a -> ReadChunkReq(a[0] as WirePath, a[1] as Long, a[2] as Long) },
            { v -> arrayOf(v.path, v.offset, v.len) },
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
            Field("end", { opt(U64) }, optional = false, nullable = true),
            Field("hash", { U64 }, optional = false, nullable = false),
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
            Field("bytes", { U64 }, optional = false, nullable = false),
            Field("sha256", { Str }, optional = false, nullable = false),
            ),
            { a -> ReadTextReply(a[0] as WirePath, a[1] as String, a[2] as Long, a[3] as String) },
            { v -> arrayOf(v.path, v.text, v.bytes, v.sha256) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ReadTextReq(
    val path: WirePath,
    val maxBytes: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ReadTextReq> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("max_bytes", { U64 }, optional = false, nullable = false),
            ),
            { a -> ReadTextReq(a[0] as WirePath, a[1] as Long) },
            { v -> arrayOf(v.path, v.maxBytes) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RebuildReply(
    val path: WirePath,
    val entries: Long,
    val residentBytes: Long,
    val unreadableDirs: Long,
    val unreadablePaths: List<WirePath>,
    val truncated: Boolean,
    val skippedMounts: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RebuildReply> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("entries", { U64 }, optional = false, nullable = false),
            Field("resident_bytes", { U64 }, optional = false, nullable = false),
            Field("unreadable_dirs", { U64 }, optional = false, nullable = false),
            Field("unreadable_paths", { arr(WirePath.ADAPTER) }, optional = false, nullable = false),
            Field("truncated", { Bool }, optional = false, nullable = false),
            Field("skipped_mounts", { U64 }, optional = false, nullable = false),
            ),
            { a -> RebuildReply(a[0] as WirePath, a[1] as Long, a[2] as Long, a[3] as Long, a[4] as List<WirePath>, a[5] as Boolean, a[6] as Long) },
            { v -> arrayOf(v.path, v.entries, v.residentBytes, v.unreadableDirs, v.unreadablePaths, v.truncated, v.skippedMounts) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RebuildReq(
    val path: WirePath? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RebuildReq> = obj(
            listOf(
            Field("path", { opt(WirePath.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> RebuildReq(a[0] as WirePath?) },
            { v -> arrayOf(v.path) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RecentDir(
    val cwd: String,
    val shown: String,
    val lastMs: Long,
    val lastText: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RecentDir> = obj(
            listOf(
            Field("cwd", { Str }, optional = false, nullable = false),
            Field("shown", { Str }, optional = false, nullable = false),
            Field("lastMs", { Lng }, optional = false, nullable = false),
            Field("lastText", { Str }, optional = false, nullable = false),
            ),
            { a -> RecentDir(a[0] as String, a[1] as String, a[2] as Long, a[3] as String) },
            { v -> arrayOf(v.cwd, v.shown, v.lastMs, v.lastText) },
        )
    }
}

enum class RecordClass(val wire: String) {
    SAID("said"),
    REPLY("reply"),
    RETRY("retry"),
    TITLE("title"),
    UNREAD("unread"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<RecordClass> = enumOf(entries) { it.wire }
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
            Field("seq", { U64 }, optional = false, nullable = false),
            Field("end", { opt(U64) }, optional = false, nullable = true),
            Field("cwd", { opt(Str) }, optional = false, nullable = true),
            Field("record", { LineRecord.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> RecordLine(a[0] as String, a[1] as String, a[2] as Long, a[3] as Long?, a[4] as String?, a[5] as LineRecord) },
            { v -> arrayOf(v.sessionId, v.path, v.seq, v.end, v.cwd, v.record) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RecordProbe(
    val present: Boolean,
    val root: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RecordProbe> = obj(
            listOf(
            Field("present", { Bool }, optional = false, nullable = false),
            Field("root", { Str }, optional = false, nullable = false),
            ),
            { a -> RecordProbe(a[0] as Boolean, a[1] as String) },
            { v -> arrayOf(v.present, v.root) },
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
    HELD_ELSEWHERE("held_elsewhere"),
    HOST_OUTDATED("host_outdated"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<Refused> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class RemoteHealthPayload(
    val origin: String,
    val kind: String,
    val message: String,
    val detail: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RemoteHealthPayload> = obj(
            listOf(
            Field("origin", { Str }, optional = false, nullable = false),
            Field("kind", { Str }, optional = false, nullable = false),
            Field("message", { Str }, optional = false, nullable = false),
            Field("detail", { Str }, optional = false, nullable = false),
            ),
            { a -> RemoteHealthPayload(a[0] as String, a[1] as String, a[2] as String, a[3] as String) },
            { v -> arrayOf(v.origin, v.kind, v.message, v.detail) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ReplyError(
    val reason: ApiReason,
    val status: Int? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ReplyError> = obj(
            listOf(
            Field("reason", { ApiReason.ADAPTER }, optional = false, nullable = false),
            Field("status", { opt(Int32) }, optional = true, nullable = true),
            ),
            { a -> ReplyError(a[0] as ApiReason, a[1] as Int?) },
            { v -> arrayOf(v.reason, v.status) },
        )
    }
}

enum class RereadWhy(val wire: String) {
    TRUNCATED("truncated"),
    REWRITTEN("rewritten"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<RereadWhy> = enumOf(entries) { it.wire }
    }
}

enum class RestartHint(val wire: String) {
    NONE("none"),
    NEEDS_RESTART("needs_restart"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<RestartHint> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class RestartReq(
    val sid: String,
    val cwd: String,
    val account: String,
    val compactFirst: Boolean,
    val compactWithinMs: Long,
    val arriveWithinMs: Long,
    val local: Boolean,
    val agent: String,
    val client: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RestartReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("cwd", { Str }, optional = false, nullable = false),
            Field("account", { Str }, optional = false, nullable = false),
            Field("compact_first", { Bool }, optional = false, nullable = false),
            Field("compact_within_ms", { Lng }, optional = false, nullable = false),
            Field("arrive_within_ms", { Lng }, optional = false, nullable = false),
            Field("local", { Bool }, optional = false, nullable = false),
            Field("agent", { Str }, optional = false, nullable = false),
            Field("client", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> RestartReq(a[0] as String, a[1] as String, a[2] as String, a[3] as Boolean, a[4] as Long, a[5] as Long, a[6] as Boolean, a[7] as String, a[8] as String?) },
            { v -> arrayOf(v.sid, v.cwd, v.account, v.compactFirst, v.compactWithinMs, v.arriveWithinMs, v.local, v.agent, v.client) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Restarted(
    val compact: Compact,
    val started: Arrival,
    val terminal: String,
    val account: LaunchedAccount?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Restarted> = obj(
            listOf(
            Field("compact", { Compact.ADAPTER }, optional = false, nullable = false),
            Field("started", { Arrival.ADAPTER }, optional = false, nullable = false),
            Field("terminal", { Str }, optional = false, nullable = false),
            Field("account", { opt(LaunchedAccount.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> Restarted(a[0] as Compact, a[1] as Arrival, a[2] as String, a[3] as LaunchedAccount?) },
            { v -> arrayOf(v.compact, v.started, v.terminal, v.account) },
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

enum class ResumeWhere(val wire: String) {
    MANAGED("managed"),
    TMUX("tmux"),
    AS_STARTED("as_started"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ResumeWhere> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class ResumeWith(
    val model: String? = null,
    val permissionMode: String? = null,
    val end: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ResumeWith> = obj(
            listOf(
            Field("model", { opt(Str) }, optional = true, nullable = true),
            Field("permissionMode", { opt(Str) }, optional = true, nullable = true),
            Field("end", { U64 }, optional = false, nullable = false),
            ),
            { a -> ResumeWith(a[0] as String?, a[1] as String?, a[2] as Long) },
            { v -> arrayOf(v.model, v.permissionMode, v.end) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ResyncReq(
    val sid: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ResyncReq> = obj(
            listOf(
            Field("sid", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> ResyncReq(a[0] as String?) },
            { v -> arrayOf(v.sid) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Resynced(
    val added: Long,
    val removed: Long,
    val retagged: Long,
    val caughtUp: Long,
    val watchers: Long,
    val unavailable: List<Unavailable>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Resynced> = obj(
            listOf(
            Field("added", { Lng }, optional = false, nullable = false),
            Field("removed", { Lng }, optional = false, nullable = false),
            Field("retagged", { Lng }, optional = false, nullable = false),
            Field("caught_up", { Lng }, optional = false, nullable = false),
            Field("watchers", { Lng }, optional = false, nullable = false),
            Field("unavailable", { arr(Unavailable.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> Resynced(a[0] as Long, a[1] as Long, a[2] as Long, a[3] as Long, a[4] as Long, a[5] as List<Unavailable>) },
            { v -> arrayOf(v.added, v.removed, v.retagged, v.caughtUp, v.watchers, v.unavailable) },
        )
    }
}

enum class RetryOutcome(val wire: String) {
    RETRYING("retrying"),
    RECOVERED("recovered"),
    FAILED("failed"),
    INTERRUPTED("interrupted"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<RetryOutcome> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class RetryRun(
    val id: String,
    val outcome: RetryOutcome,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RetryRun> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("outcome", { RetryOutcome.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> RetryRun(a[0] as String, a[1] as RetryOutcome) },
            { v -> arrayOf(v.id, v.outcome) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Rewind(
    val to: String,
    val how: RewindHow,
    val `when`: RewindWhen,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Rewind> = obj(
            listOf(
            Field("to", { Str }, optional = false, nullable = false),
            Field("how", { RewindHow.ADAPTER }, optional = false, nullable = false),
            Field("when", { RewindWhen.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> Rewind(a[0] as String, a[1] as RewindHow, a[2] as RewindWhen) },
            { v -> arrayOf(v.to, v.how, v.`when`) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RewindChoiceRow(
    val how: RewindHow,
    val text: String,
    val default: Boolean,
    val enabled: Boolean,
    val why: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RewindChoiceRow> = obj(
            listOf(
            Field("how", { RewindHow.ADAPTER }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("default", { Bool }, optional = false, nullable = false),
            Field("enabled", { Bool }, optional = false, nullable = false),
            Field("why", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> RewindChoiceRow(a[0] as RewindHow, a[1] as String, a[2] as Boolean, a[3] as Boolean, a[4] as String?) },
            { v -> arrayOf(v.how, v.text, v.default, v.enabled, v.why) },
        )
    }
}

enum class RewindHow(val wire: String) {
    CONVERSATION("conversation"),
    CODE_AND_CONVERSATION("code_and_conversation"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<RewindHow> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class RewindOptions(
    val how: List<RewindChoiceRow>,
    val `when`: List<RewindWhenRow>,
    val said: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RewindOptions> = obj(
            listOf(
            Field("how", { arr(RewindChoiceRow.ADAPTER) }, optional = false, nullable = false),
            Field("when", { arr(RewindWhenRow.ADAPTER) }, optional = false, nullable = false),
            Field("said", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> RewindOptions(a[0] as List<RewindChoiceRow>, a[1] as List<RewindWhenRow>, a[2] as String?) },
            { v -> arrayOf(v.how, v.`when`, v.said) },
        )
    }
}

enum class RewindWhen(val wire: String) {
    AFTER_TURN("after_turn"),
    NOW("now"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<RewindWhen> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class RewindWhenRow(
    val `when`: RewindWhen,
    val text: String,
    val default: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RewindWhenRow> = obj(
            listOf(
            Field("when", { RewindWhen.ADAPTER }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("default", { Bool }, optional = false, nullable = false),
            ),
            { a -> RewindWhenRow(a[0] as RewindWhen, a[1] as String, a[2] as Boolean) },
            { v -> arrayOf(v.`when`, v.text, v.default) },
        )
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
data class RotBadCell(
    val cell: String,
    val code: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotBadCell> = obj(
            listOf(
            Field("cell", { Str }, optional = false, nullable = false),
            Field("code", { Str }, optional = false, nullable = false),
            ),
            { a -> RotBadCell(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.cell, v.code) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotBadStrategy(
    val line: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotBadStrategy> = obj(
            listOf(
            Field("line", { opt(Lng) }, optional = true, nullable = true),
            ),
            { a -> RotBadStrategy(a[0] as Long?) },
            { v -> arrayOf(v.line) },
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
            Field("from", { U64 }, optional = false, nullable = false),
            Field("to", { U64 }, optional = false, nullable = false),
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

sealed interface RotEdit {
    data class Set(
        val value: RotEditSetValue,
    ) : RotEdit
    data class Seg(
        val value: RotEditSegValue,
    ) : RotEdit
    data class Same(
        val value: RotEditSameValue,
    ) : RotEdit
    data class Move(
        val value: RotEditMoveValue,
    ) : RotEdit
    data class Add(
        val value: RotEditAddValue,
    ) : RotEdit
    data class Remove(
        val value: RotEditRemoveValue,
    ) : RotEdit

    companion object {
        val ADAPTER: JsonAdapter<RotEdit> =
            external<RotEdit>(
                listOf(
                Ext("set", RotEdit.Set::class.java, { RotEditSetValue.ADAPTER }, { RotEdit.Set(it as RotEditSetValue) }, { (it as RotEdit.Set).value }),
                Ext("seg", RotEdit.Seg::class.java, { RotEditSegValue.ADAPTER }, { RotEdit.Seg(it as RotEditSegValue) }, { (it as RotEdit.Seg).value }),
                Ext("same", RotEdit.Same::class.java, { RotEditSameValue.ADAPTER }, { RotEdit.Same(it as RotEditSameValue) }, { (it as RotEdit.Same).value }),
                Ext("move", RotEdit.Move::class.java, { RotEditMoveValue.ADAPTER }, { RotEdit.Move(it as RotEditMoveValue) }, { (it as RotEdit.Move).value }),
                Ext("add", RotEdit.Add::class.java, { RotEditAddValue.ADAPTER }, { RotEdit.Add(it as RotEditAddValue) }, { (it as RotEdit.Add).value }),
                Ext("remove", RotEdit.Remove::class.java, { RotEditRemoveValue.ADAPTER }, { RotEdit.Remove(it as RotEditRemoveValue) }, { (it as RotEdit.Remove).value }),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotEditAddValue(
    val account: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotEditAddValue> = obj(
            listOf(
            Field("account", { Str }, optional = false, nullable = false),
            ),
            { a -> RotEditAddValue(a[0] as String) },
            { v -> arrayOf(v.account) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotEditMoveValue(
    val from: Long,
    val to: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotEditMoveValue> = obj(
            listOf(
            Field("from", { Lng }, optional = false, nullable = false),
            Field("to", { Lng }, optional = false, nullable = false),
            ),
            { a -> RotEditMoveValue(a[0] as Long, a[1] as Long) },
            { v -> arrayOf(v.from, v.to) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotEditRemoveValue(
    val row: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotEditRemoveValue> = obj(
            listOf(
            Field("row", { Lng }, optional = false, nullable = false),
            ),
            { a -> RotEditRemoveValue(a[0] as Long) },
            { v -> arrayOf(v.row) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotEditSameValue(
    val key: String,
    val row: Long? = null,
    val window: String,
    val same: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotEditSameValue> = obj(
            listOf(
            Field("key", { Str }, optional = false, nullable = false),
            Field("row", { opt(Lng) }, optional = true, nullable = true),
            Field("window", { Str }, optional = false, nullable = false),
            Field("same", { Bool }, optional = false, nullable = false),
            ),
            { a -> RotEditSameValue(a[0] as String, a[1] as Long?, a[2] as String, a[3] as Boolean) },
            { v -> arrayOf(v.key, v.row, v.window, v.same) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotEditSegValue(
    val key: String,
    val row: Long? = null,
    val window: String,
    val at: Long? = null,
    val seg: RotSeg? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotEditSegValue> = obj(
            listOf(
            Field("key", { Str }, optional = false, nullable = false),
            Field("row", { opt(Lng) }, optional = true, nullable = true),
            Field("window", { Str }, optional = false, nullable = false),
            Field("at", { opt(Lng) }, optional = true, nullable = true),
            Field("seg", { opt(RotSeg.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> RotEditSegValue(a[0] as String, a[1] as Long?, a[2] as String, a[3] as Long?, a[4] as RotSeg?) },
            { v -> arrayOf(v.key, v.row, v.window, v.at, v.seg) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotEditSetValue(
    val key: String,
    val row: Long? = null,
    val v: Raw,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotEditSetValue> = obj(
            listOf(
            Field("key", { Str }, optional = false, nullable = false),
            Field("row", { opt(Lng) }, optional = true, nullable = true),
            Field("v", { RawJson }, optional = false, nullable = false),
            ),
            { a -> RotEditSetValue(a[0] as String, a[1] as Long?, a[2] as Raw) },
            { v -> arrayOf(v.key, v.row, v.v) },
        )
    }
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

enum class RotGroup(val wire: String) {
    USE("use"),
    NEXT("next"),
    OUT("out"),
    OFF("off"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<RotGroup> = enumOf(entries) { it.wire }
    }
}

sealed interface RotHint {
    /** 判别格（`k`）那个词。 */
    val k: String

    @Suppress("UNCHECKED_CAST")
    data class Hold(
        val until: Long? = null,
        val account: String? = null,
        val untilText: String? = null,
    ) : RotHint, Ordered {
        override val k: String get() = "hold"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class StrategyError(
        val error: RotStrategyError,
        val path: String? = null,
    ) : RotHint, Ordered {
        override val k: String get() = "strategyError"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class TzUnknown(
        val zone: String,
    ) : RotHint, Ordered {
        override val k: String get() = "tzUnknown"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class OldScripts(
        val n: Long,
        val files: List<RotOldFile>,
    ) : RotHint, Ordered {
        override val k: String get() = "oldScripts"
        override var wireOrder: IntArray? = null
    }
    data object Newer : RotHint {
        override val k: String get() = "newer"
    }
    @Suppress("UNCHECKED_CAST")
    data class Older(
        val n: Long,
    ) : RotHint, Ordered {
        override val k: String get() = "older"
        override var wireOrder: IntArray? = null
    }
    data object WordsBad : RotHint {
        override val k: String get() = "wordsBad"
    }
    @Suppress("UNCHECKED_CAST")
    data class BookUnreadable(
        val text: String,
    ) : RotHint, Ordered {
        override val k: String get() = "bookUnreadable"
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
                        Field("until", { opt(U64) }, optional = true, nullable = true),
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

@Suppress("UNCHECKED_CAST")
data class RotInUse(
    val sids: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotInUse> = obj(
            listOf(
            Field("sids", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> RotInUse(a[0] as List<String>) },
            { v -> arrayOf(v.sids) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotLine(
    val account: String,
    val group: RotGroup,
    val state: String,
    val why: String? = null,
    val movable: Boolean,
    val stay: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotLine> = obj(
            listOf(
            Field("account", { Str }, optional = false, nullable = false),
            Field("group", { RotGroup.ADAPTER }, optional = false, nullable = false),
            Field("state", { Str }, optional = false, nullable = false),
            Field("why", { opt(Str) }, optional = true, nullable = true),
            Field("movable", { Bool }, optional = false, nullable = false),
            Field("stay", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> RotLine(a[0] as String, a[1] as RotGroup, a[2] as String, a[3] as String?, a[4] as Boolean, a[5] as String?) },
            { v -> arrayOf(v.account, v.group, v.state, v.why, v.movable, v.stay) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotLog(
    val at: Long,
    val from: String,
    val to: String,
    val cause: RotLogCause,
    val why: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotLog> = obj(
            listOf(
            Field("at", { U64 }, optional = false, nullable = false),
            Field("from", { Str }, optional = false, nullable = false),
            Field("to", { Str }, optional = false, nullable = false),
            Field("cause", { RotLogCause.ADAPTER }, optional = false, nullable = false),
            Field("why", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> RotLog(a[0] as Long, a[1] as String, a[2] as String, a[3] as RotLogCause, a[4] as String?) },
            { v -> arrayOf(v.at, v.from, v.to, v.cause, v.why) },
        )
    }
}

sealed interface RotLogCause {
    /** 判别格（`k`）那个词。 */
    val k: String

    data object Order : RotLogCause {
        override val k: String get() = "order"
    }
    data object Unusable : RotLogCause {
        override val k: String get() = "unusable"
    }
    data object Temp : RotLogCause {
        override val k: String get() = "temp"
    }
    data object UseNow : RotLogCause {
        override val k: String get() = "useNow"
    }
    @Suppress("UNCHECKED_CAST")
    data class Hold(
        val until: Long? = null,
    ) : RotLogCause, Ordered {
        override val k: String get() = "hold"
        override var wireOrder: IntArray? = null
    }
    data object Error : RotLogCause {
        override val k: String get() = "error"
    }

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
                        Field("until", { opt(U64) }, optional = true, nullable = true),
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
            Field("at", { U64 }, optional = false, nullable = false),
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
data class RotPlate(
    val color: Int,
    val label: String,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotPlate> = obj(
            listOf(
            Field("color", { Int32 }, optional = false, nullable = false),
            Field("label", { Str }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> RotPlate(a[0] as Int, a[1] as String, a[2] as String) },
            { v -> arrayOf(v.color, v.label, v.text) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotRefused(
    val cells: List<RotBadCell>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotRefused> = obj(
            listOf(
            Field("cells", { arr(RotBadCell.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> RotRefused(a[0] as List<RotBadCell>) },
            { v -> arrayOf(v.cells) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotRow(
    val account: String,
    val row: Long,
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
            Field("row", { Lng }, optional = false, nullable = false),
            Field("line", { Str }, optional = false, nullable = false),
            Field("hover", { Str }, optional = false, nullable = false),
            Field("cells", { arr(RotCell.ADAPTER) }, optional = false, nullable = false),
            Field("rules", { arr(Str) }, optional = false, nullable = false),
            Field("marks", { arr(RotMark.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> RotRow(a[0] as String, a[1] as Long, a[2] as String, a[3] as String, a[4] as List<RotCell>, a[5] as List<String>, a[6] as List<RotMark>) },
            { v -> arrayOf(v.account, v.row, v.line, v.hover, v.cells, v.rules, v.marks) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotSeg(
    val from: String,
    val to: String,
    val v: Double,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotSeg> = obj(
            listOf(
            Field("from", { Str }, optional = false, nullable = false),
            Field("to", { Str }, optional = false, nullable = false),
            Field("v", { Dbl }, optional = false, nullable = false),
            ),
            { a -> RotSeg(a[0] as String, a[1] as String, a[2] as Double) },
            { v -> arrayOf(v.from, v.to, v.v) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotStale(
    val rev: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotStale> = obj(
            listOf(
            Field("rev", { Str }, optional = false, nullable = false),
            ),
            { a -> RotStale(a[0] as String) },
            { v -> arrayOf(v.rev) },
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

sealed interface RotTarget {
    data class Sid(
        val value: String,
    ) : RotTarget
    data class Strategy(
        val value: String,
    ) : RotTarget

    companion object {
        val ADAPTER: JsonAdapter<RotTarget> =
            external<RotTarget>(
                listOf(
                Ext("sid", RotTarget.Sid::class.java, { Str }, { RotTarget.Sid(it as String) }, { (it as RotTarget.Sid).value }),
                Ext("strategy", RotTarget.Strategy::class.java, { Str }, { RotTarget.Strategy(it as String) }, { (it as RotTarget.Strategy).value }),
                ),
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
            Field("holdUntil", { opt(U64) }, optional = true, nullable = true),
            Field("holdAccount", { opt(Str) }, optional = true, nullable = true),
            Field("order", { arr(Str) }, optional = false, nullable = false),
            Field("why", { arr(RotTryRow.ADAPTER) }, optional = false, nullable = false),
            Field("fromTemp", { Bool }, optional = false, nullable = false),
            Field("skipped", { arr(Str) }, optional = false, nullable = false),
            Field("temp", { arr(Str) }, optional = false, nullable = false),
            Field("nextNode", { opt(U64) }, optional = true, nullable = true),
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
    val segs: List<RotTrySeg>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotTryLane> = obj(
            listOf(
            Field("account", { Str }, optional = false, nullable = false),
            Field("segs", { arr(RotTrySeg.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> RotTryLane(a[0] as String, a[1] as List<RotTrySeg>) },
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
data class RotTrySeg(
    val from: Long,
    val to: Long,
    val sendable: Boolean,
    val reset: Boolean,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotTrySeg> = obj(
            listOf(
            Field("from", { U64 }, optional = false, nullable = false),
            Field("to", { U64 }, optional = false, nullable = false),
            Field("sendable", { Bool }, optional = false, nullable = false),
            Field("reset", { Bool }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> RotTrySeg(a[0] as Long, a[1] as Long, a[2] as Boolean, a[3] as Boolean, a[4] as String) },
            { v -> arrayOf(v.from, v.to, v.sendable, v.reset, v.text) },
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
            Field("at", { U64 }, optional = false, nullable = false),
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
            Field("at", { U64 }, optional = false, nullable = false),
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
            Field("from", { U64 }, optional = false, nullable = false),
            Field("to", { U64 }, optional = false, nullable = false),
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
data class RotUnusable(
    val why: FactWhy? = null,
    val until: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotUnusable> = obj(
            listOf(
            Field("why", { opt(FactWhy.ADAPTER) }, optional = true, nullable = true),
            Field("until", { opt(U64) }, optional = true, nullable = true),
            ),
            { a -> RotUnusable(a[0] as FactWhy?, a[1] as Long?) },
            { v -> arrayOf(v.why, v.until) },
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
            Field("now", { U64 }, optional = false, nullable = false),
            Field("accounts", { arr(Fact.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> RotationFactsReply(a[0] as Long, a[1] as List<Fact>) },
            { v -> arrayOf(v.now, v.accounts) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotationFactsReq(
    val agent: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationFactsReq> = obj(
            listOf(
            Field("agent", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> RotationFactsReq(a[0] as String?) },
            { v -> arrayOf(v.agent) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotationForNewReq(
    val strategy: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationForNewReq> = obj(
            listOf(
            Field("strategy", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> RotationForNewReq(a[0] as String?) },
            { v -> arrayOf(v.strategy) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotationPick(
    val strategy: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationPick> = obj(
            listOf(
            Field("strategy", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> RotationPick(a[0] as String?) },
            { v -> arrayOf(v.strategy) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotationReloadReq(
    val strategy: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationReloadReq> = obj(
            listOf(
            Field("strategy", { Str }, optional = false, nullable = false),
            ),
            { a -> RotationReloadReq(a[0] as String) },
            { v -> arrayOf(v.strategy) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotationSettingsReq(
    val strategy: String,
    val edit: RotEdit,
    val ifRev: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationSettingsReq> = obj(
            listOf(
            Field("strategy", { Str }, optional = false, nullable = false),
            Field("edit", { RotEdit.ADAPTER }, optional = false, nullable = false),
            Field("ifRev", { Str }, optional = false, nullable = false),
            ),
            { a -> RotationSettingsReq(a[0] as String, a[1] as RotEdit, a[2] as String) },
            { v -> arrayOf(v.strategy, v.edit, v.ifRev) },
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

data object RotationStrategiesReq {
    val ADAPTER: JsonAdapter<RotationStrategiesReq> = obj(emptyList(), { RotationStrategiesReq }, { emptyArray() })
}

@Suppress("UNCHECKED_CAST")
data class RotationStrategyAddReq(
    val source: String? = null,
    val name: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationStrategyAddReq> = obj(
            listOf(
            Field("source", { opt(Str) }, optional = true, nullable = true),
            Field("name", { Str }, optional = false, nullable = false),
            ),
            { a -> RotationStrategyAddReq(a[0] as String?, a[1] as String) },
            { v -> arrayOf(v.source, v.name) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotationStrategyCopyReq(
    val id: String,
    val name: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationStrategyCopyReq> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            ),
            { a -> RotationStrategyCopyReq(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.id, v.name) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotationStrategyDeleteReq(
    val id: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationStrategyDeleteReq> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            ),
            { a -> RotationStrategyDeleteReq(a[0] as String) },
            { v -> arrayOf(v.id) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotationStrategyRenameReq(
    val id: String,
    val name: String,
    val ifRev: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationStrategyRenameReq> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("ifRev", { Str }, optional = false, nullable = false),
            ),
            { a -> RotationStrategyRenameReq(a[0] as String, a[1] as String, a[2] as String) },
            { v -> arrayOf(v.id, v.name, v.ifRev) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotationStrategyTzReq(
    val id: String,
    val tz: String,
    val ifRev: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationStrategyTzReq> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("tz", { Str }, optional = false, nullable = false),
            Field("ifRev", { Str }, optional = false, nullable = false),
            ),
            { a -> RotationStrategyTzReq(a[0] as String, a[1] as String, a[2] as String) },
            { v -> arrayOf(v.id, v.tz, v.ifRev) },
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
data class RotationTempReq(
    val sid: String,
    val op: Raw,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationTempReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("op", { RawJson }, optional = false, nullable = false),
            ),
            { a -> RotationTempReq(a[0] as String, a[1] as Raw) },
            { v -> arrayOf(v.sid, v.op) },
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
            Field("now", { U64 }, optional = false, nullable = false),
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
data class RotationTryReq(
    val source: String,
    val settings: Raw? = null,
    val start: String? = null,
    val current: String? = null,
    val temp: List<String>,
    val hours: Int? = null,
    val useNow: String? = null,
    val agent: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationTryReq> = obj(
            listOf(
            Field("source", { Str }, optional = false, nullable = false),
            Field("settings", { opt(RawJson) }, optional = true, nullable = true),
            Field("start", { opt(Str) }, optional = true, nullable = true),
            Field("current", { opt(Str) }, optional = true, nullable = true),
            Field("temp", { arr(Str) }, optional = false, nullable = false),
            Field("hours", { opt(Int32) }, optional = true, nullable = true),
            Field("useNow", { opt(Str) }, optional = true, nullable = true),
            Field("agent", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> RotationTryReq(a[0] as String, a[1] as Raw?, a[2] as String?, a[3] as String?, a[4] as List<String>, a[5] as Int?, a[6] as String?, a[7] as String?) },
            { v -> arrayOf(v.source, v.settings, v.start, v.current, v.temp, v.hours, v.useNow, v.agent) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotationUseNowReq(
    val sid: String,
    val account: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationUseNowReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("account", { Str }, optional = false, nullable = false),
            ),
            { a -> RotationUseNowReq(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.sid, v.account) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotationUseReq(
    val sid: String,
    val strategy: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationUseReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("strategy", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> RotationUseReq(a[0] as String, a[1] as String?) },
            { v -> arrayOf(v.sid, v.strategy) },
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
    val lineup: List<RotLine>,
    val manual: Boolean,
    val off: List<String>,
    val accounts: List<RotAccount>,
    val listed: Long,
    val decision: RotTryDecision?,
    val timeline: RotTryTimeline? = null,
    val log: List<RotLogRow>,
    val hints: List<RotNote>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationViewReply> = obj(
            listOf(
            Field("now", { U64 }, optional = false, nullable = false),
            Field("nowText", { Str }, optional = false, nullable = false),
            Field("title", { opt(Str) }, optional = false, nullable = true),
            Field("strategy", { opt(RotStrategyItem.ADAPTER) }, optional = false, nullable = true),
            Field("board", { RotBoard.ADAPTER }, optional = false, nullable = false),
            Field("start", { Str }, optional = false, nullable = false),
            Field("current", { Str }, optional = false, nullable = false),
            Field("lineup", { arr(RotLine.ADAPTER) }, optional = false, nullable = false),
            Field("manual", { Bool }, optional = false, nullable = false),
            Field("off", { arr(Str) }, optional = false, nullable = false),
            Field("accounts", { arr(RotAccount.ADAPTER) }, optional = false, nullable = false),
            Field("listed", { Lng }, optional = false, nullable = false),
            Field("decision", { opt(RotTryDecision.ADAPTER) }, optional = false, nullable = true),
            Field("timeline", { opt(RotTryTimeline.ADAPTER) }, optional = true, nullable = true),
            Field("log", { arr(RotLogRow.ADAPTER) }, optional = false, nullable = false),
            Field("hints", { arr(RotNote.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> RotationViewReply(a[0] as Long, a[1] as String, a[2] as String?, a[3] as RotStrategyItem?, a[4] as RotBoard, a[5] as String, a[6] as String, a[7] as List<RotLine>, a[8] as Boolean, a[9] as List<String>, a[10] as List<RotAccount>, a[11] as Long, a[12] as RotTryDecision?, a[13] as RotTryTimeline?, a[14] as List<RotLogRow>, a[15] as List<RotNote>) },
            { v -> arrayOf(v.now, v.nowText, v.title, v.strategy, v.board, v.start, v.current, v.lineup, v.manual, v.off, v.accounts, v.listed, v.decision, v.timeline, v.log, v.hints) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RotationViewReq(
    val target: RotTarget,
    val hours: Int? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RotationViewReq> = obj(
            listOf(
            Field("target", { RotTarget.ADAPTER }, optional = false, nullable = false),
            Field("hours", { opt(Int32) }, optional = true, nullable = true),
            ),
            { a -> RotationViewReq(a[0] as RotTarget, a[1] as Int?) },
            { v -> arrayOf(v.target, v.hours) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RowEdit(
    val cwd: String,
    val alias: String,
    val n: Long,
    val tabGroup: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RowEdit> = obj(
            listOf(
            Field("cwd", { Str }, optional = false, nullable = false),
            Field("alias", { Str }, optional = false, nullable = false),
            Field("n", { Lng }, optional = false, nullable = false),
            Field("tabGroup", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> RowEdit(a[0] as String, a[1] as String, a[2] as Long, a[3] as String?) },
            { v -> arrayOf(v.cwd, v.alias, v.n, v.tabGroup) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RowLaunched(
    val cwd: String,
    val alias: String,
    val group: GroupRef?,
    val opened: List<Opened>,
    val failed: List<LaunchFail>,
    val blocked: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RowLaunched> = obj(
            listOf(
            Field("cwd", { Str }, optional = false, nullable = false),
            Field("alias", { Str }, optional = false, nullable = false),
            Field("group", { opt(GroupRef.ADAPTER) }, optional = false, nullable = true),
            Field("opened", { arr(Opened.ADAPTER) }, optional = false, nullable = false),
            Field("failed", { arr(LaunchFail.ADAPTER) }, optional = false, nullable = false),
            Field("blocked", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> RowLaunched(a[0] as String, a[1] as String, a[2] as GroupRef?, a[3] as List<Opened>, a[4] as List<LaunchFail>, a[5] as String?) },
            { v -> arrayOf(v.cwd, v.alias, v.group, v.opened, v.failed, v.blocked) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RowPlan(
    val cwd: String,
    val alias: String,
    val want: Long,
    val have: Long,
    val more: Long,
    val group: String,
    val blocked: String?,
    val said: String,
    val haveSaid: String,
    val moreSaid: String,
    val canMore: Boolean,
    val moreWhy: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RowPlan> = obj(
            listOf(
            Field("cwd", { Str }, optional = false, nullable = false),
            Field("alias", { Str }, optional = false, nullable = false),
            Field("want", { Lng }, optional = false, nullable = false),
            Field("have", { Lng }, optional = false, nullable = false),
            Field("more", { Lng }, optional = false, nullable = false),
            Field("group", { Str }, optional = false, nullable = false),
            Field("blocked", { opt(Str) }, optional = false, nullable = true),
            Field("said", { Str }, optional = false, nullable = false),
            Field("haveSaid", { Str }, optional = false, nullable = false),
            Field("moreSaid", { Str }, optional = false, nullable = false),
            Field("canMore", { Bool }, optional = false, nullable = false),
            Field("moreWhy", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> RowPlan(a[0] as String, a[1] as String, a[2] as Long, a[3] as Long, a[4] as Long, a[5] as String, a[6] as String?, a[7] as String, a[8] as String, a[9] as String, a[10] as Boolean, a[11] as String?) },
            { v -> arrayOf(v.cwd, v.alias, v.want, v.have, v.more, v.group, v.blocked, v.said, v.haveSaid, v.moreSaid, v.canMore, v.moreWhy) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Rules(
    val childTab: Tab,
    val bgTab: Tab,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Rules> = obj(
            listOf(
            Field("childTab", { Tab.ADAPTER }, optional = false, nullable = false),
            Field("bgTab", { Tab.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> Rules(a[0] as Tab, a[1] as Tab) },
            { v -> arrayOf(v.childTab, v.bgTab) },
        )
    }
}

enum class Run(val wire: String) {
    MANAGED("managed"),
    TMUX("tmux"),
    DIRECT("direct"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<Run> = enumOf(entries) { it.wire }
    }
}

sealed interface RunDid {
    /** 判别格（`t`）那个词。 */
    val t: String

    data object Say : RunDid {
        override val t: String get() = "say"
    }
    data object Think : RunDid {
        override val t: String get() = "think"
    }
    @Suppress("UNCHECKED_CAST")
    data class Tool(
        val name: String,
    ) : RunDid, Ordered {
        override val t: String get() = "tool"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<RunDid> =
            tagged<RunDid>(
                "t",
                listOf(
                Variant("say", RunDid.Say::class.java, obj(
                        listOf(
                        Field("t", { tag("say") }, optional = false, nullable = false),
                        ),
                        { RunDid.Say },
                        { arrayOf("say") },
                    )),
                Variant("think", RunDid.Think::class.java, obj(
                        listOf(
                        Field("t", { tag("think") }, optional = false, nullable = false),
                        ),
                        { RunDid.Think },
                        { arrayOf("think") },
                    )),
                Variant("tool", RunDid.Tool::class.java, obj(
                        listOf(
                        Field("t", { tag("tool") }, optional = false, nullable = false),
                        Field("name", { Str }, optional = false, nullable = false),
                        ),
                        { a -> RunDid.Tool(a[1] as String) },
                        { v -> arrayOf("tool", v.name) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class RunEndLine(
    val head: String,
    val sub: RunLine,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RunEndLine> = obj(
            listOf(
            Field("head", { Str }, optional = false, nullable = false),
            Field("sub", { RunLine.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> RunEndLine(a[0] as String, a[1] as RunLine) },
            { v -> arrayOf(v.head, v.sub) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RunEnded(
    val run: String,
    val tool: String,
    val state: RunState,
    val statusText: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RunEnded> = obj(
            listOf(
            Field("run", { Str }, optional = false, nullable = false),
            Field("tool", { Str }, optional = false, nullable = false),
            Field("state", { RunState.ADAPTER }, optional = false, nullable = false),
            Field("status_text", { Str }, optional = false, nullable = false),
            ),
            { a -> RunEnded(a[0] as String, a[1] as String, a[2] as RunState, a[3] as String) },
            { v -> arrayOf(v.run, v.tool, v.state, v.statusText) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RunExplain(
    val lead: String? = null,
    val line: RunLine,
    val note: String? = null,
    val refresh: Boolean? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RunExplain> = obj(
            listOf(
            Field("lead", { opt(Str) }, optional = true, nullable = true),
            Field("line", { RunLine.ADAPTER }, optional = false, nullable = false),
            Field("note", { opt(Str) }, optional = true, nullable = true),
            Field("refresh", { opt(Bool) }, optional = true, nullable = true),
            ),
            { a -> RunExplain(a[0] as String?, a[1] as RunLine, a[2] as String?, a[3] as Boolean?) },
            { v -> arrayOf(v.lead, v.line, v.note, v.refresh) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RunGroups(
    val running: List<String>,
    val recent: List<String>,
    val older: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RunGroups> = obj(
            listOf(
            Field("running", { arr(Str) }, optional = false, nullable = false),
            Field("recent", { arr(Str) }, optional = false, nullable = false),
            Field("older", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> RunGroups(a[0] as List<String>, a[1] as List<String>, a[2] as List<String>) },
            { v -> arrayOf(v.running, v.recent, v.older) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RunInfo(
    val run: String,
    val label: String? = null,
    val kind: String? = null,
    val tool: String? = null,
    val parent: String? = null,
    val state: RunState,
    val statusText: String,
    val labelText: String,
    val last: RunDid? = null,
    val lastText: String? = null,
    val waiting: String? = null,
    val startedMs: Long? = null,
    val startedText: String? = null,
    val activeMs: Long? = null,
    val endedMs: Long? = null,
    val why: RunWhy? = null,
    val error: String? = null,
    val calls: Int? = null,
    val background: Boolean? = null,
    val facts: List<RunLine>? = null,
    val explain: RunExplain? = null,
    val end: RunEndLine? = null,
    val needs: Needs? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RunInfo> = obj(
            listOf(
            Field("run", { Str }, optional = false, nullable = false),
            Field("label", { opt(Str) }, optional = true, nullable = true),
            Field("kind", { opt(Str) }, optional = true, nullable = true),
            Field("tool", { opt(Str) }, optional = true, nullable = true),
            Field("parent", { opt(Str) }, optional = true, nullable = true),
            Field("state", { RunState.ADAPTER }, optional = false, nullable = false),
            Field("status_text", { Str }, optional = false, nullable = false),
            Field("label_text", { Str }, optional = false, nullable = false),
            Field("last", { opt(RunDid.ADAPTER) }, optional = true, nullable = true),
            Field("last_text", { opt(Str) }, optional = true, nullable = true),
            Field("waiting", { opt(Str) }, optional = true, nullable = true),
            Field("started_ms", { opt(U64) }, optional = true, nullable = true),
            Field("started_text", { opt(Str) }, optional = true, nullable = true),
            Field("active_ms", { opt(U64) }, optional = true, nullable = true),
            Field("ended_ms", { opt(U64) }, optional = true, nullable = true),
            Field("why", { opt(RunWhy.ADAPTER) }, optional = true, nullable = true),
            Field("error", { opt(Str) }, optional = true, nullable = true),
            Field("calls", { opt(Int32) }, optional = true, nullable = true),
            Field("background", { opt(Bool) }, optional = true, nullable = true),
            Field("facts", { opt(arr(RunLine.ADAPTER)) }, optional = true, nullable = true),
            Field("explain", { opt(RunExplain.ADAPTER) }, optional = true, nullable = true),
            Field("end", { opt(RunEndLine.ADAPTER) }, optional = true, nullable = true),
            Field("needs", { opt(Needs.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> RunInfo(a[0] as String, a[1] as String?, a[2] as String?, a[3] as String?, a[4] as String?, a[5] as RunState, a[6] as String, a[7] as String, a[8] as RunDid?, a[9] as String?, a[10] as String?, a[11] as Long?, a[12] as String?, a[13] as Long?, a[14] as Long?, a[15] as RunWhy?, a[16] as String?, a[17] as Int?, a[18] as Boolean?, a[19] as List<RunLine>?, a[20] as RunExplain?, a[21] as RunEndLine?, a[22] as Needs?) },
            { v -> arrayOf(v.run, v.label, v.kind, v.tool, v.parent, v.state, v.statusText, v.labelText, v.last, v.lastText, v.waiting, v.startedMs, v.startedText, v.activeMs, v.endedMs, v.why, v.error, v.calls, v.background, v.facts, v.explain, v.end, v.needs) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RunLine(
    val text: String,
    val from: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RunLine> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            Field("from", { opt(U64) }, optional = true, nullable = true),
            ),
            { a -> RunLine(a[0] as String, a[1] as Long?) },
            { v -> arrayOf(v.text, v.from) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class RunRow(
    val record: LineRecord,
    val rid: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<RunRow> = obj(
            listOf(
            Field("record", { LineRecord.ADAPTER }, optional = false, nullable = false),
            Field("rid", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> RunRow(a[0] as LineRecord, a[1] as String?) },
            { v -> arrayOf(v.record, v.rid) },
        )
    }
}

enum class RunState(val wire: String) {
    RUNNING("running"),
    DONE("done"),
    FAILED("failed"),
    STOPPED("stopped"),
    UNKNOWN("unknown"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<RunState> = enumOf(entries) { it.wire }
    }
}

enum class RunWhy(val wire: String) {
    REPORTED("reported"),
    OWN("own"),
    QUIET("quiet"),
    ORPHANED("orphaned"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<RunWhy> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class Said(
    val said: String,
    val detail: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Said> = obj(
            listOf(
            Field("said", { Str }, optional = false, nullable = false),
            Field("detail", { Str }, optional = false, nullable = false),
            ),
            { a -> Said(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.said, v.detail) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class ScannedAccount(
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
    val color: Int,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<ScannedAccount> = obj(
            listOf(
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
            Field("color", { Int32 }, optional = false, nullable = false),
            ),
            { a -> ScannedAccount(a[0] as String, a[1] as String, a[2] as String, a[3] as String?, a[4] as Boolean, a[5] as String, a[6] as Boolean, a[7] as Boolean, a[8] as String, a[9] as Boolean, a[10] as Boolean, a[11] as Int) },
            { v -> arrayOf(v.name, v.label, v.email, v.configDir, v.isDefault, v.mode, v.exists, v.loggedIn, v.authKind, v.authReady, v.selectable, v.color) },
        )
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
data class SearchDown(
    val origin: String,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SearchDown> = obj(
            listOf(
            Field("origin", { Str }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> SearchDown(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.origin, v.said) },
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
            Field("offset", { U64 }, optional = false, nullable = false),
            Field("line", { U64 }, optional = false, nullable = false),
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
    val moreHits: Int,
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
            Field("moreHits", { Int32 }, optional = false, nullable = false),
            Field("hitsTruncated", { Bool }, optional = false, nullable = false),
            Field("isBg", { Bool }, optional = false, nullable = false),
            Field("can", { Can.ADAPTER }, optional = false, nullable = false),
            Field("forkedFrom", { opt(ForkedFrom.ADAPTER) }, optional = true, nullable = true),
            Field("face", { SessionFace.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> SearchRow(a[0] as String, a[1] as String, a[2] as String, a[3] as Boolean, a[4] as Hidden?, a[5] as String, a[6] as String, a[7] as String, a[8] as String, a[9] as String, a[10] as String, a[11] as Long, a[12] as List<SearchHit>, a[13] as Int, a[14] as Boolean, a[15] as Boolean, a[16] as Can, a[17] as ForkedFrom?, a[18] as SessionFace) },
            { v -> arrayOf(v.atText, v.spanText, v.label, v.starred, v.hidden, v.agent, v.sessionId, v.projectPath, v.projectName, v.jsonlPath, v.title, v.updatedAt, v.hits, v.moreHits, v.hitsTruncated, v.isBg, v.can, v.forkedFrom, v.face) },
        )
    }
}

enum class SearchScope(val wire: String) {
    USER("user"),
    ASSISTANT("assistant"),
    REPORT("report"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<SearchScope> = enumOf(entries) { it.wire }
    }
}

sealed interface Seen<T> {
    /** 判别格（`state`）那个词。 */
    val state: String

    val seen: SeenAt

    @Suppress("UNCHECKED_CAST")
    data class Kept<T>(
        override val seen: SeenAt,
        val value: T,
    ) : Seen<T>, Ordered {
        override val state: String get() = "kept"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Unreadable<T>(
        override val seen: SeenAt,
    ) : Seen<T>, Ordered {
        override val state: String get() = "unreadable"
        override var wireOrder: IntArray? = null
    }

    companion object {
        fun <T> adapter(adT: JsonAdapter<T>): JsonAdapter<Seen<T>> =
            tagged<Seen<T>>(
                "state",
                listOf(
                Variant("kept", Seen.Kept::class.java as Class<out Seen<T>>, obj(
                        listOf(
                        Field("state", { tag("kept") }, optional = false, nullable = false),
                        Field("seen", { SeenAt.ADAPTER }, optional = false, nullable = false),
                        Field("value", { adT }, optional = false, nullable = false),
                        ),
                        { a -> Seen.Kept<T>(a[1] as SeenAt, a[2] as T) },
                        { v -> arrayOf("kept", v.seen, v.value) },
                    )),
                Variant("unreadable", Seen.Unreadable::class.java as Class<out Seen<T>>, obj(
                        listOf(
                        Field("state", { tag("unreadable") }, optional = false, nullable = false),
                        Field("seen", { SeenAt.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> Seen.Unreadable<T>(a[1] as SeenAt) },
                        { v -> arrayOf("unreadable", v.seen) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class SeenAt(
    val atMs: Long,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SeenAt> = obj(
            listOf(
            Field("atMs", { Lng }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> SeenAt(a[0] as Long, a[1] as String) },
            { v -> arrayOf(v.atMs, v.said) },
        )
    }
}

enum class SeenKind(val wire: String) {
    ACCOUNTS("accounts"),
    DATA("data"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<SeenKind> = enumOf(entries) { it.wire }
    }
}

enum class SegMark(val wire: String) {
    WAIT("wait"),
    OK("ok"),
    FAIL("fail"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<SegMark> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class Segs(
    val resolve: SegMark,
    val connect: SegMark,
    val auth: SegMark,
    val ready: SegMark,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Segs> = obj(
            listOf(
            Field("resolve", { SegMark.ADAPTER }, optional = false, nullable = false),
            Field("connect", { SegMark.ADAPTER }, optional = false, nullable = false),
            Field("auth", { SegMark.ADAPTER }, optional = false, nullable = false),
            Field("ready", { SegMark.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> Segs(a[0] as SegMark, a[1] as SegMark, a[2] as SegMark, a[3] as SegMark) },
            { v -> arrayOf(v.resolve, v.connect, v.auth, v.ready) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Sent(
    val result: SentResult,
    val why: Refused? = null,
    val said: String? = null,
    val screen: String? = null,
    val acts: List<SentAct>? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Sent> = obj(
            listOf(
            Field("result", { SentResult.ADAPTER }, optional = false, nullable = false),
            Field("why", { opt(Refused.ADAPTER) }, optional = true, nullable = true),
            Field("said", { opt(Str) }, optional = true, nullable = true),
            Field("screen", { opt(Str) }, optional = true, nullable = true),
            Field("acts", { opt(arr(SentAct.ADAPTER)) }, optional = true, nullable = true),
            ),
            { a -> Sent(a[0] as SentResult, a[1] as Refused?, a[2] as String?, a[3] as String?, a[4] as List<SentAct>?) },
            { v -> arrayOf(v.result, v.why, v.said, v.screen, v.acts) },
        )
    }
}

enum class SentAct(val wire: String) {
    RETRY("retry"),
    RECAPTURE("recapture"),
    RELIST("relist"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<SentAct> = enumOf(entries) { it.wire }
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
data class SessionAccountReply(
    val account: String?,
    val name: String,
    val `when`: SessionAccountReplyWhen,
    val from: String?,
    val accountLine: String?,
    val fromLine: String?,
    val usage: Usage?,
    val stale: Boolean,
    val hover: List<List<String>>,
    val sampled: Long?,
    val differs: Boolean,
    val badge: SessionAccountReplyBadge?,
    val queryable: Boolean,
    val ownUpstream: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionAccountReply> = obj(
            listOf(
            Field("account", { opt(Str) }, optional = false, nullable = true),
            Field("name", { Str }, optional = false, nullable = false),
            Field("when", { SessionAccountReplyWhen.ADAPTER }, optional = false, nullable = false),
            Field("from", { opt(Str) }, optional = false, nullable = true),
            Field("accountLine", { opt(Str) }, optional = false, nullable = true),
            Field("fromLine", { opt(Str) }, optional = false, nullable = true),
            Field("usage", { opt(Usage.ADAPTER) }, optional = false, nullable = true),
            Field("stale", { Bool }, optional = false, nullable = false),
            Field("hover", { arr(arr(Str)) }, optional = false, nullable = false),
            Field("sampled", { opt(Lng) }, optional = false, nullable = true),
            Field("differs", { Bool }, optional = false, nullable = false),
            Field("badge", { opt(SessionAccountReplyBadge.ADAPTER) }, optional = false, nullable = true),
            Field("queryable", { Bool }, optional = false, nullable = false),
            Field("ownUpstream", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> SessionAccountReply(a[0] as String?, a[1] as String, a[2] as SessionAccountReplyWhen, a[3] as String?, a[4] as String?, a[5] as String?, a[6] as Usage?, a[7] as Boolean, a[8] as List<List<String>>, a[9] as Long?, a[10] as Boolean, a[11] as SessionAccountReplyBadge?, a[12] as Boolean, a[13] as String?) },
            { v -> arrayOf(v.account, v.name, v.`when`, v.from, v.accountLine, v.fromLine, v.usage, v.stale, v.hover, v.sampled, v.differs, v.badge, v.queryable, v.ownUpstream) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionAccountReplyBadge(
    val ghost: Boolean,
    val title: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionAccountReplyBadge> = obj(
            listOf(
            Field("ghost", { Bool }, optional = false, nullable = false),
            Field("title", { Str }, optional = false, nullable = false),
            ),
            { a -> SessionAccountReplyBadge(a[0] as Boolean, a[1] as String) },
            { v -> arrayOf(v.ghost, v.title) },
        )
    }
}

enum class SessionAccountReplyWhen(val wire: String) {
    NOW("now"),
    LAST("last"),
    UNKNOWN("unknown"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<SessionAccountReplyWhen> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionAccountReq(
    val sid: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionAccountReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            ),
            { a -> SessionAccountReq(a[0] as String) },
            { v -> arrayOf(v.sid) },
        )
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
data class SessionAdded(
    val sid: String,
    val agentKind: String,
    val background: Boolean? = null,
    val attachable: Boolean? = null,
    val cwd: String? = null,
    val projectDir: String? = null,
    val name: String? = null,
    val path: String? = null,
    val face: SessionFace,
    val hidden: Hidden? = null,
    val container: WireSessionContainer? = null,
    val pid: Int,
    val label: String,
    val labelProject: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionAdded> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("agent_kind", { Str }, optional = false, nullable = false),
            Field("background", { opt(Bool) }, optional = true, nullable = true),
            Field("attachable", { opt(Bool) }, optional = true, nullable = true),
            Field("cwd", { opt(Str) }, optional = true, nullable = true),
            Field("project_dir", { opt(Str) }, optional = true, nullable = true),
            Field("name", { opt(Str) }, optional = true, nullable = true),
            Field("path", { opt(Str) }, optional = true, nullable = true),
            Field("face", { SessionFace.ADAPTER }, optional = false, nullable = false),
            Field("hidden", { opt(Hidden.ADAPTER) }, optional = true, nullable = true),
            Field("container", { opt(WireSessionContainer.ADAPTER) }, optional = true, nullable = true),
            Field("pid", { Int32 }, optional = false, nullable = false),
            Field("label", { Str }, optional = false, nullable = false),
            Field("label_project", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> SessionAdded(a[0] as String, a[1] as String, a[2] as Boolean?, a[3] as Boolean?, a[4] as String?, a[5] as String?, a[6] as String?, a[7] as String?, a[8] as SessionFace, a[9] as Hidden?, a[10] as WireSessionContainer?, a[11] as Int, a[12] as String, a[13] as String?) },
            { v -> arrayOf(v.sid, v.agentKind, v.background, v.attachable, v.cwd, v.projectDir, v.name, v.path, v.face, v.hidden, v.container, v.pid, v.label, v.labelProject) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionAttachmentCheckReply(
    val items: List<AttachVerdict>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionAttachmentCheckReply> = obj(
            listOf(
            Field("items", { arr(AttachVerdict.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> SessionAttachmentCheckReply(a[0] as List<AttachVerdict>) },
            { v -> arrayOf(v.items) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionAttachmentCheckReq(
    val sid: String,
    val items: List<AttachItem>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionAttachmentCheckReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("items", { arr(AttachItem.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> SessionAttachmentCheckReq(a[0] as String, a[1] as List<AttachItem>) },
            { v -> arrayOf(v.sid, v.items) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionAttachmentCommitReply(
    val id: String,
    val name: String,
    val path: String,
    val rel: String,
    val bytes: Long,
    val sha256: String,
    val `class`: FileClass,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionAttachmentCommitReply> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            Field("rel", { Str }, optional = false, nullable = false),
            Field("bytes", { U64 }, optional = false, nullable = false),
            Field("sha256", { Str }, optional = false, nullable = false),
            Field("class", { FileClass.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> SessionAttachmentCommitReply(a[0] as String, a[1] as String, a[2] as String, a[3] as String, a[4] as Long, a[5] as String, a[6] as FileClass) },
            { v -> arrayOf(v.id, v.name, v.path, v.rel, v.bytes, v.sha256, v.`class`) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionAttachmentCommitReq(
    val sid: String,
    val key: String,
    val name: String,
    val bytes: Long,
    val chunks: Long? = null,
    val expect: Sha256Expect,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionAttachmentCommitReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("key", { Str }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("bytes", { U64 }, optional = false, nullable = false),
            Field("chunks", { opt(U64) }, optional = true, nullable = true),
            Field("expect", { Sha256Expect.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> SessionAttachmentCommitReq(a[0] as String, a[1] as String, a[2] as String, a[3] as Long, a[4] as Long?, a[5] as Sha256Expect) },
            { v -> arrayOf(v.sid, v.key, v.name, v.bytes, v.chunks, v.expect) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionAttachmentDropReply(
    val removed: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionAttachmentDropReply> = obj(
            listOf(
            Field("removed", { Bool }, optional = false, nullable = false),
            ),
            { a -> SessionAttachmentDropReply(a[0] as Boolean) },
            { v -> arrayOf(v.removed) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionAttachmentDropReq(
    val sid: String,
    val id: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionAttachmentDropReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("id", { Str }, optional = false, nullable = false),
            ),
            { a -> SessionAttachmentDropReq(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.sid, v.id) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionBook(
    val live: Map<String, SessionAdded>,
    val left: Map<String, SessionLeft>,
    val runs: Map<String, SessionRuns>,
    val successors: Map<String, String>,
    val rereads: Map<String, Long>,
    val listed: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionBook> = obj(
            listOf(
            Field("live", { dict(SessionAdded.ADAPTER) }, optional = false, nullable = false),
            Field("left", { dict(SessionLeft.ADAPTER) }, optional = false, nullable = false),
            Field("runs", { dict(SessionRuns.ADAPTER) }, optional = false, nullable = false),
            Field("successors", { dict(Str) }, optional = false, nullable = false),
            Field("rereads", { dict(Lng) }, optional = false, nullable = false),
            Field("listed", { Bool }, optional = false, nullable = false),
            ),
            { a -> SessionBook(a[0] as Map<String, SessionAdded>, a[1] as Map<String, SessionLeft>, a[2] as Map<String, SessionRuns>, a[3] as Map<String, String>, a[4] as Map<String, Long>, a[5] as Boolean) },
            { v -> arrayOf(v.live, v.left, v.runs, v.successors, v.rereads, v.listed) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionBranchPayload(
    val sid: String,
    val off: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionBranchPayload> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("off", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> SessionBranchPayload(a[0] as String, a[1] as List<String>) },
            { v -> arrayOf(v.sid, v.off) },
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
    val stop: Allow,
    val say: CanSay,
    val end: Allow,
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
            Field("stop", { Allow.ADAPTER }, optional = false, nullable = false),
            Field("say", { CanSay.ADAPTER }, optional = false, nullable = false),
            Field("end", { Allow.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> SessionCan(a[0] as CanClose, a[1] as Allow, a[2] as CanTerm, a[3] as Allow, a[4] as Allow, a[5] as Allow, a[6] as Allow, a[7] as CanSay, a[8] as Allow) },
            { v -> arrayOf(v.close, v.resume, v.term, v.reattach, v.model, v.permission, v.stop, v.say, v.end) },
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

@Suppress("UNCHECKED_CAST")
data class SessionCommandsReq(
    val sid: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionCommandsReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            ),
            { a -> SessionCommandsReq(a[0] as String) },
            { v -> arrayOf(v.sid) },
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
    val life: Life,
    val settled: Boolean,
    val name: String,
    val tip: String,
    val line: String,
    val tone: Tone,
    val order: Int,
    val can: SessionCan,
    val account: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionFace> = obj(
            listOf(
            Field("dot", { SessionDot.ADAPTER }, optional = false, nullable = false),
            Field("life", { Life.ADAPTER }, optional = false, nullable = false),
            Field("settled", { Bool }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("tip", { Str }, optional = false, nullable = false),
            Field("line", { Str }, optional = false, nullable = false),
            Field("tone", { Tone.ADAPTER }, optional = false, nullable = false),
            Field("order", { Int32 }, optional = false, nullable = false),
            Field("can", { SessionCan.ADAPTER }, optional = false, nullable = false),
            Field("account", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> SessionFace(a[0] as SessionDot, a[1] as Life, a[2] as Boolean, a[3] as String, a[4] as String, a[5] as String, a[6] as Tone, a[7] as Int, a[8] as SessionCan, a[9] as String?) },
            { v -> arrayOf(v.dot, v.life, v.settled, v.name, v.tip, v.line, v.tone, v.order, v.can, v.account) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionFacts(
    val forkedFrom: String?,
    val touchedFiles: List<String>,
    val usage: UsageFact?,
    val modelText: String?,
    val modelPick: String?,
    val projectDir: String?,
    val agent: String?,
    val writers: List<Long>,
    val pending: List<PendingCall>,
    val lastSay: LastSay?,
    val needs: Needs?,
    val handedBack: List<String>,
    val retries: List<RetryRun>,
    val permissionMode: String?,
    val permissionModeText: String?,
    val tokens: TokenUse?,
    val cost: FactsCost?,
    val bgTasks: List<BgTask>,
    val background: FactsBackground?,
    val answerOff: String?,
    val autoAnswer: Long?,
    val autoAnswered: String?,
    val mcp: List<McpTrouble>,
    val startedBy: Kin?,
    val children: List<Kin>,
    val line: FactsLine?,
    val quietUnread: QuietUnread?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionFacts> = obj(
            listOf(
            Field("forkedFrom", { opt(Str) }, optional = false, nullable = true),
            Field("touchedFiles", { arr(Str) }, optional = false, nullable = false),
            Field("usage", { opt(UsageFact.ADAPTER) }, optional = false, nullable = true),
            Field("modelText", { opt(Str) }, optional = false, nullable = true),
            Field("modelPick", { opt(Str) }, optional = false, nullable = true),
            Field("projectDir", { opt(Str) }, optional = false, nullable = true),
            Field("agent", { opt(Str) }, optional = false, nullable = true),
            Field("writers", { arr(Lng) }, optional = false, nullable = false),
            Field("pending", { arr(PendingCall.ADAPTER) }, optional = false, nullable = false),
            Field("lastSay", { opt(LastSay.ADAPTER) }, optional = false, nullable = true),
            Field("needs", { opt(Needs.ADAPTER) }, optional = false, nullable = true),
            Field("handedBack", { arr(Str) }, optional = false, nullable = false),
            Field("retries", { arr(RetryRun.ADAPTER) }, optional = false, nullable = false),
            Field("permissionMode", { opt(Str) }, optional = false, nullable = true),
            Field("permissionModeText", { opt(Str) }, optional = false, nullable = true),
            Field("tokens", { opt(TokenUse.ADAPTER) }, optional = false, nullable = true),
            Field("cost", { opt(FactsCost.ADAPTER) }, optional = false, nullable = true),
            Field("bgTasks", { arr(BgTask.ADAPTER) }, optional = false, nullable = false),
            Field("background", { opt(FactsBackground.ADAPTER) }, optional = false, nullable = true),
            Field("answerOff", { opt(Str) }, optional = false, nullable = true),
            Field("autoAnswer", { opt(Lng) }, optional = false, nullable = true),
            Field("autoAnswered", { opt(Str) }, optional = false, nullable = true),
            Field("mcp", { arr(McpTrouble.ADAPTER) }, optional = false, nullable = false),
            Field("startedBy", { opt(Kin.ADAPTER) }, optional = false, nullable = true),
            Field("children", { arr(Kin.ADAPTER) }, optional = false, nullable = false),
            Field("line", { opt(FactsLine.ADAPTER) }, optional = false, nullable = true),
            Field("quietUnread", { opt(QuietUnread.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> SessionFacts(a[0] as String?, a[1] as List<String>, a[2] as UsageFact?, a[3] as String?, a[4] as String?, a[5] as String?, a[6] as String?, a[7] as List<Long>, a[8] as List<PendingCall>, a[9] as LastSay?, a[10] as Needs?, a[11] as List<String>, a[12] as List<RetryRun>, a[13] as String?, a[14] as String?, a[15] as TokenUse?, a[16] as FactsCost?, a[17] as List<BgTask>, a[18] as FactsBackground?, a[19] as String?, a[20] as Long?, a[21] as String?, a[22] as List<McpTrouble>, a[23] as Kin?, a[24] as List<Kin>, a[25] as FactsLine?, a[26] as QuietUnread?) },
            { v -> arrayOf(v.forkedFrom, v.touchedFiles, v.usage, v.modelText, v.modelPick, v.projectDir, v.agent, v.writers, v.pending, v.lastSay, v.needs, v.handedBack, v.retries, v.permissionMode, v.permissionModeText, v.tokens, v.cost, v.bgTasks, v.background, v.answerOff, v.autoAnswer, v.autoAnswered, v.mcp, v.startedBy, v.children, v.line, v.quietUnread) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionFileNoticePayload(
    val sessionId: String,
    val origin: String,
    val path: String,
    val change: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionFileNoticePayload> = obj(
            listOf(
            Field("session_id", { Str }, optional = false, nullable = false),
            Field("origin", { Str }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            Field("change", { Str }, optional = false, nullable = false),
            ),
            { a -> SessionFileNoticePayload(a[0] as String, a[1] as String, a[2] as String, a[3] as String) },
            { v -> arrayOf(v.sessionId, v.origin, v.path, v.change) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionFileReread(
    val sessionId: String,
    val path: String,
    val why: RereadWhy,
    val lines: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionFileReread> = obj(
            listOf(
            Field("session_id", { Str }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            Field("why", { RereadWhy.ADAPTER }, optional = false, nullable = false),
            Field("lines", { U64 }, optional = false, nullable = false),
            ),
            { a -> SessionFileReread(a[0] as String, a[1] as String, a[2] as RereadWhy, a[3] as Long) },
            { v -> arrayOf(v.sessionId, v.path, v.why, v.lines) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionHits(
    val agent: String,
    val sessionId: String,
    val projectPath: String,
    val projectName: String,
    val jsonlPath: String,
    val title: String,
    val updatedAt: Long,
    val hits: List<SearchHit>,
    val moreHits: Int,
    val hitsTruncated: Boolean,
    val isBg: Boolean,
    val can: Can,
    val forkedFrom: ForkedFrom? = null,
    val face: SessionFace,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionHits> = obj(
            listOf(
            Field("agent", { Str }, optional = false, nullable = false),
            Field("sessionId", { Str }, optional = false, nullable = false),
            Field("projectPath", { Str }, optional = false, nullable = false),
            Field("projectName", { Str }, optional = false, nullable = false),
            Field("jsonlPath", { Str }, optional = false, nullable = false),
            Field("title", { Str }, optional = false, nullable = false),
            Field("updatedAt", { Lng }, optional = false, nullable = false),
            Field("hits", { arr(SearchHit.ADAPTER) }, optional = false, nullable = false),
            Field("moreHits", { Int32 }, optional = false, nullable = false),
            Field("hitsTruncated", { Bool }, optional = false, nullable = false),
            Field("isBg", { Bool }, optional = false, nullable = false),
            Field("can", { Can.ADAPTER }, optional = false, nullable = false),
            Field("forkedFrom", { opt(ForkedFrom.ADAPTER) }, optional = true, nullable = true),
            Field("face", { SessionFace.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> SessionHits(a[0] as String, a[1] as String, a[2] as String, a[3] as String, a[4] as String, a[5] as String, a[6] as Long, a[7] as List<SearchHit>, a[8] as Int, a[9] as Boolean, a[10] as Boolean, a[11] as Can, a[12] as ForkedFrom?, a[13] as SessionFace) },
            { v -> arrayOf(v.agent, v.sessionId, v.projectPath, v.projectName, v.jsonlPath, v.title, v.updatedAt, v.hits, v.moreHits, v.hitsTruncated, v.isBg, v.can, v.forkedFrom, v.face) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionInterruptReq(
    val sid: String,
    val client: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionInterruptReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("client", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> SessionInterruptReq(a[0] as String, a[1] as String?) },
            { v -> arrayOf(v.sid, v.client) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionLeft(
    val sid: String,
    val face: SessionFace,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionLeft> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("face", { SessionFace.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> SessionLeft(a[0] as String, a[1] as SessionFace) },
            { v -> arrayOf(v.sid, v.face) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionModelReq(
    val sid: String,
    val model: String,
    val client: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionModelReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("model", { Str }, optional = false, nullable = false),
            Field("client", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> SessionModelReq(a[0] as String, a[1] as String, a[2] as String?) },
            { v -> arrayOf(v.sid, v.model, v.client) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionOpenReq(
    val what: OpenWhat,
    val account: AccountAsk? = null,
    val run: Run? = null,
    val local: Boolean,
    val ticket: String? = null,
    val client: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionOpenReq> = obj(
            listOf(
            Field("what", { OpenWhat.ADAPTER }, optional = false, nullable = false),
            Field("account", { opt(AccountAsk.ADAPTER) }, optional = true, nullable = true),
            Field("run", { opt(Run.ADAPTER) }, optional = true, nullable = true),
            Field("local", { Bool }, optional = false, nullable = false),
            Field("ticket", { opt(Str) }, optional = true, nullable = true),
            Field("client", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> SessionOpenReq(a[0] as OpenWhat, a[1] as AccountAsk?, a[2] as Run?, a[3] as Boolean, a[4] as String?, a[5] as String?) },
            { v -> arrayOf(v.what, v.account, v.run, v.local, v.ticket, v.client) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionPermissionReq(
    val sid: String,
    val mode: String,
    val client: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionPermissionReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("mode", { Str }, optional = false, nullable = false),
            Field("client", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> SessionPermissionReq(a[0] as String, a[1] as String, a[2] as String?) },
            { v -> arrayOf(v.sid, v.mode, v.client) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionQueueCancelReq(
    val machine: String,
    val sid: String,
    val id: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionQueueCancelReq> = obj(
            listOf(
            Field("machine", { Str }, optional = false, nullable = false),
            Field("sid", { Str }, optional = false, nullable = false),
            Field("id", { Str }, optional = false, nullable = false),
            ),
            { a -> SessionQueueCancelReq(a[0] as String, a[1] as String, a[2] as String) },
            { v -> arrayOf(v.machine, v.sid, v.id) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionQueuePutReq(
    val machine: String,
    val sid: String,
    val id: String,
    val end: String,
    val n: Long? = null,
    val atMs: Long,
    val text: String,
    val attachments: List<String>? = null,
    val rewind: Rewind? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionQueuePutReq> = obj(
            listOf(
            Field("machine", { Str }, optional = false, nullable = false),
            Field("sid", { Str }, optional = false, nullable = false),
            Field("id", { Str }, optional = false, nullable = false),
            Field("end", { Str }, optional = false, nullable = false),
            Field("n", { opt(U64) }, optional = true, nullable = true),
            Field("atMs", { Lng }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            Field("attachments", { opt(arr(Str)) }, optional = true, nullable = true),
            Field("rewind", { opt(Rewind.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> SessionQueuePutReq(a[0] as String, a[1] as String, a[2] as String, a[3] as String, a[4] as Long?, a[5] as Long, a[6] as String, a[7] as List<String>?, a[8] as Rewind?) },
            { v -> arrayOf(v.machine, v.sid, v.id, v.end, v.n, v.atMs, v.text, v.attachments, v.rewind) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionQueueReadReq(
    val machine: String,
    val sid: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionQueueReadReq> = obj(
            listOf(
            Field("machine", { Str }, optional = false, nullable = false),
            Field("sid", { Str }, optional = false, nullable = false),
            ),
            { a -> SessionQueueReadReq(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.machine, v.sid) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionRewindOptionsReq(
    val machine: String,
    val sid: String,
    val to: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionRewindOptionsReq> = obj(
            listOf(
            Field("machine", { Str }, optional = false, nullable = false),
            Field("sid", { Str }, optional = false, nullable = false),
            Field("to", { Str }, optional = false, nullable = false),
            ),
            { a -> SessionRewindOptionsReq(a[0] as String, a[1] as String, a[2] as String) },
            { v -> arrayOf(v.machine, v.sid, v.to) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionRuns(
    val sid: String,
    val runs: List<RunInfo>,
    val ended: List<RunEnded>,
    val groups: RunGroups,
    val summary: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionRuns> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("runs", { arr(RunInfo.ADAPTER) }, optional = false, nullable = false),
            Field("ended", { arr(RunEnded.ADAPTER) }, optional = false, nullable = false),
            Field("groups", { RunGroups.ADAPTER }, optional = false, nullable = false),
            Field("summary", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> SessionRuns(a[0] as String, a[1] as List<RunInfo>, a[2] as List<RunEnded>, a[3] as RunGroups, a[4] as String?) },
            { v -> arrayOf(v.sid, v.runs, v.ended, v.groups, v.summary) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionSelf(
    val sid: String,
    val groups: List<GroupRead>,
    val looked: Looked,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionSelf> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("groups", { arr(GroupRead.ADAPTER) }, optional = false, nullable = false),
            Field("looked", { Looked.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> SessionSelf(a[0] as String, a[1] as List<GroupRead>, a[2] as Looked) },
            { v -> arrayOf(v.sid, v.groups, v.looked) },
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
data class SessionStatus(
    val sid: String,
    val face: SessionFace,
    val waitingFor: String? = null,
    val path: String? = null,
    val label: String,
    val labelProject: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionStatus> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("face", { SessionFace.ADAPTER }, optional = false, nullable = false),
            Field("waiting_for", { opt(Str) }, optional = true, nullable = true),
            Field("path", { opt(Str) }, optional = true, nullable = true),
            Field("label", { Str }, optional = false, nullable = false),
            Field("label_project", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> SessionStatus(a[0] as String, a[1] as SessionFace, a[2] as String?, a[3] as String?, a[4] as String, a[5] as String?) },
            { v -> arrayOf(v.sid, v.face, v.waitingFor, v.path, v.label, v.labelProject) },
        )
    }
}

sealed interface SessionStreamFrame {
    data class Line(
        val value: Raw,
    ) : SessionStreamFrame
    data class Batch(
        val value: BatchEdge,
    ) : SessionStreamFrame
    data class FileNotice(
        val value: SessionFileNoticePayload,
    ) : SessionStreamFrame
    data class Added(
        val value: SessionAdded,
    ) : SessionStreamFrame
    data class Status(
        val value: SessionStatus,
    ) : SessionStreamFrame
    data class Left(
        val value: SessionLeft,
    ) : SessionStreamFrame
    data class Listed(
        val value: OriginSessionsListedPayload,
    ) : SessionStreamFrame
    data class SnapshotInflight(
        val value: SnapshotInflightPayload,
    ) : SessionStreamFrame
    data class Runs(
        val value: SessionRuns,
    ) : SessionStreamFrame
    data class Branch(
        val value: SessionBranchPayload,
    ) : SessionStreamFrame
    data class Watch(
        val value: WatchFrom,
    ) : SessionStreamFrame

    companion object {
        val ADAPTER: JsonAdapter<SessionStreamFrame> =
            external<SessionStreamFrame>(
                listOf(
                Ext("line", SessionStreamFrame.Line::class.java, { RawJson }, { SessionStreamFrame.Line(it as Raw) }, { (it as SessionStreamFrame.Line).value }),
                Ext("batch", SessionStreamFrame.Batch::class.java, { BatchEdge.ADAPTER }, { SessionStreamFrame.Batch(it as BatchEdge) }, { (it as SessionStreamFrame.Batch).value }),
                Ext("file_notice", SessionStreamFrame.FileNotice::class.java, { SessionFileNoticePayload.ADAPTER }, { SessionStreamFrame.FileNotice(it as SessionFileNoticePayload) }, { (it as SessionStreamFrame.FileNotice).value }),
                Ext("added", SessionStreamFrame.Added::class.java, { SessionAdded.ADAPTER }, { SessionStreamFrame.Added(it as SessionAdded) }, { (it as SessionStreamFrame.Added).value }),
                Ext("status", SessionStreamFrame.Status::class.java, { SessionStatus.ADAPTER }, { SessionStreamFrame.Status(it as SessionStatus) }, { (it as SessionStreamFrame.Status).value }),
                Ext("left", SessionStreamFrame.Left::class.java, { SessionLeft.ADAPTER }, { SessionStreamFrame.Left(it as SessionLeft) }, { (it as SessionStreamFrame.Left).value }),
                Ext("listed", SessionStreamFrame.Listed::class.java, { OriginSessionsListedPayload.ADAPTER }, { SessionStreamFrame.Listed(it as OriginSessionsListedPayload) }, { (it as SessionStreamFrame.Listed).value }),
                Ext("snapshot_inflight", SessionStreamFrame.SnapshotInflight::class.java, { SnapshotInflightPayload.ADAPTER }, { SessionStreamFrame.SnapshotInflight(it as SnapshotInflightPayload) }, { (it as SessionStreamFrame.SnapshotInflight).value }),
                Ext("runs", SessionStreamFrame.Runs::class.java, { SessionRuns.ADAPTER }, { SessionStreamFrame.Runs(it as SessionRuns) }, { (it as SessionStreamFrame.Runs).value }),
                Ext("branch", SessionStreamFrame.Branch::class.java, { SessionBranchPayload.ADAPTER }, { SessionStreamFrame.Branch(it as SessionBranchPayload) }, { (it as SessionStreamFrame.Branch).value }),
                Ext("watch", SessionStreamFrame.Watch::class.java, { WatchFrom.ADAPTER }, { SessionStreamFrame.Watch(it as WatchFrom) }, { (it as SessionStreamFrame.Watch).value }),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionWindow(
    val labels: List<String>,
    val chain: List<Link>,
    val tmux: Boolean,
    val face: FrontFace? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionWindow> = obj(
            listOf(
            Field("labels", { arr(Str) }, optional = false, nullable = false),
            Field("chain", { arr(Link.ADAPTER) }, optional = false, nullable = false),
            Field("tmux", { Bool }, optional = false, nullable = false),
            Field("face", { opt(FrontFace.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> SessionWindow(a[0] as List<String>, a[1] as List<Link>, a[2] as Boolean, a[3] as FrontFace?) },
            { v -> arrayOf(v.labels, v.chain, v.tmux, v.face) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SessionWindowReq(
    val machine: String,
    val sid: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SessionWindowReq> = obj(
            listOf(
            Field("machine", { Str }, optional = false, nullable = false),
            Field("sid", { Str }, optional = false, nullable = false),
            ),
            { a -> SessionWindowReq(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.machine, v.sid) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SetEdit(
    val name: String,
    val rows: List<RowEdit>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SetEdit> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("rows", { arr(RowEdit.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> SetEdit(a[0] as String, a[1] as List<RowEdit>) },
            { v -> arrayOf(v.name, v.rows) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SetLaunchReq(
    val name: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SetLaunchReq> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            ),
            { a -> SetLaunchReq(a[0] as String) },
            { v -> arrayOf(v.name) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SetLaunched(
    val rows: List<RowLaunched>,
    val started: Long,
    val failed: Long,
    val said: String,
    val detail: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SetLaunched> = obj(
            listOf(
            Field("rows", { arr(RowLaunched.ADAPTER) }, optional = false, nullable = false),
            Field("started", { Lng }, optional = false, nullable = false),
            Field("failed", { Lng }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            Field("detail", { Str }, optional = false, nullable = false),
            ),
            { a -> SetLaunched(a[0] as List<RowLaunched>, a[1] as Long, a[2] as Long, a[3] as String, a[4] as String) },
            { v -> arrayOf(v.rows, v.started, v.failed, v.said, v.detail) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SetOut(
    val name: String,
    val rows: List<SetRowOut>,
    val problems: List<ProfileProblem>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SetOut> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("rows", { arr(SetRowOut.ADAPTER) }, optional = false, nullable = false),
            Field("problems", { arr(ProfileProblem.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> SetOut(a[0] as String, a[1] as List<SetRowOut>, a[2] as List<ProfileProblem>) },
            { v -> arrayOf(v.name, v.rows, v.problems) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SetPlan(
    val rows: List<RowPlan>,
    val more: Long,
    val short: String,
    val moreSaid: String,
    val totalSaid: String,
    val startSaid: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SetPlan> = obj(
            listOf(
            Field("rows", { arr(RowPlan.ADAPTER) }, optional = false, nullable = false),
            Field("more", { Lng }, optional = false, nullable = false),
            Field("short", { Str }, optional = false, nullable = false),
            Field("moreSaid", { Str }, optional = false, nullable = false),
            Field("totalSaid", { Str }, optional = false, nullable = false),
            Field("startSaid", { Str }, optional = false, nullable = false),
            ),
            { a -> SetPlan(a[0] as List<RowPlan>, a[1] as Long, a[2] as String, a[3] as String, a[4] as String, a[5] as String) },
            { v -> arrayOf(v.rows, v.more, v.short, v.moreSaid, v.totalSaid, v.startSaid) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SetPlanReq(
    val name: String? = null,
    val rows: List<RowEdit>? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SetPlanReq> = obj(
            listOf(
            Field("name", { opt(Str) }, optional = true, nullable = true),
            Field("rows", { opt(arr(RowEdit.ADAPTER)) }, optional = true, nullable = true),
            ),
            { a -> SetPlanReq(a[0] as String?, a[1] as List<RowEdit>?) },
            { v -> arrayOf(v.name, v.rows) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SetRowOut(
    val cwd: String,
    val alias: String,
    val n: Long,
    val tabGroup: String?,
    val line: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SetRowOut> = obj(
            listOf(
            Field("cwd", { Str }, optional = false, nullable = false),
            Field("alias", { Str }, optional = false, nullable = false),
            Field("n", { Lng }, optional = false, nullable = false),
            Field("tabGroup", { opt(Str) }, optional = false, nullable = true),
            Field("line", { Lng }, optional = false, nullable = false),
            ),
            { a -> SetRowOut(a[0] as String, a[1] as String, a[2] as Long, a[3] as String?, a[4] as Long) },
            { v -> arrayOf(v.cwd, v.alias, v.n, v.tabGroup, v.line) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SettingsScope(
    val scope: String,
    val path: String,
    val state: SurfaceState,
    val hasCcBusHooks: Boolean?,
    val precedenceNote: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SettingsScope> = obj(
            listOf(
            Field("scope", { Str }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            Field("state", { SurfaceState.ADAPTER }, optional = false, nullable = false),
            Field("has_cc_bus_hooks", { opt(Bool) }, optional = false, nullable = true),
            Field("precedence_note", { Str }, optional = false, nullable = false),
            ),
            { a -> SettingsScope(a[0] as String, a[1] as String, a[2] as SurfaceState, a[3] as Boolean?, a[4] as String) },
            { v -> arrayOf(v.scope, v.path, v.state, v.hasCcBusHooks, v.precedenceNote) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Sha256Expect(
    val sha256: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Sha256Expect> = obj(
            listOf(
            Field("sha256", { Str }, optional = false, nullable = false),
            ),
            { a -> Sha256Expect(a[0] as String) },
            { v -> arrayOf(v.sha256) },
        )
    }
}

typealias Sha256Hex = String

enum class ShellKind(val wire: String) {
    POSIX("posix"),
    POWERSHELL("powershell"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ShellKind> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class Showing(
    val terminals: List<Terminal>,
    val why: ShownWhy? = null,
    val tmux: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Showing> = obj(
            listOf(
            Field("terminals", { arr(Terminal.ADAPTER) }, optional = false, nullable = false),
            Field("why", { opt(ShownWhy.ADAPTER) }, optional = true, nullable = true),
            Field("tmux", { Bool }, optional = false, nullable = false),
            ),
            { a -> Showing(a[0] as List<Terminal>, a[1] as ShownWhy?, a[2] as Boolean) },
            { v -> arrayOf(v.terminals, v.why, v.tmux) },
        )
    }
}

enum class ShownWhy(val wire: String) {
    DETACHED("detached"),
    NO_TERMINAL("no-terminal"),
    UNREADABLE("unreadable"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<ShownWhy> = enumOf(entries) { it.wire }
    }
}

typealias Sid = String

@Suppress("UNCHECKED_CAST")
data class SidReq(
    val sid: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SidReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            ),
            { a -> SidReq(a[0] as String) },
            { v -> arrayOf(v.sid) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SidsReq(
    val sids: List<String>,
    val client: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SidsReq> = obj(
            listOf(
            Field("sids", { arr(Str) }, optional = false, nullable = false),
            Field("client", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> SidsReq(a[0] as List<String>, a[1] as String?) },
            { v -> arrayOf(v.sids, v.client) },
        )
    }
}

sealed interface SignInState {
    /** 判别格（`state`）那个词。 */
    val state: String

    data object Done : SignInState {
        override val state: String get() = "done"
    }
    @Suppress("UNCHECKED_CAST")
    data class Failed(
        val said: String,
        val detail: String?,
        val again: LoginAgainHow,
    ) : SignInState, Ordered {
        override val state: String get() = "failed"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<SignInState> =
            tagged<SignInState>(
                "state",
                listOf(
                Variant("done", SignInState.Done::class.java, obj(
                        listOf(
                        Field("state", { tag("done") }, optional = false, nullable = false),
                        ),
                        { SignInState.Done },
                        { arrayOf("done") },
                    )),
                Variant("failed", SignInState.Failed::class.java, obj(
                        listOf(
                        Field("state", { tag("failed") }, optional = false, nullable = false),
                        Field("said", { Str }, optional = false, nullable = false),
                        Field("detail", { opt(Str) }, optional = false, nullable = true),
                        Field("again", { LoginAgainHow.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> SignInState.Failed(a[1] as String, a[2] as String?, a[3] as LoginAgainHow) },
                        { v -> arrayOf("failed", v.said, v.detail, v.again) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class SizeReply(
    val path: WirePath,
    val bytes: Long,
    val files: Long,
    val dirs: Long,
    val links: Long,
    val other: Long,
    val skippedMounts: Long,
    val unreadableDirs: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SizeReply> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("bytes", { U64 }, optional = false, nullable = false),
            Field("files", { U64 }, optional = false, nullable = false),
            Field("dirs", { U64 }, optional = false, nullable = false),
            Field("links", { U64 }, optional = false, nullable = false),
            Field("other", { U64 }, optional = false, nullable = false),
            Field("skipped_mounts", { U64 }, optional = false, nullable = false),
            Field("unreadable_dirs", { U64 }, optional = false, nullable = false),
            ),
            { a -> SizeReply(a[0] as WirePath, a[1] as Long, a[2] as Long, a[3] as Long, a[4] as Long, a[5] as Long, a[6] as Long, a[7] as Long) },
            { v -> arrayOf(v.path, v.bytes, v.files, v.dirs, v.links, v.other, v.skippedMounts, v.unreadableDirs) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SizeReq(
    val path: WirePath,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SizeReq> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> SizeReq(a[0] as WirePath) },
            { v -> arrayOf(v.path) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SkillApplied(
    val dir: String,
    val written: List<String>,
    val chmodFailed: List<String>,
    val recordFailed: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SkillApplied> = obj(
            listOf(
            Field("dir", { Str }, optional = false, nullable = false),
            Field("written", { arr(Str) }, optional = false, nullable = false),
            Field("chmodFailed", { arr(Str) }, optional = false, nullable = false),
            Field("recordFailed", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> SkillApplied(a[0] as String, a[1] as List<String>, a[2] as List<String>, a[3] as String?) },
            { v -> arrayOf(v.dir, v.written, v.chmodFailed, v.recordFailed) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SkillApplyReq(
    val name: String,
    val project: String?,
    val source: List<SkillSourceFile>,
    val target: List<SkillTargetFile>,
    val take: List<String>?,
    val overwrite: List<String>?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SkillApplyReq> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("project", { opt(Str) }, optional = false, nullable = true),
            Field("source", { arr(SkillSourceFile.ADAPTER) }, optional = false, nullable = false),
            Field("target", { arr(SkillTargetFile.ADAPTER) }, optional = false, nullable = false),
            Field("take", { opt(arr(Str)) }, optional = false, nullable = true),
            Field("overwrite", { opt(arr(Str)) }, optional = false, nullable = true),
            ),
            { a -> SkillApplyReq(a[0] as String, a[1] as String?, a[2] as List<SkillSourceFile>, a[3] as List<SkillTargetFile>, a[4] as List<String>?, a[5] as List<String>?) },
            { v -> arrayOf(v.name, v.project, v.source, v.target, v.take, v.overwrite) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SkillLedgerFile(
    val digest: String,
    val created: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SkillLedgerFile> = obj(
            listOf(
            Field("digest", { Str }, optional = false, nullable = false),
            Field("created", { Bool }, optional = false, nullable = false),
            ),
            { a -> SkillLedgerFile(a[0] as String, a[1] as Boolean) },
            { v -> arrayOf(v.digest, v.created) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SkillPlan(
    val root: String,
    val dir: String,
    val base: String,
    val prefix: String,
    val rows: List<SkillPlanRow>,
    val target: List<SkillTargetFile>,
    val write: List<String>?,
    val ledger: Map<String, SkillLedgerFile>?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SkillPlan> = obj(
            listOf(
            Field("root", { Str }, optional = false, nullable = false),
            Field("dir", { Str }, optional = false, nullable = false),
            Field("base", { Str }, optional = false, nullable = false),
            Field("prefix", { Str }, optional = false, nullable = false),
            Field("rows", { arr(SkillPlanRow.ADAPTER) }, optional = false, nullable = false),
            Field("target", { arr(SkillTargetFile.ADAPTER) }, optional = false, nullable = false),
            Field("write", { opt(arr(Str)) }, optional = false, nullable = true),
            Field("ledger", { opt(dict(SkillLedgerFile.ADAPTER)) }, optional = false, nullable = true),
            ),
            { a -> SkillPlan(a[0] as String, a[1] as String, a[2] as String, a[3] as String, a[4] as List<SkillPlanRow>, a[5] as List<SkillTargetFile>, a[6] as List<String>?, a[7] as Map<String, SkillLedgerFile>?) },
            { v -> arrayOf(v.root, v.dir, v.base, v.prefix, v.rows, v.target, v.write, v.ledger) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SkillPlanReq(
    val name: String,
    val source: List<SkillSourceFile>,
    val project: String?,
    val take: List<String>?,
    val overwrite: List<String>?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SkillPlanReq> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("source", { arr(SkillSourceFile.ADAPTER) }, optional = false, nullable = false),
            Field("project", { opt(Str) }, optional = false, nullable = true),
            Field("take", { opt(arr(Str)) }, optional = false, nullable = true),
            Field("overwrite", { opt(arr(Str)) }, optional = false, nullable = true),
            ),
            { a -> SkillPlanReq(a[0] as String, a[1] as List<SkillSourceFile>, a[2] as String?, a[3] as List<String>?, a[4] as List<String>?) },
            { v -> arrayOf(v.name, v.source, v.project, v.take, v.overwrite) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SkillPlanRow(
    val path: String,
    val state: String,
    val suspects: List<SkillSuspect>,
    val blocked: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SkillPlanRow> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("state", { Str }, optional = false, nullable = false),
            Field("suspects", { arr(SkillSuspect.ADAPTER) }, optional = false, nullable = false),
            Field("blocked", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> SkillPlanRow(a[0] as String, a[1] as String, a[2] as List<SkillSuspect>, a[3] as String?) },
            { v -> arrayOf(v.path, v.state, v.suspects, v.blocked) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SkillRead(
    val root: String,
    val dir: String,
    val files: List<SkillReadFile>,
    val skipped: List<String>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SkillRead> = obj(
            listOf(
            Field("root", { Str }, optional = false, nullable = false),
            Field("dir", { Str }, optional = false, nullable = false),
            Field("files", { arr(SkillReadFile.ADAPTER) }, optional = false, nullable = false),
            Field("skipped", { arr(Str) }, optional = false, nullable = false),
            ),
            { a -> SkillRead(a[0] as String, a[1] as String, a[2] as List<SkillReadFile>, a[3] as List<String>) },
            { v -> arrayOf(v.root, v.dir, v.files, v.skipped) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SkillReadFile(
    val path: String,
    val text: String?,
    val bytes: Long,
    val exec: Boolean,
    val why: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SkillReadFile> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("text", { opt(Str) }, optional = false, nullable = true),
            Field("bytes", { Lng }, optional = false, nullable = false),
            Field("exec", { Bool }, optional = false, nullable = false),
            Field("why", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> SkillReadFile(a[0] as String, a[1] as String?, a[2] as Long, a[3] as Boolean, a[4] as String?) },
            { v -> arrayOf(v.path, v.text, v.bytes, v.exec, v.why) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SkillReadReq(
    val name: String,
    val project: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SkillReadReq> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("project", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> SkillReadReq(a[0] as String, a[1] as String?) },
            { v -> arrayOf(v.name, v.project) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SkillSourceFile(
    val path: String,
    val text: String?,
    val exec: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SkillSourceFile> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("text", { opt(Str) }, optional = false, nullable = true),
            Field("exec", { Bool }, optional = false, nullable = false),
            ),
            { a -> SkillSourceFile(a[0] as String, a[1] as String?, a[2] as Boolean) },
            { v -> arrayOf(v.path, v.text, v.exec) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SkillSuspect(
    val kind: String,
    val value: String,
    val there: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SkillSuspect> = obj(
            listOf(
            Field("kind", { Str }, optional = false, nullable = false),
            Field("value", { Str }, optional = false, nullable = false),
            Field("there", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> SkillSuspect(a[0] as String, a[1] as String, a[2] as String?) },
            { v -> arrayOf(v.kind, v.value, v.there) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SkillTargetFile(
    val path: String,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SkillTargetFile> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> SkillTargetFile(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.path, v.text) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SlotRow(
    val os: String,
    val arch: String,
    val id: String,
    val path: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SlotRow> = obj(
            listOf(
            Field("os", { Str }, optional = false, nullable = false),
            Field("arch", { Str }, optional = false, nullable = false),
            Field("id", { Str }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            ),
            { a -> SlotRow(a[0] as String, a[1] as String, a[2] as String, a[3] as String) },
            { v -> arrayOf(v.os, v.arch, v.id, v.path) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SlotShow(
    val slot: String,
    val pct: Int? = null,
    val resetsAt: Long? = null,
    val resetsAtText: String? = null,
    val resetsAtRelText: String? = null,
    val resetText: String? = null,
    val full: Boolean? = null,
    val text: String,
    val tone: Tone,
    val perPctText: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SlotShow> = obj(
            listOf(
            Field("slot", { Str }, optional = false, nullable = false),
            Field("pct", { opt(Int32) }, optional = true, nullable = true),
            Field("resetsAt", { opt(U64) }, optional = true, nullable = true),
            Field("resetsAtText", { opt(Str) }, optional = true, nullable = true),
            Field("resetsAtRelText", { opt(Str) }, optional = true, nullable = true),
            Field("resetText", { opt(Str) }, optional = true, nullable = true),
            Field("full", { opt(Bool) }, optional = true, nullable = true),
            Field("text", { Str }, optional = false, nullable = false),
            Field("tone", { Tone.ADAPTER }, optional = false, nullable = false),
            Field("perPctText", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> SlotShow(a[0] as String, a[1] as Int?, a[2] as Long?, a[3] as String?, a[4] as String?, a[5] as String?, a[6] as Boolean?, a[7] as String, a[8] as Tone, a[9] as String?) },
            { v -> arrayOf(v.slot, v.pct, v.resetsAt, v.resetsAtText, v.resetsAtRelText, v.resetText, v.full, v.text, v.tone, v.perPctText) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SnapshotInflightPayload(
    val count: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SnapshotInflightPayload> = obj(
            listOf(
            Field("count", { Lng }, optional = false, nullable = false),
            ),
            { a -> SnapshotInflightPayload(a[0] as Long) },
            { v -> arrayOf(v.count) },
        )
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
    /** 判别格（`kind`）那个词。 */
    val kind: String

    data object Human : Speaker {
        override val kind: String get() = "human"
    }
    @Suppress("UNCHECKED_CAST")
    data class SlashCommand(
        val name: String,
        val args: String,
    ) : Speaker, Ordered {
        override val kind: String get() = "slashCommand"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class BashInput(
        val command: String,
    ) : Speaker, Ordered {
        override val kind: String get() = "bashInput"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class BashOutput(
        val stdout: String,
        val stderr: String,
    ) : Speaker, Ordered {
        override val kind: String get() = "bashOutput"
        override var wireOrder: IntArray? = null
    }
    data object CommandOutput : Speaker {
        override val kind: String get() = "commandOutput"
    }
    @Suppress("UNCHECKED_CAST")
    data class TaskNotification(
        val taskId: String? = null,
        val status: String? = null,
        val summary: String? = null,
        val toolUseId: String? = null,
    ) : Speaker, Ordered {
        override val kind: String get() = "taskNotification"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class AgentMessage(
        val from: String? = null,
        val name: String? = null,
        val handback: Boolean,
        val body: String? = null,
    ) : Speaker, Ordered {
        override val kind: String get() = "agentMessage"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class PeerSession(
        val from: String? = null,
        val body: String? = null,
    ) : Speaker, Ordered {
        override val kind: String get() = "peerSession"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Coordinator(
        val body: String? = null,
    ) : Speaker, Ordered {
        override val kind: String get() = "coordinator"
        override var wireOrder: IntArray? = null
    }
    data object AgentTask : Speaker {
        override val kind: String get() = "agentTask"
    }
    @Suppress("UNCHECKED_CAST")
    data class System(
        val body: String? = null,
    ) : Speaker, Ordered {
        override val kind: String get() = "system"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class CompactSummary(
        val charsText: String,
    ) : Speaker, Ordered {
        override val kind: String get() = "compactSummary"
        override var wireOrder: IntArray? = null
    }
    data object Interrupt : Speaker {
        override val kind: String get() = "interrupt"
    }
    data object ToolResult : Speaker {
        override val kind: String get() = "toolResult"
    }

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
data class SshCommand(
    val command: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SshCommand> = obj(
            listOf(
            Field("command", { Str }, optional = false, nullable = false),
            ),
            { a -> SshCommand(a[0] as String) },
            { v -> arrayOf(v.command) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SshConnection(
    val clientAddr: String,
    val clientPort: Long,
    val serverAddr: String,
    val serverPort: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SshConnection> = obj(
            listOf(
            Field("clientAddr", { Str }, optional = false, nullable = false),
            Field("clientPort", { Lng }, optional = false, nullable = false),
            Field("serverAddr", { Str }, optional = false, nullable = false),
            Field("serverPort", { Lng }, optional = false, nullable = false),
            ),
            { a -> SshConnection(a[0] as String, a[1] as Long, a[2] as String, a[3] as Long) },
            { v -> arrayOf(v.clientAddr, v.clientPort, v.serverAddr, v.serverPort) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SshImportGroup(
    val label: String,
    val host: String,
    val port: Long,
    val user: String,
    val keyPath: String?,
    val addresses: List<String>,
    val jump: String?,
    val members: List<SshImportMember>,
    val inList: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SshImportGroup> = obj(
            listOf(
            Field("label", { Str }, optional = false, nullable = false),
            Field("host", { Str }, optional = false, nullable = false),
            Field("port", { Lng }, optional = false, nullable = false),
            Field("user", { Str }, optional = false, nullable = false),
            Field("keyPath", { opt(Str) }, optional = false, nullable = true),
            Field("addresses", { arr(Str) }, optional = false, nullable = false),
            Field("jump", { opt(Str) }, optional = false, nullable = true),
            Field("members", { arr(SshImportMember.ADAPTER) }, optional = false, nullable = false),
            Field("inList", { Bool }, optional = false, nullable = false),
            ),
            { a -> SshImportGroup(a[0] as String, a[1] as String, a[2] as Long, a[3] as String, a[4] as String?, a[5] as List<String>, a[6] as String?, a[7] as List<SshImportMember>, a[8] as Boolean) },
            { v -> arrayOf(v.label, v.host, v.port, v.user, v.keyPath, v.addresses, v.jump, v.members, v.inList) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SshImportMember(
    val alias: String,
    val host: String,
    val port: Long,
    val proxyJump: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SshImportMember> = obj(
            listOf(
            Field("alias", { Str }, optional = false, nullable = false),
            Field("host", { Str }, optional = false, nullable = false),
            Field("port", { Lng }, optional = false, nullable = false),
            Field("proxyJump", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> SshImportMember(a[0] as String, a[1] as String, a[2] as Long, a[3] as String?) },
            { v -> arrayOf(v.alias, v.host, v.port, v.proxyJump) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SshImportReq(
    val known: List<KnownHost>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SshImportReq> = obj(
            listOf(
            Field("known", { arr(KnownHost.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> SshImportReq(a[0] as List<KnownHost>) },
            { v -> arrayOf(v.known) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class SshImported(
    val groups: List<SshImportGroup>,
    val partial: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SshImported> = obj(
            listOf(
            Field("groups", { arr(SshImportGroup.ADAPTER) }, optional = false, nullable = false),
            Field("partial", { Bool }, optional = false, nullable = false),
            ),
            { a -> SshImported(a[0] as List<SshImportGroup>, a[1] as Boolean) },
            { v -> arrayOf(v.groups, v.partial) },
        )
    }
}

sealed interface Stage {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    @Suppress("UNCHECKED_CAST")
    data class Dialing(
        val endpoint: String,
    ) : Stage, Ordered {
        override val kind: String get() = "dialing"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class HostKey(
        val endpoint: String,
        val fingerprint: String,
    ) : Stage, Ordered {
        override val kind: String get() = "hostKey"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Failed(
        val endpoint: String,
        val reason: String,
    ) : Stage, Ordered {
        override val kind: String get() = "failed"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Won(
        val endpoint: String,
    ) : Stage, Ordered {
        override val kind: String get() = "won"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Auth(
        val ok: Boolean,
        val detail: String?,
    ) : Stage, Ordered {
        override val kind: String get() = "auth"
        override var wireOrder: IntArray? = null
    }
    data object Established : Stage {
        override val kind: String get() = "established"
    }

    companion object {
        val ADAPTER: JsonAdapter<Stage> =
            tagged<Stage>(
                "kind",
                listOf(
                Variant("dialing", Stage.Dialing::class.java, obj(
                        listOf(
                        Field("kind", { tag("dialing") }, optional = false, nullable = false),
                        Field("endpoint", { Str }, optional = false, nullable = false),
                        ),
                        { a -> Stage.Dialing(a[1] as String) },
                        { v -> arrayOf("dialing", v.endpoint) },
                    )),
                Variant("hostKey", Stage.HostKey::class.java, obj(
                        listOf(
                        Field("kind", { tag("hostKey") }, optional = false, nullable = false),
                        Field("endpoint", { Str }, optional = false, nullable = false),
                        Field("fingerprint", { Str }, optional = false, nullable = false),
                        ),
                        { a -> Stage.HostKey(a[1] as String, a[2] as String) },
                        { v -> arrayOf("hostKey", v.endpoint, v.fingerprint) },
                    )),
                Variant("failed", Stage.Failed::class.java, obj(
                        listOf(
                        Field("kind", { tag("failed") }, optional = false, nullable = false),
                        Field("endpoint", { Str }, optional = false, nullable = false),
                        Field("reason", { Str }, optional = false, nullable = false),
                        ),
                        { a -> Stage.Failed(a[1] as String, a[2] as String) },
                        { v -> arrayOf("failed", v.endpoint, v.reason) },
                    )),
                Variant("won", Stage.Won::class.java, obj(
                        listOf(
                        Field("kind", { tag("won") }, optional = false, nullable = false),
                        Field("endpoint", { Str }, optional = false, nullable = false),
                        ),
                        { a -> Stage.Won(a[1] as String) },
                        { v -> arrayOf("won", v.endpoint) },
                    )),
                Variant("auth", Stage.Auth::class.java, obj(
                        listOf(
                        Field("kind", { tag("auth") }, optional = false, nullable = false),
                        Field("ok", { Bool }, optional = false, nullable = false),
                        Field("detail", { opt(Str) }, optional = false, nullable = true),
                        ),
                        { a -> Stage.Auth(a[1] as Boolean, a[2] as String?) },
                        { v -> arrayOf("auth", v.ok, v.detail) },
                    )),
                Variant("established", Stage.Established::class.java, obj(
                        listOf(
                        Field("kind", { tag("established") }, optional = false, nullable = false),
                        ),
                        { Stage.Established },
                        { arrayOf("established") },
                    )),
                ),
            )
    }
}

enum class StandingWord(val wire: String) {
    RUNNING("running"),
    AMBIGUOUS("ambiguous"),
    IDLE("idle"),
    NONE("none"),
    NO_TMUX("no_tmux"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<StandingWord> = enumOf(entries) { it.wire }
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
    val mode: Int? = null,
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
            Field("size", { U64 }, optional = false, nullable = false),
            Field("readonly", { Bool }, optional = false, nullable = false),
            Field("mode", { opt(Int32) }, optional = true, nullable = true),
            Field("mtime_secs", { opt(U64) }, optional = true, nullable = true),
            Field("mtime_text", { opt(Str) }, optional = true, nullable = true),
            Field("mtime_full", { opt(Str) }, optional = true, nullable = true),
            Field("owner", { opt(Str) }, optional = false, nullable = true),
            Field("link_target", { opt(WirePath.ADAPTER) }, optional = false, nullable = true),
            ),
            { a -> StatReply(a[0] as WirePath, a[1] as WirePath?, a[2] as FileKind, a[3] as Long, a[4] as Boolean, a[5] as Int?, a[6] as Long?, a[7] as String?, a[8] as String?, a[9] as String?, a[10] as WirePath?) },
            { v -> arrayOf(v.path, v.up, v.kind, v.size, v.readonly, v.mode, v.mtimeSecs, v.mtimeText, v.mtimeFull, v.owner, v.linkTarget) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class StatReq(
    val path: WirePath,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<StatReq> = obj(
            listOf(
            Field("path", { WirePath.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> StatReq(a[0] as WirePath) },
            { v -> arrayOf(v.path) },
        )
    }
}

sealed interface StepAsk {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    @Suppress("UNCHECKED_CAST")
    data class Questions(
        val questions: List<AskQuestion>,
    ) : StepAsk, Ordered {
        override val kind: String get() = "questions"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Plan(
        val text: String,
        val title: PlanTitle,
    ) : StepAsk, Ordered {
        override val kind: String get() = "plan"
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
    /** 判别格（`kind`）那个词。 */
    val kind: String

    @Suppress("UNCHECKED_CAST")
    data class Changes(
        val changes: List<TextChange>,
    ) : StepBody, Ordered {
        override val kind: String get() = "changes"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Command(
        val line: String,
    ) : StepBody, Ordered {
        override val kind: String get() = "command"
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
    val lines: Int? = null,
    val added: Int? = null,
    val removed: Int? = null,
    val files: Int? = null,
    val answer: Answer? = null,
    val exitCode: Int? = null,
    val file: String? = null,
    val patch: List<PatchHunk>? = null,
    val patchTruncated: Boolean? = null,
    val text: String,
    val timed: String? = null,
    val preview: String? = null,
    val chars: Int? = null,
    val kind: ResultKind? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<StepResult> = obj(
            listOf(
            Field("ok", { Bool }, optional = false, nullable = false),
            Field("rejected", { opt(Bool) }, optional = true, nullable = true),
            Field("lines", { opt(Int32) }, optional = true, nullable = true),
            Field("added", { opt(Int32) }, optional = true, nullable = true),
            Field("removed", { opt(Int32) }, optional = true, nullable = true),
            Field("files", { opt(Int32) }, optional = true, nullable = true),
            Field("answer", { opt(Answer.ADAPTER) }, optional = true, nullable = true),
            Field("exitCode", { opt(Int32) }, optional = true, nullable = true),
            Field("file", { opt(Str) }, optional = true, nullable = true),
            Field("patch", { opt(arr(PatchHunk.ADAPTER)) }, optional = true, nullable = true),
            Field("patchTruncated", { opt(Bool) }, optional = true, nullable = true),
            Field("text", { Str }, optional = false, nullable = false),
            Field("timed", { opt(Str) }, optional = true, nullable = true),
            Field("preview", { opt(Str) }, optional = true, nullable = true),
            Field("chars", { opt(Int32) }, optional = true, nullable = true),
            Field("kind", { opt(ResultKind.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> StepResult(a[0] as Boolean, a[1] as Boolean?, a[2] as Int?, a[3] as Int?, a[4] as Int?, a[5] as Int?, a[6] as Answer?, a[7] as Int?, a[8] as String?, a[9] as List<PatchHunk>?, a[10] as Boolean?, a[11] as String, a[12] as String?, a[13] as String?, a[14] as Int?, a[15] as ResultKind?) },
            { v -> arrayOf(v.ok, v.rejected, v.lines, v.added, v.removed, v.files, v.answer, v.exitCode, v.file, v.patch, v.patchTruncated, v.text, v.timed, v.preview, v.chars, v.kind) },
        )
    }
}

enum class StepWait(val wire: String) {
    RUNNING("running"),
    AWAITING("awaiting"),
    UNCLEAR("unclear"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<StepWait> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class StrategyFail(
    val kind: StrategyFailKind,
    val line: Long? = null,
    val col: Long? = null,
    val msg: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<StrategyFail> = obj(
            listOf(
            Field("kind", { StrategyFailKind.ADAPTER }, optional = false, nullable = false),
            Field("line", { opt(Lng) }, optional = true, nullable = true),
            Field("col", { opt(Lng) }, optional = true, nullable = true),
            Field("msg", { Str }, optional = false, nullable = false),
            ),
            { a -> StrategyFail(a[0] as StrategyFailKind, a[1] as Long?, a[2] as Long?, a[3] as String) },
            { v -> arrayOf(v.kind, v.line, v.col, v.msg) },
        )
    }
}

enum class StrategyFailKind(val wire: String) {
    FAILED("failed"),
    TIMEOUT("timeout"),
    BAD_SHAPE("badShape"),
    GONE("gone"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<StrategyFailKind> = enumOf(entries) { it.wire }
    }
}

sealed interface StreamEv {
    /** 判别格（`t`）那个词。 */
    val t: String

    @Suppress("UNCHECKED_CAST")
    data class Start(
        val rid: String,
    ) : StreamEv, Ordered {
        override val t: String get() = "start"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Block(
        val i: Long,
        val kind: BlockKind,
        val tool: String? = null,
    ) : StreamEv, Ordered {
        override val t: String get() = "block"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Text(
        val i: Long,
        val s: String,
    ) : StreamEv, Ordered {
        override val t: String get() = "text"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Stop(
        val ok: Boolean,
    ) : StreamEv, Ordered {
        override val t: String get() = "stop"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<StreamEv> =
            tagged<StreamEv>(
                "t",
                listOf(
                Variant("start", StreamEv.Start::class.java, obj(
                        listOf(
                        Field("t", { tag("start") }, optional = false, nullable = false),
                        Field("rid", { Str }, optional = false, nullable = false),
                        ),
                        { a -> StreamEv.Start(a[1] as String) },
                        { v -> arrayOf("start", v.rid) },
                    )),
                Variant("block", StreamEv.Block::class.java, obj(
                        listOf(
                        Field("t", { tag("block") }, optional = false, nullable = false),
                        Field("i", { U64 }, optional = false, nullable = false),
                        Field("kind", { BlockKind.ADAPTER }, optional = false, nullable = false),
                        Field("tool", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> StreamEv.Block(a[1] as Long, a[2] as BlockKind, a[3] as String?) },
                        { v -> arrayOf("block", v.i, v.kind, v.tool) },
                    )),
                Variant("text", StreamEv.Text::class.java, obj(
                        listOf(
                        Field("t", { tag("text") }, optional = false, nullable = false),
                        Field("i", { U64 }, optional = false, nullable = false),
                        Field("s", { Str }, optional = false, nullable = false),
                        ),
                        { a -> StreamEv.Text(a[1] as Long, a[2] as String) },
                        { v -> arrayOf("text", v.i, v.s) },
                    )),
                Variant("stop", StreamEv.Stop::class.java, obj(
                        listOf(
                        Field("t", { tag("stop") }, optional = false, nullable = false),
                        Field("ok", { Bool }, optional = false, nullable = false),
                        ),
                        { a -> StreamEv.Stop(a[1] as Boolean) },
                        { v -> arrayOf("stop", v.ok) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class SurfaceRow(
    val toolId: String,
    val toolName: String,
    val tier: EnvTier,
    val sourceLabel: String,
    val pathDeclared: String,
    val pathResolved: String?,
    val note: String?,
    val hostLabel: String,
    val effectLabel: String,
    val state: SurfaceState,
    val installable: Boolean,
    val uninstallable: Boolean,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SurfaceRow> = obj(
            listOf(
            Field("tool_id", { Str }, optional = false, nullable = false),
            Field("tool_name", { Str }, optional = false, nullable = false),
            Field("tier", { EnvTier.ADAPTER }, optional = false, nullable = false),
            Field("source_label", { Str }, optional = false, nullable = false),
            Field("path_declared", { Str }, optional = false, nullable = false),
            Field("path_resolved", { opt(Str) }, optional = false, nullable = true),
            Field("note", { opt(Str) }, optional = false, nullable = true),
            Field("host_label", { Str }, optional = false, nullable = false),
            Field("effect_label", { Str }, optional = false, nullable = false),
            Field("state", { SurfaceState.ADAPTER }, optional = false, nullable = false),
            Field("installable", { Bool }, optional = false, nullable = false),
            Field("uninstallable", { Bool }, optional = false, nullable = false),
            ),
            { a -> SurfaceRow(a[0] as String, a[1] as String, a[2] as EnvTier, a[3] as String, a[4] as String, a[5] as String?, a[6] as String?, a[7] as String, a[8] as String, a[9] as SurfaceState, a[10] as Boolean, a[11] as Boolean) },
            { v -> arrayOf(v.toolId, v.toolName, v.tier, v.sourceLabel, v.pathDeclared, v.pathResolved, v.note, v.hostLabel, v.effectLabel, v.state, v.installable, v.uninstallable) },
        )
    }
}

sealed interface SurfaceState {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    @Suppress("UNCHECKED_CAST")
    data class Present(
        val detail: String,
    ) : SurfaceState, Ordered {
        override val kind: String get() = "present"
        override var wireOrder: IntArray? = null
    }
    data object Absent : SurfaceState {
        override val kind: String get() = "absent"
    }
    @Suppress("UNCHECKED_CAST")
    data class Undetermined(
        val why: String,
    ) : SurfaceState, Ordered {
        override val kind: String get() = "undetermined"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<SurfaceState> =
            tagged<SurfaceState>(
                "kind",
                listOf(
                Variant("present", SurfaceState.Present::class.java, obj(
                        listOf(
                        Field("kind", { tag("present") }, optional = false, nullable = false),
                        Field("detail", { Str }, optional = false, nullable = false),
                        ),
                        { a -> SurfaceState.Present(a[1] as String) },
                        { v -> arrayOf("present", v.detail) },
                    )),
                Variant("absent", SurfaceState.Absent::class.java, obj(
                        listOf(
                        Field("kind", { tag("absent") }, optional = false, nullable = false),
                        ),
                        { SurfaceState.Absent },
                        { arrayOf("absent") },
                    )),
                Variant("undetermined", SurfaceState.Undetermined::class.java, obj(
                        listOf(
                        Field("kind", { tag("undetermined") }, optional = false, nullable = false),
                        Field("why", { Str }, optional = false, nullable = false),
                        ),
                        { a -> SurfaceState.Undetermined(a[1] as String) },
                        { v -> arrayOf("undetermined", v.why) },
                    )),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class SyncHead<H>(
    val total: Int,
    val info: H,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        fun <H> adapter(adH: JsonAdapter<H>): JsonAdapter<SyncHead<H>> = obj(
            listOf(
            Field("total", { Int32 }, optional = false, nullable = false),
            Field("info", { adH }, optional = false, nullable = false),
            ),
            { a -> SyncHead<H>(a[0] as Int, a[1] as H) },
            { v -> arrayOf(v.total, v.info) },
        )
    }
}

typealias SyncHeld = String

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

@Suppress("UNCHECKED_CAST")
data class SyncRange(
    val from: Int,
    val limit: Int,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<SyncRange> = obj(
            listOf(
            Field("from", { Int32 }, optional = false, nullable = false),
            Field("limit", { Int32 }, optional = false, nullable = false),
            ),
            { a -> SyncRange(a[0] as Int, a[1] as Int) },
            { v -> arrayOf(v.from, v.limit) },
        )
    }
}

enum class Tab(val wire: String) {
    SHOW("show"),
    HIDE("hide"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<Tab> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class TabSession(
    val origin: String,
    val sessionId: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TabSession> = obj(
            listOf(
            Field("origin", { Str }, optional = false, nullable = false),
            Field("sessionId", { Str }, optional = false, nullable = false),
            ),
            { a -> TabSession(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.origin, v.sessionId) },
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
data class TailPlan(
    val total: Long,
    val tailFrom: Long,
    val splitAt: Long,
    val end: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TailPlan> = obj(
            listOf(
            Field("total", { U64 }, optional = false, nullable = false),
            Field("tail_from", { U64 }, optional = false, nullable = false),
            Field("split_at", { U64 }, optional = false, nullable = false),
            Field("end", { U64 }, optional = false, nullable = false),
            ),
            { a -> TailPlan(a[0] as Long, a[1] as Long, a[2] as Long, a[3] as Long) },
            { v -> arrayOf(v.total, v.tailFrom, v.splitAt, v.end) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Task(
    val id: String,
    val subject: String,
    val status: String,
    val blocks: List<String>,
    val blockedBy: List<String>,
    val description: String? = null,
    val activeForm: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Task> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            Field("subject", { Str }, optional = false, nullable = false),
            Field("status", { Str }, optional = false, nullable = false),
            Field("blocks", { arr(Str) }, optional = false, nullable = false),
            Field("blockedBy", { arr(Str) }, optional = false, nullable = false),
            Field("description", { opt(Str) }, optional = true, nullable = true),
            Field("activeForm", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> Task(a[0] as String, a[1] as String, a[2] as String, a[3] as List<String>, a[4] as List<String>, a[5] as String?, a[6] as String?) },
            { v -> arrayOf(v.id, v.subject, v.status, v.blocks, v.blockedBy, v.description, v.activeForm) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TasksList(
    val tasks: List<Task>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TasksList> = obj(
            listOf(
            Field("tasks", { arr(Task.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> TasksList(a[0] as List<Task>) },
            { v -> arrayOf(v.tasks) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TasksListReq(
    val sid: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TasksListReq> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            ),
            { a -> TasksListReq(a[0] as String) },
            { v -> arrayOf(v.sid) },
        )
    }
}

sealed interface TermInput {
    /** 判别格（`kind`）那个词。 */
    val kind: String

    data object Shared : TermInput {
        override val kind: String get() = "shared"
    }
    @Suppress("UNCHECKED_CAST")
    data class Held(
        val holder: InputHolder,
    ) : TermInput, Ordered {
        override val kind: String get() = "held"
        override var wireOrder: IntArray? = null
    }

    companion object {
        val ADAPTER: JsonAdapter<TermInput> =
            tagged<TermInput>(
                "kind",
                listOf(
                Variant("shared", TermInput.Shared::class.java, obj(
                        listOf(
                        Field("kind", { tag("shared") }, optional = false, nullable = false),
                        ),
                        { TermInput.Shared },
                        { arrayOf("shared") },
                    )),
                Variant("held", TermInput.Held::class.java, obj(
                        listOf(
                        Field("kind", { tag("held") }, optional = false, nullable = false),
                        Field("holder", { InputHolder.ADAPTER }, optional = false, nullable = false),
                        ),
                        { a -> TermInput.Held(a[1] as InputHolder) },
                        { v -> arrayOf("held", v.holder) },
                    )),
                ),
            )
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
data class Terminal(
    val ssh: SshConnection?,
    val activity: Long?,
    val window: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Terminal> = obj(
            listOf(
            Field("ssh", { opt(SshConnection.ADAPTER) }, optional = false, nullable = true),
            Field("activity", { opt(Lng) }, optional = false, nullable = true),
            Field("window", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> Terminal(a[0] as SshConnection?, a[1] as Long?, a[2] as String?) },
            { v -> arrayOf(v.ssh, v.activity, v.window) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TerminalAt(
    val host: String,
    val terminal: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalAt> = obj(
            listOf(
            Field("host", { Str }, optional = false, nullable = false),
            Field("terminal", { Str }, optional = false, nullable = false),
            ),
            { a -> TerminalAt(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.host, v.terminal) },
        )
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
    val no: Refused,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalCanEndObjValue> = obj(
            listOf(
            Field("no", { Refused.ADAPTER }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> TerminalCanEndObjValue(a[0] as Refused, a[1] as String) },
            { v -> arrayOf(v.no, v.said) },
        )
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
data class TerminalChoices(
    val applies: Boolean,
    val auto: String?,
    val found: List<String>,
    val setting: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalChoices> = obj(
            listOf(
            Field("applies", { Bool }, optional = false, nullable = false),
            Field("auto", { opt(Str) }, optional = false, nullable = true),
            Field("found", { arr(Str) }, optional = false, nullable = false),
            Field("setting", { Str }, optional = false, nullable = false),
            ),
            { a -> TerminalChoices(a[0] as Boolean, a[1] as String?, a[2] as List<String>, a[3] as String) },
            { v -> arrayOf(v.applies, v.auto, v.found, v.setting) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TerminalEndReq(
    val terminal: String? = null,
    val sid: String? = null,
    val client: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalEndReq> = obj(
            listOf(
            Field("terminal", { opt(Str) }, optional = true, nullable = true),
            Field("sid", { opt(Str) }, optional = true, nullable = true),
            Field("client", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> TerminalEndReq(a[0] as String?, a[1] as String?, a[2] as String?) },
            { v -> arrayOf(v.terminal, v.sid, v.client) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TerminalEnded(
    val terminal: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalEnded> = obj(
            listOf(
            Field("terminal", { Str }, optional = false, nullable = false),
            ),
            { a -> TerminalEnded(a[0] as String) },
            { v -> arrayOf(v.terminal) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TerminalFollowEnd(
    val ticket: String,
    val why: FollowEnd,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalFollowEnd> = obj(
            listOf(
            Field("ticket", { Str }, optional = false, nullable = false),
            Field("why", { FollowEnd.ADAPTER }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> TerminalFollowEnd(a[0] as String, a[1] as FollowEnd, a[2] as String) },
            { v -> arrayOf(v.ticket, v.why, v.said) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TerminalHoldReq(
    val terminal: String,
    val client: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalHoldReq> = obj(
            listOf(
            Field("terminal", { Str }, optional = false, nullable = false),
            Field("client", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> TerminalHoldReq(a[0] as String, a[1] as String?) },
            { v -> arrayOf(v.terminal, v.client) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TerminalInputReq(
    val terminal: String? = null,
    val sid: String? = null,
    val text: String? = null,
    val enter: Boolean? = null,
    val key: String? = null,
    val seenScreen: String? = null,
    val client: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalInputReq> = obj(
            listOf(
            Field("terminal", { opt(Str) }, optional = true, nullable = true),
            Field("sid", { opt(Str) }, optional = true, nullable = true),
            Field("text", { opt(Str) }, optional = true, nullable = true),
            Field("enter", { opt(Bool) }, optional = true, nullable = true),
            Field("key", { opt(Str) }, optional = true, nullable = true),
            Field("seen_screen", { opt(Str) }, optional = true, nullable = true),
            Field("client", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> TerminalInputReq(a[0] as String?, a[1] as String?, a[2] as String?, a[3] as Boolean?, a[4] as String?, a[5] as String?, a[6] as String?) },
            { v -> arrayOf(v.terminal, v.sid, v.text, v.enter, v.key, v.seenScreen, v.client) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TerminalItem(
    val terminal: String,
    val host: WireTerminalHost,
    val name: String,
    val title: String,
    val program: String,
    val cwd: String,
    val session: TermSession? = null,
    val startedBy: StartedBy,
    val clients: List<Attached>,
    val input: TermInput,
    val state: TermState,
    val lastActivity: Long,
    val can: TerminalCan,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalItem> = obj(
            listOf(
            Field("terminal", { Str }, optional = false, nullable = false),
            Field("host", { WireTerminalHost.ADAPTER }, optional = false, nullable = false),
            Field("name", { Str }, optional = false, nullable = false),
            Field("title", { Str }, optional = false, nullable = false),
            Field("program", { Str }, optional = false, nullable = false),
            Field("cwd", { Str }, optional = false, nullable = false),
            Field("session", { opt(TermSession.ADAPTER) }, optional = true, nullable = true),
            Field("started_by", { StartedBy.ADAPTER }, optional = false, nullable = false),
            Field("clients", { arr(Attached.ADAPTER) }, optional = false, nullable = false),
            Field("input", { TermInput.ADAPTER }, optional = false, nullable = false),
            Field("state", { TermState.ADAPTER }, optional = false, nullable = false),
            Field("last_activity", { U64 }, optional = false, nullable = false),
            Field("can", { TerminalCan.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> TerminalItem(a[0] as String, a[1] as WireTerminalHost, a[2] as String, a[3] as String, a[4] as String, a[5] as String, a[6] as TermSession?, a[7] as StartedBy, a[8] as List<Attached>, a[9] as TermInput, a[10] as TermState, a[11] as Long, a[12] as TerminalCan) },
            { v -> arrayOf(v.terminal, v.host, v.name, v.title, v.program, v.cwd, v.session, v.startedBy, v.clients, v.input, v.state, v.lastActivity, v.can) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TerminalPreviewReq(
    val terminal: String? = null,
    val sid: String? = null,
    val color: Boolean? = null,
    val scrollback: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalPreviewReq> = obj(
            listOf(
            Field("terminal", { opt(Str) }, optional = true, nullable = true),
            Field("sid", { opt(Str) }, optional = true, nullable = true),
            Field("color", { opt(Bool) }, optional = true, nullable = true),
            Field("scrollback", { opt(U64) }, optional = true, nullable = true),
            ),
            { a -> TerminalPreviewReq(a[0] as String?, a[1] as String?, a[2] as Boolean?, a[3] as Long?) },
            { v -> arrayOf(v.terminal, v.sid, v.color, v.scrollback) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TerminalScreen(
    val ticket: String,
    val seq: Long,
    val view: Preview,
    val input: TermInput,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalScreen> = obj(
            listOf(
            Field("ticket", { Str }, optional = false, nullable = false),
            Field("seq", { U64 }, optional = false, nullable = false),
            Field("view", { Preview.ADAPTER }, optional = false, nullable = false),
            Field("input", { TermInput.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> TerminalScreen(a[0] as String, a[1] as Long, a[2] as Preview, a[3] as TermInput) },
            { v -> arrayOf(v.ticket, v.seq, v.view, v.input) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TerminalSshReq(
    val machine: String,
    val command: String? = null,
    val cwd: TerminalSshReqCwd? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalSshReq> = obj(
            listOf(
            Field("machine", { Str }, optional = false, nullable = false),
            Field("command", { opt(Str) }, optional = true, nullable = true),
            Field("cwd", { opt(TerminalSshReqCwd.ADAPTER) }, optional = true, nullable = true),
            ),
            { a -> TerminalSshReq(a[0] as String, a[1] as String?, a[2] as TerminalSshReqCwd?) },
            { v -> arrayOf(v.machine, v.command, v.cwd) },
        )
    }
}

sealed interface TerminalSshReqCwd {
    data class Text(
        val value: String,
    ) : TerminalSshReqCwd
    data class Obj(
        val value: TerminalSshReqCwdObjValue,
    ) : TerminalSshReqCwd

    companion object {
        val ADAPTER: JsonAdapter<TerminalSshReqCwd> =
            byToken<TerminalSshReqCwd>(
                listOf(
                Tok(JsonReader.Token.STRING, TerminalSshReqCwd.Text::class.java, { Str }, { TerminalSshReqCwd.Text(it as String) }, { (it as TerminalSshReqCwd.Text).value }),
                Tok(JsonReader.Token.BEGIN_OBJECT, TerminalSshReqCwd.Obj::class.java, { TerminalSshReqCwdObjValue.ADAPTER }, { TerminalSshReqCwd.Obj(it as TerminalSshReqCwdObjValue) }, { (it as TerminalSshReqCwd.Obj).value }),
                ),
            )
    }
}

@Suppress("UNCHECKED_CAST")
data class TerminalSshReqCwdObjValue(
    val b16: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalSshReqCwdObjValue> = obj(
            listOf(
            Field("b16", { Str }, optional = false, nullable = false),
            ),
            { a -> TerminalSshReqCwdObjValue(a[0] as String) },
            { v -> arrayOf(v.b16) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TerminalsBoard(
    val filters: List<BoardFilter>,
    val groups: List<BoardGroup>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalsBoard> = obj(
            listOf(
            Field("filters", { arr(BoardFilter.ADAPTER) }, optional = false, nullable = false),
            Field("groups", { arr(BoardGroup.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> TerminalsBoard(a[0] as List<BoardFilter>, a[1] as List<BoardGroup>) },
            { v -> arrayOf(v.filters, v.groups) },
        )
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
data class TerminalsListReq(
    val client: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TerminalsListReq> = obj(
            listOf(
            Field("client", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> TerminalsListReq(a[0] as String?) },
            { v -> arrayOf(v.client) },
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
            Field("cap", { U64 }, optional = false, nullable = false),
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

@Suppress("UNCHECKED_CAST")
data class TokenUse(
    val input: Long,
    val output: Long,
    val cacheRead: Long,
    val cacheWrite5m: Long,
    val cacheWrite1h: Long,
    val requests: Long,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TokenUse> = obj(
            listOf(
            Field("input", { Lng }, optional = false, nullable = false),
            Field("output", { Lng }, optional = false, nullable = false),
            Field("cacheRead", { Lng }, optional = false, nullable = false),
            Field("cacheWrite5m", { Lng }, optional = false, nullable = false),
            Field("cacheWrite1h", { Lng }, optional = false, nullable = false),
            Field("requests", { Lng }, optional = false, nullable = false),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> TokenUse(a[0] as Long, a[1] as Long, a[2] as Long, a[3] as Long, a[4] as Long, a[5] as Long, a[6] as String) },
            { v -> arrayOf(v.input, v.output, v.cacheRead, v.cacheWrite5m, v.cacheWrite1h, v.requests, v.text) },
        )
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

enum class Topic(val wire: String) {
    ACCOUNTS("accounts"),
    PROFILES("profiles"),
    QUOTA("quota"),
    ROTATION("rotation"),
    ROTATION_STRATEGIES("rotation_strategies"),
    PLAN("plan"),
    TASKS("tasks"),
    NEEDS("needs"),
    LOGIN("login"),
    MACHINES("machines"),
    MACHINE("machine"),
    EMPTY("empty"),
    SESSION_HIDDEN("session_hidden"),
    FILES_DIR("files_dir"),
    HISTORY_LIST("history_list"),
    GROUPS("groups"),
    SESSION_QUEUE("session_queue"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<Topic> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class TransferCopyAsk(
    val to: String,
    val items: List<CopyFrom>,
    val toDir: String,
    val onExists: OnExists? = null,
    val decide: Map<String, Decide>? = null,
    val ticket: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TransferCopyAsk> = obj(
            listOf(
            Field("to", { Str }, optional = false, nullable = false),
            Field("items", { arr(CopyFrom.ADAPTER) }, optional = false, nullable = false),
            Field("to_dir", { Str }, optional = false, nullable = false),
            Field("on_exists", { opt(OnExists.ADAPTER) }, optional = true, nullable = true),
            Field("decide", { opt(dict(Decide.ADAPTER)) }, optional = true, nullable = true),
            Field("ticket", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> TransferCopyAsk(a[0] as String, a[1] as List<CopyFrom>, a[2] as String, a[3] as OnExists?, a[4] as Map<String, Decide>?, a[5] as String?) },
            { v -> arrayOf(v.to, v.items, v.toDir, v.onExists, v.decide, v.ticket) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TransferCopyReq(
    val from: String,
    val to: String,
    val items: List<CopyFrom>,
    val toDir: String,
    val onExists: OnExists? = null,
    val decide: Map<String, Decide>? = null,
    val ticket: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TransferCopyReq> = obj(
            listOf(
            Field("from", { Str }, optional = false, nullable = false),
            Field("to", { Str }, optional = false, nullable = false),
            Field("items", { arr(CopyFrom.ADAPTER) }, optional = false, nullable = false),
            Field("to_dir", { Str }, optional = false, nullable = false),
            Field("on_exists", { opt(OnExists.ADAPTER) }, optional = true, nullable = true),
            Field("decide", { opt(dict(Decide.ADAPTER)) }, optional = true, nullable = true),
            Field("ticket", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> TransferCopyReq(a[0] as String, a[1] as String, a[2] as List<CopyFrom>, a[3] as String, a[4] as OnExists?, a[5] as Map<String, Decide>?, a[6] as String?) },
            { v -> arrayOf(v.from, v.to, v.items, v.toDir, v.onExists, v.decide, v.ticket) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TransferDownloadAsk(
    val items: List<DownloadItem>,
    val localDir: WirePath,
    val onExists: OnExists? = null,
    val decide: Map<String, Decide>? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TransferDownloadAsk> = obj(
            listOf(
            Field("items", { arr(DownloadItem.ADAPTER) }, optional = false, nullable = false),
            Field("local_dir", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("on_exists", { opt(OnExists.ADAPTER) }, optional = true, nullable = true),
            Field("decide", { opt(dict(Decide.ADAPTER)) }, optional = true, nullable = true),
            ),
            { a -> TransferDownloadAsk(a[0] as List<DownloadItem>, a[1] as WirePath, a[2] as OnExists?, a[3] as Map<String, Decide>?) },
            { v -> arrayOf(v.items, v.localDir, v.onExists, v.decide) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TransferDownloadReq(
    val machine: String,
    val items: List<DownloadItem>,
    val localDir: WirePath,
    val onExists: OnExists? = null,
    val decide: Map<String, Decide>? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TransferDownloadReq> = obj(
            listOf(
            Field("machine", { Str }, optional = false, nullable = false),
            Field("items", { arr(DownloadItem.ADAPTER) }, optional = false, nullable = false),
            Field("local_dir", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("on_exists", { opt(OnExists.ADAPTER) }, optional = true, nullable = true),
            Field("decide", { opt(dict(Decide.ADAPTER)) }, optional = true, nullable = true),
            ),
            { a -> TransferDownloadReq(a[0] as String, a[1] as List<DownloadItem>, a[2] as WirePath, a[3] as OnExists?, a[4] as Map<String, Decide>?) },
            { v -> arrayOf(v.machine, v.items, v.localDir, v.onExists, v.decide) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TransferOpened(
    val items: List<OpenedItem>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TransferOpened> = obj(
            listOf(
            Field("items", { arr(OpenedItem.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> TransferOpened(a[0] as List<OpenedItem>) },
            { v -> arrayOf(v.items) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TransferStartReq(
    val id: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TransferStartReq> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            ),
            { a -> TransferStartReq(a[0] as String) },
            { v -> arrayOf(v.id) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TransferStopReq(
    val id: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TransferStopReq> = obj(
            listOf(
            Field("id", { Str }, optional = false, nullable = false),
            ),
            { a -> TransferStopReq(a[0] as String) },
            { v -> arrayOf(v.id) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TransferUploadAsk(
    val items: List<UploadItem>,
    val remoteDir: WirePath,
    val home: String? = null,
    val onExists: OnExists? = null,
    val decide: Map<String, Decide>? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TransferUploadAsk> = obj(
            listOf(
            Field("items", { arr(UploadItem.ADAPTER) }, optional = false, nullable = false),
            Field("remote_dir", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("home", { opt(Str) }, optional = true, nullable = true),
            Field("on_exists", { opt(OnExists.ADAPTER) }, optional = true, nullable = true),
            Field("decide", { opt(dict(Decide.ADAPTER)) }, optional = true, nullable = true),
            ),
            { a -> TransferUploadAsk(a[0] as List<UploadItem>, a[1] as WirePath, a[2] as String?, a[3] as OnExists?, a[4] as Map<String, Decide>?) },
            { v -> arrayOf(v.items, v.remoteDir, v.home, v.onExists, v.decide) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TransferUploadReq(
    val machine: String,
    val items: List<UploadItem>,
    val remoteDir: WirePath,
    val home: String? = null,
    val onExists: OnExists? = null,
    val decide: Map<String, Decide>? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TransferUploadReq> = obj(
            listOf(
            Field("machine", { Str }, optional = false, nullable = false),
            Field("items", { arr(UploadItem.ADAPTER) }, optional = false, nullable = false),
            Field("remote_dir", { WirePath.ADAPTER }, optional = false, nullable = false),
            Field("home", { opt(Str) }, optional = true, nullable = true),
            Field("on_exists", { opt(OnExists.ADAPTER) }, optional = true, nullable = true),
            Field("decide", { opt(dict(Decide.ADAPTER)) }, optional = true, nullable = true),
            ),
            { a -> TransferUploadReq(a[0] as String, a[1] as List<UploadItem>, a[2] as WirePath, a[3] as String?, a[4] as OnExists?, a[5] as Map<String, Decide>?) },
            { v -> arrayOf(v.machine, v.items, v.remoteDir, v.home, v.onExists, v.decide) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TurnPart(
    val text: String,
    val tone: Tone,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TurnPart> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            Field("tone", { Tone.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> TurnPart(a[0] as String, a[1] as Tone) },
            { v -> arrayOf(v.text, v.tone) },
        )
    }
}

enum class TurnPhase(val wire: String) {
    IDLE("idle"),
    RUNNING("running"),
    AWAITING("awaiting"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<TurnPhase> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class TurnRow(
    val at: Long,
    val line: Long,
    val uuid: String,
    val start: String,
    val end: String,
    val startText: String,
    val endText: String,
    val said: String,
    val tools: Long,
    val thinking: Long,
    val agents: Long,
    val background: Long,
    val retries: Long,
    val peers: Long,
    val fails: Long,
    val shown: List<String>,
    val reply: String,
    val done: Boolean,
    val phase: TurnPhase,
    val folds: List<FoldSegment>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TurnRow> = obj(
            listOf(
            Field("at", { Lng }, optional = false, nullable = false),
            Field("line", { Lng }, optional = false, nullable = false),
            Field("uuid", { Str }, optional = false, nullable = false),
            Field("start", { Str }, optional = false, nullable = false),
            Field("end", { Str }, optional = false, nullable = false),
            Field("startText", { Str }, optional = false, nullable = false),
            Field("endText", { Str }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            Field("tools", { Lng }, optional = false, nullable = false),
            Field("thinking", { Lng }, optional = false, nullable = false),
            Field("agents", { Lng }, optional = false, nullable = false),
            Field("background", { Lng }, optional = false, nullable = false),
            Field("retries", { Lng }, optional = false, nullable = false),
            Field("peers", { Lng }, optional = false, nullable = false),
            Field("fails", { Lng }, optional = false, nullable = false),
            Field("shown", { arr(Str) }, optional = false, nullable = false),
            Field("reply", { Str }, optional = false, nullable = false),
            Field("done", { Bool }, optional = false, nullable = false),
            Field("phase", { TurnPhase.ADAPTER }, optional = false, nullable = false),
            Field("folds", { arr(FoldSegment.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> TurnRow(a[0] as Long, a[1] as Long, a[2] as String, a[3] as String, a[4] as String, a[5] as String, a[6] as String, a[7] as String, a[8] as Long, a[9] as Long, a[10] as Long, a[11] as Long, a[12] as Long, a[13] as Long, a[14] as Long, a[15] as List<String>, a[16] as String, a[17] as Boolean, a[18] as TurnPhase, a[19] as List<FoldSegment>) },
            { v -> arrayOf(v.at, v.line, v.uuid, v.start, v.end, v.startText, v.endText, v.said, v.tools, v.thinking, v.agents, v.background, v.retries, v.peers, v.fails, v.shown, v.reply, v.done, v.phase, v.folds) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class TurnSpan(
    val text: String,
    val from: Long?,
    val to: Long?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<TurnSpan> = obj(
            listOf(
            Field("text", { Str }, optional = false, nullable = false),
            Field("from", { opt(Lng) }, optional = false, nullable = true),
            Field("to", { opt(Lng) }, optional = false, nullable = true),
            ),
            { a -> TurnSpan(a[0] as String, a[1] as Long?, a[2] as Long?) },
            { v -> arrayOf(v.text, v.from, v.to) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class UiErrorPayload(
    val code: String,
    val key: String,
    val args: Map<String, String>,
    val said: String,
    val detail: String,
    val reconnect: String? = null,
    val at: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<UiErrorPayload> = obj(
            listOf(
            Field("code", { Str }, optional = false, nullable = false),
            Field("key", { Str }, optional = false, nullable = false),
            Field("args", { dict(Str) }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            Field("detail", { Str }, optional = false, nullable = false),
            Field("reconnect", { opt(Str) }, optional = true, nullable = true),
            Field("at", { Lng }, optional = false, nullable = false),
            ),
            { a -> UiErrorPayload(a[0] as String, a[1] as String, a[2] as Map<String, String>, a[3] as String, a[4] as String, a[5] as String?, a[6] as Long) },
            { v -> arrayOf(v.code, v.key, v.args, v.said, v.detail, v.reconnect, v.at) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Unavailable(
    val command: String,
    val code: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Unavailable> = obj(
            listOf(
            Field("command", { Str }, optional = false, nullable = false),
            Field("code", { Str }, optional = false, nullable = false),
            ),
            { a -> Unavailable(a[0] as String, a[1] as String) },
            { v -> arrayOf(v.command, v.code) },
        )
    }
}

enum class UnclearWhy(val wire: String) {
    NO_WRITER("noWriter"),
    UNTRACKED("untracked"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<UnclearWhy> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class UndoAt(
    val page: String,
    val tab: String? = null,
    val anchor: String? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<UndoAt> = obj(
            listOf(
            Field("page", { Str }, optional = false, nullable = false),
            Field("tab", { opt(Str) }, optional = true, nullable = true),
            Field("anchor", { opt(Str) }, optional = true, nullable = true),
            ),
            { a -> UndoAt(a[0] as String, a[1] as String?, a[2] as String?) },
            { v -> arrayOf(v.page, v.tab, v.anchor) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class UnfollowReq(
    val ticket: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<UnfollowReq> = obj(
            listOf(
            Field("ticket", { Str }, optional = false, nullable = false),
            ),
            { a -> UnfollowReq(a[0] as String) },
            { v -> arrayOf(v.ticket) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Uninstalled(
    val removed: Boolean,
    val said: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Uninstalled> = obj(
            listOf(
            Field("removed", { Bool }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            ),
            { a -> Uninstalled(a[0] as Boolean, a[1] as String) },
            { v -> arrayOf(v.removed, v.said) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class UnknownOnMachine(
    val machine: String,
    val records: Long?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<UnknownOnMachine> = obj(
            listOf(
            Field("machine", { Str }, optional = false, nullable = false),
            Field("records", { opt(Lng) }, optional = false, nullable = true),
            ),
            { a -> UnknownOnMachine(a[0] as String, a[1] as Long?) },
            { v -> arrayOf(v.machine, v.records) },
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
data class UploadItem(
    val localPath: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<UploadItem> = obj(
            listOf(
            Field("local_path", { Str }, optional = false, nullable = false),
            ),
            { a -> UploadItem(a[0] as String) },
            { v -> arrayOf(v.localPath) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Usage(
    val slot: String? = null,
    val window: String? = null,
    val value: String,
    val reset: String? = null,
    val text: String,
    val tone: Tone,
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
            Field("tone", { Tone.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> Usage(a[0] as String?, a[1] as String?, a[2] as String, a[3] as String?, a[4] as String, a[5] as Tone) },
            { v -> arrayOf(v.slot, v.window, v.value, v.reset, v.text, v.tone) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class UsageFact(
    val promptTokens: Long,
    val model: String?,
    val peakPromptTokens: Long,
    val limit: Long,
    val limitFrom: LimitFrom,
    val percent: Long?,
    val contextText: String,
    val contextTone: Tone,
    val promptTokensText: String,
    val limitText: String,
    val limitFromText: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<UsageFact> = obj(
            listOf(
            Field("promptTokens", { Lng }, optional = false, nullable = false),
            Field("model", { opt(Str) }, optional = false, nullable = true),
            Field("peakPromptTokens", { Lng }, optional = false, nullable = false),
            Field("limit", { Lng }, optional = false, nullable = false),
            Field("limitFrom", { LimitFrom.ADAPTER }, optional = false, nullable = false),
            Field("percent", { opt(Lng) }, optional = false, nullable = true),
            Field("contextText", { Str }, optional = false, nullable = false),
            Field("contextTone", { Tone.ADAPTER }, optional = false, nullable = false),
            Field("promptTokensText", { Str }, optional = false, nullable = false),
            Field("limitText", { Str }, optional = false, nullable = false),
            Field("limitFromText", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> UsageFact(a[0] as Long, a[1] as String?, a[2] as Long, a[3] as Long, a[4] as LimitFrom, a[5] as Long?, a[6] as String, a[7] as Tone, a[8] as String, a[9] as String, a[10] as String?) },
            { v -> arrayOf(v.promptTokens, v.model, v.peakPromptTokens, v.limit, v.limitFrom, v.percent, v.contextText, v.contextTone, v.promptTokensText, v.limitText, v.limitFromText) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class UserInputRow(
    val at: Long,
    val line: Long,
    val uuid: String,
    val timestamp: String,
    val excerpt: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<UserInputRow> = obj(
            listOf(
            Field("at", { Lng }, optional = false, nullable = false),
            Field("line", { Lng }, optional = false, nullable = false),
            Field("uuid", { Str }, optional = false, nullable = false),
            Field("timestamp", { Str }, optional = false, nullable = false),
            Field("excerpt", { Str }, optional = false, nullable = false),
            ),
            { a -> UserInputRow(a[0] as Long, a[1] as Long, a[2] as String, a[3] as String, a[4] as String) },
            { v -> arrayOf(v.at, v.line, v.uuid, v.timestamp, v.excerpt) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class UserInputs(
    val from: Long,
    val end: Long,
    val next: Long?,
    val entries: List<UserInputRow>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<UserInputs> = obj(
            listOf(
            Field("from", { Lng }, optional = false, nullable = false),
            Field("end", { Lng }, optional = false, nullable = false),
            Field("next", { opt(Lng) }, optional = false, nullable = true),
            Field("entries", { arr(UserInputRow.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> UserInputs(a[0] as Long, a[1] as Long, a[2] as Long?, a[3] as List<UserInputRow>) },
            { v -> arrayOf(v.from, v.end, v.next, v.entries) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class UserInputsReq(
    val path: String,
    val from: Long? = null,
    val limit: Long? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<UserInputsReq> = obj(
            listOf(
            Field("path", { Str }, optional = false, nullable = false),
            Field("from", { opt(Lng) }, optional = true, nullable = true),
            Field("limit", { opt(Lng) }, optional = true, nullable = true),
            ),
            { a -> UserInputsReq(a[0] as String, a[1] as Long?, a[2] as Long?) },
            { v -> arrayOf(v.path, v.from, v.limit) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class UserPath(
    val state: UserPathState,
    val said: String,
    val detail: String?,
    val command: String?,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<UserPath> = obj(
            listOf(
            Field("state", { UserPathState.ADAPTER }, optional = false, nullable = false),
            Field("said", { Str }, optional = false, nullable = false),
            Field("detail", { opt(Str) }, optional = false, nullable = true),
            Field("command", { opt(Str) }, optional = false, nullable = true),
            ),
            { a -> UserPath(a[0] as UserPathState, a[1] as String, a[2] as String?, a[3] as String?) },
            { v -> arrayOf(v.state, v.said, v.detail, v.command) },
        )
    }
}

enum class UserPathOp(val wire: String) {
    READ("read"),
    ADD("add"),
    REMOVE("remove"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<UserPathOp> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class UserPathReq(
    val op: UserPathOp,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<UserPathReq> = obj(
            listOf(
            Field("op", { UserPathOp.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> UserPathReq(a[0] as UserPathOp) },
            { v -> arrayOf(v.op) },
        )
    }
}

enum class UserPathState(val wire: String) {
    ON("on"),
    OFF("off"),
    UNKNOWN("unknown"),
    NOT_WINDOWS("not_windows"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<UserPathState> = enumOf(entries) { it.wire }
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
data class VerifyCheck(
    val level: CheckLevel,
    val account: String?,
    val text: String,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<VerifyCheck> = obj(
            listOf(
            Field("level", { CheckLevel.ADAPTER }, optional = false, nullable = false),
            Field("account", { opt(Str) }, optional = false, nullable = true),
            Field("text", { Str }, optional = false, nullable = false),
            ),
            { a -> VerifyCheck(a[0] as CheckLevel, a[1] as String?, a[2] as String) },
            { v -> arrayOf(v.level, v.account, v.text) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class VerifyReport(
    val pass: Boolean,
    val fails: Int,
    val warns: Int,
    val checks: List<VerifyCheck>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<VerifyReport> = obj(
            listOf(
            Field("pass", { Bool }, optional = false, nullable = false),
            Field("fails", { Int32 }, optional = false, nullable = false),
            Field("warns", { Int32 }, optional = false, nullable = false),
            Field("checks", { arr(VerifyCheck.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> VerifyReport(a[0] as Boolean, a[1] as Int, a[2] as Int, a[3] as List<VerifyCheck>) },
            { v -> arrayOf(v.pass, v.fails, v.warns, v.checks) },
        )
    }
}

enum class VersionRelation(val wire: String) {
    SAME("same"),
    OLDER("older"),
    NEWER("newer"),
    INCOMPARABLE("incomparable"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<VersionRelation> = enumOf(entries) { it.wire }
    }
}

@Suppress("UNCHECKED_CAST")
data class WatchAsk(
    val sids: List<String>,
    val branch: List<BranchHeld>? = null,
    val kept: List<String>? = null,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<WatchAsk> = obj(
            listOf(
            Field("sids", { arr(Str) }, optional = false, nullable = false),
            Field("branch", { opt(arr(BranchHeld.ADAPTER)) }, optional = true, nullable = true),
            Field("kept", { opt(arr(Str)) }, optional = true, nullable = true),
            ),
            { a -> WatchAsk(a[0] as List<String>, a[1] as List<BranchHeld>?, a[2] as List<String>?) },
            { v -> arrayOf(v.sids, v.branch, v.kept) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class WatchFrom(
    val sid: String,
    val path: String,
    val seq: Long,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<WatchFrom> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("path", { Str }, optional = false, nullable = false),
            Field("seq", { U64 }, optional = false, nullable = false),
            ),
            { a -> WatchFrom(a[0] as String, a[1] as String, a[2] as Long) },
            { v -> arrayOf(v.sid, v.path, v.seq) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class WatchReply(
    val from: List<WatchFrom>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<WatchReply> = obj(
            listOf(
            Field("from", { arr(WatchFrom.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> WatchReply(a[0] as List<WatchFrom>) },
            { v -> arrayOf(v.from) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Whereabout(
    val sid: String,
    val standing: StandingWord,
    val names: List<String>,
    val terminals: List<TerminalAt>,
    val run: Run,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Whereabout> = obj(
            listOf(
            Field("sid", { Str }, optional = false, nullable = false),
            Field("standing", { StandingWord.ADAPTER }, optional = false, nullable = false),
            Field("names", { arr(Str) }, optional = false, nullable = false),
            Field("terminals", { arr(TerminalAt.ADAPTER) }, optional = false, nullable = false),
            Field("run", { Run.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> Whereabout(a[0] as String, a[1] as StandingWord, a[2] as List<String>, a[3] as List<TerminalAt>, a[4] as Run) },
            { v -> arrayOf(v.sid, v.standing, v.names, v.terminals, v.run) },
        )
    }
}

@Suppress("UNCHECKED_CAST")
data class Whereabouts(
    val results: List<Whereabout>,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<Whereabouts> = obj(
            listOf(
            Field("results", { arr(Whereabout.ADAPTER) }, optional = false, nullable = false),
            ),
            { a -> Whereabouts(a[0] as List<Whereabout>) },
            { v -> arrayOf(v.results) },
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
            Field("resets", { opt(U64) }, optional = true, nullable = true),
            Field("resetsText", { opt(Str) }, optional = true, nullable = true),
            Field("rate", { opt(Dbl) }, optional = true, nullable = true),
            Field("tokensPerPct", { opt(U64) }, optional = true, nullable = true),
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
    val pct: Int? = null,
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
    val tone: Tone,
) : Ordered {
    override var wireOrder: IntArray? = null

    companion object {
        val ADAPTER: JsonAdapter<WindowShow> = obj(
            listOf(
            Field("name", { Str }, optional = false, nullable = false),
            Field("key", { opt(Str) }, optional = true, nullable = true),
            Field("pct", { opt(Int32) }, optional = true, nullable = true),
            Field("resetsAt", { opt(U64) }, optional = true, nullable = true),
            Field("resetsAtText", { opt(Str) }, optional = true, nullable = true),
            Field("resetsAtRelText", { opt(Str) }, optional = true, nullable = true),
            Field("seenAt", { U64 }, optional = false, nullable = false),
            Field("seenAtText", { opt(Str) }, optional = true, nullable = true),
            Field("seenAtRelText", { opt(Str) }, optional = true, nullable = true),
            Field("from", { QuotaSource.ADAPTER }, optional = false, nullable = false),
            Field("tokensPerPct", { opt(U64) }, optional = true, nullable = true),
            Field("perPctText", { opt(Str) }, optional = true, nullable = true),
            Field("resetSinceSeen", { opt(Bool) }, optional = true, nullable = true),
            Field("tone", { Tone.ADAPTER }, optional = false, nullable = false),
            ),
            { a -> WindowShow(a[0] as String, a[1] as String?, a[2] as Int?, a[3] as Long?, a[4] as String?, a[5] as String?, a[6] as Long, a[7] as String?, a[8] as String?, a[9] as QuotaSource, a[10] as Long?, a[11] as String?, a[12] as Boolean?, a[13] as Tone) },
            { v -> arrayOf(v.name, v.key, v.pct, v.resetsAt, v.resetsAtText, v.resetsAtRelText, v.seenAt, v.seenAtText, v.seenAtRelText, v.from, v.tokensPerPct, v.perPctText, v.resetSinceSeen, v.tone) },
        )
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

sealed interface WireSessionContainer {
    /** 判别格（`host`）那个词。 */
    val host: String

    @Suppress("UNCHECKED_CAST")
    data class Tmux(
        val terminal: String? = null,
    ) : WireSessionContainer, Ordered {
        override val host: String get() = "tmux"
        override var wireOrder: IntArray? = null
    }
    @Suppress("UNCHECKED_CAST")
    data class Managed(
        val terminal: String? = null,
    ) : WireSessionContainer, Ordered {
        override val host: String get() = "managed"
        override var wireOrder: IntArray? = null
    }
    data object None : WireSessionContainer {
        override val host: String get() = "none"
    }

    companion object {
        val ADAPTER: JsonAdapter<WireSessionContainer> =
            tagged<WireSessionContainer>(
                "host",
                listOf(
                Variant("tmux", WireSessionContainer.Tmux::class.java, obj(
                        listOf(
                        Field("host", { tag("tmux") }, optional = false, nullable = false),
                        Field("terminal", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> WireSessionContainer.Tmux(a[1] as String?) },
                        { v -> arrayOf("tmux", v.terminal) },
                    )),
                Variant("managed", WireSessionContainer.Managed::class.java, obj(
                        listOf(
                        Field("host", { tag("managed") }, optional = false, nullable = false),
                        Field("terminal", { opt(Str) }, optional = true, nullable = true),
                        ),
                        { a -> WireSessionContainer.Managed(a[1] as String?) },
                        { v -> arrayOf("managed", v.terminal) },
                    )),
                Variant("none", WireSessionContainer.None::class.java, obj(
                        listOf(
                        Field("host", { tag("none") }, optional = false, nullable = false),
                        ),
                        { WireSessionContainer.None },
                        { arrayOf("none") },
                    )),
                ),
            )
    }
}

enum class WireTerminalHost(val wire: String) {
    TMUX("tmux"),
    MANAGED("managed"),
    ;

    companion object {
        val ADAPTER: JsonAdapter<WireTerminalHost> = enumOf(entries) { it.wire }
    }
}

typealias Words = String

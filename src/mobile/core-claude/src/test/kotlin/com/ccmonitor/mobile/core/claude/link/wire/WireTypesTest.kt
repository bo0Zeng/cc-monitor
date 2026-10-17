package com.ccmonitor.mobile.core.claude.link.wire

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 手机的线上类（`WireTypes.kt`）只是出口声明，Moshi 照它在编译期生成解码器：每一格的线上键（`@Json(name)`，没写 ⇒ 参数名）
 * 都要在 `cmd-wire` 导出的那份 TS 里（`src/frontend/ui/generated/<名>.ts`）、型对得上；这里不可空的，那边必有且不为 null；
 * 枚举的线上字（每个常量的 `@Json(name)`）与 TS 那一列两向相等（那边多一个值这边就红，不会装成别的）。
 * 每个类都要有生成的解码器（`<名>JsonAdapter`）：不靠运行时反射认名字，混淆过的包里照样成立。
 * 防的是：后端改了一格的名字或型，Rust 那一份解得开、这里却悄悄装成 null。
 */
class WireTypesTest {
    @Test
    fun everyDeclaredCellIsInTheGeneratedTypeWithTheSameShape() {
        val decl = Decl.parse(SOURCE.readText())
        assertTrue("WireTypes.kt 里一个线上类都没认出来 —— 本条在空转", decl.classes.size >= 10)
        val problems = decl.problems + decl.classes.flatMap { c -> check(decl, c) }
        assertTrue(problems.joinToString("\n"), problems.isEmpty())
    }

    /** Moshi 那一侧：每个声明类都有 KSP 生成的解码器（没有 ⇒ Moshi 退回反射，混淆过的包里认不出名字）。 */
    @Test
    fun everyDeclaredClassHasAGeneratedAdapter() {
        val decl = Decl.parse(SOURCE.readText())
        assertTrue("WireTypes.kt 里一个线上类都没认出来 —— 本条在空转", decl.classes.size >= 10)
        val missing =
            decl.classes.map { "$PACKAGE.${it.name}JsonAdapter" }.filter { runCatching { Class.forName(it) }.isFailure }
        assertTrue("没有生成的解码器：$missing", missing.isEmpty())
    }

    /** 自检：改坏一格（键不在 · 可空性对不上 · 枚举少一个值）都认得出。 */
    @Test
    fun theCheckerCatchesRenamedNullableAndEnumDrift() {
        val bad =
            Decl.parse(
                """
                @WireOf("TailPlan")
                @JsonClass(generateAdapter = true)
                data class TailPlan(
                    val end: Long,
                    @Json(name = "split_at") val splitAt: Long?,
                    val tailFromX: Long,
                )

                @WireOf("Sent")
                @JsonClass(generateAdapter = true)
                data class Sent(
                    val result: SentResult,
                    val why: String,
                )

                @WireOf("Needs")
                @JsonClass(generateAdapter = true)
                data class Needs(
                    val text: Long,
                    val head: String,
                )

                @WireOf("Killed")
                data class Killed(
                    val killed: Boolean,
                )

                @JsonClass(generateAdapter = false)
                enum class SentResult {
                    @Json(name = "delivered") DELIVERED,
                    @Json(name = "refused") REFUSED,
                }

                @JsonClass(generateAdapter = false)
                enum class ModeSwitchResult {
                    @Json(name = "done") DONE,
                    UNSURE,
                }

                @WireOf("Speaker")
                @JsonClass(generateAdapter = true)
                data class Speaker(
                    val kind: SpeakerKind,
                    val command: String,
                    val stdout: String?,
                )

                @JsonClass(generateAdapter = false)
                enum class SpeakerKind {
                    @Json(name = "human") HUMAN,
                    @Json(name = "bashOutput") BASH_OUTPUT,
                }

                @WireOf("LineRecord")
                @JsonClass(generateAdapter = true)
                data class LineRecord(
                    val id: String,
                    val results: Map<String, Long>?,
                    val excerpt: String?,
                )
                """.trimIndent(),
            )
        val got = (bad.problems + bad.classes.flatMap { check(bad, it) }).joinToString("\n")
        assertTrue(got, got.contains("tailFromX"))
        assertTrue(got, got.contains("Sent.why"))
        assertTrue(got, got.contains("SentResult"))
        assertTrue("少了 @JsonClass 的类认得出：$got", got.contains("Killed"))
        assertTrue("没写线上字的枚举常量认得出：$got", got.contains("ModeSwitchResult.UNSURE"))
        assertTrue("别名那一型（Words ＝ string）照它指的核：$got", got.contains("Needs.text"))
        assertTrue("联合：只在一支里有的格这里不可空 ⇒ 认得出：$got", got.contains("Speaker.command"))
        assertTrue("联合：标签那一格的字照各支的并集两向核：$got", got.contains("SpeakerKind"))
        assertTrue("表（`{ [key in string]: X }`）里那一型照 X 核：$got", got.contains("LineRecord.results"))
        assertEquals("可空那一格 TS 那边必有 ⇒ 不算错；别名对得上的那一格 · 只在一支里有、这里可空的那一格不算错", 9, got.lines().size)
    }

    /** 装（Moshi 生成的解码器）：嵌套 · 列表 · 枚举 · 线上键；多出来的键照收；不可空那一格缺了或来了 null ⇒ 装不进（null），不补默认值。 */
    @Test
    fun bindFillsTheDeclaredCellsAndRefusesANullInANonNullCell() {
        val list =
            Wire.bind<TerminalsListReply>(
                mapOf(
                    "terminals" to
                        listOf(
                            mapOf(
                                "terminal" to "t1",
                                "tmux_name" to "demo",
                                "title" to "claude",
                                "session" to mapOf("sid" to "s1"),
                                "clients" to listOf(mapOf("since" to 3.0, "kind" to "terminal-window")),
                            ),
                        ),
                    "complete" to true,
                ),
            )
        assertEquals(TerminalItem("t1", "demo", "claude", TermSession("s1", null), listOf(Attached(3))), list?.terminals?.single())
        assertEquals(SentResult.REFUSED, Wire.bind<Sent>(mapOf("result" to "refused", "said" to "x"))?.result)
        assertNull(Wire.bind<Sent>(mapOf("result" to "maybe")))
        assertNull("不可空的 cmd 缺了", Wire.bind<Rendered>(mapOf("account" to null)))
        assertNull("基本型那一格缺了", Wire.bind<Held>(mapOf("path" to "/p", "next" to 1.0, "mark" to "m")))
        assertNotNull(Wire.bind<Held>(mapOf("path" to "/p", "next" to 1.0, "next_byte" to 2.0, "mark" to "m")))
    }

    private fun check(
        decl: Decl,
        c: Decl.Klass,
    ): List<String> {
        val ts = Ts.load(c.wire) ?: return listOf("${c.name}：没有 ${c.wire}.ts")
        val fields = ts.fields ?: return listOf("${c.name}：${c.wire}.ts 不是一个对象类型")
        return c.cells.mapNotNull { cell ->
            val f = fields[cell.key] ?: return@mapNotNull "${c.name}.${cell.name}：${c.wire} 里没有 `${cell.key}`"
            val tsNullable = f.optional || f.type.split("|").any { it.trim() == "null" }
            if (!cell.nullable && tsNullable) return@mapNotNull "${c.name}.${cell.name}：这里不可空，${c.wire}.${cell.key} 可缺或可为 null"
            val core =
                f.type
                    .split("|")
                    .map(String::trim)
                    .filter { it != "null" }
                    .joinToString(" | ")
            fits(decl, cell.type, core)?.let { "${c.name}.${cell.name}：$it" }
        }
    }

    /** Kotlin 的型对 TS 的型；对得上 ⇒ `null`，否则那一句。 */
    private fun fits(
        decl: Decl,
        kt: String,
        ts: String,
    ): String? {
        val list = Regex("^List<(.+)>$").find(kt)
        if (list != null) {
            val el = Regex("^Array<(.+)>$").find(ts) ?: return "List 对的是 `$ts`"
            return fits(decl, list.groupValues[1], el.groupValues[1])
        }
        val map = Regex("^Map<String, (.+)>$").find(kt)
        if (map != null) {
            val el = Regex("^\\{ \\[key in string\\]\\??: (.+) \\}$").find(ts) ?: return "Map 对的是 `$ts`"
            return fits(decl, map.groupValues[1], el.groupValues[1])
        }
        decl.enums[kt]?.let { words ->
            val lits = Ts.wordsOf(ts) ?: Ts.load(ts)?.literals ?: return "枚举 $kt 对的 `$ts` 不是一列字面量"
            return if (lits.toSet() == words.toSet()) null else "枚举 $kt 的线上字是 $words，$ts 是 $lits"
        }
        decl.classes.firstOrNull { it.name == kt }?.let { return if (it.wire == ts) null else "$kt 声明的是 ${it.wire}，这里是 `$ts`" }
        // 别名（`Words` ＝ `string`）：照它指的那一型核。
        Ts.load(ts)?.alias?.let { return fits(decl, kt, it) }
        val literal = Regex("^\"[^\"]*\"( \\| \"[^\"]*\")*$")
        val ok =
            when (kt) {
                "String" -> ts == "string" || literal.matches(ts) || Ts.load(ts)?.literals != null
                "Long", "Int", "Double" -> ts == "number"
                "Boolean" -> ts == "boolean" || ts == "true" || ts == "false"
                "Any" -> true
                else -> false
            }
        return if (ok) null else "$kt 对的是 `$ts`"
    }

    /** `WireTypes.kt` 按行读（格式在那份头注里守死）。[problems]：格式本身的毛病（类少了 `@JsonClass` · 枚举常量没写线上字）。 */
    private class Decl(
        val classes: List<Klass>,
        val enums: Map<String, List<String>>,
        val problems: List<String>,
    ) {
        class Cell(
            val name: String,
            val key: String,
            val type: String,
            val nullable: Boolean,
        )

        class Klass(
            val name: String,
            val wire: String,
            val cells: List<Cell>,
        )

        companion object {
            private val CLASS =
                Regex("""@WireOf\("(\w+)"\)\s*@JsonClass\(generateAdapter = true\)\s*data class (\w+)\(\n(.*?)\n\)""", RegexOption.DOT_MATCHES_ALL)
            private val ANY_CLASS = Regex("""data class (\w+)\(""")
            private val CELL = Regex("""^(?:@Json\(name = "(\w+)"\)\s*)?val (\w+): ([\w<>]+(?:, [\w<>]+)?)(\?)?,?$""")
            private val ENUM = Regex("""@JsonClass\(generateAdapter = false\)\s*enum class (\w+) \{([^}]*)\}""")
            private val ANY_ENUM = Regex("""enum class (\w+)""")
            private val CONSTANT = Regex("""^@Json\(name = "([^"]*)"\)\s*(\w+)$""")

            fun parse(src: String): Decl {
                val classes =
                    CLASS.findAll(src).map { m ->
                        val cells =
                            m.groupValues[3].lines().map(String::trim).filter(String::isNotEmpty).map { l ->
                                val c = CELL.find(l) ?: error("认不出这一格：$l")
                                Cell(c.groupValues[2], c.groupValues[1].ifEmpty { c.groupValues[2] }, c.groupValues[3], c.groupValues[4] == "?")
                            }
                        Klass(m.groupValues[2], m.groupValues[1], cells)
                    }
                val problems = mutableListOf<String>()
                val enums =
                    ENUM.findAll(src).associate { m ->
                        val name = m.groupValues[1]
                        name to
                            m.groupValues[2]
                                .split(",")
                                .map(String::trim)
                                .filter(String::isNotEmpty)
                                .mapNotNull { c ->
                                    CONSTANT.find(c)?.groupValues?.get(1) ?: null.also { problems += "$name.$c：枚举常量没写线上字（@Json(name = …)）" }
                                }
                    }
                val classes2 = classes.toList()
                ANY_CLASS.findAll(src).map { it.groupValues[1] }.filter { n -> classes2.none { it.name == n } }.forEach {
                    problems += "$it：线上类要写成 `@WireOf(…)` ＋ `@JsonClass(generateAdapter = true)`"
                }
                ANY_ENUM.findAll(src).map { it.groupValues[1] }.filter { it !in enums }.forEach {
                    problems += "$it：枚举要写 `@JsonClass(generateAdapter = false)`（Moshi 自带的混淆规则只保这样的枚举）"
                }
                return Decl(classes2, enums, problems)
            }
        }
    }

    /** ts-rs 导出的那一份：去掉注释与 import，认对象类型的各格、或一列字符串字面量。 */
    private class Ts(
        val fields: Map<String, Field>?,
        val literals: List<String>?,
        /** `export type X = string;` 这种别名指的那一型（只认基本型）。 */
        val alias: String? = null,
    ) {
        class Field(
            val type: String,
            val optional: Boolean,
        )

        companion object {
            fun load(name: String): Ts? {
                val f = File(GENERATED, "$name.ts").takeIf { Regex("^\\w+$").matches(name) && it.isFile } ?: return null
                val body =
                    f
                        .readText()
                        .replace(Regex("/\\*\\*.*?\\*/", RegexOption.DOT_MATCHES_ALL), "")
                        .lines()
                        .filterNot { it.startsWith("//") || it.startsWith("import ") }
                        .joinToString(" ")
                val rhs =
                    Regex("export type \\w+(?:<[^>]*>)? = (.*);")
                        .find(body)
                        ?.groupValues
                        ?.get(1)
                        ?.trim() ?: return null
                wordsOf(rhs)?.let { return Ts(null, it) }
                if (rhs in setOf("string", "number", "boolean")) return Ts(null, null, rhs)
                return Ts(objectOf(rhs), null)
            }

            /** 一列字符串字面量（`"a" | "b"`）⇒ 那几个字；不是 ⇒ `null`。 */
            fun wordsOf(ts: String): List<String>? =
                if (Regex("^\"[^\"]*\"( \\| \"[^\"]*\")*$").matches(ts)) Regex("\"([^\"]*)\"").findAll(ts).map { it.groupValues[1] }.toList() else null

            /**
             * 对象类型的各格：`{ … }` · 交（`A & B`，各格相并）· 标签联合（`{ "t": "a", … } | { "t": "b", … }`，ts-rs 出的平铺联合）。
             * 联合按各支并集：每支都有、都不可缺的格才不可缺；型是各支的型并起来（标签那一格 ⇒ 各支的字并成一列）。
             */
            private fun objectOf(ts: String): Map<String, Field>? {
                val t = ts.trim()
                split(t, '&').takeIf { it.size > 1 }?.let { parts ->
                    val all = parts.map { objectOf(it) ?: return null }
                    return all.fold(emptyMap()) { acc, m -> acc + m }
                }
                if (t.startsWith("(") && t.endsWith(")")) return objectOf(t.substring(1, t.length - 1))
                split(t, '|').takeIf { it.size > 1 }?.let { parts ->
                    val branches = parts.map { objectOf(it) ?: return null }
                    val keys = branches.flatMap { it.keys }.distinct()
                    return keys.associateWith { k ->
                        val got = branches.mapNotNull { it[k] }
                        Field(got.map { it.type }.distinct().joinToString(" | "), got.size < branches.size || got.any { it.optional })
                    }
                }
                if (!t.startsWith("{") || !t.endsWith("}")) return null
                return fields(t.substring(1, t.length - 1))
            }

            /** 顶层（`{}` `<>` `()` `[]` 之外、字符串之外）按 [sep] 切。 */
            private fun split(
                s: String,
                sep: Char,
            ): List<String> {
                val parts = mutableListOf<String>()
                var depth = 0
                var quoted = false
                val cur = StringBuilder()
                for (ch in s) {
                    if (ch == '"') quoted = !quoted
                    if (!quoted) {
                        when (ch) {
                            '{', '<', '(', '[' -> depth++
                            '}', '>', ')', ']' -> depth--
                        }
                    }
                    if (ch == sep && depth == 0 && !quoted) {
                        parts += cur.toString()
                        cur.clear()
                    } else {
                        cur.append(ch)
                    }
                }
                parts += cur.toString()
                return parts.map(String::trim)
            }

            /** 顶层逗号切格（`{}` `<>` `()` `[]`（元组）里的不切）。 */
            private fun fields(inner: String): Map<String, Field> {
                val parts = mutableListOf<String>()
                var depth = 0
                val cur = StringBuilder()
                for (ch in inner) {
                    when (ch) {
                        '{', '<', '(', '[' -> depth++
                        '}', '>', ')', ']' -> depth--
                    }
                    if (ch == ',' && depth == 0) {
                        parts += cur.toString()
                        cur.clear()
                    } else {
                        cur.append(ch)
                    }
                }
                parts += cur.toString()
                return parts.map(String::trim).filter(String::isNotEmpty).associate { p ->
                    val colon = p.indexOf(':')
                    val k = p.substring(0, colon).trim()
                    k.removeSuffix("?").trim('"') to Field(p.substring(colon + 1).trim(), k.endsWith("?"))
                }
            }
        }
    }

    private companion object {
        val ROOT: File =
            generateSequence(File("").absoluteFile) { it.parentFile }
                .firstOrNull { File(it, "src/backend/lib.rs").isFile }
                ?: error("往上找不到仓根")
        val GENERATED = File(ROOT, "src/frontend/ui/generated")
        val SOURCE = File(ROOT, "src/mobile/core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/link/wire/WireTypes.kt")
        const val PACKAGE = "com.ccmonitor.mobile.core.claude.link.wire"
    }
}

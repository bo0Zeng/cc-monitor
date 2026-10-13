package com.ccmonitor.mobile.core.claude.link.wire

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 手机的线上类（`WireTypes.kt`）只是出口声明：每一格都要在 `cmd-wire` 导出的那份 TS 里（`src/frontend/ui/generated/<名>.ts`）、
 * 型对得上；这里不可空的，那边必有且不为 null；枚举两向相等（那边多一个值这边就红，不会装成别的）。
 * 防的是：后端改了一格的名字或型，Rust 那一份解得开、这里却悄悄装成 null。
 */
class WireTypesTest {
    @Test
    fun everyDeclaredCellIsInTheGeneratedTypeWithTheSameShape() {
        val decl = Decl.parse(SOURCE.readText())
        assertTrue("WireTypes.kt 里一个线上类都没认出来 —— 本条在空转", decl.classes.size >= 10)
        val problems = decl.classes.flatMap { c -> check(decl, c) }
        assertTrue(problems.joinToString("\n"), problems.isEmpty())
    }

    /** 自检：改坏一格（键不在 · 可空性对不上 · 枚举少一个值）都认得出。 */
    @Test
    fun theCheckerCatchesRenamedNullableAndEnumDrift() {
        val bad =
            Decl.parse(
                """
                @WireOf("TailPlan")
                data class TailPlan(
                    val end: Long,
                    @Key("split_at") val splitAt: Long?,
                    val tailFromX: Long,
                )

                @WireOf("Sent")
                data class Sent(
                    val result: SentResult,
                    val why: String,
                )

                enum class SentResult { DELIVERED, REFUSED }
                """.trimIndent(),
            )
        val got = bad.classes.flatMap { check(bad, it) }.joinToString("\n")
        assertTrue(got, got.contains("tailFromX"))
        assertTrue(got, got.contains("Sent.why"))
        assertTrue(got, got.contains("SentResult"))
        assertEquals("可空那一格 TS 那边必有 ⇒ 不算错", 3, got.lines().size)
    }

    /** 装：嵌套 · 列表 · 枚举 · 线上键；不可空那一格来了 null ⇒ 装不进（null），不补默认值。 */
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
        assertNull("基本型那一格缺了", Wire.bind<TailPlan>(mapOf("end" to 1.0, "split_at" to 2.0, "tail_from" to 3.0)))
        assertNotNull(Wire.bind<TailPlan>(mapOf("end" to 1.0, "split_at" to 2.0, "tail_from" to 3.0, "total" to 4.0)))
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
        decl.enums[kt]?.let { consts ->
            val lits = Ts.load(ts)?.literals ?: return "枚举 $kt 对的 `$ts` 不是一列字面量"
            val want = lits.map(String::uppercase).toSet()
            return if (want == consts.toSet()) null else "枚举 $kt 是 $consts，$ts 是 $lits"
        }
        decl.classes.firstOrNull { it.name == kt }?.let { return if (it.wire == ts) null else "$kt 声明的是 ${it.wire}，这里是 `$ts`" }
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

    /** `WireTypes.kt` 按行读（格式在那份头注里守死）。 */
    private class Decl(
        val classes: List<Klass>,
        val enums: Map<String, List<String>>,
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
            private val CLASS = Regex("""@WireOf\("(\w+)"\)\s*data class (\w+)\(\n(.*?)\n\)""", RegexOption.DOT_MATCHES_ALL)
            private val CELL = Regex("""^(?:@Key\("(\w+)"\)\s*)?val (\w+): ([\w<>]+)(\?)?,?$""")
            private val ENUM = Regex("""enum class (\w+) \{([^}]*)\}""")

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
                val enums =
                    ENUM.findAll(src).associate {
                        it.groupValues[1] to
                            it.groupValues[2]
                                .split(",")
                                .map(String::trim)
                                .filter(String::isNotEmpty)
                    }
                return Decl(classes.toList(), enums)
            }
        }
    }

    /** ts-rs 导出的那一份：去掉注释与 import，认对象类型的各格、或一列字符串字面量。 */
    private class Ts(
        val fields: Map<String, Field>?,
        val literals: List<String>?,
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
                    Regex("export type \\w+ = (.*);")
                        .find(body)
                        ?.groupValues
                        ?.get(1)
                        ?.trim() ?: return null
                if (Regex("^\"[^\"]*\"( \\| \"[^\"]*\")*$").matches(rhs)) {
                    return Ts(null, Regex("\"([^\"]*)\"").findAll(rhs).map { it.groupValues[1] }.toList())
                }
                if (!rhs.startsWith("{") || !rhs.endsWith("}")) return Ts(null, null)
                return Ts(fields(rhs.substring(1, rhs.length - 1)), null)
            }

            /** 顶层逗号切格（`{}` `<>` `()` 里的不切）。 */
            private fun fields(inner: String): Map<String, Field> {
                val parts = mutableListOf<String>()
                var depth = 0
                val cur = StringBuilder()
                for (ch in inner) {
                    when (ch) {
                        '{', '<', '(' -> depth++
                        '}', '>', ')' -> depth--
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
    }
}

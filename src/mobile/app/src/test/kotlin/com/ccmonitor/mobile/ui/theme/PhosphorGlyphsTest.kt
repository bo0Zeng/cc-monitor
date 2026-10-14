package com.ccmonitor.mobile.ui.theme

import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.nio.ByteBuffer

/**
 * 图标表 `Ph` 与子集字体 `phosphor_regular.ttf` 两向相等，表里每一项都有人用（「只打用到的码位」）。
 * 字体缺一个码位 ⇒ 那个图标画成豆腐块；字体多出来的 ⇒ 白占 APK。重做字体：`scripts/phosphor-subset.py`。
 */
class PhosphorGlyphsTest {
    private val mobile: File =
        generateSequence(File("").absoluteFile) { it.parentFile }
            .map { File(it, "src/mobile") }
            .first { File(it, "settings.gradle.kts").isFile }
    private val fontFile = File(mobile, "core-ui/src/main/res/font/phosphor_regular.ttf")

    /** 字体 `cmap` 表里（format 4，基本平面）映射到非 0 字形的码位。手写读：单测类路径上没有 java.awt。 */
    private val mapped: Set<Int> by lazy {
        assertTrue("子集字体不在：${fontFile.path}", fontFile.isFile)
        val b = ByteBuffer.wrap(fontFile.readBytes())
        val tables = b.getShort(4).toInt() and 0xFFFF
        val cmap =
            (0 until tables)
                .map { 12 + it * 16 }
                .first { String(ByteArray(4) { i -> b.get(it + i) }) == "cmap" }
                .let { b.getInt(it + 8) }
        val sub =
            (0 until (b.getShort(cmap + 2).toInt() and 0xFFFF))
                .map { cmap + b.getInt(cmap + 4 + it * 8 + 4) }
                .first { (b.getShort(it).toInt() and 0xFFFF) == 4 }
        val segX2 = b.getShort(sub + 6).toInt() and 0xFFFF
        val ends = sub + 14
        val starts = ends + segX2 + 2
        val deltas = starts + segX2
        val ranges = deltas + segX2
        val out = mutableSetOf<Int>()
        for (i in 0 until segX2 / 2) {
            val end = b.getShort(ends + i * 2).toInt() and 0xFFFF
            val start = b.getShort(starts + i * 2).toInt() and 0xFFFF
            val delta = b.getShort(deltas + i * 2).toInt()
            val rangeAt = ranges + i * 2
            val range = b.getShort(rangeAt).toInt() and 0xFFFF
            for (c in start..end) {
                if (c == 0xFFFF) continue
                val glyph =
                    if (range == 0) {
                        (c + delta) and 0xFFFF
                    } else {
                        val g = b.getShort(rangeAt + range + (c - start) * 2).toInt() and 0xFFFF
                        if (g == 0) 0 else (g + delta) and 0xFFFF
                    }
                if (glyph != 0) out += c
            }
        }
        out
    }

    @Test
    fun everyIconIsInTheFontAndTheFontHasNothingElse() {
        val inTable = Ph.entries.map { it.codepoint }.toSet()
        assertTrue("字体 cmap 一个码位都没读出来（判据自己失效了）", mapped.isNotEmpty())
        val missing = Ph.entries.filterNot { it.codepoint in mapped }.map { "${it.name} ${Integer.toHexString(it.codepoint)}" }
        assertEquals("这几个图标字体里没有（画出来是豆腐块）：重跑 scripts/phosphor-subset.py", emptyList<String>(), missing)
        val extra = mapped.filter { it in 0xE000..0xF8FF && it !in inTable }.map(Integer::toHexString)
        assertEquals("字体里有 Ph 表之外的码位：重跑 scripts/phosphor-subset.py", emptyList<String>(), extra)
    }

    @Test
    fun everyIconIsUsedSomewhere() {
        val code =
            listOf("app", "core-ui")
                .flatMap { m -> File(mobile, "$m/src/main").walkTopDown().filter { it.isFile && it.extension == "kt" && it.name != "Ph.kt" }.toList() }
                .joinToString("\n") { KotlinSourceScanner.codeOnlyDroppingLiterals(it.readText()) }
        assertTrue("一份界面源码都没读到（判据自己失效了）", code.length > 10_000)
        val unused = Ph.entries.filterNot { Regex("""\bPh\.${it.name}\b""").containsMatchIn(code) }.map { it.name }
        assertEquals("Ph 表里这几项没人用：删掉再重跑 scripts/phosphor-subset.py", emptyList<String>(), unused)
    }
}

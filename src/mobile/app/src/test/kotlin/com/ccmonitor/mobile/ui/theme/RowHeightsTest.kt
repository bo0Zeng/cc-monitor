package com.ccmonitor.mobile.ui.theme

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 列表行高照手机适配层（`Touch`，稿 `外观对齐` 第三节：单行 48 · 双行 64 · 设置一项 56 · 文件行 52 · 抽屉项 52），逐屏登记：
 * 登记的每个画行的函数里要引到那一档 `Touch.rowX`，且不许自己写行高数（`height(56.dp)` · `heightIn(min = 48.dp)` · 常量）。
 * 消息流正文（Markwon 那个 TextView）字号 · 行高取 `bodyLarge`（`--font-size-prose` 15 / `--line-height-prose` 1.65），不写死。
 */
class RowHeightsTest {
    private val app: File =
        generateSequence(File("").absoluteFile) { it.parentFile }
            .map { File(it, "src/mobile/app/src/main/kotlin/com/ccmonitor/mobile/ui") }
            .first { it.isDirectory }

    /** 文件 → 画行的函数 → 那一档。 */
    private val rows =
        mapOf(
            "overview/ConversationsScreen.kt" to mapOf("DotRow" to "rowDouble"),
            "host/HostScreens.kt" to mapOf("HostRow" to "rowDouble"),
            "drawer/AppDrawer.kt" to mapOf("EntryRow" to "rowDrawer", "RecentSection" to "rowDrawer", "ServerRow" to "rowDrawer"),
            "files/FilesScreen.kt" to mapOf("EntryRow" to "rowFile"),
            "settings/SettingsScreen.kt" to mapOf("EntrySections" to "rowSetting"),
            "settings/ButtonSettingsScreen.kt" to mapOf("GroupRow" to "rowSetting", "ButtonRow" to "rowSetting"),
            "session/SessionTabsSheet.kt" to mapOf("SessionTabRowItem" to "rowSingle"),
            "session/CommandPalette.kt" to mapOf("PaletteRow" to "rowSingle"),
            "session/TmuxManager.kt" to mapOf("TmuxSessionRow" to "rowDouble"),
        )

    /** `fun name(` 起、到配对的那个 `}` 止（代码里，注释与字面量已去掉）。 */
    private fun body(
        code: String,
        name: String,
    ): String? {
        val at = Regex("""fun\s+(?:\w+\.)?$name\s*\(""").find(code)?.range?.first ?: return null
        val open = code.indexOf('{', code.indexOf(')', at).coerceAtLeast(at))
        var depth = 0
        for (i in open until code.length) {
            when (code[i]) {
                '{' -> depth++
                '}' -> if (--depth == 0) return code.substring(at, i + 1)
            }
        }
        return null
    }

    @Test
    fun everyListRowTakesItsHeightFromTheAdapterLayer() {
        val literal = Regex("""\b(height|heightIn)\s*\([^)]*\b\d+(\.\d+)?\.dp|ROW_DP|ROW_MIN_HEIGHT""")
        val wrong =
            rows.flatMap { (rel, fns) ->
                val f = File(app, rel)
                assertTrue("登记的文件不在（判据自己失效了）：$rel", f.isFile)
                val code = KotlinSourceScanner.codeOnlyDroppingLiterals(f.readText())
                fns.mapNotNull { (fn, tier) ->
                    val b = body(code, fn) ?: return@mapNotNull "$rel：没有 fun $fn（登记表过期了）"
                    when {
                        !b.contains("Touch.$tier") -> "$rel · $fn：没引到 Touch.$tier"
                        literal.containsMatchIn(b) -> "$rel · $fn：自己写了行高数「${literal.find(b)!!.value}」"
                        else -> null
                    }
                }
            }
        assertEquals("列表行高没照适配层", emptyList<String>(), wrong)
    }

    @Test
    fun proseTakesSizeAndLineHeightFromTheTypeScale() {
        val f = File(app, "claude/MarkdownText.kt")
        val code = KotlinSourceScanner.codeOnlyDroppingLiterals(f.readText())
        val tv = body(code, "MarkdownText") ?: error("没有 fun MarkdownText")
        assertTrue("正文 TextView 没取 bodyLarge（--font-size-prose / --line-height-prose）", tv.contains("typography.bodyLarge"))
        val hard = Regex("""textSize\s*=\s*\d|setLineSpacing\(""")
        assertEquals("正文字号 / 行高写死了", null, hard.find(tv)?.value)
    }
}

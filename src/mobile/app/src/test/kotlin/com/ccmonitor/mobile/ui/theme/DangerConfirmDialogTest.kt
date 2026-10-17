package com.ccmonitor.mobile.ui.theme

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 危险确认框只有一处（`core-ui` 的 `DangerConfirmDialog`）：一出来焦点在「取消」上，回车 / 返回都不误删。
 * - ① 手机生产源码里，除那一份，没有哪个 `AlertDialog(` 的确认键是红的（危险确认）；同一把尺子量那一份必须命中（正控）；
 * - ② 那一份的取消键带焦点：可聚焦 ＋ 挂了请求器 ＋ 一出来就请求。
 * 只看源码：触屏 / 实体键盘上焦点真落在取消上，交真机。
 */
class DangerConfirmDialogTest {
    private val mobile: File =
        generateSequence(File("").absoluteFile) { it.parentFile }
            .map { File(it, "src/mobile") }
            .first { File(it, "settings.gradle.kts").isFile }
    private val wrapper = "core-ui/src/main/kotlin/com/ccmonitor/mobile/core/ui/dialog/DangerConfirmDialog.kt"

    private fun mainSources(): List<File> =
        mobile
            .listFiles()
            .orEmpty()
            .map { File(it, "src/main") }
            .filter { it.isDirectory }
            .flatMap { d -> d.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList() }

    /** 每个 `AlertDialog(` 调用的括号内文（按括号配对切）。 */
    private fun dialogs(code: String): List<String> =
        Regex("""\bAlertDialog\s*\(""")
            .findAll(code)
            .map { m ->
                var depth = 0
                var end = m.range.last
                for (i in m.range.last until code.length) {
                    when (code[i]) {
                        '(' -> depth++
                        ')' -> depth--
                    }
                    if (depth == 0) {
                        end = i
                        break
                    }
                }
                code.substring(m.range.last + 1, end)
            }.toList()

    /** 一个框里某个槽（`confirmButton` / `dismissButton`）的内文：从槽名切到下一个槽名或框尾。 */
    private fun slot(
        dialog: String,
        name: String,
    ): String {
        val at = dialog.indexOf("$name =").takeIf { it >= 0 } ?: return ""
        val next =
            Regex("""\b(onDismissRequest|confirmButton|dismissButton|title|text|icon)\s*=""")
                .findAll(dialog, at + name.length)
                .firstOrNull()
                ?.range
                ?.first ?: dialog.length
        return dialog.substring(at, next)
    }

    private fun dangerDialogs(f: File): List<String> =
        dialogs(KotlinSourceScanner.codeOnlyDroppingLiterals(f.readText())).filter { slot(it, "confirmButton").contains("danger = true") }

    @Test
    fun everyDangerConfirmGoesThroughTheOneDialog() {
        val files = mainSources()
        assertTrue("手机生产源码一份都没扫到（判据自己失效了）", files.size > 100)
        val w = File(mobile, wrapper)
        assertTrue("那一份不在：$wrapper", w.isFile)
        assertEquals("尺子量不出那一份自己的危险确认（判据自己失效了）", 1, dangerDialogs(w).size)
        val hits = files.filter { it.relativeTo(mobile).path != wrapper }.filter { dangerDialogs(it).isNotEmpty() }.map { it.relativeTo(mobile).path }
        assertEquals("自己拼了危险确认框（确认键红）：改调 DangerConfirmDialog（焦点在取消上）", emptyList<String>(), hits)
    }

    @Test
    fun theOneDialogFocusesCancel() {
        val cancel = slot(dangerDialogs(File(mobile, wrapper)).single(), "dismissButton")
        for (needle in listOf("canFocus = true", ".focusRequester(", ".requestFocus()")) {
            assertTrue("取消那一键缺 `$needle`：一出来焦点不在取消上", cancel.contains(needle))
        }
    }
}

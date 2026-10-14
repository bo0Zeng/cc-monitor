package com.ccmonitor.mobile.ui.theme

import com.ccmonitor.mobile.core.ui.button.secondaryContentColor
import com.ccmonitor.mobile.core.ui.theme.AppColorScheme
import com.ccmonitor.mobile.core.ui.theme.AppTokens
import com.ccmonitor.mobile.core.ui.theme.ThemeTokens
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 次按钮（字按钮）的字统一 `--text`：M3 的 `TextButton` 默认拿 `primary`（`--accent-strong`）当字色，在 `--card` 上不到 3:1。
 * 一处包装（`core-ui` 的 `SecondaryButton`），各屏只调它：
 * - ① 手机全部生产源码里，除包装那一份，不许出现 M3 的 `TextButton`（代码里，注释不算）；
 * - ② 包装有人用（各屏都改调它了，不是空壳）；
 * - ③ 包装的字色：常态 ＝ `--text`，危险项 ＝ `--error-text`（照映射现算，不写死）。
 */
class SecondaryButtonTest {
    private val mobile: File =
        generateSequence(File("").absoluteFile) { it.parentFile }
            .map { File(it, "src/mobile") }
            .first { File(it, "settings.gradle.kts").isFile }
    private val wrapper = "core-ui/src/main/kotlin/com/ccmonitor/mobile/core/ui/button/SecondaryButton.kt"

    private fun mainSources(): List<File> =
        mobile
            .listFiles()
            .orEmpty()
            .map { File(it, "src/main") }
            .filter { it.isDirectory }
            .flatMap { d -> d.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList() }

    @Test
    fun noBareTextButtonOutsideTheWrapper() {
        assertTrue("包装那一份不在：$wrapper", File(mobile, wrapper).isFile)
        val files = mainSources()
        assertTrue("手机生产源码一份都没扫到（判据自己失效了）", files.size > 100)
        val bare = Regex("""\bTextButton\b""")
        val hits =
            files.filter { it.relativeTo(mobile).path != wrapper }.flatMap { f ->
                KotlinSourceScanner.codeOnlyDroppingLiterals(f.readText()).lines().withIndex().mapNotNull { (i, l) ->
                    if (bare.containsMatchIn(l)) "${f.relativeTo(mobile)}:${i + 1}  ${l.trim()}" else null
                }
            }
        assertEquals("直接用了 M3 的 TextButton：改调 SecondaryButton（字 ＝ --text）", emptyList<String>(), hits)
        assertTrue(
            "包装里没有 M3 的 TextButton（判据自己失效了）",
            bare.containsMatchIn(KotlinSourceScanner.codeOnlyDroppingLiterals(File(mobile, wrapper).readText())),
        )
    }

    @Test
    fun theWrapperIsWhatTheScreensCall() {
        val call = Regex("""\bSecondaryButton\s*\(""")
        val callers = mainSources().filter { it.relativeTo(mobile).path != wrapper && call.containsMatchIn(KotlinSourceScanner.codeOnlyDroppingLiterals(it.readText())) }
        assertTrue("SecondaryButton 只有 ${callers.size} 份在调（各屏没改过来）", callers.size >= 10)
    }

    @Test
    fun secondaryTextIsTextAndDangerIsErrorText() {
        assertEquals(ThemeTokens.Text, secondaryContentColor(AppColorScheme, AppTokens(), danger = false))
        assertEquals(ThemeTokens.ErrorText, secondaryContentColor(AppColorScheme, AppTokens(), danger = true))
    }
}

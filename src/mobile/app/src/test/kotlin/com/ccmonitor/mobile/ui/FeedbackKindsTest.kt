package com.ccmonitor.mobile.ui

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 反馈只两种（两端同一条规矩）：一次性结局走根部那一个底部条（`LocalAppSnackbar`），持续状态就地（机器条 · 格下那一句）。
 * Android Toast 是第三种样子（不带撤销、不带复制详情、读屏也读不全），手机生产源码里一处都不许有。
 */
class FeedbackKindsTest {
    private val toast = Regex("""\bandroid\.widget\.Toast\b|\bToast\.makeText\(""")

    @Test
    fun `手机生产源码里没有 Toast`() {
        val root = generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }.first { File(it, "settings.gradle.kts").isFile }
        val sources = File(root, "app/src/main/kotlin").walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
        assertTrue("前提：得扫到生产源码（实得 ${sources.size} 份）", sources.size > 50)
        assertTrue("前提：扫到了底部条那一处的用法", sources.any { KotlinSourceScanner.codeOnlyKeepingLiterals(it.readText()).contains("LocalAppSnackbar.current") })
        val hits =
            sources.flatMap { f ->
                val code = KotlinSourceScanner.codeOnlyKeepingLiterals(f.readText())
                toast.findAll(code).map { m -> "${f.name}:${code.substring(0, m.range.first).count { it == '\n' } + 1}" }.toList()
            }
        assertEquals("这几处还在弹 Toast（一次性结局改走底部条）", emptyList<String>(), hits)
    }

    @Test
    fun `正控：同一个式子逮得住两种写法`() {
        assertTrue(toast.containsMatchIn("import android.widget.Toast"))
        assertTrue(toast.containsMatchIn("Toast.makeText(context, s, Toast.LENGTH_SHORT).show()"))
        assertTrue(!toast.containsMatchIn("snackbar.show(s)"))
    }
}

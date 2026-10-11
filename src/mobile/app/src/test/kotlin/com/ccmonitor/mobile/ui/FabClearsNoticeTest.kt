package com.ccmonitor.mobile.ui

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 悬浮按钮与提示同处一个排版：提示（根部那一个宿主）出来时，每个悬浮按钮按它占的高度往上让（`Modifier.clearOfNotice()`）。
 * 不让的话长提示（带「复制详情」）会盖住「+」，点不到（冒烟第一步撞上过）。
 * 冒烟台架在模拟器上量真屏（`smoke.py` 第一步：提示在时「+」的底边在提示之上）；这里钉的是每个悬浮按钮都接了那一格。
 */
class FabClearsNoticeTest {
    private val fab = Regex("""\b(?:Extended)?FloatingActionButton\(""")

    @Test
    fun `每个悬浮按钮都按提示的高度让开`() {
        val root = generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }.first { File(it, "settings.gradle.kts").isFile }
        val sources = File(root, "app/src/main/kotlin").walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
        val missing = mutableListOf<String>()
        var calls = 0
        for (f in sources) {
            val code = KotlinSourceScanner.codeOnlyKeepingLiterals(f.readText())
            for (m in fab.findAll(code)) {
                calls++
                val call = code.substring(m.range.first, (m.range.first + CALL_SPAN).coerceAtMost(code.length))
                if (!call.contains(".clearOfNotice()")) missing += "${f.name}:${code.substring(0, m.range.first).count { it == '\n' } + 1}"
            }
        }
        assertTrue("前提：得扫到悬浮按钮（实得 $calls 个）", calls >= 3)
        assertEquals("这些悬浮按钮没按提示让开（盖住时点不到）", emptyList<String>(), missing)
    }

    private companion object {
        /** 调用开头往后看多远找 modifier（那几处都在头几行里）。 */
        const val CALL_SPAN = 400
    }
}

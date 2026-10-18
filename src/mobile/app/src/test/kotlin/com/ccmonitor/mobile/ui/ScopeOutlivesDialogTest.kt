package com.ccmonitor.mobile.ui

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 框里点了一下、先关框再起协程（推公钥选了那把 · 抓屏点了复制）：协程作用域若是在「框开着才走到」的那一段里取的，
 * 框一关它就跟着撤，那件事被取消、结局那一句永远不出（冒烟撞上过：推公钥选了钥匙之后什么都不说）。
 * 所以每个 `rememberCoroutineScope()` 都在函数体顶层、在任何提前 `return` 之前取（框关了它还在，事照样做完）。
 *
 * 判别力边界：只看取作用域那一处的位置（函数体顶层 · 之前没有提前返回）；一个整体只在框开着时才排进来的可组合函数
 * 自己取作用域、又在关框后靠它做事的那一形看不出来（眼下没有：那几个框里起的协程都不关框）。
 */
class ScopeOutlivesDialogTest {
    private val scope = Regex("""\brememberCoroutineScope\(\)""")
    private val fn = Regex("""(?m)^(?:(?:private|internal|public) )?fun """)
    private val early = Regex("""\breturn\b(?!@)""")

    @Test
    fun `协程作用域在函数体顶层、提前返回之前取`() {
        val root = generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }.first { File(it, "settings.gradle.kts").isFile }
        val sources = File(root, "app/src/main/kotlin").walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
        val bad = mutableListOf<String>()
        var seen = 0
        for (f in sources) {
            val code = KotlinSourceScanner.codeOnlyKeepingLiterals(f.readText())
            for (m in scope.findAll(code)) {
                seen++
                val at = m.range.first
                val line = code.lastIndexOf('\n', at) + 1
                val head =
                    fn
                        .findAll(code.substring(0, at))
                        .lastOrNull()
                        ?.range
                        ?.first ?: 0
                val where = "${f.name}:${code.substring(0, at).count { it == '\n' } + 1}"
                when {
                    !code.substring(line, at).matches(Regex("""^ {4}val \w+ = $""")) -> bad += "$where（不在函数体顶层）"
                    early.containsMatchIn(code.substring(head, line)) -> bad += "$where（前面有提前返回）"
                }
            }
        }
        assertTrue("前提：得扫到取作用域的那几处（实得 $seen 处）", seen >= 10)
        assertEquals("这几处的作用域会随框关掉而撤（关框后起的协程被取消）", emptyList<String>(), bad)
    }
}

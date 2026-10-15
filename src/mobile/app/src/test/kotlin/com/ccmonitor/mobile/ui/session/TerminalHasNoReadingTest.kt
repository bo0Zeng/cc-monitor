package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 终端屏只有终端：不带「阅读」子页、不带历史页、tmux 弹层里没有「读」。
 * 看一条会话的正文只在聊天屏（`ChatTranscript`）一处。
 *
 * 判法：终端屏那一包（`ui/session`）的生产源码里，一处都不出现正文那一套（聊天屏的正文组件 · 会话正文的读口 · 记录 ⇒ 单元）
 * 与 tab 条。买不到：换个名字另写一份阅读看不见（那要靠「正文只有一处实现」的代码审）。
 */
class TerminalHasNoReadingTest {
    private fun sessionSources(): List<File> {
        val dir = File("src/main/kotlin/com/ccmonitor/mobile/ui/session").takeIf { it.isDirectory } ?: File("app/src/main/kotlin/com/ccmonitor/mobile/ui/session")
        return dir.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
    }

    @Test
    fun theTerminalScreenCarriesNoReadingPage() {
        val files = sessionSources()
        assertTrue("前提：得真扫到终端屏那一包（实得 ${files.size} 个）", files.size > 5)
        val hits =
            files.flatMap { f ->
                val code = KotlinSourceScanner.codeOnlyKeepingLiterals(f.readText())
                FORBIDDEN.filter { it in code }.map { "${f.name}：$it" }
            }
        assertEquals("终端屏又长出了阅读那一套（正文回聊天屏看）", emptyList<String>(), hits)
    }

    companion object {
        private val FORBIDDEN =
            listOf(
                "ChatTranscript",
                "ClaudeReadingPane",
                "ChatSession",
                "RecordUnits",
                "HistoryTail",
                "HistoryRead",
                "HistoryList",
                "ClaudeHistorySheet",
                "TabRow(",
            )
    }
}

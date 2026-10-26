package com.ccmonitor.mobile.ui

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 给人看的分隔符与 tmux 会话那几个字只住文案表（`kit.text.sep` · `terminal.tag.tmux`）：`app` · `core-claude` 的 `src/main` 字符串字面量里
 * 不许出现「·」与「tmux:」（拼出来的句子换一份文案就对不上桌面那一侧）。
 *
 * 只看字面量本身（不换成文案表那句 zh：表里的句子本来就带「·」）；测试源不扫。
 */
class HardcodedSeparatorTest {
    private val banned = listOf("·", "tmux:")

    /**
     * 允许的例外：`文件名 to 理由`。每一条都是还没收进文案表的那一处，表空出来就删掉这一条、改读键。
     */
    private val allowed =
        mapOf(
            "SessionTabsSheet.kt" to "同名 tab 的编号「{label} ·{n}」还没有文案键（table.json 此刻在别的权力范围手里）",
        )

    private fun mainDir(module: String): File {
        val rel = "$module/src/main/kotlin/com/ccmonitor/mobile"
        // 单测的工作目录是模块目录（`app/`）；从仓根跑时路径已经对了。
        return File(rel).takeIf { it.isDirectory } ?: File("../$rel")
    }

    private fun scan(module: String): List<String> {
        val dir = mainDir(module)
        assertTrue("找不到 $module 的 main 源目录（判据自己失效了）：${dir.absolutePath}", dir.isDirectory)
        return dir
            .walkTopDown()
            .filter { it.isFile && it.extension == "kt" }
            .flatMap { f ->
                KotlinSourceScanner
                    .literalsOf(f.readText())
                    .asSequence()
                    .filter { lit -> banned.any { lit.body.contains(it) } }
                    .filter { lit -> !(f.name in allowed && lit.body.contains(" ·$")) }
                    .map { "${f.name}:${it.line}  \"${it.body}\"" }
            }.toList()
    }

    @Test
    fun noLiteralSpellsTheSeparatorOrTheTmuxTag() {
        val hits = listOf("app", "core-claude").flatMap { scan(it) }
        assertEquals("字面量里写死了分隔符或 tmux 字样：改读 kit.text.sep · terminal.tag.tmux\n" + hits.joinToString("\n"), emptyList<String>(), hits)
    }
}

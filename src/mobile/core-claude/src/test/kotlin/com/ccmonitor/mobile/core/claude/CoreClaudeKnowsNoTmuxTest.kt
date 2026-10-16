package com.ccmonitor.mobile.core.claude

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * `core-claude` 不许知道 tmux：「怎么跑」（tmux 编排）全在那台核心。
 * `kotlin("jvm")` 只守住了「没有 Android」那一半，「没有 tmux 语法」这一半由这条守。
 *
 * 判据只看代码：注释里提 tmux 是正常的，所以扫描前先剥注释，剥完还剩 `tmux` 的才算在碰它。
 * 只认小写 `tmux`：`newInTmux` 这类名字是合法的。
 * 放过的只有协议里的线上词（[WIRE_WORDS]）：`session-open` 那一格「起在哪」· 指名的会话名 · `session-new-facts` 的「有没有 tmux」·
 * `terminals-list` 每行的会话名。那是请那台核心在它的 tmux 里做，手机不碰 tmux。
 */
class CoreClaudeKnowsNoTmuxTest {
    private companion object {
        val WIRE_WORDS = listOf("put(\"run\", \"tmux\")", "\"tmuxName\" to ", "val tmux: Boolean", "@Json(name = \"tmux_name\")")
    }

    /** 本模块的 main 源集。测试的工作目录可能是仓根或模块目录，两种都试。 */
    private fun mainSources(): List<File> {
        val dir =
            File("src/main/kotlin").takeIf { it.isDirectory }
                ?: File("core-claude/src/main/kotlin")
        return dir.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
    }

    /**
     * 剥注释、留字面量，走词法扫描器 [KotlinSourceScanner.codeOnlyKeepingLiterals]。
     *
     * 注意：必须留字面。`tmux` 在生产代码里恰恰住在命令串字面里；这是「零命中 = 守住了」形状的判据，
     * 连字面一起删就恒绿。扫描器看不见的情形写在 [KotlinSourceScanner] 的头注里。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)

    @Test
    fun coreClaudeContainsNoTmuxSyntax() {
        val files = mainSources()
        assertTrue("前提：得真扫到源文件，否则这条测试是空跑", files.size > 10)

        val offenders =
            files
                .mapNotNull { f ->
                    val hits =
                        codeOnly(f.readText())
                            .lineSequence()
                            .withIndex()
                            .filter { (_, line) -> WIRE_WORDS.fold(line) { l, w -> l.replace(w, "") }.contains("tmux") }
                            .map { (i, line) -> "${f.name}:${i + 1} ${line.trim().take(80)}" }
                            .toList()
                    hits.takeIf { it.isNotEmpty() }
                }.flatten()

        assertTrue(
            "core-claude 不许出现 tmux 语法（「怎么跑」全在那台核心）：\n" + offenders.joinToString("\n"),
            offenders.isEmpty(),
        )
    }

    /** 前提自检：剥注释这一步真的在起作用，而不是把整份文件剥光了才绿。 */
    @Test
    fun theCommentStrippingActuallyStripsCommentsAndKeepsCode() {
        val sample =
            """
            /** 这段 KDoc 里提 tmux 是合法的 */
            // 这行注释里也提 tmux
            val real = "tmux kill-session"
            """.trimIndent()
        val code = codeOnly(sample)
        assertTrue("代码里的必须留下来", code.contains("tmux kill-session"))
        assertFalse("KDoc 里的必须被剥掉", code.contains("这段 KDoc"))
        assertFalse("行注释里的必须被剥掉", code.contains("这行注释"))
    }
}

package com.ccmonitor.mobile.core.remote

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

/**
 * [ShellWord]：判据交给真的 `/bin/sh` 去拆词，不比字符串（两层引号在字符串上看也像是「对的」）。
 * 量法：`printf '[%s]' <词>`，shell 拆出几个参数就打几对方括号，展开成了什么也原样可见。
 *
 * 依赖 `/bin/sh`；缺了就红，不 skip。
 * 子进程环境先清空：只给假的 `HOME` 与 `PATH`，不碰跑测试那台机器的任何配置目录。
 */
class ShellWordTest {
    @get:Rule
    val tmp = TemporaryFolder()

    /** 测试自己的受信表达式 —— 不借生产的 `CLAUDE_CONFIG_DIR`，免得误展开到真的配置目录。 */
    private val trusted = "\${ATERM_T_DIR:-\$HOME/.t}"

    /** 让真 `/bin/sh` 拆这个词，回 `[参数1][参数2]…`。cwd = 临时目录（命令若被注入执行，产物落在这里）。 */
    private fun shellSplits(
        word: ShellWord,
        home: String = "/h o m e",
    ): String {
        check(File(SH).canExecute()) { "本机没有可执行的 $SH —— 这条判据靠真 shell，缺了要红，不 skip" }
        val pb = ProcessBuilder(SH, "-c", "printf '[%s]' $word").directory(tmp.root).redirectErrorStream(true)
        pb.environment().clear()
        pb.environment().putAll(mapOf("HOME" to home, "PATH" to "/usr/bin:/bin"))
        val p = pb.start()
        p.outputStream.close()
        val out = p.inputStream.readBytes().decodeToString()
        p.waitFor()
        return out
    }

    /** 受信默认表达式：远端展开，且家目录带空格也仍是一个参数。 */
    @Test
    fun theTrustedDefaultIsExpandedByTheShellIntoOneWord() {
        assertEquals("[/h o m e/.t/projects]", shellSplits(ShellWord.configDir(trusted, trusted, "/projects")))
    }

    /** 设置里填的目录 = 字面路径：空格、单引号、`$HOME` 一个字不改，也不展开。 */
    @Test
    fun anOverrideIsTakenLiterally() {
        val override = "/a 'b' \$HOME"
        assertEquals("[/a 'b' \$HOME/projects]", shellSplits(ShellWord.configDir(override, trusted, "/projects")))
    }

    /** 安全字符的尾巴原样接。 */
    @Test
    fun aSafeTailIsAppendedVerbatim() {
        val word = ShellWord.configDir(trusted, trusted) + "/projects/-x/11111111-2222-4333-8444-555566667777.jsonl"
        assertEquals("\"$trusted\"/projects/-x/11111111-2222-4333-8444-555566667777.jsonl", word.text)
        assertEquals("[/h o m e/.t/projects/-x/11111111-2222-4333-8444-555566667777.jsonl]", shellSplits(word))
    }

    /** 尾巴里有空格或 `;` ⇒ 被引起来，仍是同一个词；`;` 后面那段没有被执行。 */
    @Test
    fun anUnsafeTailStaysInsideTheSameWordAndIsNeverExecuted() {
        val word = ShellWord.literal("/a") + "/x y; touch pwned"
        assertEquals("[/a/x y; touch pwned]", shellSplits(word))
        assertFalse("尾巴里的命令不许被执行", File(tmp.root, "pwned").exists())
    }

    /** 进双引号的尾巴只许是调用方自己的常量 —— 带了双引号里的特殊字符就当场炸。 */
    @Test
    fun anInnerTailWithDoubleQuoteSpecialsIsRejected() {
        for (bad in listOf("/\$x", "/`id`", "/\"", "/\\")) {
            val threw = runCatching { ShellWord.configDir(trusted, trusted, bad) }.exceptionOrNull()
            assertTrue("$bad 必须被拒", threw is IllegalArgumentException)
        }
    }

    private companion object {
        const val SH = "/bin/sh"
    }
}

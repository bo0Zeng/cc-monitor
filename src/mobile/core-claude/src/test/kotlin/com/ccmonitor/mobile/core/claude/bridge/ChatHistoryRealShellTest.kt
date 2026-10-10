package com.ccmonitor.mobile.core.claude.bridge

import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.core.claude.transport.ClaudePaths
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

/**
 * 聊天屏「翻更早的历史」拼出来的那串命令，交给真的 `sh` 跑，读得到假目录里的会话记录。
 *
 * 用真 shell 是因为比字符串看不出引号层数的错：记录路径若被 quote 两次，远端 `awk` 会去开一个
 * 名字里带着引号字符、`${…}` 也没展开的文件，骨架读空 ⇒ 界面显示「已是最早」，零报错、历史永远空白。
 * 两层引号在字符串上看是「对的」，所以判据只能是跑一遍、看读到没有。
 *
 * 依赖：
 * - `/bin/sh` 以及 `PATH=/usr/bin:/bin` 上的 `awk`/`tail`/`head`。缺 `/bin/sh` 就红、不 skip：
 *   `assumeTrue` 会让这条在缺 shell 的机器上静默变成空真；这里只在 Linux 上构建，缺了是环境坏了。
 * - 子进程的环境先清空再给：只有假的 `HOME`、`PATH`、（需要时）假的 `CLAUDE_CONFIG_DIR`。
 *   不清空会继承本机的 `CLAUDE_CONFIG_DIR`，默认表达式就会去展开真的配置目录。
 *
 * 素材：记录形状照真实记录造（与 `ChatHistorySourceTest` 同形）；正文带中文，让 `LC_ALL=C` 的字节偏移
 * 真的走一遍多字节。项目目录名 `-home-u-proj` 是手写的，不调 `projectDirName` 去算：
 * 否则路径算法错了，造文件和读文件两侧会一起错、一起绿。
 */
class ChatHistoryRealShellTest {
    @get:Rule
    val tmp = TemporaryFolder()

    private val sid = "11111111-2222-4333-8444-555566667777"
    private val cwd = "/home/u/proj"

    private val records =
        listOf(
            """{"type":"user","message":{"role":"user","content":"早先说过的话"},"uuid":"u1","parentUuid":null,""" +
                """"timestamp":"2026-10-09T00:00:00Z","sessionId":"$sid"}""",
            """{"type":"assistant","message":{"id":"m1","role":"assistant","content":[{"type":"text","text":"早先的回答"}]},""" +
                """"uuid":"u2","parentUuid":"u1","timestamp":"2026-10-09T00:00:01Z","sessionId":"$sid"}""",
        )

    /** 在 [configDir] 下按 Claude 的落盘约定放一份会话记录。 */
    private fun plantRecord(configDir: File) {
        val dir = File(configDir, "projects/-home-u-proj").also { it.mkdirs() }
        File(dir, "$sid.jsonl").writeText(records.joinToString("\n", postfix = "\n"))
    }

    /** 家目录带空格：默认表达式那对双引号要防的正是展开后的 word-split。 */
    private fun fakeHome(): File = File(tmp.root, "h o m e").also { it.mkdirs() }

    /**
     * 把每条命令交给真的 `/bin/sh -c` 跑；回 stdout，stderr 记下来供失败时说清原因。
     *
     * 注意：stderr 落到文件而不是管道：两条管道都不读完就 `waitFor` 会在输出大时互相卡死。
     */
    private inner class RealShell(
        private val env: Map<String, String>,
    ) : RemoteCommandChannel {
        val transcript = StringBuilder()

        override fun exec(command: String): Flow<ByteArray> =
            flow {
                val err = tmp.newFile()
                val pb = ProcessBuilder(SH, "-c", command).redirectError(err)
                pb.environment().clear()
                pb.environment().putAll(env)
                val p = pb.start()
                p.outputStream.close()
                val out = p.inputStream.readBytes()
                val code = p.waitFor()
                transcript.append("\$ $command\n[exit=$code] stderr=${err.readText()}\n")
                emit(out)
            }
    }

    private fun shellEnv(
        home: File,
        claudeConfigDir: File? = null,
    ): Map<String, String> =
        buildMap {
            put("HOME", home.path)
            put("PATH", "/usr/bin:/bin")
            if (claudeConfigDir != null) put("CLAUDE_CONFIG_DIR", claudeConfigDir.path)
        }

    /** 照生产的拼法（`ChatRoute.attachHistory`）起一个历史源，翻一页，断言读回了假记录里的两句话。 */
    private fun assertHistoryIsReadBack(
        claudeDir: String,
        env: Map<String, String>,
    ) = runTest {
        check(File(SH).canExecute()) { "本机没有可执行的 $SH —— 这条判据靠真 shell，缺了要红，不 skip" }
        val shell = RealShell(env)
        // 与 `ChatRoute.attachHistory` 同一条取路径的调用，不手写路径
        val path = ClaudePaths.sessionRecordPath(claudeDir, cwd, sid)
        assertNotNull("前提：cwd 已知时必须答得出记录在哪", path)

        val page = ChatHistorySource(shell, path!!).loadOlder()

        assertNotNull("读不到记录 ⇒ 界面会显示「已是最早」而历史永远空白。远端实际跑了：\n${shell.transcript}", page)
        assertEquals(
            "助手那句要从磁盘上原样读回。远端实际跑了：\n${shell.transcript}",
            "早先的回答",
            page!!.filterIsInstance<BridgeFrame.AssistantText>().joinToString("") { it.x },
        )
        assertTrue(
            "用户那句也要在。远端实际跑了：\n${shell.transcript}",
            page.any { it is BridgeFrame.UserText && it.x.contains("早先说过的话") },
        )
    }

    /** 默认账号目录、没设 `CLAUDE_CONFIG_DIR` ⇒ 远端 shell 把默认表达式展开成 `$HOME/.claude`。 */
    @Test
    fun theDefaultConfigDirIsExpandedByTheRemoteShellAndTheRecordIsRead() {
        val home = fakeHome()
        plantRecord(File(home, ".claude"))
        assertHistoryIsReadBack(ClaudePaths.DEFAULT_CLAUDE_DIR, shellEnv(home))
    }

    /**
     * 默认账号目录、远端设了 `CLAUDE_CONFIG_DIR` ⇒ 要读的是那个目录。
     *
     * 家目录下故意不放记录：只有 `${CLAUDE_CONFIG_DIR:-…}` 真的在远端展开时才绿。
     */
    @Test
    fun theDefaultConfigDirFollowsClaudeConfigDirWhenTheRemoteHasItSet() {
        val account = File(tmp.root, "acct b").also { it.mkdirs() }
        plantRecord(account)
        assertHistoryIsReadBack(ClaudePaths.DEFAULT_CLAUDE_DIR, shellEnv(fakeHome(), claudeConfigDir = account))
    }

    /**
     * 设置里填了账号目录 ⇒ 它是字面路径；带空格、带单引号也得一个字不差地找到。
     *
     * 家目录与 `CLAUDE_CONFIG_DIR` 下都不放记录：读到了只能是因为照着填的那个目录找的。
     */
    @Test
    fun anOverriddenConfigDirIsTakenLiterallyEvenWithSpacesAndQuotes() {
        val override = File(tmp.root, "it's my dir/.claude")
        plantRecord(override)
        assertHistoryIsReadBack(override.path, shellEnv(fakeHome()))
    }

    /**
     * 同一个答案的另一个消费者：起管道时那句「这条对话有记录没有」（`[ -e <记录> ]`）。
     *
     * 两边读的必须是同一个文件：一边找得到、一边找不到，表现是「翻得到历史却每次都当新对话」
     * 或反过来，都静默。用生产拼出来的那段判定（截到 `fi;`），交给真 `sh`，看它选的是不是 `--resume`。
     */
    @Test
    fun theStartupProbeSeesTheSameRecordTheHistoryReads() {
        check(File(SH).canExecute()) { "本机没有可执行的 $SH —— 这条判据靠真 shell，缺了要红，不 skip" }
        val home = fakeHome()
        plantRecord(File(home, ".claude"))

        val cmd = ClaudeInvocation.pipeInvocation(null, sid, workdir = cwd, newSessionId = sid)
        val end = cmd.indexOf("fi; ")
        assertTrue("前提：命令里得真有那段判定，否则下面是空真：$cmd", cmd.contains("[ -e ") && end >= 0)
        val decide = cmd.substring(0, end + "fi;".length) + " printf '%s' \"\$ATERM_SID_FLAG\""

        val pb = ProcessBuilder(SH, "-c", decide).redirectErrorStream(true)
        pb.environment().clear()
        pb.environment().putAll(shellEnv(home))
        val p = pb.start()
        p.outputStream.close()
        val out = p.inputStream.readBytes().decodeToString()
        p.waitFor()

        assertEquals("记录在 ⇒ 判定必须选接着跑。远端跑的是：$decide", "--resume $sid", out)
    }

    private companion object {
        const val SH = "/bin/sh"
    }
}

package com.ccmonitor.mobile.core.claude.bridge

import com.ccmonitor.mobile.core.claude.command.PipeCommands
import com.ccmonitor.mobile.core.claude.transport.DaemonCommands
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

/**
 * 上行幂等追加：检查与追加在同一条远端命令里，且远端只跑一遍。
 *
 * 只断言命令串长什么样是不够的（那个串是我们自己拼的，它永远「对」）。这里把命令交给真 shell
 * 跑在真文件上，断言文件里有几行：量的是远端产出的字节，不是 app 的状态、不是退出码。
 * 数 exec 次数的那条（[theWholeSendIsExactlyOneRemoteExec]）只守「拆成两次 exec 了没有」，
 * 旁边配着这里的文件读数。
 *
 * 没有 `/bin/sh` 就红、不 skip：`assumeTrue` 会让这条在缺 shell 的机器上静默变成空真；
 * 这里只在 Linux 上构建，缺 `/bin/sh` 是环境坏了。
 */
class UplinkIdempotentAppendTest {
    @get:Rule
    val tmp = TemporaryFolder()

    private val sid = "abc123-DEF_456"

    /** 会话目录（相对登录 cwd），与生产同一条路径算法，不手写。 */
    private fun remoteRoot(): File =
        File(tmp.root, PipeCommands.sessionDir(sid)).also { it.mkdirs() }

    private fun inNdjson(): File = File(remoteRoot(), PipeCommands.IN)

    /**
     * 把一条已经拼好的远端命令交给真 `/bin/sh` 跑，cwd = 假的登录目录。回 stdout。
     *
     * 注意：先 `mkdirs` 会话目录。生产里那是 `PipeLauncher` 起管道时建的
     * （`ClaudeInvocation.pipeInvocation` 的 `mkdir -p`）。不建的话 `>>` 直接失败，
     * 而失败长得像「幂等生效了」（也是 0 行），会把本文件的判据变成假绿。
     */
    private fun runRemote(command: String): String {
        remoteRoot()
        val p =
            ProcessBuilder("/bin/sh", "-c", command)
                .directory(tmp.root)
                .redirectErrorStream(false)
                .start()
        val out = p.inputStream.readBytes().decodeToString()
        val err = p.errorStream.readBytes().decodeToString()
        val code = p.waitFor()
        // 远端 shell 自己炸了要响，不许静默变成「0 行」而被读成幂等生效
        check(code == 0) { "远端 shell 退 $code\ncmd=$command\nstdout=$out\nstderr=$err" }
        return out
    }

    /** 远端读数：`in.ndjson` 里非空行有几行。 */
    private fun remoteLineCount(): Int =
        inNdjson().takeIf { it.exists() }?.readLines()?.count { it.isNotBlank() } ?: 0

    /** 远端读数：某个标识在 `in.ndjson` 里出现几次。 */
    private fun remoteOccurrences(uplinkId: String): Int =
        inNdjson().takeIf { it.exists() }?.readText()?.let { text ->
            val needle = PipeSession.uplinkIdNeedle(uplinkId)
            var n = 0
            var at = text.indexOf(needle)
            while (at >= 0) {
                n++
                at = text.indexOf(needle, at + needle.length)
            }
            n
        } ?: 0

    private fun session() = PipeSession(NoChannel, sid)

    /**
     * 发一句 → 重发同一句 → 远端 `in.ndjson` 里那个标识只出现 1 次。
     *
     * 这是「断线重连后重发不进对话两遍」的远端字节读数。
     */
    @Test
    fun resendingTheSameMessageLeavesExactlyOneLineOnTheRemote() {
        val id = "aterm-0011223344556677889900aabbccddee-local#0"
        val cmd = session().idempotentAppendCommand("我这句话只许进去一次", id)

        val first = runRemote(cmd)
        assertTrue("第一次必须是「追加了」：$first", first.contains(PipeCommands.APPENDED_MARKER))
        assertEquals("前提：第一次真的写进去了", 1, remoteLineCount())

        val second = runRemote(cmd)
        assertTrue("第二次必须是「已经在里面了」：$second", second.contains(PipeCommands.DUPLICATE_MARKER))

        assertEquals("远端读数：那个标识只许出现 1 次", 1, remoteOccurrences(id))
        assertEquals("远端读数：文件只许有 1 行", 1, remoteLineCount())
    }

    /**
     * 反面：换一条消息就必须真的追加。
     *
     * 没有这条，一个「永远不追加」的实现（例如把 `grep` 写成恒真）也会让上一条全绿，
     * 而那个 bug 的形态是所有消息全被吞掉，比重复坏得多。
     */
    @Test
    fun aDifferentMessageStillGetsAppended() {
        val s = session()
        runRemote(s.idempotentAppendCommand("第一句", "aterm-aaaa0000aaaa0000aaaa0000aaaa0000-local#0"))
        runRemote(s.idempotentAppendCommand("第二句", "aterm-aaaa0000aaaa0000aaaa0000aaaa0000-local#1"))
        assertEquals("两条不同的消息必须都在", 2, remoteLineCount())

        // 正文一模一样、标识不同（真的说了两遍）⇒ 仍然要两行，不许被内容去重合并
        runRemote(s.idempotentAppendCommand("第一句", "aterm-aaaa0000aaaa0000aaaa0000aaaa0000-local#2"))
        assertEquals("说两遍同样的话不许被合并", 3, remoteLineCount())
    }

    /**
     * 真 shell 上的读数：正文含引号 / 换行 / 反斜杠 / 单引号时，
     * 那一行仍然是合法 JSON，且第二次仍然认得出重复。
     *
     * 针与那一行对不上时这条会红（第二次会变成追加 ⇒ 2 行）。
     */
    @Test
    fun nastyTextSurvivesTheShellAndStillDeduplicates() {
        val nasty = "他说\"你好\"\n还有 \\ 反斜杠、' 单引号、$ 美元号和 `反引号`"
        val id = "aterm-ffffeeeeddddccccbbbbaaaa99998888-local#9"
        val cmd = session().idempotentAppendCommand(nasty, id)

        runRemote(cmd)
        assertEquals("前提：第一次写进去了", 1, remoteLineCount())

        val line = inNdjson().readLines().first()
        val parsed =
            com.squareup.moshi.Moshi
                .Builder()
                .build()
                .adapter(Any::class.java)
                .fromJson(line) as Map<*, *>
        val msg = parsed["message"] as Map<*, *>
        assertEquals("正文过了 shell 还要逐字节还原", nasty, msg["content"])
        assertEquals("标识也要原样落在文件里", id, parsed[PipeSession.UPLINK_ID_KEY])

        runRemote(cmd)
        assertEquals("坏正文照样只许有 1 行", 1, remoteLineCount())
    }

    /**
     * 阴性读数：把检查拿掉、只做盲追加 ⇒ 远端真的出现两行相同的用户帧。
     *
     * 这一条证明上面那几条不是空真：它们守的那个 bug 真的会让读数变成 2。
     */
    @Test
    fun aBlindAppendReallyProducesTwoIdenticalLines() {
        val id = "aterm-1111111111111111111111111111abcd-local#0"
        val path = "'${PipeCommands.sessionDir(sid)}/${PipeCommands.IN}'"
        val payload = PipeSession.userLineJson("同一句话", id).replace("'", "'\\''")
        // 盲追加：`printf '%s\n' <payload> >> <path>`，只有追加、没有检查
        val blind = "printf '%s\\n' '$payload' >> $path"

        runRemote(blind)
        runRemote(blind)

        assertEquals("盲追加下远端读数是 2：这就是幂等判据会红的那一格", 2, remoteOccurrences(id))
        val distinct =
            inNdjson()
                .readLines()
                .filter { it.isNotBlank() }
                .toSet()
        assertEquals("两行逐字节相同", 1, distinct.size)
    }

    /**
     * 整条发送只有一次 `channel.exec`。
     *
     * 拆成两次 exec 的话，「远端只有一行」那条不一定红（大多数时候两次之间什么都没发生），
     * 所以承重的是这一条，上面几条是它的实证。
     */
    @Test
    fun theWholeSendIsExactlyOneRemoteExec() =
        runTest {
            val ch = CountingChannel("${PipeCommands.APPENDED_MARKER}\n\n${DaemonCommands.QUERY_OK_MARKER}\n")
            val sink = PipeUplinkSink(ch, session()) { null }

            sink.send(SendRequest("local#0", "一条消息"))
            assertEquals("检查与追加必须在同一条 exec 里：${ch.commands}", 1, ch.commands.size)

            val only = ch.commands.single()
            assertTrue("那一条里既要有检查", only.contains("grep -qF "))
            assertTrue("也要有追加", only.contains(" >> "))
        }

    /**
     * 第二次的结局落在「已经在里面了」那一档：
     * 既不是 [UplinkAppendOutcome.Appended]，也不是 [UplinkAppendOutcome.Failed]。
     *
     * 把「已经在里面了」并进「追加成功」⇒ 三档退回两档，界面再也说不出「这条已经发过了」。
     */
    @Test
    fun theSecondSendLandsInItsOwnAlreadyThereBucket() =
        runTest {
            val marker = DaemonCommands.QUERY_OK_MARKER
            val added = CountingChannel("${PipeCommands.APPENDED_MARKER}\n\n$marker\n")
            val dup = CountingChannel("${PipeCommands.DUPLICATE_MARKER}\n\n$marker\n")

            assertEquals(
                "前提：真写进去那次是 Appended",
                UplinkAppendOutcome.Appended,
                PipeUplinkSink(added, session()) { null }.sendReporting(SendRequest("local#0", "x")),
            )
            assertEquals(
                "重发那次必须落在「已经在里面了」这一档",
                UplinkAppendOutcome.AlreadyThere,
                PipeUplinkSink(dup, session()) { null }.sendReporting(SendRequest("local#0", "x")),
            )
        }

    /**
     * [UplinkAppendOutcome.asOutcome] 把「已经在里面了」折进 `Accepted`：两档的门面上分不出来。
     * 钉住这次折叠，要让门面区分三档时必须有意识地改它。
     */
    @Test
    fun theTwoStateFacadeCollapsesAlreadyThereIntoAccepted() {
        assertEquals(SendOutcome.Accepted, UplinkAppendOutcome.Appended.asOutcome())
        assertEquals(SendOutcome.Accepted, UplinkAppendOutcome.AlreadyThere.asOutcome())
        val failed = UplinkAppendOutcome.Failed("炸了", retryable = false).asOutcome()
        assertEquals(SendOutcome.Rejected("炸了", retryable = false), failed)
    }

    /** 数 exec 次数用的通道：每次都回同一段 stdout。 */
    private class CountingChannel(
        private val reply: String,
    ) : RemoteCommandChannel {
        val commands = mutableListOf<String>()

        override fun exec(command: String) = flowOf(reply.toByteArray()).also { commands += command }
    }

    /** 只用来造命令串、永不该被调用的通道。 */
    private object NoChannel : RemoteCommandChannel {
        override fun exec(command: String) = error("这条判据只造命令串，不该走通道")
    }
}

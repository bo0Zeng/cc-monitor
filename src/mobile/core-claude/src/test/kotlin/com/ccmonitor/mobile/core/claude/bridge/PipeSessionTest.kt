package com.ccmonitor.mobile.core.claude.bridge

import com.ccmonitor.mobile.core.claude.command.PipeCommands
import com.ccmonitor.mobile.core.claude.transport.DaemonCommands
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.withContext
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.util.concurrent.Executors

/**
 * 常驻管道两头的接线。
 *
 * 桩喂的是真 golden 的行，不是手编的：单测 fixture 与桩是同一件事的两个 fake，只修一个的话真机上会出错。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class PipeSessionTest {
    private companion object {
        /** 线程归属那条判据的上限：挂死比失败糟。 */
        const val FIVE_SECONDS = 5_000L

        /** 收集者跑在这个专属线程上：生产里它是主线程，与解析用的池天然不同。 */
        const val COLLECTOR_THREAD = "test-collector"

        /**
         * 这里探活一律说「在听」。
         *
         * 这里的判据量的是别的东西（回显语义 / 成功标记 / 长度闸门 / 取消语义），带上「对面在不在」
         * 会让每条判据变成两条断言的合体，一红就分不清是哪半红。「对面不在」由 `PipeSessionInterruptTest` 钉。
         */
        val LISTENING = PipeListening { null }
    }

    private val sid = "abc123-DEF_456"

    private class FakeChannel(
        private val reply: (String) -> String,
    ) : RemoteCommandChannel {
        val commands = mutableListOf<String>()

        override fun exec(command: String) = flowOf(reply(command).toByteArray()).also { commands += command }
    }

    /** 真 golden 的那 14 行（原样，逐字节）。 */
    private fun goldenOutNdjson(): String {
        val dir = File("../bridge/vectors").takeIf { it.isDirectory } ?: File("bridge/vectors")
        return File(dir, "cli-text-short.cli.ndjson")
            .readLines()
            .filterNot { it.contains("__meta__") }
            .joinToString("\n") { line ->
                // 剥掉录制信封（`{"t_ns":…,"line":{…}}`），只留管道真会写进 out.ndjson 的那一行
                val at = line.indexOf("\"line\":")
                // 桩要喂契约信封（`{"t_ns":…,"event":…}`），不是裸行：桩必须覆盖真实形状
                "{\"t_ns\":1,\"event\":" + line.substring(at + 7).removeSuffix("}") + "}"
            } + "\n"
    }

    /** 下行整条链：tail `out.ndjson` → 行 → 帧。判据是与 SDK 侧产物同型。 */
    @Test
    fun theDownlinkTurnsRealPipeOutputIntoFrames() =
        runTest {
            val ch = FakeChannel { goldenOutNdjson() }
            val frames = PipeSession(ch, sid).frames().toList()
            assertTrue("前提：桩喂的是真 golden", frames.isNotEmpty())
            assertEquals("正文要出来", listOf("hello"), frames.filterIsInstance<BridgeFrame.AssistantText>().map { it.x })
            assertTrue(
                "catalog 要出来（命令选择器建在它上面）",
                frames
                    .filterIsInstance<BridgeFrame.Init>()
                    .single()
                    .tools
                    .isNotEmpty(),
            )
            assertTrue("轮次收口要出来", frames.filterIsInstance<BridgeFrame.Result>().single().ok)
        }

    /** tail 的是管道 stdout 的重定向，不是会话 JSONL。 */
    @Test
    fun itTailsTheSessionsOutNdjsonNotTheClaudeSessionFile() =
        runTest {
            val ch = FakeChannel { goldenOutNdjson() }
            PipeSession(ch, sid).frames().toList()
            val cmd = ch.commands.single()
            // 对外契约文件名是 `events.ndjson`：首行 `__meta__.source` 让 relay / native / pipeline
            // 三种产出方式对下游同构。
            assertTrue("要 tail 契约文件：$cmd", cmd.contains(".aterm/s/$sid/events.ndjson"))
            assertFalse("tail 的路径里也不许有 \$HOME", cmd.contains("\$HOME"))
            assertTrue("复用 TailTransport 的形状（-c +N -F）", cmd.contains("tail -c +1 -F"))
            // 聊天面不 tail 正在聊的会话记录，否则同一内容双份：管道的下行来自 `events.ndjson`，
            // 而同一批内容也会被 Claude 写进 `~/.claude/projects/<项目>/<sid>.jsonl`。两个都 tail ⇒ 每句话上屏两遍。
            assertFalse("不许去 tail Claude 自己的会话记录", cmd.contains(".claude/projects"))
            // 再加一道正面判据：它 tail 的只能是本对话的契约文件。
            // 上面那条是「不许是 A」，绕开的方式有无数种；这条是「必须是 B」，绕不开。
            assertTrue(
                "只许 tail 本对话的契约文件，别的一概不行：$cmd",
                cmd.contains("-F '.aterm/s/$sid/${PipeCommands.EVENTS}'"),
            )
        }

    /**
     * 下行的解析不许跑在收集者的线程上。
     *
     * 收集者是 `ChatSession`（`Dispatchers.Main.immediate`）；流式一轮几百行，每行的 Map 构建与造帧
     * 不能压在主线程。
     *
     * 探针必须放在上游：`onEach` 在 `flowOn` 的下游，本来就跑在收集者线程上；收集者若也取自
     * `Dispatchers.Default`，可能与解析落在同一个 worker。所以收集者跑在专属线程，线程探针放在
     * 假通道里（与解析同处 `flowOn` 上游、同一个 `flow{}` 块）。没有那个 `flowOn` 时上游就跑在收集者线程上。
     */
    @Test(timeout = FIVE_SECONDS)
    fun theDownlinkIsParsedOffTheCollectorsThread() =
        runTest {
            val upstreamThreads = java.util.Collections.synchronizedSet(mutableSetOf<String>())
            val collectThreads = java.util.Collections.synchronizedSet(mutableSetOf<String>())
            val ch =
                FakeChannel {
                    upstreamThreads += Thread.currentThread().name
                    goldenOutNdjson()
                }

            val collector = Executors.newSingleThreadExecutor { Thread(it, COLLECTOR_THREAD) }
            try {
                withContext(collector.asCoroutineDispatcher()) {
                    PipeSession(ch, sid).frames().collect { collectThreads += Thread.currentThread().name }
                }
            } finally {
                collector.shutdownNow()
            }

            assertTrue("前提：上游真的跑了，否则下面判的是空集", upstreamThreads.isNotEmpty())
            assertEquals("前提：收集确实发生在那个专属线程上", setOf(COLLECTOR_THREAD), collectThreads.toSet())
            assertFalse(
                "解析所在的上游不许落在收集者线程上：上游=$upstreamThreads 收集=$collectThreads",
                upstreamThreads.contains(COLLECTOR_THREAD),
            )
        }

    /** 坏行不许弄崩整条流：`tail -F` 轮转时会给半行。 */
    @Test
    fun aTruncatedLineDoesNotKillTheStream() =
        runTest {
            val ch =
                FakeChannel {
                    """{"__meta__":{"source":"pipeline"}}""" + "\n" +
                        """{"t_ns":1,"event":{"type":"system","subtype":"init","tools":["Bash"],"cwd":"/p"}}""" + "\n" +
                        """{"t_ns":2,"event":{"type":"assist""" + "\n" + // 半行
                        """{"t_ns":3,"event":{"type":"result","is_error":false,"session_id":"s"}}""" + "\n"
                }
            val frames = PipeSession(ch, sid).frames().toList()
            assertEquals("坏行跳过，前后两条都要在", listOf("init", "res"), frames.map { it.t })
        }

    // ---- 上行 ---------------------------------------------------------------

    /**
     * 上行那一行的形状见 `bridge/PROTOCOL.md`，且必须能扛住任意正文（引号/换行/反斜杠）：手拼 JSON 会在这炸。
     */
    @Test
    fun theUplinkLineIsRealJsonThatSurvivesNastyText() {
        val nasty = "他说\"你好\"\n还有 \\ 反斜杠"
        val json = PipeSession.userLineJson(nasty, "aterm-0123456789abcdef-local#7")
        val parsed =
            com.squareup.moshi.Moshi
                .Builder()
                .build()
                .adapter(Any::class.java)
                .fromJson(json) as Map<*, *>
        assertEquals("user", parsed["type"])
        val msg = parsed["message"] as Map<*, *>
        assertEquals("user", msg["role"])
        assertEquals("正文必须逐字节还原", nasty, msg["content"])
        // 标识要出现在那一行的字节里，不是只活在 app 内存里。
        assertEquals("幂等标识必须上 wire", "aterm-0123456789abcdef-local#7", parsed[PipeSession.UPLINK_ID_KEY])
    }

    /**
     * 追加用 `printf '%s\n'` 而不是 `echo`：`echo` 对反斜杠/`-n` 各家 shell 不一致。
     *
     * 这条只钉命令串的形状，证明不了幂等；真读数在 `UplinkIdempotentAppendTest`（真 `/bin/sh` 跑、数文件里的行）。
     */
    @Test
    fun theAppendCommandUsesPrintfAndQuotesBothPayloadAndPath() {
        val cmd = PipeSession(FakeChannel { "" }, sid).idempotentAppendCommand("hi", "aterm-cafe-local#0")
        assertTrue("用 printf：$cmd", cmd.contains("printf '%s\\n' "))
        assertFalse("不许用 echo", cmd.contains("echo "))
        // 路径里不许有 `$HOME`：它被 `shellQuote` 单引号包住就不展开，而 `pipeInvocation` 那端会展开
        // ⇒ 两端指着不同的目录，功能完全不可用。
        assertTrue("路径要 quote 且指向 in.ndjson", cmd.contains("'.aterm/s/$sid/in.ndjson'"))
        assertFalse("路径里不许出现 \$HOME", cmd.contains("\$HOME"))
        assertTrue("追加不是覆盖", cmd.contains(">>"))
        // 检查与追加在同一条命令里。
        assertTrue("要先查再追加：$cmd", cmd.startsWith("if grep -qF "))
        assertTrue("查的是整个键值对，不是裸标识", cmd.contains("'\"aterm_id\":\"aterm-cafe-local#0\"'"))
        assertTrue("文件还不存在时 grep 的 stderr 不许污染 stdout", cmd.contains("2>/dev/null"))
    }

    /** 中断路是盲追加，不做幂等检查（`newInterruptRequestId` 不去重）。 */
    @Test
    fun theInterruptCommandIsABlindAppendWithNoDuplicateCheck() {
        val cmd = PipeSession(FakeChannel { "" }, sid).interruptCommand("req_1_deadbeef")
        assertTrue("中断仍是盲追加：$cmd", cmd.startsWith("printf '%s\\n' "))
        assertFalse("中断不许被拉进幂等检查", cmd.contains("grep"))
    }

    /**
     * `echoesBack = false`：CLI golden 的 14 行里零条 `user`。
     * 与 tmux 路径（TUI 会回显 ⇒ true）相反，上层要做乐观回显。
     */
    @Test
    fun thePipeDoesNotEchoSoTheViewModelMustShowOptimistically() {
        val sink = PipeUplinkSink(FakeChannel { "" }, PipeSession(FakeChannel { "" }, sid), LISTENING)
        assertFalse("管道不回显", sink.echoesBack)
    }

    /**
     * 要肯定的成功证据：没标记就不算写进去了（`exec` 拿不到退出码）。
     *
     * 两道标记都要：`DaemonCommands` 那道说「shell 跑成了」，`ATERM_UP_ADD`/`ATERM_UP_DUP` 那道说
     * 「跑成的是哪一档」。只有前一道 ⇒ 认不出哪一档 ⇒ 判失败（二义的 stdout 不是证据）。
     */
    @Test
    fun anAppendWithoutTheSuccessMarkerIsNotAccepted() =
        runTest {
            val session = PipeSession(FakeChannel { "" }, sid)
            val ok =
                PipeUplinkSink(
                    FakeChannel { "${PipeCommands.APPENDED_MARKER}\n\n${DaemonCommands.QUERY_OK_MARKER}\n" },
                    session,
                    LISTENING,
                )
            assertEquals(SendOutcome.Accepted, ok.send(SendRequest("l1", "hi")))

            // 目录不存在 / 磁盘满：stdout 空、没有标记
            val bad = PipeUplinkSink(FakeChannel { "" }, session, LISTENING)
            val outcome = bad.send(SendRequest("l1", "hi")) as SendOutcome.Rejected
            assertTrue("空 stdout 不算成功", outcome.retryable)

            // shell 跑成了、但哨兵认不出来 ⇒ 仍然不算成功
            val mute = PipeUplinkSink(FakeChannel { "\n${DaemonCommands.QUERY_OK_MARKER}\n" }, session, LISTENING)
            assertTrue(
                "认不出哪一档必须判失败",
                mute.sendReporting(SendRequest("l1", "hi")) is UplinkAppendOutcome.Failed,
            )
        }

    /** 整条命令是 sshd 的单个 argv 元素，56KB 上限，发之前就拒。 */
    @Test
    fun anOversizedMessageIsRefusedBeforeItReachesTheWire() =
        runTest {
            var reached = false
            val ch =
                FakeChannel {
                    reached = true
                    ""
                }
            val sink = PipeUplinkSink(ch, PipeSession(FakeChannel { "" }, sid), LISTENING)
            val big = "堆".repeat(PipeUplinkSink.MAX_UPLINK_BYTES)
            val outcome = sink.send(SendRequest("l1", big)) as SendOutcome.Rejected
            assertFalse("一个字节都不该发出去", reached)
            assertFalse("重试同一条只会再失败", outcome.retryable)
            assertTrue("要因为长度被拒，不是别的原因：${outcome.reason}", outcome.reason.contains("太长"))

            // 闸门量的是拼好之后的命令串，不是转义前的正文。上行要先 JSON 序列化（`"`→`\"`、换行→`\n`）
            // 再整体 shell-quote，两轮膨胀叠加：正文刚好卡在上限的消息转义后必然顶穿，量正文的闸门会放它过去。
            var reached2 = false
            val gate =
                PipeUplinkSink(
                    FakeChannel {
                        reached2 = true
                        ""
                    },
                    PipeSession(FakeChannel { "" }, sid),
                    LISTENING,
                )
            val exactly = "a".repeat(PipeUplinkSink.MAX_UPLINK_BYTES)
            val r2 = gate.send(SendRequest("l2", exactly)) as SendOutcome.Rejected
            assertTrue("正文卡上限 ⇒ 转义后必超 ⇒ 要拒：${r2.reason}", r2.reason.contains("太长"))
            assertFalse("仍然一个字节都不发", reached2)
        }

    /**
     * 取消不是失败，不许被报成「远端拒绝、可重试」。
     *
     * `runCatching` 会连 [CancellationException] 一起抓走，于是「离开这条对话 / 上层换了协程」在界面上
     * 表现成一条红色的失败 + 一个重试按钮，而这条发送本来就该悄悄结束；吞掉它还会破坏协程取消语义。
     * 判据落在「取消要原样传出去」上：拿到 `Rejected` 就说明被吞了。
     */
    @Test
    fun aCancelledSendIsNotReportedAsARemoteRejection() =
        runTest {
            val cancelling =
                object : RemoteCommandChannel {
                    override fun exec(command: String) = flow<ByteArray> { throw CancellationException("走了") }
                }
            val sink = PipeUplinkSink(cancelling, PipeSession(FakeChannel { "" }, sid), LISTENING)

            // 前提：同样的调用在普通异常下确实是走「拒绝」那条 —— 否则下面的断言可能因别的原因通过
            val boom =
                object : RemoteCommandChannel {
                    override fun exec(command: String) = flow<ByteArray> { throw java.io.IOException("断了") }
                }
            val rejected = PipeUplinkSink(boom, PipeSession(FakeChannel { "" }, sid), LISTENING).send(SendRequest("l0", "hi"))
            assertTrue("前提：普通异常该判拒绝", rejected is SendOutcome.Rejected)

            var outcome: SendOutcome? = null
            val threw =
                runCatching {
                    outcome = sink.send(SendRequest("l1", "hi"))
                }.exceptionOrNull()
            assertTrue("取消必须原样抛出去，实得 outcome=$outcome", threw is CancellationException)
        }
}

package com.ccmonitor.mobile.core.claude.transport
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.flow.filterIsInstance
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TailTransportTest {
    @Test fun emitsHelloThenLinesWithIncrementingSeq() =
        runTest {
            val channel =
                RemoteCommandChannel {
                    flowOf("line1\nli".toByteArray(), "ne2\nline3\n".toByteArray())
                }
            val frames = TailTransport(channel, "/p/s.jsonl", sessionId = "sid").frames().toList()

            assertTrue(frames.first() is JsonlFrame.Hello)
            val lines = frames.filterIsInstance<JsonlFrame.Line>()
            assertEquals(listOf("line1", "line2", "line3"), lines.map { it.raw })
            assertEquals(listOf(0L, 1L, 2L), lines.map { it.seq })
            assertEquals("sid", lines[0].sessionId)
            assertEquals("/p/s.jsonl", lines[0].path)
        }

    @Test fun quotesPathAndUsesTailDashF() =
        runTest {
            var captured = ""
            val channel =
                RemoteCommandChannel { cmd ->
                    captured = cmd
                    flowOf(ByteArray(0))
                }
            TailTransport(channel, "/home/pi/a b/s.jsonl").frames().toList()
            // 命令外面还套着 timeout、丢 stderr，所以断言关系而不是抄一整串字面量
            assertTrue("路径要 quote 住（有空格）：$captured", captured.contains("-F '/home/pi/a b/s.jsonl'"))
            assertTrue("从头读", captured.contains("tail -c +1 -F"))
        }

    @Test fun resumeFromOffsetUsesTailDashCPlusOffsetPlusOne() =
        runTest {
            var captured = ""
            val channel =
                RemoteCommandChannel { cmd ->
                    captured = cmd
                    flowOf(ByteArray(0))
                }
            TailTransport(channel, "/p/s.jsonl", startOffsetBytes = 100L).frames().toList()
            assertTrue("从第 101 字节起 = 已消费 100 字节之后：$captured", captured.contains("tail -c +101 -F '/p/s.jsonl'"))
        }

    @Test fun currentOffsetAdvancesPerLine() =
        runTest {
            val channel = RemoteCommandChannel { flowOf("ab\ncde\n".toByteArray()) } // 行1=3字节(ab\n), 行2=4字节(cde\n)
            val t = TailTransport(channel, "/p/s.jsonl", startOffsetBytes = 10L)
            t.frames().toList()
            assertEquals(10L + 3L + 4L, t.currentOffset) // 起点 10 + 两行原始字节
        }

    @Test fun currentOffsetIsPerLineNotPerChunk() =
        runTest {
            // 一个 chunk 含 2 行：读到第 1 行时 offset 必须是行1末(3)、不是整 chunk 末(7)——否则 mid-chunk 取消会跳行。
            val channel = RemoteCommandChannel { flowOf("ab\ncde\n".toByteArray()) }
            val t = TailTransport(channel, "/p/s.jsonl")
            val offsets = mutableListOf<Long>()
            t.frames().filterIsInstance<JsonlFrame.Line>().collect { offsets.add(t.currentOffset) }
            assertEquals(listOf(3L, 7L), offsets)
        }

    @Test fun singleQuoteInPathEscaped() =
        runTest {
            var captured = ""
            val channel =
                RemoteCommandChannel { cmd ->
                    captured = cmd
                    flowOf(ByteArray(0))
                }
            TailTransport(channel, "/p/it's.jsonl").frames().toList()
            assertTrue("单引号要 POSIX 转义：$captured", captured.contains("""-F '/p/it'\''s.jsonl'"""))
        }

    /**
     * `timeout` 外套：断网会在服务器上漏僵尸 tail。
     *
     * 本地 ssh 被杀（socket 干净关闭）时 3 秒内回收；但黑洞（socket 开着不转发字节，
     * 如进隧道/切飞行模式）下 51 秒后 tail 仍在跑。sshd 默认 `clientaliveinterval 0`，
     * 只靠内核 keepalive 的 7200 秒 ⇒ 每次移动网络掉线漏一个 tail + 一个 sshd session，最长 2 小时。
     */
    @Test fun theRemoteTailHasAnAbsoluteLifetimeCapSoBlackholedNetworksDoNotLeakZombies() =
        runTest {
            var captured = ""
            val channel =
                RemoteCommandChannel { cmd ->
                    captured = cmd
                    flowOf(ByteArray(0))
                }
            TailTransport(channel, "/p/s.jsonl").frames().toList()
            assertTrue("要有绝对上限：$captured", captured.startsWith("timeout ${TailTransport.TAIL_MAX_SECONDS} tail "))
        }

    /**
     * stderr 必须丢弃。
     *
     * sshj 的 stderr 与 stdout 共用同一个 `lwin` 通道窗口，而 `tail -F` 在文件还不存在时
     * 会持续往 stderr 刷重试信息 ⇒ 窗口耗尽 ⇒ 把 stdout 一起拖停。流式 `exec` 读不了 stderr，只能丢。
     */
    @Test fun stderrIsDiscardedOtherwiseItWouldStallStdoutThroughTheSharedWindow() =
        runTest {
            var captured = ""
            val channel =
                RemoteCommandChannel { cmd ->
                    captured = cmd
                    flowOf(ByteArray(0))
                }
            TailTransport(channel, "/p/s.jsonl").frames().toList()
            assertTrue("要丢 stderr：$captured", captured.endsWith("2>/dev/null"))
        }

    /**
     * 清僵尸的那条命令要能真的匹配到上一次那条 tail。
     *
     * 两个坑：匹配串不能带 offset（`-c +N` 每次重连都不同，带上就永远杀不掉上一次）；
     * 也不能是裸 `pkill -f tail`（会连别的会话的 tail 一起杀）。
     */
    @Test fun theZombieKillerMatchesOurOwnStaleTailAndNothingElse() {
        val kill = TailTransport.killStaleCommand("/p/s.jsonl")
        // 先把真正喂给 `-f` 的那个模式取出来再判：拿松正则扫整条命令会误伤。
        val pattern = Regex("""pkill -f '(.*)'""").find(kill)?.groupValues?.get(1)
        assertNotNull("前提：命令形如 `pkill -f '<模式>'`：$kill", pattern)
        assertTrue("要指名到这个文件：$pattern", pattern!!.contains("/p/s.jsonl"))
        assertTrue("offset 处要通配，否则杀不掉上一次那条", pattern.contains("-c +[0-9]*"))
        assertNotEquals("模式不许退化成裸 `tail`（会连别的 tail 一起杀）", "tail", pattern.trim())
        assertTrue("没得杀不算失败", kill.endsWith("|| true"))
    }

    @Test fun flushesTrailingPartialLineWhenStreamEnds() =
        runTest {
            val channel = RemoteCommandChannel { flow { emit("a\npartial".toByteArray()) } }
            val lines = TailTransport(channel, "/p/s.jsonl").frames().toList().filterIsInstance<JsonlFrame.Line>()
            assertEquals(listOf("a", "partial"), lines.map { it.raw })
        }
}

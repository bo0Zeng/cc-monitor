package com.ccmonitor.mobile.core.claude.bridge

import com.ccmonitor.mobile.core.claude.transport.DaemonCommands
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 上行的控制面（中断），与「写进文件 ≠ 对面在听」那道门。
 *
 * `interrupt()` 与 `send()` 都走 [PipeUplinkSink.appendGuarded]，第一步就是探活；
 * 放在一处是为了守住「探活对 `interrupt` 也生效」。漏掉的话，管道已经死了、按停止，
 * 界面说「正在让远端停下…」，而那条中断落进一个没人读的文件。
 *
 * 抓得到：三个键名改了任何一个；探活被删或顺序换成「先写后探」。
 * 抓不到：键名对、但 CLI 换了控制通道；`request_id` 撞号（不读回 `control_response`）。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class PipeSessionInterruptTest {
    private val sid = "abc123-DEF_456"

    private class RecordingChannel(
        private val stdout: String = "\n${DaemonCommands.QUERY_OK_MARKER}\n",
    ) : RemoteCommandChannel {
        val commands = mutableListOf<String>()

        override fun exec(command: String): Flow<ByteArray> {
            commands += command
            return flowOf(stdout.toByteArray())
        }
    }

    /**
     * 中断那一行的形状逐字对齐 Claude Agent SDK 的 control request：
     *
     * ```json
     * {"type":"control_request","request_id":"req_<n>_<8hex>","request":{"subtype":"interrupt"}}
     * ```
     *
     * 注意：三个键名一个都不能改。CLI 认的是这个形状，改一个字它就当成普通行忽略掉，
     * 而且完全静默：远端照跑，app 停在「正在让远端停下…」，没有任何地方报错。
     */
    @Test
    fun theInterruptLineMatchesTheSdkShapeWordForWord() {
        val line = PipeSession.interruptLineJson("req_1_deadbeef")
        assertTrue("帧型键：$line", line.contains("\"type\":\"control_request\""))
        assertTrue("请求 id 键：$line", line.contains("\"request_id\":\"req_1_deadbeef\""))
        assertTrue("子类型键：$line", line.contains("\"request\":{\"subtype\":\"interrupt\"}"))
        // 它不是一条 user 消息：走错通道的话远端会把「请停下」当成一句话回答
        assertFalse("绝不许退化成普通消息：$line", line.contains("\"type\":\"user\""))
    }

    /** `request_id` 只求不重，不参与任何判定（不读回 `control_response`）。 */
    @Test
    fun eachInterruptGetsItsOwnRequestId() {
        val ids = (1..20).map { PipeSession.newInterruptRequestId(it) }
        assertEquals("20 次不该撞号：$ids", ids.size, ids.distinct().size)
        assertNotEquals("同一个序号连铸两次也不该相同（后缀是随机的）", PipeSession.newInterruptRequestId(1), PipeSession.newInterruptRequestId(1))
    }

    /** 中断与消息走同一条路：追一行进 `in.ndjson`。不是另开通道。 */
    @Test
    fun theInterruptGoesDownTheSameUplinkAsAMessage() =
        runTest {
            val ch = RecordingChannel()
            val sink = PipeUplinkSink(ch, PipeSession(ch, sid)) { null }
            assertEquals(SendOutcome.Accepted, sink.interrupt())

            val cmd = ch.commands.single()
            assertTrue("追进 in.ndjson：$cmd", cmd.contains("'.aterm/s/$sid/in.ndjson'"))
            assertTrue("用 printf 不用 echo：$cmd", cmd.startsWith("printf '%s\\n' "))
            assertTrue("追加不是覆盖：$cmd", cmd.contains(">>"))
            assertTrue("要肯定的成功证据（exec 拿不到退出码）：$cmd", cmd.contains(DaemonCommands.QUERY_OK_MARKER))
        }

    /**
     * 对面不在听时，一个字节都不许写出去，且必须判 `Rejected`。
     *
     * `printf … >> in.ndjson` 在管道已经不在时照样成功（文件还在，追加恒成功），
     * 只看 shell 的成功标记的话，管道被外部杀掉之后每条消息都显示「已送达」而全部丢失。
     *
     * 顺序也要钉：「写完再判」与「先探再写」返回值一样（都 `Rejected`），
     * 但前者那条消息已经落进文件了。断言 `commands` 为空是「先探再写」唯一能从外部观察到的痕迹。
     */
    @Test
    fun nothingIsWrittenWhenTheOtherEndIsNotListening() =
        runTest {
            val ch = RecordingChannel()
            val sink = PipeUplinkSink(ch, PipeSession(ch, sid)) { "那条管道不在了" }

            val sent = sink.send(SendRequest("l1", "喂")) as SendOutcome.Rejected
            assertEquals("要把探活给的原话原样端出来", "那条管道不在了", sent.reason)
            assertTrue("这条可以重试（管道弄回来就能发）", sent.retryable)

            // 中断走同一道门：漏掉的话：界面说「正在让远端停下…」而中断根本没出去
            val stopped = sink.interrupt() as SendOutcome.Rejected
            assertEquals("中断也必须过这道门", "那条管道不在了", stopped.reason)

            assertEquals("顺序判据：探活没过 ⇒ 一个字节都不许写出去，实得 ${ch.commands}", 0, ch.commands.size)
        }

    /** 探活自己炸了也不许弄崩页面，而且不许被当成「在听」放行。 */
    @Test
    fun aProbeThatThrowsIsTreatedAsNotListeningNotAsFine() =
        runTest {
            val ch = RecordingChannel()
            val sink = PipeUplinkSink(ch, PipeSession(ch, sid)) { error("探活自己炸了") }

            val outcome = sink.send(SendRequest("l1", "喂")) as SendOutcome.Rejected
            assertTrue("要说得出为什么：${outcome.reason}", outcome.reason.contains("探活自己炸了"))
            assertEquals("仍然一个字节都不写", 0, ch.commands.size)
        }
}

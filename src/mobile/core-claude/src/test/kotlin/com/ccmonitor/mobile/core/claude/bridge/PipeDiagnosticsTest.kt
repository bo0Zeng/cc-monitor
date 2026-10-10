package com.ccmonitor.mobile.core.claude.bridge

import com.ccmonitor.mobile.core.claude.command.PipeCommands
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 把管道命令的错误输出读出来上屏。
 *
 * 素材是真机上出现过的原话：`Not logged in · Please run /login`（远端 Claude 没登录）、
 * `Error: Session ID … is already in use.`（编号已有记录还硬指定）。
 */
class PipeDiagnosticsTest {
    private val sid = "abc123-DEF_456"

    private class FakeChannel(
        private val reply: String,
    ) : RemoteCommandChannel {
        val commands = mutableListOf<String>()

        override fun exec(command: String) = flowOf(reply.toByteArray()).also { commands += command }
    }

    /** 远端说了什么，就得能被读出来；否则屏幕空着，说不出为什么。 */
    @Test
    fun whatTheRemoteSaidCanActuallyBeRead() =
        runTest {
            val ch = FakeChannel("Not logged in · Please run /login\n")
            val reason = PipeDiagnostics(ch, sid).explainFailure()
            assertEquals("原话要读得出来", "Not logged in · Please run /login", reason)
        }

    /**
     * 读必须有界：远端进程可能往错误输出吐几百 MB，整份拉回来就是第二个事故。
     * 判据落在发出去的命令上：有界读原语，且带上限。
     */
    @Test
    fun theLogIsReadWithABoundNotSluppedWhole() =
        runTest {
            val ch = FakeChannel("x\n")
            PipeDiagnostics(ch, sid).explainFailure()
            val cmd = ch.commands.single()
            assertTrue("要有字节上限：$cmd", cmd.contains("head -c ${PipeDiagnostics.LOG_BYTE_CAP}"))
            assertTrue("读的是那条命令的错误输出", cmd.contains(PipeCommands.LOG))
            assertFalse("不许 tail -f（这是一次性读，不是跟随）", cmd.contains("tail -f"))
        }

    /**
     * 没线索就返回 null，不拿空串冒充「一切正常」。
     * 空日志与「有错但没写出来」不可辨，由调用方决定这时说什么。
     */
    @Test
    fun anEmptyLogIsReportedAsNoClueNotAsSuccess() =
        runTest {
            for (blank in listOf("", "\n", "   \n  \n")) {
                assertNull("「$blank」必须判成没线索", PipeDiagnostics(FakeChannel(blank), sid).explainFailure())
            }
        }

    /** 远端读不到（文件不存在 / 通道断了）也要给 null，不许把异常抛给调用方。 */
    @Test
    fun aFailedReadIsNoClueRatherThanACrash() =
        runTest {
            val boom =
                object : RemoteCommandChannel {
                    override fun exec(command: String) = flow<ByteArray> { throw java.io.IOException("断了") }
                }
            assertNull("读不到 ⇒ 没线索，不许炸", PipeDiagnostics(boom, sid).explainFailure())
        }

    /**
     * 上屏那句话要短，取第一行有内容的：状态行不是日志查看器，错误总在最前面。
     * 不做结构化解析：那是任意程序的 stderr，没有可依赖的结构。
     */
    @Test
    fun theReasonIsTheFirstNonBlankLineAndIsBounded() {
        assertEquals(
            "前面的空行要跳过",
            "Error: Session ID 2f611dc4 is already in use.",
            PipeDiagnostics.firstMeaningfulLine("\n\n  Error: Session ID 2f611dc4 is already in use.  \n后面还有很多\n"),
        )
        val long = PipeDiagnostics.firstMeaningfulLine("x".repeat(10_000))
        assertNotNull(long)
        assertEquals("要截断", PipeDiagnostics.MAX_REASON_CHARS, long!!.length)
    }
}

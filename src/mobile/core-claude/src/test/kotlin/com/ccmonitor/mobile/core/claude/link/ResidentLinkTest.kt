package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.remote.ExecResult
import com.ccmonitor.mobile.core.remote.RemoteDuplexChannel
import com.ccmonitor.mobile.core.remote.RemoteExecutor
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** 接一台只核一处：同一个 `BUILD_ID`（门槛问一次 · hello 自报再对一次）。接上以后不再问任何东西。 */
class ResidentLinkTest {
    private val asked = mutableListOf<String>()
    private val executor =
        RemoteExecutor { cmd ->
            asked += cmd
            when {
                cmd.endsWith(BackendGate.PROBE) -> ExecResult(0, """{"buildId":"${EmbeddedBuild.ID}"}""", "")
                else -> ExecResult(0, "{}", "")
            }
        }

    private fun link(d: FakeDuplex) = ResidentLink(OneShot(executor), RemoteDuplexChannel { d })

    @Test
    fun `同一个 BUILD_ID 接上就通：attach 之后一句都不问`() =
        runTest {
            val d = FakeDuplex()
            d.push("""{"kind":"hello","v":1,"build_id":"${EmbeddedBuild.ID}"}""")
            d.push("""{"attach":"ok"}""")
            val got = link(d).open(backgroundScope, "n1", ResidentLink.Timeouts(1_000)) {}
            runCurrent()
            assertTrue("应当接上：$got", got is LinkOutcome.Up)
            assertEquals("门槛 · 起常驻，只这两次一次性调用", 2, asked.size)
            assertEquals("流上只写了 attach 那一行", listOf("""{"attach":true,"flags":["--tail-only"]}"""), drain(d))
        }

    @Test
    fun `hello 自报的身份和这一版不同：不接`() =
        runTest {
            val d = FakeDuplex()
            d.push("""{"kind":"hello","v":1,"build_id":"other"}""")
            d.push("""{"attach":"ok"}""")
            val got = link(d).open(backgroundScope, "n1", ResidentLink.Timeouts(1_000)) {}
            assertEquals(LinkOutcome.HelloDiffers("other"), got)
            assertTrue("不接 ⇒ 关流", d.closed)
        }

    private fun drain(d: FakeDuplex): List<String> = generateSequence { d.written.tryReceive().getOrNull() }.toList()
}

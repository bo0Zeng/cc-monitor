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

    private fun link(vararg ds: FakeDuplex): ResidentLink {
        val queue = ArrayDeque(ds.toList())
        return ResidentLink(OneShot(executor), RemoteDuplexChannel { queue.removeFirst() })
    }

    private fun ours(d: FakeDuplex) =
        d.apply {
            push("""{"kind":"hello","v":1,"build_id":"${EmbeddedBuild.ID}"}""")
            push("""{"attach":"ok"}""")
        }

    @Test
    fun `常驻在跑、同一个 BUILD_ID：只开 attach 那一条，一次性调用一次都不问`() =
        runTest {
            val d = ours(FakeDuplex())
            val got = link(d).open(backgroundScope, "n1", "Asia/Shanghai", ResidentLink.Timeouts(1_000)) {}
            runCurrent()
            assertTrue("应当接上：$got", got is LinkOutcome.Up)
            assertEquals("hello 自报同一版就是门槛：不另问门槛、不起常驻", emptyList<String>(), asked)
            assertEquals("流上只写了 attach 那一行", listOf("""{"attach":true,"flags":["--tail-only"],"tz":"Asia/Shanghai"}"""), drain(d))
        }

    @Test
    fun `常驻没在跑（中继回 absent）：门槛 → 起常驻 → 再接一次`() =
        runTest {
            val refused = FakeDuplex().apply { push("""{"attach":"refused","reason":"absent"}""") }
            val d = ours(FakeDuplex())
            val got = link(refused, d).open(backgroundScope, "n1", "Asia/Shanghai", ResidentLink.Timeouts(1_000)) {}
            runCurrent()
            assertTrue("应当接上：$got", got is LinkOutcome.Up)
            assertTrue("被拒的那条关掉", refused.closed)
            assertEquals(listOf(BackendGate.PROBE, ResidentLink.ENSURE), asked.map { it.substringAfterLast(' ') })
        }

    @Test
    fun `hello 自报的身份和这一版不同：照门槛再问一遍，还是不同就不接`() =
        runTest {
            val first = FakeDuplex().apply { push("""{"kind":"hello","v":1,"build_id":"other"}""") }
            val d = FakeDuplex().apply { push("""{"kind":"hello","v":1,"build_id":"other"}""") }
            d.push("""{"attach":"ok"}""")
            val got = link(first, d).open(backgroundScope, "n1", "Asia/Shanghai", ResidentLink.Timeouts(1_000)) {}
            assertEquals(LinkOutcome.HelloDiffers("other"), got)
            assertTrue("不接 ⇒ 关流", first.closed && d.closed)
        }

    @Test
    fun `那台没装后端（attach 那条一开就断）：照门槛说没装`() =
        runTest {
            val gone = FakeDuplex().apply { end() }
            val absentExec =
                RemoteExecutor { cmd ->
                    asked += cmd
                    ExecResult(127, "", "ccm: not found")
                }
            val got =
                ResidentLink(OneShot(absentExec), RemoteDuplexChannel { gone })
                    .open(backgroundScope, "n1", "UTC", ResidentLink.Timeouts(1_000)) {}
            assertEquals(LinkOutcome.Gate(GateVerdict.Absent), got)
        }

    private fun drain(d: FakeDuplex): List<String> = generateSequence { d.written.tryReceive().getOrNull() }.toList()
}

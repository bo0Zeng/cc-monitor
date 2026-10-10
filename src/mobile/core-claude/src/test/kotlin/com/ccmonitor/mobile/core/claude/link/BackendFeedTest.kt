package com.ccmonitor.mobile.core.claude.link

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 一台的常驻流断了以后自己接回去：SSH 那层重连过（换了一条）⇒ 在新连接上重走 probe → ensure → attach；
 * SSH 还通、只是流断了 ⇒ 按退避再接；回前台 / ［重试］⇒ 马上再接。接回来以后会话帧照常往下走：
 * 会话表是那台整份重报的（不留旧的、不重一份），`turn_end` 一帧不重、接上期间的一帧不漏。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class BackendFeedTest {
    private val hello = """{"kind":"hello","v":1,"build_id":"${EmbeddedBuild.ID}"}"""
    private val catalog = CellsCatalog.of(mapOf("products" to emptyList<Any>()))!!

    private fun added(sid: String) = """{"kind":"session_added","sid":"$sid","activity_text":"空闲","activity_tone":"calm"}"""

    private val replayed = """{"kind":"sessions_replayed"}"""

    private fun turnEnd(
        sid: String,
        uuid: String,
    ) = """{"kind":"turn_end","session_id":"$sid","uuid":"$uuid"}"""

    /** 每开一次流取下一份台本里的 [FakeDuplex]；[outcomes] 里排着的先回（走不通的那几种）。 */
    private class Remote {
        val duplexes = ArrayDeque<FakeDuplex>()
        val outcomes = ArrayDeque<LinkOutcome>()
        var opens = 0

        fun next(): FakeDuplex = FakeDuplex().also { duplexes += it }
    }

    private fun TestScope.feed(
        remote: Remote,
        ssh: MutableStateFlow<Any?>,
        lost: MutableList<Unit> = mutableListOf(),
    ): BackendFeed {
        val open: suspend (CoroutineScope) -> LinkOutcome = { s ->
            remote.opens++
            remote.outcomes.removeFirstOrNull() ?: run {
                val d = remote.duplexes.removeFirst()
                d.push(hello)
                d.push("""{"attach":"ok"}""")
                when (val a = FrameClient.attach(d, listOf("--tail-only"), s, HANDSHAKE_MS, "n${remote.opens}") {}) {
                    is AttachOutcome.Attached -> LinkOutcome.Up(a.client, catalog)
                    else -> LinkOutcome.Attach(a)
                }
            }
        }
        return BackendFeed(backgroundScope, ssh, open, onLost = { lost += Unit })
    }

    @Test
    fun `SSH 还通、流断了：不用按重试，自己接回去；会话表与 turn_end 照常往下走、不重不漏`() =
        runTest {
            val remote = Remote()
            val first = remote.next()
            val second = remote.next()
            val lost = mutableListOf<Unit>()
            val f = feed(remote, MutableStateFlow(TOKEN_1), lost)
            val ends = mutableListOf<TurnEnd>()
            backgroundScope.launch { f.turnEnds.collect { ends += it } }
            f.ensure()
            runCurrent()
            assertEquals(LinkState.Up, f.state.value)
            first.push(added("s1"))
            first.push(replayed)
            first.push(turnEnd("s1", "u1"))
            runCurrent()
            assertEquals(setOf("s1"), f.table.value.sessions.keys)

            first.end() // 流断了（SSH 那层没说断）
            runCurrent()
            assertTrue("断了要说断了", f.state.value is LinkState.Down)
            assertEquals("断了要叫 SSH 那层去探活", 1, lost.size)
            advanceTimeBy(BackendFeed.Backoff().delayMs(0) + 1)
            runCurrent()
            assertEquals("退避到点自己再接，不等人按", 2, remote.opens)
            assertEquals(LinkState.Up, f.state.value)
            second.push(added("s1")) // 那台整份重报
            second.push(added("s2"))
            second.push(replayed)
            second.push(turnEnd("s1", "u2"))
            runCurrent()
            assertEquals(setOf("s1", "s2"), f.table.value.sessions.keys)
            assertTrue(f.table.value.replayed)
            assertEquals("turn_end 一帧不重、一帧不漏", listOf(TurnEnd("s1", "u1"), TurnEnd("s1", "u2")), ends)
        }

    @Test
    fun `SSH 重连过（换了一条）：旧流关掉，当场在新连接上重接`() =
        runTest {
            val remote = Remote()
            val first = remote.next()
            remote.next()
            val ssh = MutableStateFlow<Any?>(TOKEN_1)
            val f = feed(remote, ssh)
            f.ensure()
            runCurrent()
            assertEquals(1, remote.opens)
            ssh.value = TOKEN_2
            runCurrent()
            assertTrue("旧连接上那条流要关掉", first.closed)
            assertEquals("不等退避，当场重接", 2, remote.opens)
            assertEquals(LinkState.Up, f.state.value)
        }

    @Test
    fun `SSH 没连上就不去接，连上那一刻接；又断了就停在断开`() =
        runTest {
            val remote = Remote()
            val first = remote.next()
            val ssh = MutableStateFlow<Any?>(null)
            val f = feed(remote, ssh)
            f.ensure()
            advanceTimeBy(MINUTE_MS)
            runCurrent()
            assertEquals(0, remote.opens)
            assertTrue(f.state.value is LinkState.Down)
            ssh.value = TOKEN_1
            runCurrent()
            assertEquals(1, remote.opens)
            assertEquals(LinkState.Up, f.state.value)
            ssh.value = null
            runCurrent()
            assertTrue(first.closed)
            assertTrue(f.state.value is LinkState.Down)
            advanceTimeBy(MINUTE_MS)
            runCurrent()
            assertEquals("SSH 断着就不反复试", 1, remote.opens)
        }

    @Test
    fun `走不通要人动手的那几种（那台是另一版）不自己反复试，回前台或重试才再接`() =
        runTest {
            val remote = Remote()
            remote.outcomes += LinkOutcome.Gate(GateVerdict.Differs("other"))
            remote.next()
            val f = feed(remote, MutableStateFlow(TOKEN_1))
            f.ensure()
            advanceTimeBy(MINUTE_MS * 10)
            runCurrent()
            assertEquals(1, remote.opens)
            assertEquals(LinkState.Down(LinkOutcome.Gate(GateVerdict.Differs("other"))), f.state.value)
            f.reconnect()
            runCurrent()
            assertEquals(2, remote.opens)
            assertEquals(LinkState.Up, f.state.value)
        }

    @Test
    fun `回前台：通着就不动，断着就当场再接`() =
        runTest {
            val remote = Remote()
            val first = remote.next()
            remote.next()
            val f = feed(remote, MutableStateFlow(TOKEN_1))
            f.ensure()
            runCurrent()
            f.onForeground()
            runCurrent()
            assertEquals("通着回前台不重接", 1, remote.opens)
            first.end()
            runCurrent()
            f.onForeground()
            runCurrent()
            assertEquals("断着回前台当场接，不等退避", 2, remote.opens)
        }

    @Test
    fun `退避：从一秒起翻倍，封顶半分钟`() {
        val b = BackendFeed.Backoff()
        assertEquals(listOf(1_000L, 2_000L, 4_000L, 8_000L, 16_000L, 30_000L, 30_000L), (0..6).map(b::delayMs))
    }

    private companion object {
        const val HANDSHAKE_MS = 1_000L
        const val MINUTE_MS = 60_000L
        val TOKEN_1 = Any()
        val TOKEN_2 = Any()
    }
}

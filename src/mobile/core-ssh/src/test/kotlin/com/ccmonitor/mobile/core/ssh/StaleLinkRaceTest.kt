package com.ccmonitor.mobile.core.ssh

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 旧连接还能不能用，让对端答：先只探活，[FRESH_AFTER_MS] 内没回话才另拨一条新的、两边接着赛跑，谁先站住用谁
 * （台子读数「复用 · 刚断过」5.4 秒 vs 新建 0.4 秒）。探活及时回了话就不另拨（不让那台多记两次登录）。
 * 黑洞上的探活永远不回话（[FakeTransport.probeGate] 不放行），判据只推到「另拨」那一刻为止：
 * 换上新连接靠的是「新的那条先站住」这件事，不是探活期限到了。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class StaleLinkRaceTest {
    private val cfg = testCfg("good")

    @Test
    fun `回前台时旧连接成了黑洞：新的一条站住就换上，不等探活期限`() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = testManager(produced)
            mgr.connect("id", "h", "L", cfg)
            val seen = mutableListOf<SshTransport?>()
            backgroundScope.launch { mgr.linkOf("id").collect { seen += it } }
            runCurrent()
            produced[0].probeGate = CompletableDeferred() // 黑洞：探活不回话
            mgr.probeAndReconnectStale()
            runCurrent()
            assertEquals("探活还没回话、还没到另拨那一刻：不另拨", 1, produced.size)
            advanceTimeBy(FRESH_AFTER_MS)
            runCurrent()
            assertEquals("新起了一条", 2, produced.size)
            assertSame("新的那条站住了就换上", produced[1], mgr.connection("id"))
            assertSame("linkOf 跟着换一条", produced[1], seen.last())
            assertEquals("只等了另拨前那一小段，没等探活期限", FRESH_AFTER_MS, currentTime)
            advanceUntilIdle()
            assertTrue("黑洞那条后台关掉", produced[0].closed)
        }

    @Test
    fun `终端 tab 复用时旧连接成了黑洞：connect 拿到新的一条，不等探活期限`() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = testManager(produced)
            mgr.connect("id", "h", "L", cfg)
            produced[0].probeGate = CompletableDeferred()
            val got = async { mgr.connect("id", "h", "L", cfg) }
            advanceTimeBy(FRESH_AFTER_MS)
            runCurrent()
            assertTrue("没等探活期限就回了", got.isCompleted)
            assertSame("回的是新的那条", produced[1], got.await())
            assertEquals(FRESH_AFTER_MS, currentTime)
        }

    @Test
    fun `旧连接先回话：留旧的，新起的那条当场关掉`() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val handshake = CompletableDeferred<Unit>()
            var first = true
            val mgr =
                testManager(produced) {
                    if (first) {
                        first = false
                    } else {
                        handshake.await() // 新的那条还在握手
                    }
                }
            mgr.connect("id", "h", "L", cfg)
            val answer = CompletableDeferred<Unit>()
            produced[0].probeGate = answer
            mgr.probeAndReconnectStale()
            advanceTimeBy(FRESH_AFTER_MS)
            runCurrent()
            assertEquals("探活没及时回话：另拨一条，接着赛跑", 2, produced.size)
            answer.complete(Unit) // 对端回话：旧的还活着
            runCurrent()
            advanceUntilIdle()
            assertSame("留旧的", produced[0], mgr.connection("id"))
            assertTrue("新起的那条关掉（握手被关断）", produced[1].closed)
        }

    @Test
    fun `回前台时探活几百毫秒内回了话：留旧的，不另拨`() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = testManager(produced)
            mgr.connect("id", "h", "L", cfg)
            val answer = CompletableDeferred<Unit>()
            produced[0].probeGate = answer
            mgr.probeAndReconnectStale()
            advanceTimeBy(FRESH_AFTER_MS - 1)
            runCurrent()
            answer.complete(Unit) // 另拨之前对端回了话
            runCurrent()
            advanceUntilIdle()
            assertEquals("一条都没另拨", 1, produced.size)
            assertSame("留旧的", produced[0], mgr.connection("id"))
            assertEquals(1, produced[0].probeCalls.get())
        }

    @Test
    fun `探活及时答「死」：不等到另拨那一刻，当场另拨`() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            val mgr = testManager(produced)
            mgr.connect("id", "h", "L", cfg)
            produced[0].probeAliveResult = false // 对端回了 RST 一类
            mgr.probeAndReconnectStale()
            runCurrent()
            assertEquals("当场另拨了一条", 2, produced.size)
            assertSame("换上新的", produced[1], mgr.connection("id"))
            assertEquals("没等到另拨那一刻", 0L, currentTime)
        }

    @Test
    fun `两边都不成：摘掉旧的，照退避重连`() =
        runTest {
            val produced = mutableListOf<FakeTransport>()
            var fail = false
            val mgr = testManager(produced) { if (fail) error("网不通") }
            mgr.connect("id", "h", "L", testCfgP("good", ReconnectPolicy(baseDelayMs = 1_000, maxAttempts = 2)))
            produced[0].probeAliveResult = false // 对端回了 RST 一类：判死
            fail = true
            mgr.probeAndReconnectStale()
            advanceUntilIdle()
            assertTrue("旧的摘掉", mgr.connection("id") !== produced[0])
            assertTrue("还在照退避试", produced.size >= 3)
        }
}

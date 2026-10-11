package com.ccmonitor.mobile.core.ssh

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 旧连接还能不能用，让对端答：探活与一条新连接同时起跑，谁先站住用谁（net2 · 台子读数「复用 · 刚断过」5.4 秒 vs 新建 0.4 秒）。
 * 黑洞上的探活永远不回话（[FakeTransport.probeGate] 不放行），判据一律不推虚拟时间：
 * 换上新连接靠的是「新的那条先站住」这件事，不是哪个期限到了。
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
            assertEquals("新起了一条", 2, produced.size)
            assertSame("新的那条站住了就换上", produced[1], mgr.connection("id"))
            assertSame("linkOf 跟着换一条", produced[1], seen.last())
            assertEquals("虚拟时间没走：没等任何期限", 0L, currentTime)
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
            runCurrent()
            assertTrue("没等探活期限就回了", got.isCompleted)
            assertSame("回的是新的那条", produced[1], got.await())
            assertEquals(0L, currentTime)
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
            runCurrent()
            assertEquals("探活与新连接同时起跑", 2, produced.size)
            answer.complete(Unit) // 对端回话：旧的还活着
            runCurrent()
            advanceUntilIdle()
            assertSame("留旧的", produced[0], mgr.connection("id"))
            assertTrue("新起的那条关掉（握手被关断）", produced[1].closed)
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

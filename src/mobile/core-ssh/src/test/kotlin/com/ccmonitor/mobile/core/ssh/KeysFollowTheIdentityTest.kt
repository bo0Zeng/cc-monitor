package com.ccmonitor.mobile.core.ssh

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * **核心里的钥匙跟着连接身份走**：每一条传输握着自己那份身份的钥匙、关掉时放掉（[RustTransport]）。所以这一层要守住：
 * 交出去的拨号请求（共用那条重连循环照它拨）背后总有一条没关的传输；不再用的那几条都关了 —— 连多少次、换多少条，
 * 没关的只有装着的那一条；断开后一条都不剩。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class KeysFollowTheIdentityTest {
    private val id = "h1"

    /** 造过的全部传输（连同 [SshTransport.again] 派生的）里还没关的那几条 ＝ 核心里还被握着的那几份身份。 */
    private fun open(made: List<FakeTransport>): List<FakeTransport> {
        fun all(t: FakeTransport): List<FakeTransport> = listOf(t) + synchronized(t.derived) { t.derived.toList() }.flatMap(::all)
        return made.flatMap(::all).filterNot { it.closed }
    }

    @Test
    fun twentyHoldsThatDieAndAreFollowedLeaveOneOpenAndNoneAfterDisconnect() =
        runTest {
            val made = mutableListOf<FakeTransport>()
            val m = testManager(made)
            m.connect(id, id, "box", testCfg("a"))
            m.loopReached(id, true)
            repeat(TIMES) {
                (m.connection(id) as FakeTransport).simulateDrop()
                advanceUntilIdle()
            }
            assertEquals(listOf(m.connection(id)), open(made))
            m.disconnect(id)
            advanceUntilIdle()
            assertEquals(emptyList<FakeTransport>(), open(made))
        }

    /** 没够着那台：拨号请求交给共用那条，它背后那一条不关（关了，共用那条照它拨就拿不到钥匙）；再点几次也只留一条。 */
    @Test
    fun aDialHandedToTheSharedLoopIsBackedByExactlyOneOpenTransport() =
        runTest {
            val made = mutableListOf<FakeTransport>()
            val m = testManager(made) { throw HostUnreachedException("拨号超时") }
            repeat(TIMES) { assertTrue(runCatching { m.connect(id, id, "box", testCfg("a")) }.isFailure) }
            advanceUntilIdle()
            val left = open(made)
            assertEquals(1, left.size)
            assertEquals(left.single().dialRequest, m.dialOf(id).first())
            m.disconnect(id)
            advanceUntilIdle()
            assertEquals(emptyList<FakeTransport>(), open(made))
        }

    /** 那台拒了（认证没过）：请求不交出去，那一条关掉。 */
    @Test
    fun aRefusedDialLeavesNothingOpen() =
        runTest {
            val made = mutableListOf<FakeTransport>()
            val m = testManager(made) { throw java.io.IOException("认证没过") }
            repeat(TIMES) { assertTrue(runCatching { m.connect(id, id, "box", testCfg("a")) }.isFailure) }
            advanceUntilIdle()
            assertEquals(emptyList<FakeTransport>(), open(made))
        }

    /** 拨到一半撤掉（断开）：在飞的那一条也关掉。 */
    @Test
    fun aDialWithdrawnMidwayLeavesNothingOpen() =
        runTest {
            val made = mutableListOf<FakeTransport>()
            val inside = CompletableDeferred<Unit>()
            val m =
                testManager(made) {
                    inside.complete(Unit)
                    awaitClose()
                    throw java.io.IOException("被关了")
                }
            val job = launch { runCatching { m.connect(id, id, "box", testCfg("a")) } }
            inside.await()
            m.disconnect(id)
            job.join()
            advanceUntilIdle()
            assertEquals(emptyList<FakeTransport>(), open(made))
        }

    private companion object {
        const val TIMES = 20
    }
}

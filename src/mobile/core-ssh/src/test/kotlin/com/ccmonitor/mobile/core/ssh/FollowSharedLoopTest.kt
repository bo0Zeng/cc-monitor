package com.ccmonitor.mobile.core.ssh

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * **SSH 那层跟着共用那条重连循环走，自己不拨、不判死、不退避**（手机上只有一条重连循环：嵌的核心里的 `resident-client`）。
 *
 * - 握着的那条死了 ⇒ 名义状态「重连中」，等共用那条报「够着那一台了」（[SshConnectionManager.loopReached]）才换一条；
 *   换的那条是同一份拨号身份（[SshTransport.again]：池里有活的就复用，拨号请求一字不差 ⇒ 常驻流那边看不出换了、不被打断）。
 * - 共用那条没报够着 ⇒ 干等多久都不自己拨。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class FollowSharedLoopTest {
    private val id = "h1"

    @Test
    fun aDroppedHoldWaitsForTheSharedLoopAndNeverRedialsOnItsOwn() =
        runTest {
            val made = mutableListOf<FakeTransport>()
            val m = testManager(made)
            m.connect(id, id, "box", testCfg("a"))
            made.single().simulateDrop()
            advanceTimeBy(TEN_MINUTES_MS)
            advanceUntilIdle()
            assertEquals("自己又拨了一条", 1, made.size)
            assertTrue("自己又握了一条", made.single().derived.isEmpty())
            assertEquals(SessionStatus.RECONNECTING, m.sessions.value[id]?.status)
        }

    @Test
    fun whenTheSharedLoopReachesTheHostTheSshLayerTakesOneOnTheSameIdentity() =
        runTest {
            val made = mutableListOf<FakeTransport>()
            val m = testManager(made)
            m.connect(id, id, "box", testCfg("a"))
            val first = made.single()
            val dial = m.dialOf(id).first()
            first.simulateDrop()
            m.loopReached(id, true)
            advanceUntilIdle()
            val now = m.connection(id)
            assertNotSame("没换一条", first, now)
            assertSame("换的不是同一份身份上的那一条", first.derived.single(), now)
            assertEquals("又造了一份新身份（常驻流会被打断）", 1, made.size)
            assertEquals("拨号请求变了（常驻流会被打断）", dial, m.dialOf(id).first())
            assertEquals(SessionStatus.CONNECTED, m.sessions.value[id]?.status)
        }

    @Test
    fun aHoldThatDiesWhileTheLoopIsUpIsReplacedAtOnce() =
        runTest {
            val made = mutableListOf<FakeTransport>()
            val m = testManager(made)
            m.connect(id, id, "box", testCfg("a"))
            m.loopReached(id, true)
            advanceUntilIdle()
            assertTrue("握着的那条还活着就不该换", made.single().derived.isEmpty())
            made.single().simulateDrop()
            advanceUntilIdle()
            assertEquals(1, made.single().derived.size)
            assertEquals(SessionStatus.CONNECTED, m.sessions.value[id]?.status)
        }

    @Test
    fun aLoopThatHasNotReachedTheHostIsNotFollowed() =
        runTest {
            val made = mutableListOf<FakeTransport>()
            val m = testManager(made)
            m.connect(id, id, "box", testCfg("a"))
            m.loopReached(id, true)
            m.loopReached(id, false)
            made.single().simulateDrop()
            advanceUntilIdle()
            assertTrue(made.single().derived.isEmpty())
            assertEquals(SessionStatus.RECONNECTING, m.sessions.value[id]?.status)
        }

    @Test
    fun aHostWithAutoReconnectOffIsNotFollowed() =
        runTest {
            val made = mutableListOf<FakeTransport>()
            val m = testManager(made)
            m.connect(id, id, "box", testCfgP("a", ReconnectPolicy(auto = false)))
            made.single().simulateDrop()
            m.loopReached(id, true)
            advanceUntilIdle()
            assertTrue(made.single().derived.isEmpty())
        }

    @Test
    fun connectingAgainOnADeadHoldKeepsTheIdentity() =
        runTest {
            val made = mutableListOf<FakeTransport>()
            val m = testManager(made)
            m.connect(id, id, "box", testCfg("a"))
            val dial = m.dialOf(id).first()
            made.single().simulateDrop()
            val got = m.connect(id, id, "box", testCfg("a"))
            assertSame(made.single().derived.single(), got)
            assertEquals(1, made.size)
            assertEquals(dial, m.dialOf(id).first())
        }

    @Test
    fun disconnectDropsTheDialRequestAndStopsFollowing() =
        runTest {
            val made = mutableListOf<FakeTransport>()
            val m = testManager(made)
            m.connect(id, id, "box", testCfg("a"))
            made.single().simulateDrop()
            m.disconnect(id)
            m.loopReached(id, true)
            advanceUntilIdle()
            assertNull(m.dialOf(id).first())
            assertTrue(made.single().derived.isEmpty())
            assertNull(m.sessions.value[id])
        }

    private companion object {
        const val TEN_MINUTES_MS = 600_000L
    }
}

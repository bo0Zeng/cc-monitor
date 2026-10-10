package com.ccmonitor.mobile.core.ssh

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** 会话计数与重连筛选谓词：保活计数和后台重连都靠它们。 */
class SessionStateTest {
    private fun state(status: SessionStatus) = SessionState(id = "h#x", hostId = "h", label = "L", status = status)

    // countsAsActive：只有 CONNECTED

    @Test
    fun connectedCountsAsActive() {
        assertTrue(state(SessionStatus.CONNECTED).countsAsActive())
    }

    @Test
    fun nonConnectedNeverCounts() {
        // 覆盖其余全部非 CONNECTED 状态。
        for (s in listOf(SessionStatus.CONNECTING, SessionStatus.RECONNECTING, SessionStatus.ERROR)) {
            assertFalse("$s 不应计入活跃", state(s).countsAsActive())
        }
    }

    // isErrorReconnectCandidate：ERROR

    @Test
    fun errorIsErrorReconnectCandidate() {
        assertTrue(state(SessionStatus.ERROR).isErrorReconnectCandidate())
    }

    @Test
    fun connectedIsNotErrorReconnectCandidate() {
        assertFalse(state(SessionStatus.CONNECTED).isErrorReconnectCandidate())
    }

    // isStaleReconnectCandidate：CONNECTED 且底层已死

    @Test
    fun connectedButDeadIsStaleCandidate() {
        assertTrue(state(SessionStatus.CONNECTED).isStaleReconnectCandidate(underlyingAlive = false))
    }

    @Test
    fun connectedAndAliveIsNotStale() {
        assertFalse(state(SessionStatus.CONNECTED).isStaleReconnectCandidate(underlyingAlive = true))
    }

    @Test
    fun nonConnectedIsNotStale() {
        assertFalse(state(SessionStatus.RECONNECTING).isStaleReconnectCandidate(underlyingAlive = false))
    }
}

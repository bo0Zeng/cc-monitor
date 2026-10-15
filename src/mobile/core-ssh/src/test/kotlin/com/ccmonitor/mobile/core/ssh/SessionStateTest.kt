package com.ccmonitor.mobile.core.ssh

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** 会话计数谓词：保活计数靠它。 */
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
}

package com.ccmonitor.mobile.ui

import com.ccmonitor.mobile.ui.host.hostDeleteWarning
import com.ccmonitor.mobile.ui.identity.identityDeleteWarning
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** 删除确认文案——必须点明不可撤销/级联后果。 */
class DeleteWarningTest {
    @Test fun hostWarningMentionsIrreversibleAndLabel() {
        val w = hostDeleteWarning("devbox")
        assertTrue(w.contains("devbox"))
        assertTrue("主机删除须点明不可撤销", w.contains("不可撤销"))
    }

    @Test fun identityWarningKeystoreMentionsUnrecoverableDestruction() {
        val w = identityDeleteWarning("hw-key", keystoreBacked = true)
        assertTrue(w.contains("hw-key"))
        assertTrue("硬件密钥须点明不可恢复销毁", w.contains("不可恢复"))
        assertTrue("须点明级联：主机无法连接", w.contains("无法连接"))
    }

    @Test fun identityWarningSoftwareMentionsCascadeButNotHardwareDestruction() {
        val w = identityDeleteWarning("sw-key", keystoreBacked = false)
        assertTrue("软件身份也须点明级联", w.contains("无法连接"))
        assertFalse("软件身份不该谎称硬件销毁", w.contains("不可恢复"))
        assertTrue(w.contains("不可撤销"))
    }
}

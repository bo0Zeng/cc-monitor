package com.ccmonitor.mobile.ui.session

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 终端自动重挂判据的纯逻辑测试。核心是不诈尸不变量：用户 `exit` 掉的 shell 永不重挂，
 * 哪怕那条连接之后独立死掉并被后台重建。分两层：[updateShellExited] 把断因钉在断流那一刻、[shouldReattach] 据此决定。
 */
class ShouldReattachTest {
    // ---- updateShellExited：断因闩（钉在断流上升沿，不随之后连接漂移）----

    @Test
    fun latchesUserExitWhenShellDroppedOnSameLiveConnection() {
        // 断的那一刻 shell 的连接仍是当前活着的同一对象 = 用户 exit（只关远端通道、transport 未死）→ 闩置 true。
        assertTrue(updateShellExited(prev = false, dropped = true, droppedOnSameLiveConn = true))
    }

    @Test
    fun latchesConnectionLossWhenShellDroppedWithConnectionGoneOrDead() {
        // 断的那一刻连接已死/已换（非同一活连接）= 因连接死而断 → 闩置 false（非 exit，允许后续重挂）。
        assertFalse(updateShellExited(prev = false, dropped = true, droppedOnSameLiveConn = false))
    }

    @Test
    fun keepsLatchAcrossNonDropEdges() {
        // 关键：dropped=false（新连/重连中）不重算、保持 prev——断因钉死不漂移。exit 闩住 true 后，
        // 即便后续连接态翻转（走到 dropped=false 边沿），仍保 true → 不诈尸。
        assertTrue("exit 闩住后非断流边沿保持 true", updateShellExited(prev = true, dropped = false, droppedOnSameLiveConn = false))
        assertFalse("连接死闩住后保持 false", updateShellExited(prev = false, dropped = false, droppedOnSameLiveConn = true))
    }

    // ---- shouldReattach：重挂决策 ----

    @Test
    fun reattachesWhenDroppedFromConnectionLossAndConnectionBack() {
        // shell 断 + 断因非 exit（连接死）+ 现有 live 连接（已重连）→ 重挂。
        assertTrue(shouldReattach(terminalDropped = true, userExited = false, liveConnActive = true))
    }

    @Test
    fun neverReattachesUserExitedShellEvenWithLiveConnection() {
        // 核心不变量：断因是用户 exit → 恒不重挂，哪怕现有一条 live 连接（连接后来独立死+重建也一样）。
        assertFalse(
            "用户 exit 掉的 shell 绝不重挂——即便连接已重建成新 live 连接",
            shouldReattach(terminalDropped = true, userExited = true, liveConnActive = true),
        )
    }

    @Test
    fun doesNotReattachWhileConnectionNotYetRecovered() {
        // 断因是连接死、但当前无 live 连接（还没重连回来）→ 不重挂、等连接恢复那一刻再触发。
        assertFalse(shouldReattach(terminalDropped = true, userExited = false, liveConnActive = false))
    }

    @Test
    fun doesNotReattachWhenShellAlive() {
        // shell 没断 → 无需重挂。
        assertFalse(shouldReattach(terminalDropped = false, userExited = false, liveConnActive = true))
    }

    // ---- 组合：exit 后连接重建（纯逻辑层）----

    @Test
    fun exitThenConnectionRebuildStillDoesNotReattach() {
        // 1) 用户 exit（断流上升沿、同活连接）→ 闩住 userExited=true。
        val userExited = updateShellExited(prev = false, dropped = true, droppedOnSameLiveConn = true)
        assertTrue(userExited)
        // 2) 之后连接独立死+重建、terminalDropped 全程保持 true（无新断流边沿 → 闩不重算，仍 true）；
        //    现有一条 live 连接。决策仍不重挂。
        assertFalse(
            "exit 后连接重建，仍绝不重挂",
            shouldReattach(terminalDropped = true, userExited = userExited, liveConnActive = true),
        )
    }
}

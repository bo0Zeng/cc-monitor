package com.ccmonitor.mobile.ui.session

import androidx.lifecycle.ViewModel
import com.ccmonitor.mobile.ui.session.SessionTabManager.TabTarget
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/** 会话总览的应用级状态（open 复用/get/close/store 稳定，去重按 hostId+TabTarget）。 */
class SessionTabManagerTest {
    @Test
    fun openAddsSessionAndReturnsKey() {
        val m = SessionTabManager()
        val k = m.open("h1", TabTarget.Terminal())
        assertEquals(1, m.sessions.value.size)
        assertEquals("h1", m.get(k)?.hostId)
    }

    @Test
    fun openReusesSameHostCd() {
        val m = SessionTabManager()
        val k1 = m.open("h1", TabTarget.Terminal("/a"))
        val k2 = m.open("h1", TabTarget.Terminal("/a")) // 相同 host + target → 复用（防列表点两次开重复）
        assertEquals("相同 target 应复用同一 key", k1, k2)
        assertEquals(1, m.sessions.value.size)
    }

    @Test
    fun differentParamsOpenDistinctSessions() {
        val m = SessionTabManager()
        val k1 = m.open("h1", TabTarget.Terminal())
        val k2 = m.open("h1", TabTarget.Terminal("/a")) // 不同 cd → 新会话（同主机可多会话=多 shell）
        val k3 = m.open("h2", TabTarget.Terminal()) // 不同 host → 新会话
        assertNotEquals(k1, k2)
        assertNotEquals(k1, k3)
        assertEquals(3, m.sessions.value.size)
    }

    // 去重键含 target 类型——同主机可同时有 1 终端 tab + 1 文件 tab。
    @Test
    fun sameHostDifferentTargetOpensDistinctTabs() {
        val m = SessionTabManager()
        val term = m.open("h1", TabTarget.Terminal())
        val files = m.open("h1", TabTarget.Files)
        assertNotEquals("同主机 终端 vs 文件 → 两个不同 tab", term, files)
        assertEquals(2, m.sessions.value.size)
        assertTrue(m.get(term)?.target is TabTarget.Terminal)
        assertEquals(TabTarget.Files, m.get(files)?.target)
    }

    @Test
    fun sameHostSameTargetReusesTab() {
        val m = SessionTabManager()
        val s1 = m.open("h1", TabTarget.Files)
        val s2 = m.open("h1", TabTarget.Files) // 同主机同文件屏 → 复用（每主机一个文件 tab）
        assertEquals("同主机同文件屏应复用", s1, s2)
        assertEquals(1, m.sessions.value.size)
    }

    // Terminal target 的 cd / tmuxSession payload 正确落库、可回读（HostedSession 分派据此）。
    @Test
    fun terminalTargetCarriesPayload() {
        val m = SessionTabManager()
        val k = m.open("h1", TabTarget.Terminal(cd = "/proj", tmuxSession = "cc"))
        val target = m.get(k)?.target
        assertTrue(target is TabTarget.Terminal)
        assertEquals("/proj", (target as TabTarget.Terminal).cd)
        assertEquals("cc", target.tmuxSession)
    }

    // attach=新 tab：tmuxSession 进去重键：默认/异名 attach = 新 tab，同名 attach 复用。
    @Test
    fun forceNewCreatesSecondTabWithOwnStore() {
        val m = SessionTabManager()
        val k1 = m.open("h1", TabTarget.Terminal())
        val k2 = m.open("h1", TabTarget.Terminal(), forceNew = true)
        assertNotEquals("forceNew 应强制新建（不复用同构 tab）", k1, k2)
        assertEquals(2, m.sessions.value.size)
        assertNotSame("两 tab 各持独立 ViewModelStore", m.storeFor(k1), m.storeFor(k2))
    }

    // forceNew=false（缺省）：主机屏直点仍复用。
    @Test
    fun defaultOpenDedupsWhenForceNewIsUnset() {
        val m = SessionTabManager()
        val k1 = m.open("h1", TabTarget.Terminal())
        val k2 = m.open("h1", TabTarget.Terminal(), forceNew = false)
        val k3 = m.open("h1", TabTarget.Terminal())
        assertEquals("forceNew=false 应复用", k1, k2)
        assertEquals("缺省参应复用", k1, k3)
        assertEquals(1, m.sessions.value.size)
    }

    // label 落 OpenSession 供渲染，但不进去重键：异 label 同 target 仍复用，非 null label 不被覆写。
    @Test
    fun closeOneForceNewCopyKeepsTheOther() {
        val m = SessionTabManager()
        val k1 = m.open("h1", TabTarget.Terminal())
        val k2 = m.open("h1", TabTarget.Terminal(), forceNew = true)
        var otherCleared = false
        // close 前捕获 k2 的 store 实例：close 后须是同一实例才证明未被误删（若误删，getOrPut
        // 会静默复建一个新实例、assertSame(storeFor,storeFor) 恒过检不出该 bug）。
        val s2 = m.storeFor(k2)
        s2.put(
            "vm",
            object : ViewModel() {
                override fun onCleared() {
                    otherCleared = true
                }
            },
        )
        m.close(k1)
        assertEquals(1, m.sessions.value.size)
        assertEquals("另一副本仍在册", "h1", m.get(k2)?.hostId)
        assertTrue("另一副本的 store 未被清", !otherCleared)
        assertSame("close(k1) 后 k2 仍是关闭前的同一 store（未被误删+静默复建）", s2, m.storeFor(k2))
    }

    @Test
    fun closeRemovesSession() {
        val m = SessionTabManager()
        val k = m.open("h1", TabTarget.Terminal())
        m.close(k)
        assertEquals(0, m.sessions.value.size)
        assertNull("关闭后取不到", m.get(k))
    }

    @Test
    fun storeForStableAcrossCallsSameKey() {
        val m = SessionTabManager()
        val k = m.open("h1", TabTarget.Terminal())
        assertSame("同 key 的 ViewModelStore 稳定（VM 保活载体）", m.storeFor(k), m.storeFor(k))
    }

    @Test
    fun closeClearsStoreTriggeringOnCleared() {
        // close(key) → store.clear() → 该 store 里 VM 的 onCleared（→真实链路里=SessionViewModel.onCleared→release）。
        val m = SessionTabManager()
        val k = m.open("h1", TabTarget.Terminal())
        val store = m.storeFor(k)
        var cleared = false
        store.put(
            "vm",
            object : ViewModel() {
                override fun onCleared() {
                    cleared = true
                }
            },
        ) // 模拟会话 VM 落该 store
        m.close(k)
        assertTrue("close 应 clear store → VM.onCleared 被调（→ 真实链路 release 连接）", cleared)
        // close 后 storeFor 不复活新 store（复活就是泄漏向量），而是拒绝。
        assertThrows(IllegalStateException::class.java) { m.storeFor(k) }
    }

    // storeFor 不得复活未知/已关 key 的 store：getOrPut 会造一个永不被 close 的幽灵 store
    // （其内 VM 的 onCleared 永不触发 → 连接引用泄漏）。未知与已关均拒；在册 key 行为不变（稳定同实例）。
    @Test
    fun storeForUnknownOrClosedKeyThrowsInsteadOfResurrecting() {
        val m = SessionTabManager()
        assertThrows("从未 open 的 key 应拒绝", IllegalStateException::class.java) { m.storeFor("sess-999") }
        val k = m.open("h1", TabTarget.Terminal())
        assertSame("在册 key 正常返回稳定 store", m.storeFor(k), m.storeFor(k))
        m.close(k)
        assertThrows("已 close 的 key 应拒绝（不复活幽灵 store）", IllegalStateException::class.java) { m.storeFor(k) }
    }
}

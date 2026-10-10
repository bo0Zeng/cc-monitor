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
    fun openReusesSameHostCdLauncher() {
        val m = SessionTabManager()
        val k1 = m.open("h1", TabTarget.Terminal("/a", "L"))
        val k2 = m.open("h1", TabTarget.Terminal("/a", "L")) // 相同 host + target → 复用（防列表点两次开重复）
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

    // 去重键含 target 类型——同主机可同时有 1 终端 tab + 1 SFTP tab。
    @Test
    fun sameHostDifferentTargetOpensDistinctTabs() {
        val m = SessionTabManager()
        val term = m.open("h1", TabTarget.Terminal())
        val sftp = m.open("h1", TabTarget.Sftp)
        assertNotEquals("同主机 终端 vs SFTP → 两个不同 tab", term, sftp)
        assertEquals(2, m.sessions.value.size)
        assertTrue(m.get(term)?.target is TabTarget.Terminal)
        assertEquals(TabTarget.Sftp, m.get(sftp)?.target)
    }

    @Test
    fun sameHostSameTargetReusesTab() {
        val m = SessionTabManager()
        val s1 = m.open("h1", TabTarget.Sftp)
        val s2 = m.open("h1", TabTarget.Sftp) // 同主机同 SFTP → 复用（一 SFTP tab/主机）
        assertEquals("同主机同 SFTP target 应复用", s1, s2)
        assertEquals(1, m.sessions.value.size)
    }

    // Terminal target 的 cd/launcherId payload 正确落库、可回读（HostedSession 分派据此）。
    @Test
    fun terminalTargetCarriesPayload() {
        val m = SessionTabManager()
        val k = m.open("h1", TabTarget.Terminal(cd = "/proj", launcherId = "cc"))
        val target = m.get(k)?.target
        assertTrue(target is TabTarget.Terminal)
        assertEquals("/proj", (target as TabTarget.Terminal).cd)
        assertEquals("cc", target.launcherId)
    }

    // attach=新 tab：tmuxSession 进去重键：默认/异名 attach = 新 tab，同名 attach 复用。
    @Test
    fun attachTmuxDedupsByName() {
        val m = SessionTabManager()
        val def = m.open("h1", TabTarget.Terminal()) // 默认 tab（tmuxSession=null）
        val work1 = m.open("h1", TabTarget.Terminal(tmuxSession = "work")) // 附着 work → 新 tab（不挤默认）
        val work2 = m.open("h1", TabTarget.Terminal(tmuxSession = "work")) // 再附着 work → 复用
        val other = m.open("h1", TabTarget.Terminal(tmuxSession = "other")) // 附着 other → 又一新 tab
        assertNotEquals("attach tmux tab 应区别于默认终端 tab", def, work1)
        assertEquals("同名 attach 复用", work1, work2)
        assertNotEquals("异名 attach 各成新 tab", work1, other)
        assertEquals(3, m.sessions.value.size)
        assertEquals("work", (m.get(work1)?.target as TabTarget.Terminal).tmuxSession)
    }

    // resume=新 tab：resumeSessionId 进去重键：同 sid 复用、异 sid 各开、不挤默认 tab。
    @Test
    fun resumeTabDedupsBySessionId() {
        val m = SessionTabManager()
        val def = m.open("h1", TabTarget.Terminal())
        val r1 = m.open("h1", TabTarget.Terminal(cd = "/p", resumeSessionId = "sid-a"))
        val r2 = m.open("h1", TabTarget.Terminal(cd = "/p", resumeSessionId = "sid-a")) // 历史页重复点同会话 → 复用
        val r3 = m.open("h1", TabTarget.Terminal(cd = "/p", resumeSessionId = "sid-b")) // 另一会话 → 新 tab
        assertNotEquals("resume tab 应区别于默认终端 tab", def, r1)
        assertEquals("同 sid resume 复用", r1, r2)
        assertNotEquals("异 sid resume 各成新 tab", r1, r3)
        assertEquals(3, m.sessions.value.size)
        assertEquals("sid-a", (m.get(r1)?.target as TabTarget.Terminal).resumeSessionId)
    }

    // resume tab 去重归一为 hostId+resumeSessionId：cd 只是 fallbackCwd，不进 resume 身份。
    // 「cwd 解不出时长按 resume（cd=null）」与「阅读器续接同 sid（cd 非空）」必须复用同一 tab。
    @Test
    fun resumeTabDedupsBySessionIdIgnoringCd() {
        val m = SessionTabManager()
        val r1 = m.open("h1", TabTarget.Terminal(cd = null, resumeSessionId = "sid-a")) // 历史页长按（cwd 未解出）
        val r2 = m.open("h1", TabTarget.Terminal(cd = "/proj", resumeSessionId = "sid-a")) // 阅读器「▶ 续接」（rt.cwd 非空）
        assertEquals("同 sid 异 cd → 复用同一 tab（cd 不进 resume 去重身份）", r1, r2)
        assertEquals(1, m.sessions.value.size)
        // 异 launcherId 同 sid 也复用（launcherId 对 resume 无区分意义）。
        val r3 = m.open("h1", TabTarget.Terminal(cd = "/x", launcherId = "L2", resumeSessionId = "sid-a"))
        assertEquals("同 sid 异 launcherId → 仍复用", r1, r3)
        assertEquals(1, m.sessions.value.size)
        // 异 sid 各开、异 host 各开。
        val r4 = m.open("h1", TabTarget.Terminal(resumeSessionId = "sid-b"))
        val r5 = m.open("h2", TabTarget.Terminal(resumeSessionId = "sid-a"))
        assertNotEquals(r1, r4)
        assertNotEquals(r1, r5)
        assertEquals(3, m.sessions.value.size)
    }

    // 归一只作用于「两侧都是 resume tab」：普通/attach tab（resumeSessionId=null）去重仍走结构相等。
    @Test
    fun nonResumeTabDedupUsesStructuralEquality() {
        val m = SessionTabManager()
        val a = m.open("h1", TabTarget.Terminal(cd = "/a"))
        val b = m.open("h1", TabTarget.Terminal(cd = "/b")) // 异 cd 且无 sid → 结构不等 → 各开
        assertNotEquals("普通 tab 异 cd 仍各开", a, b)
        // resume tab（有 sid）不得与普通 tab（无 sid）复用：一侧 sid=null 走结构相等，cd/sid 都不同。
        val r = m.open("h1", TabTarget.Terminal(cd = "/a", resumeSessionId = "sid-a"))
        assertNotEquals("resume tab 不与同 cd 的普通 tab 复用", a, r)
        assertEquals(3, m.sessions.value.size)
    }

    // forceNew=true 跳过复用查找：同构 target 也各成独立 tab（两 key、两独立 store=独立 VM/shell）。
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
    fun labelStoredButNotPartOfDedupKey() {
        val m = SessionTabManager()
        val k1 = m.open("h1", TabTarget.Terminal(cd = "/p", resumeSessionId = "sid-a"), label = "修复登录 bug")
        assertEquals("label 应落 OpenSession", "修复登录 bug", m.get(k1)?.label)
        val k2 = m.open("h1", TabTarget.Terminal(cd = "/p", resumeSessionId = "sid-a"), label = "另一个标题")
        assertEquals("label 不参与去重：同 sid 二次 resume 仍复用同一 tab", k1, k2)
        assertEquals(1, m.sessions.value.size)
        assertEquals("去重命中时既有非 null label 不被覆写", "修复登录 bug", m.get(k1)?.label)
    }

    // null label 可被非 null 覆写：阅读器 null-label resume 后、历史页对同 sid 再 resume
    // 补上真 title（两次非同源）；非 null → 非 null 仍 first-write-wins（上一用例）。
    @Test
    fun nullLabelUpgradedByLaterNonNullOnDedupHit() {
        val m = SessionTabManager()
        val k1 = m.open("h1", TabTarget.Terminal(cd = "/p", resumeSessionId = "sid-a"), label = null) // 阅读器「续接」查不到 title
        assertNull(m.get(k1)?.label)
        val k2 = m.open("h1", TabTarget.Terminal(cd = "/p", resumeSessionId = "sid-a"), label = "修复登录 bug") // 历史页带真 title
        assertEquals("同 sid 复用同一 tab", k1, k2)
        assertEquals("null label 被非 null 覆写（可读名补齐）", "修复登录 bug", m.get(k1)?.label)
        val k3 = m.open("h1", TabTarget.Terminal(cd = "/p", resumeSessionId = "sid-a"), label = null)
        assertEquals(k1, k3)
        assertEquals("后续 null 不清掉已覆写的 label", "修复登录 bug", m.get(k1)?.label)
    }

    // resumeCommand 落 OpenSession 但不进去重键：同 sid 异命令复用首 tab（第二 tab 只会
    // attach 同一 tmux 会话、命令永不生效，各成 tab 是欺骗 UI）；且 first-write-wins 不覆写（复用 tab 已在跑）。
    @Test
    fun resumeCommandStoredNotDedupKeyAndFirstWriteWins() {
        val m = SessionTabManager()
        val k1 = m.open("h1", TabTarget.Terminal(cd = "/p", resumeSessionId = "sid-a"), label = "T", resumeCommand = "cct")
        assertEquals("resumeCommand 应落 OpenSession", "cct", m.get(k1)?.resumeCommand)
        val k2 = m.open("h1", TabTarget.Terminal(cd = "/p", resumeSessionId = "sid-a"), label = "T", resumeCommand = "cc")
        assertEquals("同 sid 异 command 复用同一 tab（command 不进去重键）", k1, k2)
        assertEquals(1, m.sessions.value.size)
        assertEquals("去重命中时 resumeCommand first-write-wins 不覆写", "cct", m.get(k1)?.resumeCommand)
        val k3 = m.open("h1", TabTarget.Terminal(cd = "/p", resumeSessionId = "sid-b"), resumeCommand = "cc")
        assertNotEquals("异 sid 仍各开 tab（resumeTabDedupsBySessionId 语义不破）", k1, k3)
        assertEquals("cc", m.get(k3)?.resumeCommand)
    }

    // 去重命中「resume 已失败」的 tab（注册句柄接受）→ 新命令/cwd 送达句柄恰一次 + 账本被覆写
    // （同 tab re-drive；retry 不用被钉死的坏命令）。label 的 null→非 null 升级照常并行生效。
    @Test
    fun failedResumeReuseRedrivesAndUpdatesCommandAndCwd() {
        val m = SessionTabManager()
        val k1 = m.open("h1", TabTarget.Terminal(cd = null, resumeSessionId = "sid-a"), resumeCommand = "cct")
        val calls = mutableListOf<Pair<String?, String?>>()
        m.registerResumeRedriver("h1", "sid-a") { c, w ->
            calls.add(c to w)
            true // 模拟 VM 处于失败态（watchdog FAILED/ABSENT）：接受并重跑
        }
        val k2 = m.open("h1", TabTarget.Terminal(cd = "/proj", resumeSessionId = "sid-a"), label = "T", resumeCommand = "claude")
        assertEquals("失败态 re-drive 仍复用同一 tab（sid 归一不破，不退回异命令各开 tab）", k1, k2)
        assertEquals("新命令/cwd 送达句柄恰一次", listOf<Pair<String?, String?>>("claude" to "/proj"), calls)
        assertEquals("接受后账本命令被覆写（失败态放宽 first-write-wins）", "claude", m.get(k1)?.resumeCommand)
        assertEquals("接受后 target.cd 升级为新 fallbackCwd", "/proj", (m.get(k1)?.target as TabTarget.Terminal).cd)
        assertEquals("null label 照常被非 null 覆写", "T", m.get(k1)?.label)
        assertEquals(1, m.sessions.value.size)
    }

    // running/在飞态复用（句柄拒绝）→ 不覆写、维持 first-write-wins（绝不 re-drive 运行中的 Claude）。
    @Test
    fun runningResumeReuseRejectedKeepsFirstWriteWins() {
        val m = SessionTabManager()
        val k1 = m.open("h1", TabTarget.Terminal(cd = "/p", resumeSessionId = "sid-a"), resumeCommand = "cct")
        var consulted = 0
        m.registerResumeRedriver("h1", "sid-a") { _, _ ->
            consulted++
            false // 模拟 VM running/在飞/连接中：拒绝
        }
        val k2 = m.open("h1", TabTarget.Terminal(cd = "/q", resumeSessionId = "sid-a"), resumeCommand = "cc")
        assertEquals("仍复用同一 tab", k1, k2)
        assertEquals("句柄被询问一次", 1, consulted)
        assertEquals("拒绝 → resumeCommand 维持 first-write-wins", "cct", m.get(k1)?.resumeCommand)
        assertEquals("拒绝 → target.cd 不动", "/p", (m.get(k1)?.target as TabTarget.Terminal).cd)
    }

    // 首次 open（新建 tab）不询问句柄：首次 resume 行为不变。
    @Test
    fun firstOpenDoesNotConsultRedriver() {
        val m = SessionTabManager()
        var consulted = false
        m.registerResumeRedriver("h1", "sid-a") { _, _ ->
            consulted = true
            true
        }
        val k = m.open("h1", TabTarget.Terminal(resumeSessionId = "sid-a"), resumeCommand = "cct")
        assertTrue("新建路径不走 re-drive", !consulted)
        assertEquals("首写照常落账本", "cct", m.get(k)?.resumeCommand)
    }

    // 句柄已注销（VM onCleared）→ 去重命中回到 first-write-wins 语义（不误调已死 VM）。
    @Test
    fun unregisteredRedriverKeepsStatusQuoOnReuse() {
        val m = SessionTabManager()
        val k1 = m.open("h1", TabTarget.Terminal(cd = "/p", resumeSessionId = "sid-a"), resumeCommand = "cct")
        m.registerResumeRedriver("h1", "sid-a") { _, _ -> true }
        m.unregisterResumeRedriver("h1", "sid-a")
        val k2 = m.open("h1", TabTarget.Terminal(cd = "/q", resumeSessionId = "sid-a"), resumeCommand = "cc")
        assertEquals(k1, k2)
        assertEquals("注销后回到 first-write-wins（同 resumeCommandStoredNotDedupKeyAndFirstWriteWins）", "cct", m.get(k1)?.resumeCommand)
    }

    // 接受的 re-drive 里 null 值语义：command=null 也覆写（回默认候选链，坏命令必须掉头）；
    // cwd=null 不降级已有 fallbackCwd（null 无信息量）。
    @Test
    fun acceptedRedriveNullCommandOverwritesNullCwdKeeps() {
        val m = SessionTabManager()
        val k1 = m.open("h1", TabTarget.Terminal(cd = "/p", resumeSessionId = "sid-a"), resumeCommand = "cct")
        m.registerResumeRedriver("h1", "sid-a") { _, _ -> true }
        m.open("h1", TabTarget.Terminal(cd = null, resumeSessionId = "sid-a"), resumeCommand = null)
        assertNull("command=null 覆写为默认候选链（坏命令不再钉死）", m.get(k1)?.resumeCommand)
        assertEquals("cwd=null 不降级已有 fallbackCwd", "/p", (m.get(k1)?.target as TabTarget.Terminal).cd)
    }

    // forceNew 的两个同构副本相互独立：关其一不影响另一（store 不被误清、tab 不被误删）。
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

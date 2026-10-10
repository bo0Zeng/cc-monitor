package com.ccmonitor.mobile.core.claude.catalog

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 红绿灯：pidfile status → 灯、同 sid 多份 pidfile 的 kind 消解、分组。 */
class SessionStatusLightTest {
    private fun pf(
        sid: String,
        status: String? = null,
        kind: String? = null,
        procStart: String? = null,
        pid: Long = 1,
    ) = SessionPidfile(
        pid = pid,
        sessionId = sid,
        cwd = null,
        name = null,
        procStart = procStart,
        kind = kind,
        status = status,
    )

    @Test
    fun lightMapping() {
        assertEquals(SessionLight.Working, lightForStatus("busy", alive = true))
        assertEquals(SessionLight.WaitingInput, lightForStatus("waiting", alive = true))
        assertEquals(SessionLight.Idle, lightForStatus("idle", alive = true))
        assertEquals(SessionLight.Shell, lightForStatus("shell", alive = true))
    }

    @Test
    fun unknownStatusIsNeutral() {
        assertEquals("null status→Unknown 不误报", SessionLight.Unknown, lightForStatus(null, alive = true))
        assertEquals("未知取值→Unknown", SessionLight.Unknown, lightForStatus("frobnicate", alive = true))
    }

    @Test
    fun deadIsStopped() =
        assertEquals("alive=false→Stopped（即便 status=busy）", SessionLight.Stopped, lightForStatus("busy", alive = false))

    @Test
    fun resolveSingleReturnsIt() {
        val p = pf("s", "busy")
        assertEquals(p, resolveLivePidfile(listOf(p)))
    }

    @Test
    fun resolveEmptyIsNull() = assertNull(resolveLivePidfile(emptyList()))

    @Test
    fun interactiveBeatsBg() {
        // 同 sid：bg 分身 procStart 更新，但 interactive 压 bg → 取 interactive。
        val bg = pf("s", "idle", kind = "bg", procStart = "999")
        val inter = pf("s", "busy", kind = "interactive", procStart = "100")
        assertEquals("interactive 压 bg", inter, resolveLivePidfile(listOf(bg, inter)))
    }

    @Test
    fun sameKindTakesLatestProcStartNumeric() {
        val old = pf("s", "idle", kind = "interactive", procStart = "9")
        val new = pf("s", "busy", kind = "interactive", procStart = "100")
        // 100>9 数值比较（字典序会误判 "100"<"9"）。
        assertEquals("同 kind 取 procStart 最新（数值）", new, resolveLivePidfile(listOf(old, new)))
    }

    @Test
    fun unknownKindDemotedLikeBg() {
        // 未知非 bg 值（如 "worker"）不算 interactive 级、被降到 bg 级 → interactive 压过它。
        val worker = pf("s", "busy", kind = "worker", procStart = "999")
        val inter = pf("s", "idle", kind = "interactive", procStart = "100")
        assertEquals("interactive 压过未知 kind", inter, resolveLivePidfile(listOf(worker, inter)))
    }

    @Test
    fun bgOnlyIsNotActive() {
        // 全 bg 组（父退出、bg-spare 残留）→ resolveActivePidfile 返 null（只有 interactive 才判活）。
        val bg1 = pf("s", "idle", kind = "bg", procStart = "100")
        val bg2 = pf("s", "shell", kind = "bg", procStart = "200")
        assertNull("全 bg → 无活会话", resolveActivePidfile(listOf(bg1, bg2)))
        // 有 interactive → 取 interactive（仍活）。
        val inter = pf("s", "busy", kind = "interactive", procStart = "50")
        assertEquals(inter, resolveActivePidfile(listOf(bg1, inter)))
        // kind=null（缺失）算 interactive 级 → 活。
        val nullKind = pf("s", "busy", procStart = "10")
        assertEquals(nullKind, resolveActivePidfile(listOf(nullKind)))
    }

    @Test
    fun tieBreaksByLargerPid() {
        // procStart 相等/缺失 → pid 大者胜（消 glob 顺序任意性）。
        val a = pf("s", "idle", kind = "interactive", procStart = "100", pid = 10)
        val b = pf("s", "busy", kind = "interactive", procStart = "100", pid = 20)
        assertEquals("procStart 平局→大 pid 胜", b, resolveLivePidfile(listOf(a, b)))
        val c = pf("s", "idle", kind = "interactive", procStart = null, pid = 30)
        val d = pf("s", "busy", kind = "interactive", procStart = null, pid = 40)
        assertEquals("procStart 都缺→大 pid 胜", d, resolveLivePidfile(listOf(c, d)))
    }
}

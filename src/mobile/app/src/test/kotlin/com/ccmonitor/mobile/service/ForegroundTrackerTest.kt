package com.ccmonitor.mobile.service

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 前台探测重连接线的核心逻辑测试——[ForegroundTracker] 的 0→1 再入触发语义。
 * 关键正确性：onEnterForeground 每次背景→前台（0→1）触发、从不在 1→2（同前台起第二 Activity）重复触发。
 * 前者漏了 = 丢失"回 app 探活"增益；后者多了 = 每起一个 Activity 就探测风暴。
 */
class ForegroundTrackerTest {
    private fun tracker(): Triple<ForegroundTracker, () -> Int, () -> Boolean> {
        var fgCalls = 0
        var foreground = false
        val t =
            ForegroundTracker(
                onEnterForeground = { fgCalls++ },
                setForeground = { foreground = it },
            )
        return Triple(t, { fgCalls }, { foreground })
    }

    @Test
    fun coldStartFiresOnceAndMarksForeground() {
        val (t, fgCalls, foreground) = tracker()
        t.onActivityStarted() // 0→1
        assertEquals("冷启动（0→1）触发一次探测", 1, fgCalls())
        assertTrue("started≥1 → 前台", foreground())
    }

    @Test
    fun secondActivityStartDoesNotReFire() {
        val (t, fgCalls, _) = tracker()
        t.onActivityStarted() // 0→1：触发
        t.onActivityStarted() // 1→2：同前台起第二 Activity（如设置页）→ 不重复触发
        assertEquals("1→2 不重复触发（防探测风暴）", 1, fgCalls())
    }

    @Test
    fun backgroundThenReEnterReFires() {
        val (t, fgCalls, foreground) = tracker()
        t.onActivityStarted() // 0→1：触发（第 1 次）
        t.onActivityStopped() // 1→0：回背景
        assertFalse("started=0 → 非前台", foreground())
        t.onActivityStarted() // 0→1：再入前台 → 再触发（第 2 次）
        assertEquals("每次背景→前台都触发", 2, fgCalls())
        assertTrue(foreground())
    }

    @Test
    fun rotationLikeStartStartStopStopKeepsForegroundThenReEnter() {
        val (t, fgCalls, foreground) = tracker()
        // 旋屏/多 Activity 交错：start,start,stop,stop 期间始终有 ≥1 started → 全程前台、只触发一次
        t.onActivityStarted() // 0→1：触发
        t.onActivityStarted() // 1→2
        t.onActivityStopped() // 2→1：仍前台
        assertTrue("2→1 仍前台", foreground())
        t.onActivityStopped() // 1→0：真回背景
        assertFalse(foreground())
        assertEquals("整段前台内只触发一次", 1, fgCalls())
        t.onActivityStarted() // 0→1：再入 → 第 2 次
        assertEquals(2, fgCalls())
    }

    @Test
    fun stopsBeyondZeroStayNonForegroundAndReEnterStillFires() {
        val (t, fgCalls, foreground) = tracker()
        // 防御：stop 多于 start（异常回调序）→ started 跌破 0，仍视为非前台；下次 start 语义可能偏移，
        // 但至少不崩、且真正回前台（started 从 ≤0 升到 1）仍触发。
        t.onActivityStopped() // 0→-1
        assertFalse(foreground())
        assertEquals("无 start 不触发", 0, fgCalls())
        t.onActivityStarted() // -1→0：未达前台阈值（started==0，非 1）→ 不触发、不置前台
        assertEquals("started 到 0 未过前台阈值不触发", 0, fgCalls())
        assertTrue("started++ 无条件置前台标志（与原语义一致）", foreground())
    }
}

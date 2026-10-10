package com.ccmonitor.mobile.core.claude.transport

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * [TurnEndDebouncer]：把 daemon 流的 `turn_end` 帧折成回合结束事件。
 * 每一条钉着一条后端契约（见 [TurnEndDebouncer] 头注）；证明的是「帧按契约到达时折法是对的」。
 */
class TurnEndDebouncerTest {
    private fun te(
        sid: String,
        uuid: String,
    ) = JsonlFrame.TurnEnd(sid, uuid)

    /**
     * 后端逐记录发、不去重：同一轮被拆成多条记录 ⇒ 多帧、uuid 各不同。
     * 只许结算成一条事件，取最后那个 uuid。
     */
    @Test
    fun oneRoundSplitIntoManyFramesSettlesOnce() {
        val d = TurnEndDebouncer()
        d.onFrame(te("s1", "u-a"), 0)
        d.onFrame(te("s1", "u-b"), 100)
        d.onFrame(te("s1", "u-c"), 200)
        assertEquals("还在静默窗口内，不许结算", emptyList<TurnEndDebouncer.Settled>(), d.settle(900))
        assertEquals(
            "同一轮多帧只出一条事件，uuid 取最后一帧",
            listOf(TurnEndDebouncer.Settled("s1", "u-c")),
            d.settle(1_400),
        )
    }

    /**
     * 一条流管所有会话。用 `Flow.debounce` 会让后到的会话压掉先到的；按 sid 各记一份，两条都要出来。
     */
    @Test
    fun twoSessionsOnOneStreamDoNotSwallowEachOther() {
        val d = TurnEndDebouncer()
        d.onFrame(te("s1", "u1"), 0)
        // s2 比 s1 晚 1000ms 才来：若去抖作用于整条流，s1 的窗口会被 s2 不断续上、永不结算。
        d.onFrame(te("s2", "u2"), 1_000)
        assertEquals(
            "s1 已静默满窗 ⇒ 它该结算，不受 s2 的新帧影响",
            listOf(TurnEndDebouncer.Settled("s1", "u1")),
            d.settle(1_300),
        )
        assertEquals(
            "s2 随后各自结算",
            listOf(TurnEndDebouncer.Settled("s2", "u2")),
            d.settle(2_300),
        )
    }

    /** 去重在 aterm 侧：重连 / 重读时同一轮再来一遍，不许再通知。 */
    @Test
    fun theSameRoundArrivingAgainIsNotNotifiedTwice() {
        val d = TurnEndDebouncer()
        d.onFrame(te("s1", "u1"), 0)
        assertEquals(listOf(TurnEndDebouncer.Settled("s1", "u1")), d.settle(1_300))
        d.onFrame(te("s1", "u1"), 5_000) // 重连重读，同一轮
        assertEquals("同一个 uuid 不许再结算一次", emptyList<TurnEndDebouncer.Settled>(), d.settle(6_300))
        d.onFrame(te("s1", "u2"), 9_000) // 真的新一轮
        assertEquals(listOf(TurnEndDebouncer.Settled("s1", "u2")), d.settle(10_300))
    }

    /**
     * 会话被移除再回来 ⇒ 故意重新通知（[TurnEndDebouncer.forget] 的注写了取向）。
     * `/compact` 会原地重写文件，继续拿旧 uuid 去重会把真的新一轮当成重复而漏掉通知。
     * 宁可多一条，不要少一条。
     */
    @Test
    fun aSessionComingBackAfterRemovalMayNotifyAgainOnPurpose() {
        val d = TurnEndDebouncer()
        d.onFrame(te("s1", "u1"), 0)
        assertEquals(listOf(TurnEndDebouncer.Settled("s1", "u1")), d.settle(1_300))
        d.forget("s1")
        d.onFrame(te("s1", "u1"), 5_000)
        assertEquals(
            "移除后重来：同一个 uuid 也要再出一条（多一条胜过漏一条）",
            listOf(TurnEndDebouncer.Settled("s1", "u1")),
            d.settle(6_300),
        )
    }

    /** 没结算的帧不许被 [TurnEndDebouncer.settle] 吃掉（空返回 ≠ 丢状态）。 */
    @Test
    fun framesStillInsideTheWindowStayPending() {
        val d = TurnEndDebouncer()
        d.onFrame(te("s1", "u1"), 0)
        d.settle(500)
        assertEquals("窗口内结算一次不许丢掉待结算项", 1, d.pendingCount())
        assertEquals(listOf(TurnEndDebouncer.Settled("s1", "u1")), d.settle(1_300))
        assertEquals("结算后清空", 0, d.pendingCount())
    }

    /** 静默窗口与逐行跟尾那条路用的值相同，两条路上感知的延迟一样。 */
    @Test
    fun theSettleWindowIs1200Ms() {
        assertEquals(1_200L, TurnEndDebouncer.SETTLE_MS)
    }
}

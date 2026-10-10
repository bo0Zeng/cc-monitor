package com.ccmonitor.mobile.core.claude.catalog

import com.ccmonitor.mobile.core.claude.transport.SessionSignals
import com.ccmonitor.mobile.core.claude.transport.SignalSource
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/**
 * daemon 流的点灯口 [lightForActivity]：活动档 → 灯。
 *
 * 取值是后端 `SessionActivity` 的 snake_case：`working` / `needs_you` / `idle`。
 * 这里只守「档 → 灯」这张表本身；总览面读的是这一格而不是 `status`，由 app 侧从真帧出发守。
 */
class SessionActivityLightTest {
    /** 三个 wire 值各点各的灯，且三盏互不相同（并成一盏的话「需手动」那一区就分不出来了）。 */
    @Test
    fun theThreeWireValuesEachLightTheirOwnLamp() {
        assertEquals(SessionLight.Working, lightForActivity("working"))
        assertEquals(SessionLight.WaitingInput, lightForActivity("needs_you"))
        assertEquals(SessionLight.Idle, lightForActivity("idle"))
    }

    /**
     * 缺席与认不出的词一律中性，pidfile 的原词也算认不出。
     *
     * `busy` / `waiting` / `shell` 是 `status` 的词，不是活动档的词。收进来就是在点灯口里
     * 留了一条 `status` 的读法；`shell` 一收，后端判成「闲着」的会话会被重新判成「在跑别的命令」。
     *
     * 另守进程生死不上屏：任何输入都点不出进程层的 `Stopped`（这个口没有 `alive` 参数），
     * 也点不出活动档里不存在的 `Shell`。
     */
    @Test
    fun anythingElseIsNeutralIncludingTheStatusWords() {
        val notActivity = listOf(null, "", "busy", "waiting", "shell", "NeedsYou", "needs-you", "Working", "IDLE")
        for (a in notActivity) {
            assertEquals("「$a」不是活动档 ⇒ 中性灯", SessionLight.Unknown, lightForActivity(a))
        }
        for (a in notActivity + listOf("working", "needs_you", "idle")) {
            val light = lightForActivity(a)
            assertFalse("「$a」⇒ $light：这个口不许点出进程层的「已停」", light == SessionLight.Stopped)
            assertFalse("「$a」⇒ $light：活动档里没有 shell 那一档", light == SessionLight.Shell)
        }
    }

    /**
     * 「在等人」在两个消费方必须是同一个判定：总览把它排进「需手动」区（灯），
     * 聊天屏靠等待态总线决定拦不拦上行（[SessionSignals.publishWaiting]）。
     * 两边各认各的字面量，就会一边说「需手动」一边照常放行，而各自的判据都绿。
     * 只喂 `status` 恒空的帧。
     */
    @Test
    fun theLampAndTheWaitingBusAgreeOnWhoIsWaiting() {
        for (a in listOf("working", "needs_you", "idle", null, "shell", "waiting")) {
            val bus = SessionSignals()
            bus.publishWaiting(
                "s",
                status = null,
                waitingFor = "permission prompt",
                source = SignalSource.DAEMON_SNAPSHOT,
                nowMs = 1L,
                activity = a,
            )
            assertEquals(
                "活动档「$a」：灯说在等 ⇔ 总线说在等",
                bus.signalFor("s")?.waiting != null,
                lightForActivity(a) == SessionLight.WaitingInput,
            )
        }
    }
}

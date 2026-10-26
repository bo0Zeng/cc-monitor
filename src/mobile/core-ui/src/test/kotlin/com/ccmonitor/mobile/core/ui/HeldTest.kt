package com.ccmonitor.mobile.core.ui

import org.junit.Assert.assertEquals
import org.junit.Test

/** 一格输入框手上的字（[Held]）：打了一半的字遇到一次推送还在；没动过的跟着读到的那份走。 */
class HeldTest {
    @Test
    fun halfTypedTextSurvivesAPush() {
        val h = Held("20:00")
        h.text = "21:"
        h.offer("19:00")
        assertEquals("打了一半的字不被推送冲掉", "21:", h.text)
    }

    @Test
    fun untouchedTextFollowsWhatWasRead() {
        val h = Held("20:00")
        h.offer("19:00")
        assertEquals("19:00", h.text)
    }

    @Test
    fun onceTheReadCatchesUpWithTheTypedTextItFollowsAgain() {
        val h = Held("30")
        h.text = "40"
        h.offer("40")
        h.offer("50")
        assertEquals("交成了（读回来的就是打的那个）⇒ 之后又跟着读到的走", "50", h.text)
    }

    @Test
    fun resetGoesBackToWhatWasLastRead() {
        val h = Held("30")
        h.text = "4x"
        h.offer("35")
        h.reset()
        assertEquals("35", h.text)
    }
}

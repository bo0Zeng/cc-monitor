package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.copy.copyText
import org.junit.Assert.assertEquals
import org.junit.Test

/** 往上翻没读成：核心写好的那一句原样一行（手机不包），「上滑重试」另起一行、从文案表取。 */
class HistoryErrorLinesTest {
    @Test
    fun `核心那句原样在第一行，重试那句另起一行`() {
        val why = "dev 无应答 · 未读到更早的记录"
        assertEquals(listOf(why, copyText("mobChat.history.retry")), historyErrorLines(why))
        assertEquals("上滑重试", copyText("mobChat.history.retry"))
    }
}

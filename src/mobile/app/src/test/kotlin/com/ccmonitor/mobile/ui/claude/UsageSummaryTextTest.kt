package com.ccmonitor.mobile.ui.claude

import com.ccmonitor.mobile.core.claude.model.UsageSummary
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** 用量条显示格式化纯函数 [fmtTokens] / [usageSummaryText]。 */
class UsageSummaryTextTest {
    @Test
    fun fmtTokensKAndM() {
        assertEquals("500", fmtTokens(500))
        assertEquals("1.5K", fmtTokens(1_500))
        assertEquals("12.3K", fmtTokens(12_345))
        assertEquals("1.5M", fmtTokens(1_500_000))
        assertEquals("0", fmtTokens(0))
    }

    private fun summary(
        input: Long = 1000,
        output: Long = 500,
        w5: Long = 0,
        w1: Long = 0,
        read: Long = 0,
        ctx: Long = 45_000,
    ) = UsageSummary(1, input, output, w5, w1, read, ctx, "sonnet")

    @Test
    fun summaryHasTokensAndContext() {
        val t = usageSummaryText(summary(), windowed = false)
        assertTrue("有 in/out", t.contains("↑1.0K") && t.contains("↓500"))
        assertTrue("有缓存", t.contains("缓存"))
        assertTrue("有上下文 + 百分比", t.contains("上下文 45.0K/200.0K") && t.contains("22%"))
        assertFalse("非窗口不标", t.contains("窗口内"))
    }

    @Test
    fun windowedMarked() {
        val t = usageSummaryText(summary(), windowed = true)
        assertTrue("窗口内标注", t.contains("窗口内"))
    }

    /**
     * 用量条永远不出现钱：任何美元符号或「花费」字样出现在用量条上就红。
     */
    @Test
    fun usageLineNeverShowsMoney() {
        val t = usageSummaryText(summary(), windowed = false)
        assertFalse("用量条不许出现钱：$t", t.contains("$") || t.contains("花费") || t.contains("≈"))
    }
}

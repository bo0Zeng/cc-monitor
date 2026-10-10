package com.ccmonitor.mobile.core.claude.diff

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** 行级 diff：上下文/增/删、换行归一、截断与降级。 */
class LineDiffTest {
    @Test fun identicalAllContext() {
        val r = LineDiff.diff("a\nb\nc", "a\nb\nc")
        assertEquals(0, r.addCount)
        assertEquals(0, r.delCount)
        assertEquals(3, r.rows.size)
        assertTrue(r.rows.all { it.type == DiffType.CTX })
    }

    @Test fun pureAdd() {
        val r = LineDiff.diff("", "x\ny")
        assertEquals(2, r.addCount)
        assertEquals(0, r.delCount)
        assertTrue(r.rows.all { it.type == DiffType.ADD })
    }

    @Test fun pureDel() {
        val r = LineDiff.diff("x\ny", "")
        assertEquals(0, r.addCount)
        assertEquals(2, r.delCount)
        assertTrue(r.rows.all { it.type == DiffType.DEL })
    }

    @Test fun mixedEditDelBeforeAdd() {
        val r = LineDiff.diff("a\nb\nc", "a\nB\nc")
        assertEquals(1, r.addCount)
        assertEquals(1, r.delCount)
        val types = r.rows.map { it.type }
        assertEquals(DiffType.CTX, types.first())
        assertEquals(DiffType.CTX, types.last())
        // 惯例：红删在绿增之上。
        // 注意：先证两边都在。`indexOf` 找不到给 -1，`-1 < 任何下标` 恒真，DEL 整个不见时会静默变绿。
        val delAt = types.indexOf(DiffType.DEL)
        val addAt = types.indexOf(DiffType.ADD)
        assertTrue("前提：得真有一条 DEL，否则下面是空真断言", delAt >= 0)
        assertTrue("前提：得真有一条 ADD，否则下面是空真断言", addAt >= 0)
        assertTrue("惯例：红删排在绿增之上", delAt < addAt)
    }

    @Test fun crlfAndTrailingNewlineNormalized() {
        assertEquals(0, LineDiff.diff("a\r\nb", "a\nb").let { it.addCount + it.delCount })
        val r = LineDiff.diff("a\n", "a")
        assertEquals(0, r.addCount + r.delCount)
        assertEquals(1, r.rows.size)
    }

    @Test fun perLineCharCap() {
        val long = "x".repeat(50)
        val r = LineDiff.diff("", long, maxCharsPerLine = 10)
        assertEquals(11, r.rows[0].text.length) // 10 + 省略号
        assertTrue(r.rows[0].text.endsWith("…"))
    }

    @Test fun maxLinesTruncationKeepsFullCounts() {
        val newStr = (1..10).joinToString("\n") { "line$it" }
        val r = LineDiff.diff("", newStr, maxLines = 5)
        assertTrue(r.truncated)
        assertEquals(5, r.rows.size)
        assertEquals(10, r.addCount) // 全量计数不受截断影响
    }

    @Test fun cellBudgetDegradesToFullDelAdd() {
        val a = (1..100).joinToString("\n") { "a$it" }
        val b = (1..100).joinToString("\n") { "b$it" }
        val r = LineDiff.diff(a, b, cellBudget = 10) // 100*100 > 10 → 整删+整增
        assertEquals(100, r.delCount)
        assertEquals(100, r.addCount)
    }
}

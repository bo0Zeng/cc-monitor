package com.ccmonitor.mobile.cache

import org.junit.Assert.assertEquals
import org.junit.Test

/** 续读先比那台文件的大小：变短就从头读；末页满了另起一页。 */
class PagingTest {
    private val anchor = Anchor("/p/s.jsonl", offset = 50, seq = 5, file = "f1")

    @Test
    fun `那台的文件比续点短了（被截过 · 换过）⇒ 从头读，不从已经不在的位置往后读`() {
        assertEquals(Resume.FromScratch, resumeOf(anchor, true, "/p/s.jsonl", "f1", end = 30, total = 3, maxGap = 60))
        assertEquals("行数没少、字节短了也算变短", Resume.FromScratch, resumeOf(anchor, true, "/p/s.jsonl", "f1", end = 30, total = 6, maxGap = 60))
    }

    @Test
    fun `没变短、缺得不多 ⇒ 从续点往下读；一样长 ⇒ 也从续点（读到的是空的）`() {
        assertEquals(Resume.Continue(50), resumeOf(anchor, true, "/p/s.jsonl", "f1", end = 80, total = 8, maxGap = 60))
        assertEquals(Resume.Continue(50), resumeOf(anchor, true, "/p/s.jsonl", "f1", end = 50, total = 5, maxGap = 60))
    }

    @Test
    fun `换了文件 · 没续点 · 手上一行都没有 · 缺得太多 ⇒ 从头读`() {
        assertEquals(Resume.FromScratch, resumeOf(anchor, true, "/p/other.jsonl", "f1", end = 80, total = 8, maxGap = 60))
        assertEquals("同一路径、换了一份", Resume.FromScratch, resumeOf(anchor, true, "/p/s.jsonl", "f2", end = 80, total = 8, maxGap = 60))
        assertEquals(Resume.FromScratch, resumeOf(null, true, "/p/s.jsonl", "f1", end = 80, total = 8, maxGap = 60))
        assertEquals(Resume.FromScratch, resumeOf(anchor, false, "/p/s.jsonl", "f1", end = 80, total = 8, maxGap = 60))
        assertEquals(Resume.FromScratch, resumeOf(anchor, true, "/p/s.jsonl", "f1", end = 900, total = 90, maxGap = 60))
    }

    private fun row(end: Long) = mapOf("end" to end, "record" to mapOf("id" to "r$end"))

    @Test
    fun `末页满了另起一页，新页从末页的终点起；那几格原样搬`() {
        val last = RawPage(0, 20, listOf(row(10), row(20)))
        val out = appendRows(last, listOf(row(30), row(40)), end = 40, cap = 3)
        assertEquals(listOf(RawPage(0, 30, listOf(row(10), row(20), row(30))), RawPage(30, 40, listOf(row(40)))), out)
    }

    @Test
    fun `没有末页 ⇒ 从 0 起一页；没带字节的格照接在末页上`() {
        assertEquals(listOf(RawPage(0, 10, listOf(row(10)))), appendRows(null, listOf(row(10)), end = 10))
        val noEnd = mapOf("end" to null, "record" to null)
        assertEquals(listOf(RawPage(0, 10, listOf(row(10), noEnd))), appendRows(RawPage(0, 10, listOf(row(10))), listOf(noEnd), end = null))
    }
}

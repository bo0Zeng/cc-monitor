package com.ccmonitor.mobile.cache

import org.junit.Assert.assertEquals
import org.junit.Test

/** 末页满了另起一页。 */
class PagingTest {
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

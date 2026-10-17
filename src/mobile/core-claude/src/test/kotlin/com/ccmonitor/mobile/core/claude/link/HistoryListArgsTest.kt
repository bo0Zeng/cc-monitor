package com.ccmonitor.mobile.core.claude.link

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** `history-list` 的入参：每个筛选只带自己那一格，没设的格不出现（那台照缺省）。 */
class HistoryListArgsTest {
    @Test
    fun eachFilterCarriesOnlyItsOwnCell() {
        assertNull("什么都没设 ⇒ 不带入参", HistoryList.args(ListAsk()))
        assertEquals(mapOf("query" to "登录"), HistoryList.args(ListAsk(query = "登录")))
        assertEquals(mapOf("within_days" to 7), HistoryList.args(ListAsk(withinDays = 7)))
        assertEquals(mapOf("sort" to "created"), HistoryList.args(ListAsk(sort = ListSort.CREATED)))
        assertEquals(mapOf("hidden" to true), HistoryList.args(ListAsk(hidden = true)))
        assertEquals(
            mapOf("query" to "q", "within_days" to 30, "sort" to "activity", "hidden" to true),
            HistoryList.args(ListAsk("q", 30, ListSort.ACTIVITY, true)),
        )
    }
}

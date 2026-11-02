package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.LOCAL
import org.junit.Assert.assertEquals
import org.junit.Test

/** `history-list` 的入参：`origin` 总带（那台眼里的这台）；每个筛选只带自己那一格，没设的格不出现（那台照缺省）。 */
class HistoryListArgsTest {
    @Test
    fun eachFilterCarriesOnlyItsOwnCell() {
        val here = "origin" to LOCAL
        assertEquals("什么都没设 ⇒ 只带 origin", mapOf(here), HistoryList.args(ListAsk()))
        assertEquals(mapOf(here, "query" to "登录"), HistoryList.args(ListAsk(query = "登录")))
        assertEquals(mapOf(here, "within_days" to 7), HistoryList.args(ListAsk(withinDays = 7)))
        assertEquals(mapOf(here, "sort" to "created"), HistoryList.args(ListAsk(sort = ListSort.CREATED)))
        assertEquals(mapOf(here, "hidden" to true), HistoryList.args(ListAsk(hidden = true)))
        assertEquals(
            mapOf(here, "query" to "q", "within_days" to 30, "sort" to "activity", "hidden" to true),
            HistoryList.args(ListAsk("q", 30, ListSort.ACTIVITY, true)),
        )
    }
}

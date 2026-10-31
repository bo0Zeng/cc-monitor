package com.ccmonitor.mobile.ui.chats

import androidx.compose.runtime.saveable.SaverScope
import com.ccmonitor.mobile.core.claude.link.ListSort
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** 会话页的筛选：进程被收之前存下，新起的看板照它筛。 */
class ChatsLookTest {
    private val bundle = SaverScope { it is String || it is Boolean || it is Int || it is List<*> }

    private fun reborn(f: Filters): Filters {
        val saved = with(CHATS_FILTERS_SAVER) { bundle.save(f) }!!
        assertTrue("存下的进不了 Bundle：$saved", (saved as List<*>).all { it == null || bundle.canBeSaved(it) })
        return CHATS_FILTERS_SAVER.restore(saved)!!
    }

    @Test
    fun `筛选五样存下再取回一样`() {
        val f = Filters(view = ChatsView.DIR, unchecked = setOf("h2", "h3"), withinDays = 7, sort = ListSort.CREATED, hidden = true)
        assertEquals(f, reborn(f))
        assertEquals(Filters(), reborn(Filters()))
    }
}

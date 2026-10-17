package com.ccmonitor.mobile.ui.chats

import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.core.claude.link.wire.CanDelete
import com.ccmonitor.mobile.core.claude.link.wire.SessionDot
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.copy.copyText
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 长按一行那一列：项照那一行的格选字；能不能删照那台判的 `can.delete`；删除确认的正文照那台说的写。 */
class RowMenuTest {
    private fun row(
        hidden: String? = null,
        starred: Boolean = false,
        can: CanDelete = CanDelete.YES,
    ) = ChatRow("h1", "s1", "修登录", 0, null, null, null, Toned("空闲", Tone.PLAIN), SessionDot.IDLE, hidden = hidden, starred = starred, canDelete = can)

    private val noop = MenuOps({}, {}, {}, {}, {})

    @Test
    fun `五项照次序；删除在最后、红`() {
        val items = rowMenu(row(), noop)
        assertEquals(
            listOf("history.menu.star", "history.menu.rename", "history.menu.hide", "history.menu.newInDir", "history.menu.delete").map { copyText(it) },
            items.map { it.text },
        )
        assertTrue(items.last().danger)
        assertTrue(items.all { it.enabled })
    }

    @Test
    fun `标了星 ⇒ 取消标星；隐藏着 ⇒ 取消隐藏、下面一行是那台说的为什么`() {
        val items = rowMenu(row(hidden = "照规则隐藏", starred = true), noop)
        assertEquals(copyText("history.menu.unstar"), items[0].text)
        assertEquals(copyText("history.menu.unhide"), items[2].text)
        assertEquals("照规则隐藏", items[2].hint)
        assertNull(rowMenu(row(), noop)[2].hint)
    }

    @Test
    fun `运行中 ⇒ 删除灰着、下面写先结束；说不准 ⇒ 照样能点`() {
        val live = rowMenu(row(can = CanDelete.LIVE), noop).last()
        assertFalse(live.enabled)
        assertEquals(copyText("history.delete.liveHint"), live.hint)
        assertTrue(rowMenu(row(can = CanDelete.UNSURE), noop).last().enabled)
    }

    @Test
    fun `删除确认的正文：哪台上的记录 · 不可恢复；说不准在不在跑 ⇒ 多一句`() {
        val remote = copyText("history.delete.bodyRemote", "machine" to "gpu")
        assertEquals(listOf(remote), deleteBody("gpu", CanDelete.YES))
        assertEquals(listOf(remote, copyText("history.delete.unsure")), deleteBody("gpu", CanDelete.UNSURE))
    }
}

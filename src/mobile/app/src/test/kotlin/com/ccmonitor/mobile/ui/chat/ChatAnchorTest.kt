package com.ccmonitor.mobile.ui.chat

import androidx.compose.runtime.saveable.SaverScope
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 聊天页滚到哪：进程被收之前存下视口顶上那一行，回来那一页里有它就滚回去。 */
class ChatAnchorTest {
    private val bundle = SaverScope { it is String || it is Int || it is List<*> }

    private fun reborn(a: ChatAnchor?): ChatAnchor? {
        val saved = with(CHAT_ANCHOR_SAVER) { bundle.save(a) } ?: return null
        assertTrue("存下的进不了 Bundle：$saved", (saved as List<*>).all { it != null && bundle.canBeSaved(it) })
        return CHAT_ANCHOR_SAVER.restore(saved)
    }

    @Test
    fun `往上翻着被收：回来那一页里有那一行 ⇒ 滚回它、偏移照旧`() {
        val kept = reborn(ChatAnchor("u-7#0", 120))!!
        assertEquals(ChatScroll.To(2, 120), kept.backIn(listOf("u-5#0", "u-6#0", "u-7#0", "u-8#0")))
    }

    @Test
    fun `那一行不在回来那一页里（在更早的页）⇒ 照贴着底`() {
        val kept = reborn(ChatAnchor("u-1#0", 0))!!
        assertEquals(ChatScroll.Bottom, kept.backIn(listOf("u-5#0", "u-6#0")))
    }

    @Test
    fun `回来那一页还没到 ⇒ 先等`() {
        assertNull(ChatAnchor("u-1#0", 0).backIn(emptyList()))
    }

    @Test
    fun `贴着底时不存锚点`() {
        assertNull(reborn(null))
    }
}

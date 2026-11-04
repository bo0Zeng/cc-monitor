package com.ccmonitor.mobile.ui.chat

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** 「↓ 新内容」那一颗：往上翻着时流尾长了才出，回到底收起；往上翻载进更早的不算新内容。 */
class ChatNewContentTest {
    private val tail = ChatNewContent.Tail(key = "u-9#0", length = 40, live = 0)

    @Test
    fun `往上翻着 · 尾那一张换了或长了 · 活卡长了 ⇒ 出`() {
        assertTrue(ChatNewContent.shown(false, tail, tail.copy(key = "u-10#0"), following = false))
        assertTrue(ChatNewContent.shown(false, tail, tail.copy(length = 41), following = false))
        assertTrue(ChatNewContent.shown(false, tail, tail.copy(live = 7), following = false))
    }

    @Test
    fun `出了之后尾不再变 ⇒ 照亮着，直到回到底`() {
        assertTrue(ChatNewContent.shown(true, tail, tail, following = false))
        assertFalse("贴着底 ⇒ 收起", ChatNewContent.shown(true, tail, tail, following = true))
    }

    @Test
    fun `贴着底时尾长了 ⇒ 不出（跟到底那一格自己滚）`() {
        assertFalse(ChatNewContent.shown(false, tail, tail.copy(length = 99), following = true))
    }

    @Test
    fun `尾没变（只在头上插了更早的）· 还没见过尾 ⇒ 不出`() {
        assertFalse(ChatNewContent.shown(false, tail, tail, following = false))
        assertFalse(ChatNewContent.shown(false, null, tail, following = false))
    }
}

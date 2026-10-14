package com.ccmonitor.mobile.core.claude.link

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 活卡：照后端 `live_card` 帧换卡、长卡、撤卡（语义同桌面 `live-card.ts` 的 `LiveBoard`）。 */
class LiveTest {
    private fun frame(
        card: Long,
        n: Long,
        op: Map<String, Any?>,
        sid: String = "s1",
        run: String? = null,
    ): Map<String, Any?> = mapOf("kind" to "live_card", "sid" to sid, "card" to card.toDouble(), "n" to n.toDouble()) + (run?.let { mapOf("run" to it) } ?: emptyMap()) + op

    private fun open(
        card: Long,
        text: String = "",
        sid: String = "s1",
        run: String? = null,
        n: Long = 0,
    ) = frame(
        card,
        n,
        mapOf("op" to "open", "rid" to "r$card", "phase" to "streaming", "clipped" to false, "blocks" to listOf(mapOf("kind" to "text", "text" to text))),
        sid,
        run,
    )

    private fun text(
        card: Long,
        n: Long,
        s: String,
        i: Long = 0,
    ) = frame(card, n, mapOf("op" to "text", "i" to i.toDouble(), "s" to s))

    private fun LiveBoard.feed(vararg fs: Map<String, Any?>) = fs.map { apply(LiveFrame.of(it)!!) }

    @Test
    fun `帧解得出五种 op；不是活卡 · 缺格 · 认不出的 op ⇒ null`() {
        assertEquals(LiveFrame.Op.Awaiting, LiveFrame.of(frame(1, 1, mapOf("op" to "awaiting")))?.op)
        assertEquals(LiveFrame.Op.Close, LiveFrame.of(frame(1, 1, mapOf("op" to "close", "why" to "record")))?.op)
        assertEquals(LiveFrame.Op.Text(0, "字"), LiveFrame.of(text(1, 1, "字"))?.op)
        assertEquals(LiveFrame.Op.Block(2, "tool", "Bash"), LiveFrame.of(frame(1, 1, mapOf("op" to "block", "i" to 2.0, "block" to "tool", "tool" to "Bash")))?.op)
        val o = LiveFrame.of(open(7, "你好", run = "a1"))!!
        assertEquals("a1", o.run)
        assertEquals(LiveFrame.Op.Open("r7", awaiting = false, clipped = false, blocks = listOf(LiveBlock("text", "你好", null))), o.op)
        assertNull(LiveFrame.of(mapOf("kind" to "line", "sid" to "s1")))
        assertNull("缺 n", LiveFrame.of(mapOf("kind" to "live_card", "sid" to "s1", "card" to 1.0, "op" to "awaiting")))
        assertNull("认不出的 op", LiveFrame.of(frame(1, 1, mapOf("op" to "grow"))))
        assertNull("认不出的相", LiveFrame.of(frame(1, 0, mapOf("op" to "open", "rid" to "r", "phase" to "x", "clipped" to false, "blocks" to emptyList<Any>()))))
    }

    @Test
    fun `开 → 逐字长 → 说完等记录 → 记录到了撤`() {
        val b = LiveBoard()
        assertEquals(listOf(true, true, true), b.feed(open(1, "改"), text(1, 1, "好"), text(1, 2, "了")))
        assertEquals("改好了", b.cardsOf("s1").single().blocks.single().text)
        assertFalse(b.cardsOf("s1").single().awaiting)
        b.feed(frame(1, 3, mapOf("op" to "awaiting")))
        assertTrue(b.cardsOf("s1").single().awaiting)
        b.feed(frame(1, 4, mapOf("op" to "close", "why" to "record")))
        assertEquals(emptyList<LiveCardState>(), b.cardsOf("s1"))
    }

    @Test
    fun `n 不接上一帧 ⇒ 这张先不画；之后来一张整卡照它整张换`() {
        val b = LiveBoard()
        b.feed(open(1, "a"), text(1, 2, "跳号"))
        assertEquals(0, b.size)
        assertFalse("没开过的卡来的长帧不认", b.apply(LiveFrame.of(text(1, 3, "x"))!!))
        b.feed(open(1, "整张", n = 4))
        assertEquals("整张", b.cardsOf("s1").single().blocks.single().text)
        b.feed(text(1, 5, "接上"))
        assertEquals("整张接上", b.cardsOf("s1").single().blocks.single().text)
    }

    @Test
    fun `加块补齐块号；工具块记工具名；先来的卡在前；子运行的卡不进主运行`() {
        val b = LiveBoard()
        b.feed(open(2, "前"), open(1, "后"), open(3, "子", run = "a1"))
        b.feed(frame(2, 1, mapOf("op" to "block", "i" to 2.0, "block" to "tool", "tool" to "Bash")))
        val cards = b.cardsOf("s1")
        assertEquals(listOf(2L, 1L), cards.map { it.card })
        assertEquals(listOf("text", "other", "tool"), cards.first().blocks.map { it.kind })
        assertEquals("Bash", cards.first().blocks[2].tool)
        assertEquals(listOf(3L), b.cardsOf("s1", run = "a1").map { it.card })
    }

    @Test
    fun `换了一条流 ⇒ 全撤；只撤一个会话的不碰别的`() {
        val b = LiveBoard()
        b.feed(open(1, sid = "s1"), open(2, sid = "s2"))
        assertTrue(b.drop("s1"))
        assertEquals(listOf(2L), b.cardsOf("s2").map { it.card })
        assertTrue(b.clear())
        assertFalse(b.clear())
        assertEquals(0, b.size)
    }
}

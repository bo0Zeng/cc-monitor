package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.wire.gen.BranchRun
import org.junit.Assert.assertEquals
import org.junit.Test

/** 主线外清单照后端的段摆：手机不分段、不数数，框头就是段的 `said`。 */
class BranchFoldsTest {
    private fun said(id: String) = RenderUnit.UserText(id, "t-$id", id)

    private fun cards(vararg ids: String) = ids.map { StreamItem.Card(said(it)) }

    private val two =
        BranchList(
            off = listOf("b", "c", "e"),
            runs = listOf(BranchRun(at = 0, n = 2, said = "已被 Esc 回退 · 消息 ×2"), BranchRun(at = 2, n = 1, said = "已被 Esc 回退 · 消息 ×1")),
        )

    @Test
    fun `两段 ⇒ 两个框，框头是那两句，收着时段里的卡不画`() {
        val out = BranchFolds.fold(cards("a", "b", "c", "d", "e", "f"), two, emptySet())
        assertEquals(listOf("a#u", "branch#b", "d#u", "branch#e", "f#u"), out.map(::tag))
        assertEquals(listOf("已被 Esc 回退 · 消息 ×2", "已被 Esc 回退 · 消息 ×1"), out.filterIsInstance<StreamItem.Branch>().map { it.said })
    }

    @Test
    fun `点开一段 ⇒ 段里的卡接在框头后面、标着那一段；另一段照收`() {
        val out = BranchFolds.fold(cards("a", "b", "c", "d", "e"), two, setOf("b"))
        assertEquals(listOf("a#u", "branch#b", "b#u", "c#u", "d#u", "branch#e"), out.map(::tag))
        assertEquals(listOf(null, "b", "b", null), out.filterIsInstance<StreamItem.Card>().map { it.branch })
        assertEquals(listOf(true, false), out.filterIsInstance<StreamItem.Branch>().map { it.open })
    }

    @Test
    fun `同一段中间夹着不在清单里的卡 ⇒ 断成两个框`() {
        val one = BranchList(listOf("b", "c"), listOf(BranchRun(at = 0, n = 2, said = "x")))
        val out = BranchFolds.fold(cards("b", "z", "c"), one, emptySet())
        assertEquals(listOf("branch#b", "z#u", "branch#c"), out.map(::tag))
        assertEquals("开合认段首条", listOf("b", "b"), out.filterIsInstance<StreamItem.Branch>().map { it.run })
    }

    @Test
    fun `冷读一页页接：照 from 截、接上，段照接`() {
        val p1 = BranchList.EMPTY.put(0, listOf("b", "c"), listOf(BranchRun(at = 0, n = 3, said = "x")))
        val p2 = p1.put(2, listOf("d"), emptyList())
        assertEquals(listOf("b", "c", "d"), p2.off)
        assertEquals(listOf(BranchRun(at = 0, n = 3, said = "x")), p2.runs)
    }

    @Test
    fun `流上一帧 at 落在第二段段首 ⇒ 第一段与前 at 条不动，其后换成这一帧的`() {
        val next = two.put(2, listOf("e", "g"), listOf(BranchRun(at = 2, n = 2, said = "已被 Esc 回退 · 消息 ×2")))
        assertEquals(listOf("b", "c", "e", "g"), next.off)
        assertEquals(listOf(two.runs[0], BranchRun(at = 2, n = 2, said = "已被 Esc 回退 · 消息 ×2")), next.runs)
    }

    @Test
    fun `清单空 ⇒ 流原样`() {
        val s = cards("a", "b")
        assertEquals(s, BranchFolds.fold(s, BranchList.EMPTY, emptySet()))
    }

    private fun tag(s: StreamItem): String =
        when (s) {
            is StreamItem.Card -> "${s.unit.key}#u"
            else -> s.key
        }
}

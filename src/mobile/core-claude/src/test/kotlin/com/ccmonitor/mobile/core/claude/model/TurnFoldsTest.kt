package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.wire.gen.FoldFail
import com.ccmonitor.mobile.core.wire.gen.FoldSegment
import com.ccmonitor.mobile.core.wire.gen.Tone
import com.ccmonitor.mobile.core.wire.gen.TurnPart
import com.ccmonitor.mobile.core.wire.gen.TurnPhase
import com.ccmonitor.mobile.core.wire.gen.TurnRow
import com.ccmonitor.mobile.core.wire.gen.TurnSpan
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * 过程照后端 `history-turns` 分段折：`after` 那一条后面插一行过程、管到 `until` 为止的单元收进去；手机不按卡型判、不自己划段。
 * 夹具只造结构（uuid · 段的两头），正文是占位字。
 */
class TurnFoldsTest {
    private fun said(u: String) = RenderUnit.UserText("$u#0", "q", u)

    private fun text(u: String) = RenderUnit.AssistantMarkdown("$u#0", "t", u)

    private fun tool(u: String) = RenderUnit.ToolCall("$u#0", "c-$u", "Read", emptyMap(), "r", false, false, u)

    private fun seg(
        after: String,
        until: String?,
        parts: List<TurnPart> = listOf(TurnPart("过程", Tone.PLAIN)),
        fail: FoldFail? = null,
        phase: TurnPhase = TurnPhase.IDLE,
    ) = FoldSegment(after, 0, parts, TurnSpan("", null, null), until, null, phase, fail)

    private fun turn(
        u: String,
        vararg folds: FoldSegment,
    ) = TurnRow(0, 0, u, "", "", "", "", "q", 0, 0, 0, 0, 0, 0, 0, emptyList(), "", true, TurnPhase.IDLE, folds.toList())

    private fun shape(items: List<StreamItem>) =
        items.map {
            when (it) {
                is StreamItem.Card -> it.unit.key
                is StreamItem.Process -> "proc:${it.fold.after}:${if (it.open) "open" else "shut"}"
            }
        }

    private val units = listOf(said("u1"), tool("a1"), tool("a2"), text("a3"), tool("a4"), text("a5"), said("u2"), tool("b1"))
    private val turns =
        listOf(
            turn("u1", seg("u1", "a3"), seg("a3", "a5")),
            turn("u2", seg("u2", null, phase = TurnPhase.RUNNING)),
        )

    @Test
    fun `两段之间的单元不画、只画一行过程；露着的照画；正在跑的尾巴那一段也折`() {
        assertEquals(
            listOf("u1#0", "proc:u1:shut", "a3#0", "proc:a3:shut", "a5#0", "u2#0", "proc:u2:shut"),
            shape(TurnFolds.fold(units, turns, emptySet())),
        )
    }

    @Test
    fun `点开的那一段（按 after 记）多一层：段里的卡接在过程行后面，别的段照旧收着`() {
        assertEquals(
            listOf("u1#0", "proc:u1:shut", "a3#0", "proc:a3:open", "a4#0", "a5#0", "u2#0", "proc:u2:shut"),
            shape(TurnFolds.fold(units, turns, setOf("a3"))),
        )
    }

    @Test
    fun `最上面那一段的 after 还没拉到：过程行画在已拉到的最上面`() {
        val tail = units.drop(1)
        assertEquals(
            listOf("proc:u1:shut", "a3#0", "proc:a3:shut", "a5#0", "u2#0", "proc:u2:shut"),
            shape(TurnFolds.fold(tail, turns, emptySet())),
        )
    }

    @Test
    fun `一张卡收在哪一段：段里的回那一段的 after，露着的 · 不在的回 null；点开画出来的卡带着它`() {
        assertEquals("a3", TurnFolds.owner(units, turns, "a4#0"))
        assertEquals("u1", TurnFolds.owner(units, turns, "a2#0"))
        assertEquals("u2", TurnFolds.owner(units, turns, "b1#0"))
        assertEquals(null, TurnFolds.owner(units, turns, "a3#0"))
        assertEquals(null, TurnFolds.owner(units, turns, "zz"))
        val open = TurnFolds.fold(units, turns, setOf("u1")).filterIsInstance<StreamItem.Card>()
        assertEquals(listOf(null, "u1", "u1", null, null, null), open.map { it.fold })
    }

    @Test
    fun `没有分段（还没问到 · 问不到）⇒ 照原样逐条画`() {
        assertEquals(units.map { it.key }, shape(TurnFolds.fold(units, emptyList(), emptySet())))
    }

    @Test
    fun `过程行两行：第一行 plain 那几截，第二行 now · need · fail 那几截`() {
        val parts =
            listOf(
                TurnPart("过程", Tone.PLAIN),
                TurnPart("工具 ×2", Tone.PLAIN),
                TurnPart("失败 ×1", Tone.FAIL),
                TurnPart("现在：Read a", Tone.NOW),
            )
        val (top, bottom) = TurnFolds.lines(seg("u1", null, parts))
        assertEquals(listOf("过程", "工具 ×2"), top.map { it.text })
        assertEquals(listOf("失败 ×1", "现在：Read a"), bottom.map { it.text })
    }
}

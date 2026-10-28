package com.ccmonitor.mobile.ui.rotation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** 手机会话顺序的拖动状态机（同桌面 `rot-drag.ts` 那张表）：每行高 40、行距 8；第 0 行在用 · 1–3 可挪 · 4 用不了。期望手写。 */
class RotDragTest {
    private val rows =
        listOf("team", "lab", "personal", "api", "work").mapIndexed { k, a ->
            DragRow(a, movable = k in 1..3, top = k * 48f, height = 40f)
        }

    @Test
    fun `长按拿起往下拖 30px：拖的那一行跟着走、下面那一行已让位；松手前不交`() {
        val d = OrderDrag { true }
        assertTrue(d.lift(rows, 2))
        d.move(30f)
        assertEquals(30f, d.offset(2))
        assertEquals("下面那一行往上让一格", -48f, d.offset(3))
        assertEquals(DragPhase.LIFTED, d.phase)
        assertEquals(DragEnd.Drop("personal", 3), d.release())
        assertEquals(DragPhase.WAITING, d.phase)
    }

    @Test
    fun `夹在可挪那一组里；拖到第一格 ⇒ 在用那一行让到第二格，松手交 to 0`() {
        val d = OrderDrag { true }
        d.lift(rows, 1)
        d.move(400f)
        assertEquals(3, d.slot)
        d.move(-48f)
        assertTrue(d.first)
        assertEquals(48f, d.offset(0))
        assertEquals(DragEnd.Drop("lab", 0), d.release())
    }

    @Test
    fun `不可挪 · 离线 · 等回包 ⇒ 拿不起；原位松开 ⇒ 退回；回包到了回静止`() {
        assertFalse(OrderDrag { true }.lift(rows, 4))
        assertFalse(OrderDrag { false }.lift(rows, 2))
        val d = OrderDrag { true }
        d.lift(rows, 2)
        d.move(2f)
        assertEquals(DragEnd.Back, d.release())
        d.lift(rows, 2)
        d.move(-60f)
        d.release()
        assertFalse("等回包时不收新的拖动", d.lift(rows, 1))
        d.settle()
        assertEquals(0f, d.offset(1))
        assertTrue(d.lift(rows, 1))
    }
}

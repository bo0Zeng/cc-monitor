package com.ccmonitor.mobile.core.claude.link

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class RotationTest {
    private fun reply(
        lineup: List<Any>,
        manual: Boolean,
    ) = mapOf(
        "now" to 1.0,
        "nowText" to "14:30",
        "title" to "s",
        "strategy" to null,
        "board" to mapOf("all" to emptyList<Any>(), "rows" to emptyList<Any>()),
        "start" to "work",
        "current" to "work",
        "lineup" to lineup,
        "manual" to manual,
        "off" to emptyList<String>(),
        "accounts" to emptyList<Any>(),
        "listed" to 0.0,
        "decision" to null,
        "log" to emptyList<Any>(),
        "hints" to emptyList<Any>(),
    )

    @Test
    fun `rotation-view：不轮换（strategy · decision 为 null、没有推演）照样解得开；缺必有的 lineup ⇒ 解不出`() {
        val none = reply(emptyList(), false)
        assertEquals("work", Rotation.ofView(none)?.start)
        assertNull(Rotation.ofView(none - "lineup"))
    }

    @Test
    fun `rotation-view 会话那一形：顺序一号一行，右格的字 · 能不能拖 · 手动排过都是那台给的`() {
        val line = mapOf("account" to "lab", "group" to "next", "state" to "下一个 · 约 16:50", "movable" to true, "why" to "余量多")
        val v = Rotation.ofView(reply(listOf(line), true))
        val got = v?.lineup?.single()
        assertEquals(listOf("下一个 · 约 16:50", "余量多"), listOf(got?.state, got?.why))
        assertEquals(listOf(true, true), listOf(got?.movable, v?.manual))
    }

    @Test
    fun `入参：会话那一形不带推几小时；设置页带 id 与推几小时；改一格带那份的版本；会话载入一份不带别的`() {
        assertEquals(mapOf("target" to mapOf("sid" to "s")), Rotation.view(RotWhere.Session("s"), 12))
        assertEquals(mapOf("target" to mapOf("strategy" to "a"), "hours" to 24L), Rotation.view(RotWhere.Strategy("a"), 24))
        assertEquals(mapOf("strategy" to "a", "edit" to RotEdit.add("x"), "ifRev" to "r"), Rotation.settings("a", RotEdit.add("x"), "r"))
        assertEquals(mapOf("sid" to "s", "strategy" to "a"), Rotation.use("s", "a"))
    }
}

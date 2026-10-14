package com.ccmonitor.mobile.core.claude.link

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class RotationTest {
    @Test
    fun `changed 帧：topic · key 照线上读；别的帧不是它`() {
        assertEquals(Changed("rotation", "s1"), Changed.of(mapOf("kind" to "changed", "topic" to "rotation", "key" to "s1")))
        assertEquals(Changed("rotation_strategies", null), Changed.of(mapOf("kind" to "changed", "topic" to "rotation_strategies")))
        assertNull(Changed.of(mapOf("kind" to "turn_end", "topic" to "rotation")))
    }

    @Test
    fun `rotation-view：不轮换（strategy · decision · timeline 为 null）照样解得开；缺必有的 baseRev ⇒ 解不出`() {
        val none =
            mapOf(
                "now" to 1.0,
                "nowText" to "14:30",
                "strategy" to null,
                "form" to emptyList<Any>(),
                "settings" to null,
                "dirty" to false,
                "start" to "work",
                "current" to "work",
                "accounts" to emptyList<Any>(),
                "listed" to 0.0,
                "decision" to null,
                "timeline" to null,
                "past" to null,
                "baseRev" to "",
                "log" to emptyList<Any>(),
                "hints" to emptyList<Any>(),
            )
        assertEquals("work", Rotation.ofView(none)?.start)
        assertNull(Rotation.ofView(none - "baseRev"))
    }

    @Test
    fun `入参：设置页的目标带手上的草稿与它照哪版改的；没草稿只带 id；存会话不带 settings`() {
        assertEquals(mapOf("target" to mapOf("strategy" to "a"), "hours" to 24), Rotation.view(RotWhere.Strategy("a"), 24))
        assertEquals(
            mapOf("target" to mapOf("strategy" to "a", "settings" to mapOf("x" to 1), "rev" to "r"), "hours" to 12),
            Rotation.view(RotWhere.Strategy("a"), 12, mapOf("x" to 1), "r"),
        )
        assertEquals(mapOf("target" to mapOf("sid" to "s"), "ifRev" to "r"), Rotation.save(RotWhere.Session("s"), "r", mapOf("x" to 1)))
        assertEquals("策略写的字：一句 · 分语言", listOf("a", "b", ""), listOf(Rotation.text("a"), Rotation.text(mapOf("zh" to "b", "en" to "B")), Rotation.text(null)))
    }
}

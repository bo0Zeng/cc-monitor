package com.ccmonitor.mobile.core.claude.link

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class RotationTest {
    @Test
    fun `changed 帧：topic · key 照线上读；别的帧不是它`() {
        assertEquals(Changed("rotation", "s1"), Changed.of(mapOf("kind" to "changed", "topic" to "rotation", "key" to "s1")))
        assertEquals(Changed("rotation_strategies", null), Changed.of(mapOf("kind" to "changed", "topic" to "rotation_strategies")))
        assertNull(Changed.of(mapOf("kind" to "session_status", "topic" to "rotation")))
    }

    @Test
    fun `rotation-view：不轮换（strategy · decision · timeline 为 null）照样解得开；缺必有的 lineup ⇒ 解不出`() {
        val none =
            mapOf(
                "now" to 1.0,
                "nowText" to "14:30",
                "strategy" to null,
                "board" to mapOf("all" to emptyList<Any>(), "rows" to emptyList<Any>()),
                "start" to "work",
                "current" to "work",
                "lineup" to emptyList<String>(),
                "off" to emptyList<String>(),
                "accounts" to emptyList<Any>(),
                "listed" to 0.0,
                "decision" to null,
                "timeline" to null,
                "past" to null,
                "log" to emptyList<Any>(),
                "hints" to emptyList<Any>(),
            )
        assertEquals("work", Rotation.ofView(none)?.start)
        assertNull(Rotation.ofView(none - "lineup"))
    }

    @Test
    fun `入参：设置页的目标只带 id；改一格带那份的版本；会话载入一份不带别的`() {
        assertEquals(mapOf("target" to mapOf("strategy" to "a"), "hours" to 24), Rotation.view(RotWhere.Strategy("a"), 24))
        assertEquals(mapOf("strategy" to "a", "edit" to RotEdit.add("x"), "ifRev" to "r"), Rotation.settings("a", RotEdit.add("x"), "r"))
        assertEquals(mapOf("sid" to "s", "strategy" to "a"), Rotation.use("s", "a"))
    }
}

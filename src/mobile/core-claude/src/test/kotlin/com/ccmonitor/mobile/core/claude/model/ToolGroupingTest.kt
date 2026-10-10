package com.ccmonitor.mobile.core.claude.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** 连续工具分组。 */
class ToolGroupingTest {
    private fun tc(
        key: String,
        name: String = "Bash",
        result: String? = "out",
    ) = RenderUnit.ToolCall(key, "id-$key", name, emptyMap(), result, false, result == null, key)

    private fun text(key: String) = RenderUnit.UserText(key, "hi", key)

    @Test fun consecutiveToolsGroupedIntoOne() {
        val out = groupTools(listOf(tc("a"), tc("b"), tc("c")))
        assertEquals(1, out.size)
        val g = out[0] as RenderUnit.ToolGroup
        assertEquals(3, g.calls.size)
        assertEquals("a", g.key) // 组 key = 首 call key（稳定）
    }

    @Test fun singleToolNotGrouped() {
        val out = groupTools(listOf(tc("a")))
        assertEquals(1, out.size)
        assertTrue(out[0] is RenderUnit.ToolCall)
    }

    @Test fun interactiveToolNeverGrouped() {
        // 交互工具单独、断 run。
        val out = groupTools(listOf(tc("a"), tc("ask", name = "AskUserQuestion"), tc("b")))
        assertEquals(3, out.size) // 每个都是 size-1 run → 都不入组
        assertTrue(out.all { it is RenderUnit.ToolCall })
    }

    @Test fun interactiveBreaksRun() {
        val out = groupTools(listOf(tc("a"), tc("b"), tc("plan", name = "ExitPlanMode"), tc("c"), tc("d")))
        assertEquals(3, out.size)
        assertEquals(2, (out[0] as RenderUnit.ToolGroup).calls.size)
        assertTrue(out[1] is RenderUnit.ToolCall) // 交互工具原样
        assertEquals(2, (out[2] as RenderUnit.ToolGroup).calls.size)
    }

    @Test fun nonToolBreaksRun() {
        val out = groupTools(listOf(tc("a"), tc("b"), text("u"), tc("c"), tc("d")))
        assertEquals(3, out.size)
        assertTrue(out[0] is RenderUnit.ToolGroup)
        assertTrue(out[1] is RenderUnit.UserText)
        assertTrue(out[2] is RenderUnit.ToolGroup)
    }

    @Test fun searchableTextConcatenatesCalls() {
        val g = groupTools(listOf(tc("a", result = "alpha"), tc("b", result = "beta")))[0]
        val s = g.searchableText()
        assertTrue(s.contains("alpha") && s.contains("beta"))
    }

    @Test fun growingRunKeepsFirstKeyStable() {
        // 单工具 → 第二个流入 → 变组，key 仍是首 call（流式 key 稳定）。
        val single = groupTools(listOf(tc("a")))
        assertTrue(single[0] is RenderUnit.ToolCall)
        val grown = groupTools(listOf(tc("a"), tc("b")))
        assertEquals("a", (grown[0] as RenderUnit.ToolGroup).key)
    }

    @Test fun purePassThroughForNoTools() {
        val units = listOf(text("u1"), text("u2"))
        assertEquals(units, groupTools(units))
    }
}

package com.ccmonitor.mobile.ui.agents

import com.ccmonitor.mobile.core.claude.link.EMPTY_BOOK
import com.ccmonitor.mobile.core.wire.gen.RunGroups
import com.ccmonitor.mobile.core.wire.gen.RunInfo
import com.ccmonitor.mobile.core.wire.gen.RunState
import com.ccmonitor.mobile.core.wire.gen.SessionRuns
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * ⋯ 里「agent」那一项与 agent 列表：字照那台写好的 `summary` 抄、没有运行表不出这一项；行序照 `groups` 三组的先后，不按 `state` 自己分。
 */
class AgentsListTest {
    private fun info(
        run: String,
        state: RunState,
    ) = RunInfo(run = run, state = state, statusText = "状态·$run", labelText = "名·$run", lastText = "最近·$run")

    /** 运行表里的先后（a · b · c · d）故意跟三组的先后对不上：在跑的 c 排在最近结束的 a 前面，`groups` 里点到的 x 表里没有。 */
    private val runs =
        SessionRuns(
            sid = "s1",
            runs = listOf(info("a", RunState.DONE), info("b", RunState.FAILED), info("c", RunState.RUNNING), info("d", RunState.DONE)),
            ended = emptyList(),
            groups = RunGroups(running = listOf("c"), recent = listOf("a", "x"), older = listOf("d", "b")),
            summary = "成品写的子 agent 那一句",
        )

    @Test
    fun `会话有运行表 ⇒ ⋯ 里那一项的字是那台写好的 summary；没有运行表 ⇒ 不出这一项`() {
        val book = EMPTY_BOOK.copy(runs = mapOf("s1" to runs))
        assertEquals("成品写的子 agent 那一句", agentsEntry(book, "s1"))
        assertNull(agentsEntry(book, "s2"))
        assertNull(agentsEntry(EMPTY_BOOK, "s1"))
    }

    @Test
    fun `列表照 groups 的先后排：在跑 · 最近结束 · 更早，各带组头；点到表里没有的不出行`() {
        val items = agentsList(runs)
        assertEquals(
            listOf("head:RUNNING", "row:c", "head:RECENT", "row:a", "head:OLDER", "row:d", "row:b"),
            items.map {
                when (it) {
                    is AgentsItem.Head -> "head:${it.group.name}"
                    is AgentsItem.Row -> "row:${it.info.run}"
                }
            },
        )
        val c = (items[1] as AgentsItem.Row).info
        assertEquals(listOf("名·c", "状态·c", "最近·c"), listOf(c.labelText, c.statusText, c.lastText))
    }

    @Test
    fun `空的那一组不出组头`() {
        val only = runs.copy(groups = RunGroups(running = emptyList(), recent = listOf("a"), older = emptyList()))
        assertEquals(2, agentsList(only).size)
        assertEquals(RunGroup.RECENT, (agentsList(only).first() as AgentsItem.Head).group)
    }
}

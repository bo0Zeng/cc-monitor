package com.ccmonitor.mobile.ui.overview

import com.ccmonitor.mobile.core.claude.link.HistoryList
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.LiveSession
import com.ccmonitor.mobile.core.claude.link.Needs
import com.ccmonitor.mobile.core.claude.link.SessionTable
import com.ccmonitor.mobile.core.claude.link.SessionsNeeds
import com.ccmonitor.mobile.core.claude.link.Tone
import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.link.Problem
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 「对话」这一屏只挑格、按核心写好的段头归段：不判天、不重排、不自己写状态字。 */
class ConversationsViewTest {
    private fun row(
        sid: String,
        section: String?,
    ) = HistoryList.Row(sid, "标题 $sid", "09:0$sid", section, "/w", null, "claude", null)

    private val list =
        Fetched.Got(
            HistoryList.Answer(
                rows = listOf(row("1", "今天"), row("2", "今天"), row("3", "昨天"), row("4", "今天")),
                notice = null,
                truncated = true,
            ),
        )

    @Test
    fun `段头照核心写的、按先后连着归段，不重排`() {
        val v = view(LinkState.Up, SessionTable(), list, Fetched.Got(emptyList()), "m")
        assertEquals(listOf("今天", "昨天", "今天"), v.sections.map { it.title })
        assertEquals(listOf("1", "2", "3", "4"), v.sections.flatMap { it.rows }.map { it.sessionId })
        assertTrue(v.truncated)
        assertEquals(listOf("1", "2", "3", "4"), v.recent.map { it.sessionId })
    }

    @Test
    fun `状态字与语气照抄会话帧；不在流上的不画`() {
        val table = SessionTable(sessions = mapOf("2" to LiveSession(sid = "2", activity = Toned("后台任务运行中", Tone.BUSY))))
        val v = view(LinkState.Up, table, list, Fetched.Got(emptyList()), "m")
        val rows = v.sections.flatMap { it.rows }.associateBy { it.sessionId }
        assertEquals(Toned("后台任务运行中", Tone.BUSY), rows.getValue("2").status)
        assertNull(rows.getValue("1").status)
    }

    @Test
    fun `需手动照核心的先后、字照抄；清单里没有的不拿 sid 顶标题`() {
        val needs =
            listOf(
                SessionsNeeds.Row("3", Needs("approve", "等批准 · Bash", Tone.NEED, 0, "3m")),
                SessionsNeeds.Row("9", Needs("answer", "等回答", Tone.NEED, 1, null)),
            )
        val v = view(LinkState.Up, SessionTable(), list, Fetched.Got(needs), "m")
        assertEquals(listOf("3", "9"), v.needs.map { it.sessionId })
        assertEquals("标题 3", v.needs[0].label)
        assertEquals("等批准 · Bash", v.needs[0].text)
        assertNull(v.needs[1].label)
    }

    @Test
    fun `流走不通只说那一句；问失败的那一样单独说`() {
        val p = Problem("那一句", "详情")
        val down = view(LinkState.Down(null, "boom"), SessionTable(), list, Fetched.Got(emptyList()), "m")
        assertTrue(down.linkProblem != null && down.sections.isEmpty())
        val failed = view(LinkState.Up, SessionTable(), Fetched.Failed(p), Fetched.Got(emptyList()), "m")
        assertEquals(p, failed.listProblem)
        assertEquals(false, failed.listLoading)
    }
}

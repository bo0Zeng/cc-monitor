package com.ccmonitor.mobile.ui.overview

import com.ccmonitor.mobile.core.claude.link.Deploying
import com.ccmonitor.mobile.core.claude.link.Fix
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.LiveSession
import com.ccmonitor.mobile.core.claude.link.Said
import com.ccmonitor.mobile.core.claude.link.SessionTable
import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.core.claude.link.wire.HistoryListPage
import com.ccmonitor.mobile.core.claude.link.wire.HistoryRow
import com.ccmonitor.mobile.core.claude.link.wire.Needs
import com.ccmonitor.mobile.core.claude.link.wire.NeedsRow
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.copy.DurationFormat
import com.ccmonitor.mobile.link.Problem
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 「对话」这一屏只挑格、按核心写好的段头归段：不判天、不重排、不自己写状态字。 */
class ConversationsViewTest {
    private fun row(
        sid: String,
        section: String,
    ) = HistoryRow(sid, "标题 $sid", "09:0$sid", section, "/w", "/w/$sid.jsonl", "claude", null)

    private val list =
        Fetched.Got(
            HistoryListPage(
                rows = listOf(row("1", "今天"), row("2", "今天"), row("3", "昨天"), row("4", "今天")),
                notice = null,
                truncated = true,
            ),
        )

    @Test
    fun `装一次会好的那一种照核心点名带上（断着 · 通着旧了）；装到哪一步原样带上，连着的那一会儿也不丢`() {
        val step = Deploying.Step("安装 cc-monitor → box", bar = true)
        val down = view(LinkState.Down(Said("远端 cc-monitor 未装", ""), needsHand = true, fix = Fix.INSTALL), SessionTable(), list, Fetched.Loading, deploying = step)
        assertEquals(Fix.INSTALL, down.fix)
        assertEquals(step, down.deploying)
        val old = view(LinkState.Up(Said("box 后端要更新", ""), fix = Fix.UPDATE), SessionTable(), list, Fetched.Got(emptyList()))
        assertEquals(Fix.UPDATE, old.fix)
        assertEquals(Problem("box 后端要更新", ""), old.linkNotice)
        assertEquals(step, view(LinkState.Connecting, SessionTable(), list, Fetched.Loading, deploying = step).deploying)
        assertNull(view(LinkState.Up(), SessionTable(), list, Fetched.Got(emptyList())).fix)
    }

    @Test
    fun `段头照核心写的、按先后连着归段，不重排`() {
        val v = view(LinkState.Up(), SessionTable(), list, Fetched.Got(emptyList()))
        assertEquals(listOf("今天", "昨天", "今天"), v.sections.map { it.title })
        assertEquals(listOf("1", "2", "3", "4"), v.sections.flatMap { it.rows }.map { it.sessionId })
        assertTrue(v.truncated)
        assertEquals(listOf("1", "2", "3", "4"), v.recent.map { it.sessionId })
    }

    @Test
    fun `状态字与语气照抄会话帧；不在流上的不画`() {
        val table = SessionTable(sessions = mapOf("2" to LiveSession(sid = "2", activity = Toned("后台任务运行中", Tone.BUSY))))
        val v = view(LinkState.Up(), table, list, Fetched.Got(emptyList()))
        val rows = v.sections.flatMap { it.rows }.associateBy { it.sessionId }
        assertEquals(Toned("后台任务运行中", Tone.BUSY), rows.getValue("2").status)
        assertNull(rows.getValue("1").status)
    }

    @Test
    fun `需手动照核心的先后、字照抄；清单里没有的不拿 sid 顶标题`() {
        val needs =
            listOf(
                NeedsRow("3", Needs("approve", "等批准 · Bash", Tone.NEED, 0, 180_000L)),
                NeedsRow("9", Needs("answer", "等回答", Tone.NEED, 1, null)),
            )
        val v = view(LinkState.Up(), SessionTable(), list, Fetched.Got(needs), needsAt = 1_000)
        assertEquals(listOf("3", "9"), v.needs.map { it.sessionId })
        assertEquals("标题 3", v.needs[0].label)
        assertEquals("等批准 · Bash", v.needs[0].text)
        assertNull(v.needs[1].label)
        assertEquals("已等在屏上接着走：收到时 3m、过了一分钟是 4m（不重问那台）", DurationFormat.short(240_000), v.needs[0].waitedAt(61_000))
        assertNull("没有起点 ⇒ 不画时长", v.needs[1].waitedAt(61_000))
    }

    @Test
    fun `流走不通只说核心那一句；问失败的那一样单独说`() {
        val p = Problem("那一句", "详情")
        val down = view(LinkState.Down(Said("m 离线", "boom"), needsHand = false), SessionTable(), list, Fetched.Got(emptyList()))
        assertEquals(Problem("m 离线", "boom"), down.linkProblem)
        assertTrue(down.sections.isEmpty())
        val failed = view(LinkState.Up(), SessionTable(), Fetched.Failed(p), Fetched.Got(emptyList()))
        assertEquals(p, failed.listProblem)
        assertEquals(false, failed.listLoading)
    }

    @Test
    fun `通着但那台后端旧了：照列清单，最前多一句需手动（核心写好）`() {
        val v = view(LinkState.Up(Said("m 后端要更新", "theirs: p9")), SessionTable(), list, Fetched.Got(emptyList()))
        assertEquals(Problem("m 后端要更新", "theirs: p9"), v.linkNotice)
        assertNull(v.linkProblem)
        assertNull("不旧 ⇒ 不说", view(LinkState.Up(), SessionTable(), list, Fetched.Got(emptyList())).linkNotice)
    }
}

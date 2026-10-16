package com.ccmonitor.mobile.ui.overview

import com.ccmonitor.mobile.core.claude.link.Deploying
import com.ccmonitor.mobile.core.claude.link.Fix
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.LiveSession
import com.ccmonitor.mobile.core.claude.link.Said
import com.ccmonitor.mobile.core.claude.link.SessionDot
import com.ccmonitor.mobile.core.claude.link.SessionFate
import com.ccmonitor.mobile.core.claude.link.SessionTable
import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.core.claude.link.wire.Hidden
import com.ccmonitor.mobile.core.claude.link.wire.HistoryListPage
import com.ccmonitor.mobile.core.claude.link.wire.HistoryRow
import com.ccmonitor.mobile.core.claude.link.wire.Needs
import com.ccmonitor.mobile.core.claude.link.wire.NeedsRow
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.copy.DurationFormat
import com.ccmonitor.mobile.core.copy.copyText
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
        hidden: Hidden? = null,
    ) = HistoryRow(sid, "标题 $sid", "09:0$sid", section, "/w", "/w/$sid.jsonl", "claude", null, "live", hidden)

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
        val down = view("m", LinkState.Down(Said("远端 cc-monitor 未装", ""), needsHand = true, fix = Fix.INSTALL), SessionTable(), list, Fetched.Loading, deploying = step)
        assertEquals(Fix.INSTALL, down.fix)
        assertEquals(step, down.deploying)
        val old = view("m", LinkState.Up(Said("box 后端要更新", ""), fix = Fix.UPDATE), SessionTable(), list, Fetched.Got(emptyList()))
        assertEquals(Fix.UPDATE, old.fix)
        assertEquals(Problem("box 后端要更新", ""), old.linkNotice)
        assertEquals(step, view("m", LinkState.Connecting, SessionTable(), list, Fetched.Loading, deploying = step).deploying)
        assertNull(view("m", LinkState.Up(), SessionTable(), list, Fetched.Got(emptyList())).fix)
    }

    @Test
    fun `段头照核心写的、按先后连着归段，不重排`() {
        val v = view("m", LinkState.Up(), SessionTable(), list, Fetched.Got(emptyList()))
        assertEquals(listOf("今天", "昨天", "今天"), v.sections.map { it.title })
        assertEquals(listOf("1", "2", "3", "4"), v.sections.flatMap { it.rows }.map { it.sessionId })
        assertTrue(v.truncated)
        assertEquals(listOf("1", "2", "3", "4"), v.recent.map { it.sessionId })
    }

    @Test
    fun `状态字与语气照抄会话帧；不在流上的不画`() {
        val table = SessionTable(sessions = mapOf("2" to LiveSession(sid = "2", activity = Toned("后台任务运行中", Tone.BUSY))))
        val v = view("m", LinkState.Up(), table, list, Fetched.Got(emptyList()))
        val rows = v.sections.flatMap { it.rows }.associateBy { it.sessionId }
        assertEquals(Toned("后台任务运行中", Tone.BUSY), rows.getValue("2").status)
        assertNull(rows.getValue("1").status)
    }

    @Test
    fun `需手动照核心的先后、字照抄；清单里没有的不拿 sid 顶标题`() {
        val needs =
            listOf(
                NeedsRow("3", Needs("approve", null, "等批准 · Bash", "等批准 · Bash", null, Tone.NEED, 0, 180_000L, null, null)),
                NeedsRow("9", Needs("answer", null, "等回答", "等回答", null, Tone.NEED, 1, null, null, null)),
            )
        val v = view("m", LinkState.Up(), SessionTable(), list, Fetched.Got(needs), needsAt = 1_000)
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
        val down = view("m", LinkState.Down(Said("m 离线", "boom"), needsHand = false), SessionTable(), list, Fetched.Got(emptyList()))
        assertEquals(Problem("m 离线", "boom"), down.linkProblem)
        assertTrue(down.sections.isEmpty())
        val failed = view("m", LinkState.Up(), SessionTable(), Fetched.Failed(p), Fetched.Got(emptyList()))
        assertEquals(p, failed.listProblem)
        assertEquals(false, failed.listLoading)
    }

    @Test
    fun `通着但那台后端旧了：照列清单，最前多一句需手动（核心写好）`() {
        val v = view("m", LinkState.Up(Said("m 后端要更新", "theirs: p9")), SessionTable(), list, Fetched.Got(emptyList()))
        assertEquals(Problem("m 后端要更新", "theirs: p9"), v.linkNotice)
        assertNull(v.linkProblem)
        assertNull("不旧 ⇒ 不说", view("m", LinkState.Up(), SessionTable(), list, Fetched.Got(emptyList())).linkNotice)
    }

    @Test
    fun `每行一颗状态点：活的照语气、离开活的照去向、流上没有的照清单那一格；字跟着那一格`() {
        val rows =
            listOf("a" to "live", "b" to "ended", "c" to "ended", "d" to "unknown", "e" to "live", "f" to "别的词")
                .map { (sid, st) -> HistoryRow(sid, "标题 $sid", "09:00", "今天", "/w", "/w/$sid.jsonl", "claude", null, st, null) }
        val table =
            SessionTable(
                sessions = mapOf("a" to LiveSession(sid = "a", activity = Toned("需手动", Tone.NEED))),
                fates = mapOf("b" to SessionFate(SessionFate.State.RECONNECTABLE, "Claude 已退出")),
            )
        val v = view("m", LinkState.Up(), table, Fetched.Got(HistoryListPage(rows, null, false)), Fetched.Got(emptyList()))
        val by = v.sections.flatMap { it.rows }.associateBy { it.sessionId }
        assertEquals(SessionDot.NEEDS_YOU, by.getValue("a").dot)
        assertEquals(Toned("需手动", Tone.NEED), by.getValue("a").status)
        assertEquals(SessionDot.EXITED, by.getValue("b").dot)
        assertEquals("去向那一句照抄", "Claude 已退出", by.getValue("b").status?.text)
        assertEquals(SessionDot.ENDED, by.getValue("c").dot)
        assertEquals(copyText("sessionState.ended.name"), by.getValue("c").status?.text)
        assertEquals(SessionDot.UNKNOWN, by.getValue("d").dot)
        assertEquals(copyText("sessionState.unseen.name"), by.getValue("d").status?.text)
        assertEquals(SessionDot.RUNNING, by.getValue("e").dot)
        assertNull("还没报到 ⇒ 不写字", by.getValue("e").status)
        assertNull("认不出 ⇒ 不画", by.getValue("f").dot)
        assertNull(by.getValue("f").status)
    }

    @Test
    fun `需手动那几条的点照核心给的语气`() {
        val needs = listOf(NeedsRow("3", Needs("approve", null, "等批准 · Bash", "等批准 · Bash", null, Tone.NEED, 0, null, null, null)))
        val v = view("m", LinkState.Up(), SessionTable(), list, Fetched.Got(needs))
        assertEquals(SessionDot.NEEDS_YOU, v.needs.single().dot)
    }

    @Test
    fun `离线（流走不通 · 还在接）：照画本地缓存里上次那一份，离线那一句照说；连上问到了 ⇒ 换成那台这一份`() {
        val cached = HistoryListPage(listOf(row("1", "昨天"), row("2", "昨天")), null, false)
        val down = view("m", LinkState.Down(Said("m 离线", "boom"), needsHand = false), SessionTable(), Fetched.Loading, Fetched.Loading, cached = cached)
        assertEquals(Problem("m 离线", "boom"), down.linkProblem)
        assertEquals(listOf("1", "2"), down.recent.map { it.sessionId })
        val connecting = view("m", LinkState.Connecting, SessionTable(), Fetched.Loading, Fetched.Loading, cached = cached)
        assertEquals(listOf("1", "2"), connecting.recent.map { it.sessionId })
        val asking = view("m", LinkState.Up(), SessionTable(), Fetched.Loading, Fetched.Loading, cached = cached)
        assertEquals("在问 ⇒ 先画上次那一份", listOf("1", "2"), asking.recent.map { it.sessionId })
        val fresh = view("m", LinkState.Up(), SessionTable(), list, Fetched.Got(emptyList()), cached = cached)
        assertEquals("问到了 ⇒ 照那台这一份", listOf("1", "2", "3", "4"), fresh.recent.map { it.sessionId })
    }

    @Test
    fun `断着：每行的点画说不清、字写那台离线，不照断前那张会话表与缓存里的状态`() {
        val cached = HistoryListPage(listOf(row("1", "昨天")), null, false)
        val stale = SessionTable(sessions = mapOf("1" to LiveSession(sid = "1", activity = Toned("运行中", Tone.NOW))))
        val down = view("m", LinkState.Down(Said("m 离线", "boom"), needsHand = false), stale, Fetched.Loading, Fetched.Loading, cached = cached)
        val r = down.recent.single()
        assertEquals(SessionDot.UNKNOWN, r.dot)
        assertEquals(Toned(copyText("newSession.machine.offline", "machine" to "m"), Tone.PLAIN), r.status)
        val up = view("m", LinkState.Up(), stale, Fetched.Loading, Fetched.Loading, cached = cached)
        assertEquals("通着 ⇒ 照会话帧", SessionDot.RUNNING, up.recent.single().dot)
    }

    @Test
    fun `隐藏只照行上那一格：「显示已隐藏」关着不画带格的行（上次缓存里的也不画）；开着照画、带后端那一句；抽屉「最近」一律不列`() {
        val page = HistoryListPage(listOf(row("1", "今天"), row("2", "今天", Hidden("已隐藏")), row("3", "昨天")), null, false)
        val off = view("m", LinkState.Up(), SessionTable(), Fetched.Got(page), Fetched.Got(emptyList()))
        assertEquals(listOf("1", "3"), off.sections.flatMap { it.rows }.map { it.sessionId })
        assertEquals(false, off.showHidden)
        val cachedOff = view("m", LinkState.Connecting, SessionTable(), Fetched.Loading, Fetched.Loading, cached = page)
        assertEquals(listOf("1", "3"), cachedOff.sections.flatMap { it.rows }.map { it.sessionId })
        val on = view("m", LinkState.Up(), SessionTable(), Fetched.Got(page), Fetched.Got(emptyList()), showHidden = true)
        val rows = on.sections.flatMap { it.rows }
        assertEquals(listOf("1", "2", "3"), rows.map { it.sessionId })
        assertEquals(listOf(null, "已隐藏", null), rows.map { it.hidden })
        assertEquals(true, on.showHidden)
        assertEquals("抽屉「最近」不列隐藏的", listOf("1", "3"), on.recent.map { it.sessionId })
        val down = view("m", LinkState.Down(Said("m 离线", "boom"), needsHand = false), SessionTable(), Fetched.Loading, Fetched.Loading, cached = page, showHidden = true)
        assertEquals("离线照缓存画时也照开关", listOf("1", "2", "3"), down.sections.flatMap { it.rows }.map { it.sessionId })
    }
}

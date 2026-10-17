package com.ccmonitor.mobile.ui.overview

import com.ccmonitor.mobile.core.claude.link.Deploying
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.LiveSession
import com.ccmonitor.mobile.core.claude.link.Said
import com.ccmonitor.mobile.core.claude.link.SessionTable
import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.core.claude.link.wire.Hidden
import com.ccmonitor.mobile.core.claude.link.wire.HistoryListPage
import com.ccmonitor.mobile.core.claude.link.wire.HistoryRow
import com.ccmonitor.mobile.core.claude.link.wire.MachineFix
import com.ccmonitor.mobile.core.claude.link.wire.MachineFixButton
import com.ccmonitor.mobile.core.claude.link.wire.Needs
import com.ccmonitor.mobile.core.claude.link.wire.NeedsRow
import com.ccmonitor.mobile.core.claude.link.wire.SessionDot
import com.ccmonitor.mobile.core.claude.link.wire.SessionFace
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.copy.DurationFormat
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.ui.Faces
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
        face: SessionFace = Faces.of("live-idle"),
    ) = HistoryRow(sid, "标题 $sid", "09:0$sid", section, "/w", "/w/$sid.jsonl", "claude", null, face, hidden, null, null)

    private fun needs(
        sid: String,
        label: String,
        text: String,
        rank: Long,
        waitedMs: Long?,
    ) = NeedsRow(sid, label, Faces.of("live-needs-you-tmux"), Needs("approve", null, text, text, null, Tone.NEED, rank, waitedMs, null, null))

    private fun live(
        sid: String,
        face: String,
    ) = SessionTable(sessions = mapOf(sid to LiveSession(sid)), faces = mapOf(sid to Faces.of(face)))

    private val list =
        Fetched.Got(
            HistoryListPage(
                rows = listOf(row("1", "今天"), row("2", "今天"), row("3", "昨天"), row("4", "今天")),
                groups = emptyList(),
                notice = null,
                truncated = true,
            ),
        )

    @Test
    fun `装一次会好的那一颗照成品带上（断着 · 通着旧了），别的修法不挑；装到哪一步原样带上，连着的那一会儿也不丢`() {
        val step = Deploying.Step("安装 cc-monitor → box", bar = true)
        val install = MachineFixButton(MachineFix.INSTALL, copyText("machineState.fix.install"))
        val update = MachineFixButton(MachineFix.UPDATE, copyText("machineState.fix.update"))
        val down = view("m", LinkState.Down(Said("远端 cc-monitor 未装", ""), listOf(install)), SessionTable(), list, Fetched.Loading, deploying = step)
        assertEquals(install, down.fix)
        assertEquals(step, down.deploying)
        val retry = listOf(MachineFixButton(MachineFix.RETRY, copyText("machineState.fix.retry")), MachineFixButton(MachineFix.CONN_SETTINGS, copyText("machinePage.conn.toggle")))
        assertNull(view("m", LinkState.Down(Said("m 离线", ""), retry), SessionTable(), list, Fetched.Loading).fix)
        val old = view("m", LinkState.Up(Said("box 后端要更新", ""), listOf(update)), SessionTable(), list, Fetched.Got(emptyList()))
        assertEquals(update, old.fix)
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
    fun `状态字与语气照抄那台给的脸：流上有它照流上的，没有照清单那一行的`() {
        val v = view("m", LinkState.Up(), live("2", "live-background"), list, Fetched.Got(emptyList()))
        val rows = v.sections.flatMap { it.rows }.associateBy { it.sessionId }
        assertEquals(Toned("后台任务运行中", Tone.BUSY), rows.getValue("2").status)
        assertEquals(SessionDot.BACKGROUND, rows.getValue("2").dot)
        assertEquals("流上没有 ⇒ 清单那一行的脸", Toned("空闲", Tone.PLAIN), rows.getValue("1").status)
    }

    @Test
    fun `需手动照核心的先后、字与标题照抄那一行（清单里没有的也有标题）`() {
        val needs = listOf(needs("3", "那台写的 3", "等批准 · Bash", 0, 180_000L), needs("9", "那台写的 9", "等回答", 1, null))
        val v = view("m", LinkState.Up(), SessionTable(), list, Fetched.Got(needs), needsAt = 1_000)
        assertEquals(listOf("3", "9"), v.needs.map { it.sessionId })
        assertEquals("那台写的 3", v.needs[0].label)
        assertEquals("等批准 · Bash", v.needs[0].text)
        assertEquals("清单里没有它也照那一行的标题", "那台写的 9", v.needs[1].label)
        assertEquals("已等在屏上接着走：收到时 3m、过了一分钟是 4m（不重问那台）", DurationFormat.short(240_000), v.needs[0].waitedAt(61_000))
        assertNull("没有起点 ⇒ 不画时长", v.needs[1].waitedAt(61_000))
    }

    @Test
    fun `流走不通只说核心那一句；问失败的那一样单独说`() {
        val p = Problem("那一句", "详情")
        val down = view("m", LinkState.Down(Said("m 离线", "boom")), SessionTable(), list, Fetched.Got(emptyList()))
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
    fun `每行一颗状态点 ⑬：流上有它的脸 ⇒ 取流上的（离开活了是那一帧的），流上没有 ⇒ 清单那一行的；字跟着那一份`() {
        val rows = listOf(row("a", "今天", face = Faces.of("live-idle")), row("b", "今天", face = Faces.of("live-idle")), row("c", "今天", face = Faces.of("unclear")))
        val table =
            SessionTable(
                sessions = mapOf("a" to LiveSession("a")),
                faces = mapOf("a" to Faces.of("live-needs-you-tmux"), "b" to Faces.of("reconnectable")),
            )
        val v = view("m", LinkState.Up(), table, Fetched.Got(HistoryListPage(rows, emptyList(), null, false)), Fetched.Got(emptyList()))
        val by = v.sections.flatMap { it.rows }.associateBy { it.sessionId }
        assertEquals(SessionDot.NEEDS_YOU, by.getValue("a").dot)
        assertEquals(Toned("需手动", Tone.NEED), by.getValue("a").status)
        assertEquals(SessionDot.EXITED, by.getValue("b").dot)
        assertEquals("离开活那一帧的短名照抄", "Claude 已退出", by.getValue("b").status.text)
        assertEquals(SessionDot.UNKNOWN, by.getValue("c").dot)
        assertEquals("状态不明", by.getValue("c").status.text)
    }

    @Test
    fun `需手动那几条的点照那一行的脸`() {
        val v = view("m", LinkState.Up(), SessionTable(), list, Fetched.Got(listOf(needs("3", "t", "等批准 · Bash", 0, null))))
        assertEquals(SessionDot.NEEDS_YOU, v.needs.single().dot)
    }

    @Test
    fun `离线（流走不通 · 还在接）：照画本地缓存里上次那一份，离线那一句照说；连上问到了 ⇒ 换成那台这一份`() {
        val cached = HistoryListPage(listOf(row("1", "昨天"), row("2", "昨天")), emptyList(), null, false)
        val down = view("m", LinkState.Down(Said("m 离线", "boom")), SessionTable(), Fetched.Loading, Fetched.Loading, cached = cached)
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
    fun `断着：那台说看不见时照旧作数的（已结束 · 记录已不在）照画，其余点画说不清、字写那台离线，不照断前那张会话表`() {
        val cached = HistoryListPage(listOf(row("1", "昨天"), row("2", "昨天", face = Faces.of("ended")), row("3", "昨天", face = Faces.of("gone"))), emptyList(), null, false)
        val stale = live("1", "live-working-plain")
        val down = view("m", LinkState.Down(Said("m 离线", "boom")), stale, Fetched.Loading, Fetched.Loading, cached = cached)
        val by = down.recent.associateBy { it.sessionId }
        assertEquals(SessionDot.UNKNOWN, by.getValue("1").dot)
        assertEquals(Toned(copyText("newSession.machine.offline", "machine" to "m"), Tone.PLAIN), by.getValue("1").status)
        assertEquals(SessionDot.ENDED, by.getValue("2").dot)
        assertEquals("已结束", by.getValue("2").status.text)
        assertEquals(SessionDot.GONE, by.getValue("3").dot)
        val up = view("m", LinkState.Up(), stale, Fetched.Loading, Fetched.Loading, cached = cached)
        assertEquals("通着 ⇒ 照会话帧", SessionDot.RUNNING, up.recent.first().dot)
    }

    @Test
    fun `隐藏只照行上那一格：「显示已隐藏」关着不画带格的行（上次缓存里的也不画）；开着照画、带后端那一句；抽屉「最近」一律不列`() {
        val page = HistoryListPage(listOf(row("1", "今天"), row("2", "今天", Hidden("已隐藏")), row("3", "昨天")), emptyList(), null, false)
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
        val down = view("m", LinkState.Down(Said("m 离线", "boom")), SessionTable(), Fetched.Loading, Fetched.Loading, cached = page, showHidden = true)
        assertEquals("离线照缓存画时也照开关", listOf("1", "2", "3"), down.sections.flatMap { it.rows }.map { it.sessionId })
    }
}

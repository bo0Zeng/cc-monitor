package com.ccmonitor.mobile.ui.chats

import com.ccmonitor.mobile.core.claude.link.Busy
import com.ccmonitor.mobile.core.claude.link.Deploying
import com.ccmonitor.mobile.core.claude.link.EMPTY_BOOK
import com.ccmonitor.mobile.core.claude.link.HistoryListPage
import com.ccmonitor.mobile.core.claude.link.ListAsk
import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.Can
import com.ccmonitor.mobile.core.wire.gen.CanDelete
import com.ccmonitor.mobile.core.wire.gen.CanResume
import com.ccmonitor.mobile.core.wire.gen.FleetLines
import com.ccmonitor.mobile.core.wire.gen.HistoryGroup
import com.ccmonitor.mobile.core.wire.gen.HistoryRow
import com.ccmonitor.mobile.core.wire.gen.LOCAL
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.MachineFix
import com.ccmonitor.mobile.core.wire.gen.MachineFixButton
import com.ccmonitor.mobile.core.wire.gen.MachineStateKind
import com.ccmonitor.mobile.core.wire.gen.SessionBook
import com.ccmonitor.mobile.core.wire.gen.SessionDot
import com.ccmonitor.mobile.core.wire.gen.SessionFace
import com.ccmonitor.mobile.core.wire.gen.SessionLeft
import com.ccmonitor.mobile.core.wire.gen.Tone
import com.ccmonitor.mobile.link.Fetched
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.ui.Faces
import com.ccmonitor.mobile.ui.chat.leftSeat
import com.ccmonitor.mobile.ui.chat.linkConnecting
import com.ccmonitor.mobile.ui.chat.linkDown
import com.ccmonitor.mobile.ui.chat.linkUp
import com.ccmonitor.mobile.ui.chat.liveSeat
import com.ccmonitor.mobile.ui.common.HostLink
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 会话屏只挑格、按核心写好的段头归段、几台只按 `at` 并：不判天、不另排、不自己写状态字。 */
class ChatsViewTest {
    private fun row(
        sid: String,
        section: String,
        at: Long = 0,
        face: SessionFace = Faces.of("live-idle"),
        group: String = "/w",
        parent: String? = null,
    ) = HistoryRow(
        agent = "claude",
        sessionId = sid,
        projectPath = group,
        projectName = "w",
        aiTitle = null,
        firstUserExcerpt = "",
        title = "标题 $sid",
        startedAt = at,
        updatedAt = at,
        jsonlPath = "$group/$sid.jsonl",
        messageCountApprox = 0,
        isBg = false,
        starred = false,
        customTitle = null,
        hidden = null,
        projectDir = group,
        group = group,
        label = "标题 $sid",
        untitled = false,
        face = face,
        can = Can(CanResume.YES, accounts = false, fork = false, delete = CanDelete.YES),
        agentTag = null,
        at = at,
        atText = "09:0$sid",
        sectionText = section,
        spanText = "09:0$sid",
        forkedFromSessionId = parent,
        origin = LOCAL,
        reveal = false,
        deleteAsk = "",
    )

    private fun group(
        key: String,
        order: Long,
    ) = HistoryGroup(
        key = key,
        agent = "claude",
        projectName = key.substringAfterLast('/'),
        projectPath = key,
        projectDir = key,
        count = 1,
        hasLive = false,
        starred = false,
        lastActivity = 0,
        atText = "09:00",
        order = order,
        failed = null,
        origin = LOCAL,
    )

    private fun page(vararg rows: HistoryRow) = HistoryListPage(rows.toList(), emptyList(), null, false)

    private val list = Fetched.Got(page(row("1", "今天"), row("2", "今天"), row("3", "昨天"), row("4", "今天")).copy(truncated = true))

    private fun snap(
        link: LinkLine? = linkUp(),
        table: SessionBook = EMPTY_BOOK,
        got: Fetched<HistoryListPage> = list,
        cached: HistoryListPage? = null,
        deploying: Deploying? = null,
        ask: ListAsk = ListAsk(),
        id: String = "h1",
        machine: String = "m",
    ) = HostSnap(id, machine, link, table, got, cached, deploying, ask)

    private fun live(
        sid: String,
        face: String,
    ) = EMPTY_BOOK.copy(live = mapOf(sid to liveSeat(sid, Faces.of(face))))

    private fun view(
        vararg parts: HostPart,
        current: String = "h1",
        filters: Filters = Filters(),
        query: String? = null,
        face: FleetLines? = allListed,
        open: Set<Pair<String, String>> = emptySet(),
    ) = chatsView(parts.toList(), current, parts.map { HostName(it.hostId, it.machine) }, filters, query, face, open)

    /** 核心交的首屏那一句：各台都报完了的那一形。 */
    private val allListed = FleetLines(copyText("beEmpty.line.none"), copyText("beEmpty.hint.none"), "核心·计数")

    @Test
    fun `装一次会好的那一颗照成品带上（断着 · 通着旧了），别的修法不挑；正在装或更新照成品那一句带上，结局在条上没画时单独一行`() {
        val install = MachineFixButton(MachineFix.INSTALL, copyText("machineState.fix.install"))
        val update = MachineFixButton(MachineFix.UPDATE, copyText("front.act.update"))
        val down = hostPart(snap(linkDown("远端 cc-monitor 未装", fixes = listOf(install))))
        assertEquals(install, down.bar?.fix)
        assertEquals("装一次会好 ⇒ 红条（不是断线那一条）", false, down.bar?.offline)
        assertNull(down.bar?.busy)
        val retry = listOf(MachineFixButton(MachineFix.RETRY, copyText("front.act.retry")))
        assertNull(hostPart(snap(linkDown("m 离线", fixes = retry))).bar?.fix)
        val old = hostPart(snap(linkUp("box 后端要更新", fixes = listOf(update), state = MachineStateKind.NEEDS_UPDATE)))
        assertEquals(update, old.bar?.fix)
        assertEquals(Problem("box 后端要更新", ""), old.bar?.problem)
        val installing = Busy("安装 cc-monitor → m", bar = true)
        assertEquals(installing, hostPart(snap(linkDown(installing.text, state = MachineStateKind.INSTALLING))).bar?.busy)
        val updating = Busy("更新 m · cc-monitor", bar = true)
        assertEquals(updating, hostPart(snap(linkUp(updating.text, state = MachineStateKind.UPDATING))).bar?.busy)
        val done = Deploying.Done("已装上")
        val connecting = view(hostPart(snap(linkConnecting(), deploying = done)))
        assertEquals("连着的那一会儿条上没东西 ⇒ 结局单独一行", done, connecting.deploying.single().deploying)
        assertNull(hostPart(snap()).bar)
    }

    @Test
    fun `段头照核心写的、按先后连着归段，不重排`() {
        val v = view(hostPart(snap()))
        assertEquals(listOf("今天", "昨天", "今天"), v.sections.map { it.title })
        assertEquals(listOf("1", "2", "3", "4"), v.rows.map { it.sessionId })
        assertTrue(v.truncated)
    }

    @Test
    fun `状态字与语气照抄那台给的脸：流上有它照流上的，没有照清单那一行的`() {
        val rows = hostPart(snap(table = live("2", "live-background"))).rows.associateBy { it.sessionId }
        assertEquals(Toned("后台任务运行中", Tone.BUSY), rows.getValue("2").status)
        assertEquals(SessionDot.BACKGROUND, rows.getValue("2").dot)
        assertEquals("流上没有 ⇒ 清单那一行的脸", Toned("空闲", Tone.PLAIN), rows.getValue("1").status)
    }

    @Test
    fun `流走不通只说核心那一句；问失败的单独说；通着但那台后端旧了照列、多一句`() {
        val down = hostPart(snap(linkDown("m 离线", "boom")))
        assertEquals(Problem("m 离线", "boom"), down.bar?.problem)
        assertEquals(true, down.bar?.offline)
        assertTrue(down.rows.isEmpty())
        val p = Problem("那一句", "详情")
        val failed = hostPart(snap(got = Fetched.Failed(p)))
        assertEquals(p, failed.bar?.problem)
        assertEquals(false, failed.bar?.offline)
        assertEquals(false, failed.loading)
        val old = hostPart(snap(linkUp("m 后端要更新", "theirs: p9", state = MachineStateKind.NEEDS_UPDATE)))
        assertEquals(Problem("m 后端要更新", "theirs: p9"), old.bar?.problem)
        assertEquals(4, old.rows.size)
    }

    @Test
    fun `SSH 那层接不上：那一句当这台的条，不算在读`() {
        val p = hostPart(snap(linkConnecting(), got = Fetched.Loading), HostLink.Failed("认证失败"))
        assertEquals(Problem("认证失败", ""), p.bar?.problem)
        assertEquals(false, p.loading)
    }

    @Test
    fun `每行一颗状态点：流上有它的脸 ⇒ 取流上的（离开活了是那一帧的），流上没有 ⇒ 清单那一行的；字跟着那一份`() {
        val rows = listOf(row("a", "今天", face = Faces.of("live-idle")), row("b", "今天", face = Faces.of("live-idle")), row("c", "今天", face = Faces.of("unclear")))
        val table =
            EMPTY_BOOK.copy(
                live = mapOf("a" to liveSeat("a", Faces.of("live-needs-you-tmux"))),
                left = mapOf("b" to leftSeat("b", Faces.of("reconnectable"))),
            )
        val by = hostPart(snap(table = table, got = Fetched.Got(page(*rows.toTypedArray())))).rows.associateBy { it.sessionId }
        assertEquals(SessionDot.NEEDS_YOU, by.getValue("a").dot)
        assertEquals(Toned("需手动", Tone.NEED), by.getValue("a").status)
        assertEquals(SessionDot.EXITED, by.getValue("b").dot)
        assertEquals("离开活那一帧的短名照抄", "Claude 已退出", by.getValue("b").status.text)
        assertEquals(SessionDot.UNKNOWN, by.getValue("c").dot)
        assertEquals("状态不明", by.getValue("c").status.text)
    }

    @Test
    fun `没筛选：断着 · 还在接 · 在问都先画缓存；问到了换成那台这一份`() {
        val cached = page(row("1", "昨天"), row("2", "昨天"))
        val down = hostPart(snap(linkDown("m 离线", "boom"), got = Fetched.Loading, cached = cached))
        assertEquals(listOf("1", "2"), down.rows.map { it.sessionId })
        assertEquals(Problem("m 离线", "boom"), down.bar?.problem)
        assertEquals(listOf("1", "2"), hostPart(snap(linkConnecting(), got = Fetched.Loading, cached = cached)).rows.map { it.sessionId })
        assertEquals("在问 ⇒ 先画上次那一份", listOf("1", "2"), hostPart(snap(got = Fetched.Loading, cached = cached)).rows.map { it.sessionId })
        assertEquals("问到了 ⇒ 照那台这一份", listOf("1", "2", "3", "4"), hostPart(snap(cached = cached)).rows.map { it.sessionId })
    }

    @Test
    fun `带了筛选：缓存不画；那台断着 ⇒ 零行、那一句是离线 · 未列出（在搜 ⇒ 未搜索）`() {
        val cached = page(row("1", "昨天"))
        val downLink = linkDown("m 离线", "boom")
        val week = hostPart(snap(downLink, got = Fetched.Loading, cached = cached, ask = ListAsk(withinDays = 7)))
        assertTrue(week.rows.isEmpty())
        assertEquals(copyText("history.list.machineDown", "machine" to "m"), week.bar?.problem?.text)
        val searching = hostPart(snap(downLink, got = Fetched.Loading, cached = cached, ask = ListAsk(query = "q")))
        assertEquals(copyText("history.search.machineDown", "machine" to "m"), searching.bar?.problem?.text)
        assertTrue("在问、带着筛选 ⇒ 不先画缓存", hostPart(snap(got = Fetched.Loading, cached = cached, ask = ListAsk(hidden = true))).rows.isEmpty())
        val plain = hostPart(snap(downLink, got = Fetched.Loading, cached = cached))
        assertEquals(listOf("1"), plain.rows.map { it.sessionId })
        assertTrue(view(plain).bars.single().offline)
    }

    @Test
    fun `★ 断着：只照会话簿那一张看不见的脸（名状态不明 · 点说不清，同桌面）；看不见时照旧作数的（已结束 · 记录已不在）照画`() {
        val cached = page(row("1", "昨天"), row("2", "昨天", face = Faces.of("ended")), row("3", "昨天", face = Faces.of("gone")), row("4", "昨天"))
        val unseen = Faces.of("unseen")
        // 核心折好的：断的那一刻活的那一条落成看不见那一张、簿上记着它（`resident_client::book::unseen_now`）。
        val down = EMPTY_BOOK.copy(left = mapOf("1" to SessionLeft("1", unseen)), unseen = unseen)
        val by = hostPart(snap(linkDown("m 离线", "boom"), down, Fetched.Loading, cached)).rows.associateBy { it.sessionId }
        assertEquals(Toned(unseen.name, unseen.tone) to SessionDot.UNKNOWN, by.getValue("1").status to by.getValue("1").dot)
        assertEquals("名是状态不明，不是离线那一句", copyText("sessionState.unseen.name"), by.getValue("1").status.text)
        assertEquals("簿里没有、清单那一行又不作数 ⇒ 也照簿那一张", Toned(unseen.name, unseen.tone), by.getValue("4").status)
        assertEquals(SessionDot.ENDED, by.getValue("2").dot)
        assertEquals("已结束", by.getValue("2").status.text)
        assertEquals(SessionDot.GONE, by.getValue("3").dot)
        val stale = live("1", "live-working-plain")
        assertEquals("通着 ⇒ 照会话帧", SessionDot.RUNNING, hostPart(snap(table = stale, got = Fetched.Loading, cached = cached)).rows.first().dot)
    }

    @Test
    fun `两台各三行、at 交错 ⇒ 并成一列六行；连着的同一个段头跨台归一段`() {
        val a = hostPart(snap(got = Fetched.Got(page(row("a1", "今天", 90), row("a2", "今天", 60), row("a3", "昨天", 30)))))
        val b = hostPart(snap(got = Fetched.Got(page(row("b1", "今天", 80), row("b2", "昨天", 40), row("b3", "昨天", 10))), id = "h2", machine = "n"))
        val v = view(a, b)
        assertEquals(listOf("a1", "b1", "a2", "b2", "a3", "b3"), v.rows.map { it.sessionId })
        assertEquals(listOf("h1", "h2", "h1", "h2", "h1", "h2"), v.rows.map { it.hostId })
        assertEquals(listOf("今天", "昨天"), v.sections.map { it.title })
        val tie = view(hostPart(snap(got = Fetched.Got(page(row("x", "今天", 5))))), hostPart(snap(got = Fetched.Got(page(row("y", "今天", 5))), id = "h2")))
        assertEquals("同一个 at 照机器表先后", listOf("x", "y"), tie.rows.map { it.sessionId })
    }

    @Test
    fun `机器小标签：只勾一台都不写；两台只有不在当前机器上的那几行写`() {
        val a = hostPart(snap(got = Fetched.Got(page(row("a1", "今天", 9)))))
        val b = hostPart(snap(got = Fetched.Got(page(row("b1", "今天", 8))), id = "h2", machine = "gpu"))
        assertEquals(listOf<String?>(null), view(b, current = "h1").rows.map { it.machineTag })
        assertEquals(listOf(null, "gpu"), view(a, b).rows.map { it.machineTag })
    }

    @Test
    fun `空：没筛选 ⇒ 照核心那一句；核心还没交 ⇒ 骨架、不说无会话；带筛选 · 在搜 ⇒ 无匹配；还有台在读 ⇒ 骨架`() {
        val none = hostPart(snap(got = Fetched.Got(page())))
        assertEquals(Empty.NO_SESSIONS, view(none).empty)
        assertEquals(allListed, view(none).fleet)
        // 各台没报完：核心那一句不是「无会话」，屏上照它写。
        val waiting = FleetLines(copyText("beEmpty.line.waiting"), "gpu 连接中", "核心·计数")
        assertEquals(waiting, view(none, face = waiting).fleet)
        val unknown = view(none, face = null)
        assertTrue("核心还没交 ⇒ 骨架", unknown.skeleton)
        assertEquals(Empty.NONE, unknown.empty)
        assertEquals(Empty.NO_MATCH, view(none, filters = Filters(hidden = true)).empty)
        assertEquals(Empty.NO_MATCH, view(none, query = "q").empty)
        val reading = view(none, hostPart(snap(got = Fetched.Loading, id = "h2")))
        assertTrue(reading.skeleton)
        assertEquals(Empty.NONE, reading.empty)
    }

    @Test
    fun `分叉照核心挂：收着 ⇒ 父行带几条分叉、子行不出；展开 ⇒ 子行紧跟父、标成子；父不在这一列 ⇒ 其他会话`() {
        val p = hostPart(snap(got = Fetched.Got(page(row("P", "今天", 90), row("F", "昨天", 50, parent = "P"), row("G", "昨天", 40, parent = "X")))))
        val shut = view(p)
        assertEquals(listOf("P", "G"), shut.rows.map { it.sessionId })
        assertEquals(Nest(forks = 1), shut.rows[0].nest)
        assertEquals(Nest(orphan = true), shut.rows[1].nest)
        val open = view(p, open = setOf("h1" to "P"))
        assertEquals(listOf("P", "F", "G"), open.rows.map { it.sessionId })
        assertEquals(Nest(forks = 1, forksOpen = true), open.rows[0].nest)
        assertEquals(Nest(child = true), open.rows[1].nest)
        assertEquals("子行跟着父，不另起「昨天」那一段", listOf(listOf("P", "F"), listOf("G")), open.sections.map { s -> s.rows.map { it.sessionId } })
        assertEquals("别的台的同名会话不算展开", listOf("P", "G"), view(p, open = setOf("h2" to "P")).rows.map { it.sessionId })
    }

    @Test
    fun `按目录：两台的组按 order 并；同名目录在两台是两行；展开那一组 ＝ 并好那一列里它的行、次序照并好的`() {
        val a =
            hostPart(
                snap(got = Fetched.Got(HistoryListPage(listOf(row("a1", "今天", 90, group = "/a"), row("a2", "今天", 50, group = "/b"), row("a3", "昨天", 20, group = "/a")), listOf(group("/a", 30), group("/b", 10)), null, false))),
            )
        val b =
            hostPart(
                snap(got = Fetched.Got(HistoryListPage(listOf(row("b1", "今天", 70, group = "/a"), row("b2", "昨天", 40, group = "/c")), listOf(group("/a", 20), group("/c", 5)), null, false)), id = "h2", machine = "gpu"),
            )
        val v = view(a, b, filters = Filters(view = ChatsView.DIR))
        assertEquals(listOf("h1" to "/a", "h2" to "/a", "h1" to "/b", "h2" to "/c"), v.groups.map { it.hostId to it.group.key })
        assertEquals(
            "同名目录两台两个键",
            4,
            v.groups
                .map { it.id }
                .toSet()
                .size,
        )
        assertEquals(listOf(null, "gpu", null, "gpu"), v.groups.map { it.machineTag })
        assertEquals(listOf("a1", "a3"), v.rowsOf(v.groups[0]).map { it.sessionId })
        assertEquals(listOf("b1"), v.rowsOf(v.groups[1]).map { it.sessionId })
    }
}

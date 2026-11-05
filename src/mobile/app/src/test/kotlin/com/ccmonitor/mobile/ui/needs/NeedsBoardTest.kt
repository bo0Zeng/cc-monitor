package com.ccmonitor.mobile.ui.needs

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.copy.DurationFormat
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.Changed
import com.ccmonitor.mobile.core.wire.gen.Failed
import com.ccmonitor.mobile.core.wire.gen.ManualItem
import com.ccmonitor.mobile.ui.Faces
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import com.ccmonitor.mobile.ui.chat.linkConnecting
import com.ccmonitor.mobile.ui.chat.linkUp
import com.ccmonitor.mobile.ui.chat.needs.NeedsDesk
import com.ccmonitor.mobile.ui.chats.HostName
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 跨会话的需手动板：每条常驻流各问一份 `sessions-needs` 整份（不问 `manual-list`）、照推送 · 换流重问那一台；总览照它排、入口照它数；
 * 聊天屏那一块读它，不另问。夹具只造结构（sid · 占位句子）。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class NeedsBoardTest {
    private fun session(sid: String): Map<String, Any?> =
        mapOf(
            "kind" to "session",
            "rank" to 0.0,
            "session" to
                mapOf(
                    "sid" to sid,
                    "label" to "标题 $sid",
                    "face" to Faces.raw["live-needs-you-tmux"],
                    "needs" to mapOf("kind" to "approve", "text" to "等批准", "head" to "等批准 · Bash", "tone" to "need", "rank" to 0.0),
                ),
        )

    /** 计划里一条：落在 `cell` 那一格（标签照后端写好的「编号 标题」）；`null` ⇒ 没有格可落、不带那一键。 */
    private fun plan(
        slice: String,
        cell: String?,
    ): Map<String, Any?> =
        mapOf("kind" to "plan", "rank" to 1.0, "workspace" to "/w", "slice" to slice) +
            (cell?.let { mapOf("cell" to mapOf("id" to it, "label" to "$it 标题")) } ?: emptyMap())

    private fun manual(account: String): Map<String, Any?> =
        mapOf(
            "kind" to "manual",
            "rank" to 2.0,
            "manual" to
                mapOf(
                    "kind" to "accountLogin",
                    "agent" to "claude",
                    "account" to account,
                    "said" to "$account 需登录",
                    "fix" to mapOf("command" to "login-start", "args" to mapOf("kind" to "account", "agent" to "claude", "account" to account)),
                ),
        )

    /** 一台替身：答的那一份 · 推送口。 */
    private class Machine(
        var items: List<Map<String, Any?>>,
        var unreadable: String? = null,
    ) {
        val link = FakeCoreLink()
        val pushes = MutableSharedFlow<Changed>(extraBufferCapacity = 16)

        init {
            link.answer = { cmd, _ ->
                if (cmd == "sessions-needs") Reply.Ok(mapOf("items" to items, "ended" to emptyList<Any>(), "manualUnreadable" to unreadable)) else Reply.LinkDown
            }
        }

        fun asked(cmd: String) = link.calls(cmd).size
    }

    private fun TestScope.settle() {
        runCurrent()
        advanceTimeBy(NeedsBoard.DEBOUNCE_MS + 1)
        runCurrent()
    }

    private fun TestScope.boardOf(vararg m: Pair<String, Machine>): NeedsBoard {
        val b = NeedsBoard(MutableStateFlow(m.associate { (id, x) -> id to NeedsSource(x.link, x.pushes) }), backgroundScope)
        settle()
        return b
    }

    private val hosts = listOf(HostName("a", "devbox"), HostName("b", "gpu-01"))

    @Test
    fun `两台各回一份 ⇒ 当前那台在前、每台照回包先后（计划一条一行）、总数相加；每台只问 sessions-needs、零次 manual-list`() =
        runTest {
            val a = Machine(listOf(session("a1"), plan("x", "A2-1"), plan("x", null), manual("t")))
            val b = Machine(listOf(session("b1")))
            val board = boardOf("a" to a, "b" to b)
            val ov = overviewOf(board.state.value, hosts, current = "b")
            assertEquals(listOf("b1", "a1", "x@A2-1", "x@-", "@t"), ov.needs.map(::name))
            assertEquals(5, ov.total)
            assertEquals("入口那一份与总览同一份合法", 5, mergedOf(board.state.value, hosts).total)
            assertEquals("头那一句由核心写好", copyText("beGroups.count.needs", "n" to 5), ov.head)
            for (m in listOf(a, b)) {
                assertEquals("每台只问一次整份", 1, m.asked("sessions-needs"))
                assertEquals("会外那几项在整份里，不另问", 0, m.asked("manual-list"))
            }
            assertTrue("列表里不止一台 ⇒ 会外那一行写机器名", ov.manyHosts)
        }

    @Test
    fun `需手动屏那一行：那台写好的那一句，已等照收到以后走过的接着走（过 60 s 多 1m）；不知道等了多久 ⇒ 照抄 needs 的字`() =
        runTest {
            var clock = 5_000L
            val waiting =
                session("w").let { item ->
                    val row = item.getValue("session") as Map<*, *>
                    val needs = (row["needs"] as Map<*, *>) + ("waitedMs" to 12_000.0)
                    item + ("session" to (row + ("needs" to needs))) + ("clock" to mapOf("text" to "核心·t {dur}", "head" to "核心·h {dur}"))
                }
            val a = Machine(listOf(waiting, session("plain")))
            val board = NeedsBoard(MutableStateFlow(mapOf("a" to NeedsSource(a.link, a.pushes))), backgroundScope, now = { clock })
            settle()
            val (w, plain) = overviewOf(board.state.value, hosts, current = "a").needs.filterIsInstance<NeedsLine.Session>()
            assertEquals("核心·t ${DurationFormat.short(12_000L)}", w.state(5_000L))
            assertEquals("核心·t ${DurationFormat.short(72_000L)}", w.state(65_000L))
            assertEquals("等批准", plain.state(65_000L))
        }

    @Test
    fun `计划那一行写后端给的「编号 标题」、点了进那一格；没有格 ⇒ 写片名、进那一片的大纲`() =
        runTest {
            val a = Machine(listOf(plan("x", "A2-1"), plan("x", null)))
            val board = boardOf("a" to a)
            val lines = overviewOf(board.state.value, hosts, current = "a").needs.filterIsInstance<NeedsLine.Plan>()
            assertEquals(listOf("A2-1 标题", "x"), lines.map { it.title })
            assertEquals(listOf("A2-1", null), lines.map { it.cell?.id })
        }

    @Test
    fun `一台流断 ⇒ 那台不进数、板说它断着；回来（换了一条流）⇒ 重问那一台`() =
        runTest {
            val a = Machine(listOf(session("a1")))
            val b = Machine(listOf(session("b1"), manual("t")))
            val board = boardOf("a" to a, "b" to b)
            b.link.offline.value = "gpu-01 离线"
            b.link.state.value = linkConnecting()
            settle()
            val down = overviewOf(board.state.value, hosts, current = "a")
            assertEquals(listOf("a1"), down.needs.map(::name))
            assertEquals(1, down.total)
            assertEquals(
                "断着那台进核心合：那一行是那条链上那台写好的那一句（已叫出那台），不出［重试］",
                listOf(Triple("gpu-01", "gpu-01 离线", false)),
                down.down.map { Triple(it.host.label, it.said, it.retry) },
            )
            assertTrue("只是断着 ⇒ 数照样说得准（断着的本来不计）", down.sure)
            b.items = listOf(session("b2"))
            b.link.state.value = linkUp()
            b.link.epoch.value = 1L
            settle()
            assertEquals(2, b.asked("sessions-needs"))
            assertEquals(1, a.asked("sessions-needs"))
            assertEquals(listOf("a1", "b2"), overviewOf(board.state.value, hosts, current = "a").needs.map(::name))
        }

    @Test
    fun `那台推 changed needs（带不带会话）⇒ 只重问那一台；别的主题不问；会外读不出 ⇒ 数说不准、那台那一句`() =
        runTest {
            val a = Machine(listOf(session("a1")))
            val b = Machine(listOf())
            val board = boardOf("a" to a, "b" to b)
            for (c in listOf(Changed.Needs("a1"), Changed.Needs())) {
                a.pushes.emit(c)
                settle()
            }
            for (c in listOf(Changed.Plan("/w"), Changed.Tasks("a1"))) {
                a.pushes.emit(c)
                settle()
            }
            assertEquals(3, a.asked("sessions-needs"))
            assertEquals(1, b.asked("sessions-needs"))
            b.unreadable = "账号库无法解析"
            b.pushes.emit(Changed.Needs())
            settle()
            val ov = overviewOf(board.state.value, hosts, current = "a")
            assertFalse(ov.sure)
            assertFalse("入口那一格与总览同一份（核心合的）", mergedOf(board.state.value, hosts).sure)
            assertEquals(
                "那一行由核心写好（照称呼叫出那台），那台那一句长按看",
                listOf(Triple("gpu-01", copyText("needs.panel.unreadable", "machine" to "gpu-01"), "账号库无法解析")),
                ov.unreadable.map { Triple(it.host.label, it.line, it.said) },
            )
        }

    @Test
    fun `第一次 sessions-needs 没问到 ⇒ 不用谁再叫，过一会儿自己又问；问到了就不再自己问`() =
        runTest {
            val a = Machine(listOf(session("a1")))
            var failFirst = true
            val ok = a.link.answer
            a.link.answer = { cmd, args ->
                if (cmd == "sessions-needs" && failFirst) {
                    failFirst = false
                    Reply.LinkDown
                } else {
                    ok(cmd, args)
                }
            }
            val board = boardOf("a" to a)
            assertEquals(1, a.asked("sessions-needs"))
            assertNull(board.state.value["a"]?.list)
            advanceTimeBy(NeedsBoard.RETRY_FIRST_MS + NeedsBoard.DEBOUNCE_MS + 1)
            runCurrent()
            assertEquals(2, a.asked("sessions-needs"))
            assertEquals(1, mergedOf(board.state.value, hosts).total)
            advanceTimeBy(60_000)
            runCurrent()
            assertEquals(2, a.asked("sessions-needs"))
        }

    @Test
    fun `会话表里谁的点变了不另问：进出需手动由那台推 changed needs`() =
        runTest {
            val a = Machine(listOf(session("a1")))
            boardOf("a" to a)
            a.link.report("x", "/w", Faces.of("live-needs-you-tmux"))
            settle()
            assertEquals(1, a.asked("sessions-needs"))
        }

    @Test
    fun `聊天屏那一块读板上那一条：打开它不另问 sessions-needs`() =
        runTest {
            val a = Machine(listOf(session("s1")))
            val board = boardOf("a" to a)
            val before = a.asked("sessions-needs")
            val desk = NeedsDesk(a.link, board.state.map { it["a"] }, { board.reask("a") }, MutableStateFlow("s1"), "devbox", backgroundScope, now = { 0L })
            settle()
            assertEquals(
                "等批准 · Bash",
                desk.state.value.bar
                    ?.head,
            )
            assertEquals(before, a.asked("sessions-needs"))
        }

    @Test
    fun `入口：说得准且 0 ⇒ 不画；别的照核心写好的头那一句`() {
        assertNull(entryText(head = "核心·头", total = 0, sure = true))
        assertEquals("核心·头", entryText(head = "核心·头", total = 3, sure = true))
        assertEquals("核心·头", entryText(head = "核心·头", total = 0, sure = false))
    }

    @Test
    fun `板还没回 ⇒ 那台算读中、不出「无需手动事项」；都读到且 0 件 ⇒ 才说没有，并写读到了哪几台`() =
        runTest {
            val reading = overviewOf(mapOf("a" to MachineNeeds()), hosts, current = "a")
            assertEquals(listOf("devbox"), reading.loading.map { it.label })
            assertFalse("没读到不当没有", reading.empty)
            assertEquals("读中 ⇒ 头那一句不带数（核心写的）", copyText("beSession.needs.unknown"), reading.head)
            val board = boardOf("a" to Machine(emptyList()), "b" to Machine(emptyList()))
            val none = overviewOf(board.state.value, hosts, current = "a")
            assertTrue(none.empty)
            assertEquals(listOf("devbox", "gpu-01"), none.readFrom.map { it.label })
            assertTrue(none.loading.isEmpty())
        }

    @Test
    fun `两台里一台答了一个错 ⇒ 核心合出来的 down 里有那台、那一句带机器名（照称呼）、出［重试］；数说不准、头那一句不带数；重问问到了那一行就消失`() =
        runTest {
            val a = Machine(emptyList())
            val ok = a.link.answer
            var fail = true
            a.link.answer = { cmd, args -> if (cmd == "sessions-needs" && fail) Reply.Failed(Failed("io", "记录读取失败", "原话")) else ok(cmd, args) }
            val b = Machine(listOf(session("b1")))
            val board = boardOf("a" to a, "b" to b)
            val ov = overviewOf(board.state.value, hosts, current = "a")
            assertEquals(
                listOf(Triple("devbox", copyText("kit.text.machineSaid", "machine" to "devbox", "said" to "记录读取失败"), true)),
                ov.down.map { Triple(it.host.label, it.said, it.retry) },
            )
            assertEquals("详情原样带着", listOf("原话"), ov.down.map { it.detail })
            assertEquals("没答上的那台不计", 1, ov.total)
            assertFalse(ov.sure)
            assertEquals(copyText("beSession.needs.unknown"), ov.head)
            assertFalse(ov.empty)
            fail = false
            board.reask("a")
            settle()
            val after = overviewOf(board.state.value, hosts, current = "a")
            assertTrue(after.down.isEmpty())
            assertTrue(after.sure)
        }

    @Test
    fun `回包解不开 ⇒ 核心写读不出那台回复那一句（叫出那台）、详情带命令名；不计、数说不准`() =
        runTest {
            val a = Machine(emptyList())
            a.link.answer = { cmd, _ -> if (cmd == "sessions-needs") Reply.Ok(mapOf("items" to 3)) else Reply.LinkDown }
            val board = boardOf("a" to a)
            val ov = overviewOf(board.state.value, hosts, current = "a")
            assertEquals(listOf(copyText("peerVersion.said.unreadable", "machine" to "devbox")), ov.down.map { it.said })
            val d = ov.down.single()
            assertTrue(d.detail.contains("sessions-needs"))
            assertTrue(d.retry)
            assertFalse(ov.sure)
        }

    private fun name(l: NeedsLine): String =
        when (l) {
            is NeedsLine.Session -> l.row.sid
            is NeedsLine.Plan -> "${l.slice}@${l.cell?.id ?: "-"}"
            is NeedsLine.Manual -> "@${(l.item as ManualItem.AccountLogin).account}"
        }
}

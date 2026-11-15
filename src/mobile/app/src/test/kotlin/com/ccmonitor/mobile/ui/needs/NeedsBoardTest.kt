package com.ccmonitor.mobile.ui.needs

import com.ccmonitor.mobile.core.claude.link.RawJson
import com.ccmonitor.mobile.core.copy.DurationFormat
import com.ccmonitor.mobile.core.wire.gen.Changed
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
 * 跨会话的需手动板：一圈只问核心一次（逐台问 `sessions-needs` 与合都在核心，那一份另有 Rust 判据管），交的是各台那条流的编号与 hostId；
 * 照推送 · 换流 · 没答上之后退避再问一圈；总览照合好的那一份排、入口照它数；聊天屏那一块读它，不另问。夹具只造结构（sid · 占位句子）。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class NeedsBoardTest {
    private fun session(sid: String): Map<String, Any?> =
        mapOf(
            "kind" to "session",
            "rank" to 0,
            "session" to
                mapOf(
                    "sid" to sid,
                    "label" to "标题 $sid",
                    "face" to Faces.raw["live-needs-you-tmux"],
                    "needs" to mapOf("kind" to "approve", "text" to "等批准", "head" to "等批准 · Bash", "tone" to "need", "rank" to 0),
                ),
        )

    /** 计划里一条：落在 `cell` 那一格（标签照后端写好的「编号 标题」）；`null` ⇒ 没有格可落、不带那一键。 */
    private fun plan(
        slice: String,
        cell: String?,
    ): Map<String, Any?> =
        mapOf("kind" to "plan", "rank" to 1, "workspace" to "/w", "slice" to slice) +
            (cell?.let { mapOf("cell" to mapOf("id" to it, "label" to "$it 标题")) } ?: emptyMap())

    private fun manual(account: String): Map<String, Any?> =
        mapOf(
            "kind" to "manual",
            "rank" to 2,
            "manual" to
                mapOf(
                    "kind" to "accountLogin",
                    "agent" to "claude",
                    "account" to account,
                    "said" to "$account 需登录",
                    "fix" to mapOf("command" to "login-start", "args" to mapOf("kind" to "account", "agent" to "claude", "account" to account)),
                ),
        )

    /**
     * 核心合好的那一份（替身，线上那一形）：[items] 需手动那一列照给的先后（哪台 · 那一项）；[asks] 待答请求那一列；
     * [down] 没答上的（哪台 · 那一句 · 详情）。
     */
    private fun merged(
        items: List<Pair<String, Map<String, Any?>>> = emptyList(),
        down: List<Triple<String, String, String>> = emptyList(),
        unreadable: List<Triple<String, String, String>> = emptyList(),
        ended: List<Triple<String, String, String>> = emptyList(),
        head: String = "核心·头",
        sure: Boolean = down.isEmpty() && unreadable.isEmpty(),
        asks: List<Pair<String, Map<String, Any?>>> = emptyList(),
    ): String =
        RawJson.write(
            mapOf(
                "asks" to
                    mapOf(
                        "items" to asks.map { (o, i) -> mapOf("origin" to o, "item" to i) },
                        "total" to asks.size,
                        "head" to "核心·待答请求",
                        "sure" to down.isEmpty(),
                    ),
                "manual" to
                    mapOf(
                        "items" to items.map { (o, i) -> mapOf("origin" to o, "item" to i) },
                        "total" to items.size,
                        "head" to head,
                        "sure" to sure,
                    ),
                "unreadable" to unreadable.map { (o, line, said) -> mapOf("origin" to o, "line" to line, "said" to said) },
                "down" to down.map { (o, said, detail) -> mapOf("origin" to o, "said" to said, "detail" to detail) },
                "ended" to ended.map { (o, sid, said) -> mapOf("origin" to o, "sid" to sid, "end" to mapOf("said" to said, "stays" to false)) },
                "windowTitle" to "核心·标题",
            ),
        )

    /** 核心那一口的替身：记下每一圈交进来的那一行，答 [out]。 */
    private class Core {
        val got = mutableListOf<String>()
        var out = ""

        fun ask(input: String): String {
            got += input
            return out
        }
    }

    /** 一台替身：问答那一条（板不该经它问）· 推送口。 */
    private class Machine {
        val link = FakeCoreLink()
        val pushes = MutableSharedFlow<Changed>(extraBufferCapacity = 16)

        fun asked(cmd: String) = link.calls(cmd).size
    }

    private fun TestScope.settle() {
        runCurrent()
        advanceTimeBy(NeedsBoard.DEBOUNCE_MS + 1)
        runCurrent()
    }

    private fun TestScope.boardOf(
        core: Core,
        vararg m: Pair<String, Machine>,
        now: () -> Long = { 0L },
    ): NeedsBoard {
        val handles = m.mapIndexed { i, (id, _) -> id to (i + 1).toLong() }.toMap()
        val b =
            NeedsBoard(
                MutableStateFlow(m.associate { (id, x) -> id to NeedsSource(x.link, x.pushes) { handles.getValue(id) } }),
                backgroundScope,
                core::ask,
                now,
            )
        settle()
        return b
    }

    private val hosts = listOf(HostName("a", "devbox"), HostName("b", "gpu-01"))

    @Test
    fun `两台 ⇒ 一圈只问核心一次（交各台的编号与 hostId），各台的流上零次 sessions-needs；总览照核心合好的先后 · 总数 · 头那一句`() =
        runTest {
            val core = Core().apply { out = merged(listOf("b" to session("b1"), "a" to session("a1"), "a" to plan("x", "A2-1"), "a" to plan("x", null), "a" to manual("t"))) }
            val a = Machine()
            val b = Machine()
            val board = boardOf(core, "a" to a, "b" to b)
            assertEquals(
                "一圈一次、交的是哪几台",
                listOf("""{"hosts":[{"handle":1,"origin":"a"},{"handle":2,"origin":"b"}],"withinMs":15000}"""),
                core.got,
            )
            for (m in listOf(a, b)) assertEquals("板不经各台的流问", 0, m.asked("sessions-needs"))
            val ov = overviewOf(board.state.value, hosts, current = "b")
            assertEquals(listOf("b1", "a1", "x@A2-1", "x@-", "@t"), ov.needs.map(::name))
            assertEquals(5, ov.total)
            assertEquals("头那一句照核心写好的", "核心·头", ov.head)
            assertTrue("列表里不止一台 ⇒ 会外那一行写机器名", ov.manyHosts)
            assertEquals(listOf("b", "a"), ov.readFrom.map { it.id })
        }

    @Test
    fun `只有一条待答请求、需手动 0 件、都说得准 ⇒ 需手动屏一行都不列、说无需手动事项、入口不画`() =
        runTest {
            val core = Core().apply { out = merged(asks = listOf("a" to session("可答"))) }
            val board = boardOf(core, "a" to Machine())
            val ov = overviewOf(board.state.value, hosts, current = "a")
            assertEquals("待答请求那一列不进需手动屏", emptyList<String>(), ov.needs.map(::name))
            assertEquals(0, ov.total)
            assertTrue("说得准且 0 ⇒ 无需手动事项", ov.empty)
            val m = checkNotNull(board.state.value.merged).manual
            assertNull("入口照需手动那一列：0 件且说得准 ⇒ 不画", entryText(m.head, m.total, m.sure))
        }

    @Test
    fun `需手动屏那一行：那台写好的那一句，已等照收到以后走过的接着走（过 60 s 多 1m）；不知道等了多久 ⇒ 照抄 needs 的字`() =
        runTest {
            val clock = 5_000L
            val waiting =
                session("w").let { item ->
                    val row = item.getValue("session") as Map<*, *>
                    val needs = (row["needs"] as Map<*, *>) + ("waitedMs" to 12_000)
                    item + ("session" to (row + ("needs" to needs))) + ("clock" to mapOf("text" to "核心·t {dur}", "head" to "核心·h {dur}"))
                }
            val core = Core().apply { out = merged(listOf("a" to waiting, "a" to session("plain"))) }
            val board = boardOf(core, "a" to Machine(), now = { clock })
            val (w, plain) = overviewOf(board.state.value, hosts, current = "a").needs.filterIsInstance<NeedsLine.Session>()
            assertEquals(DurationFormat.clock("核心·t {dur}", 0L, 12_000L, 0L), w.state(5_000L))
            assertEquals(DurationFormat.clock("核心·t {dur}", 0L, 72_000L, 0L), w.state(65_000L))
            assertEquals("等批准", plain.state(65_000L))
        }

    @Test
    fun `计划那一行写后端给的「编号 标题」、点了进那一格；没有格 ⇒ 写片名、进那一片的大纲`() =
        runTest {
            val core = Core().apply { out = merged(listOf("a" to plan("x", "A2-1"), "a" to plan("x", null))) }
            val board = boardOf(core, "a" to Machine())
            val lines = overviewOf(board.state.value, hosts, current = "a").needs.filterIsInstance<NeedsLine.Plan>()
            assertEquals(listOf("A2-1 标题", "x"), lines.map { it.title })
            assertEquals(listOf("A2-1", null), lines.map { it.cell?.id })
        }

    @Test
    fun `一台流断 ⇒ 核心说它没答上的那一行不出［重试］；回来（换了一条流）⇒ 再问一圈`() =
        runTest {
            val core = Core().apply { out = merged(listOf("a" to session("a1"), "b" to session("b1"))) }
            val a = Machine()
            val b = Machine()
            val board = boardOf(core, "a" to a, "b" to b)
            core.out = merged(listOf("a" to session("a1")), down = listOf(Triple("b", "核心·gpu-01 离线", "")))
            b.link.state.value = linkConnecting()
            board.reask()
            settle()
            val down = overviewOf(board.state.value, hosts, current = "a")
            assertEquals(listOf("a1"), down.needs.map(::name))
            assertEquals(
                "断着那台：那一句照核心写好的，不出［重试］",
                listOf(Triple("gpu-01", "核心·gpu-01 离线", false)),
                down.down.map { Triple(it.host.label, it.said, it.retry) },
            )
            assertFalse("核心说不准", down.sure)
            assertTrue("断着的不算读中", down.loading.isEmpty())
            val rounds = core.got.size
            core.out = merged(listOf("a" to session("a1"), "b" to session("b2")))
            b.link.state.value = linkUp()
            b.link.epoch.value = 1L
            settle()
            assertEquals("换了一条流 ⇒ 再问一圈", rounds + 1, core.got.size)
            assertEquals(listOf("a1", "b2"), overviewOf(board.state.value, hosts, current = "a").needs.map(::name))
        }

    @Test
    fun `核心说不准、加起来 0 件 ⇒ 不说「无需手动事项」`() =
        runTest {
            val core = Core().apply { out = merged(down = listOf(Triple("b", "核心·离线", ""))) }
            val board = boardOf(core, "a" to Machine(), "b" to Machine())
            val ov = overviewOf(board.state.value, hosts, current = "a")
            assertEquals(0, ov.total)
            assertFalse(ov.sure)
            assertFalse("核心说不准 ⇒ 不出「无需手动事项」", ov.empty)
        }

    @Test
    fun `哪台推 changed needs（带不带会话）⇒ 再问一圈（连着来的合成一次）；别的主题不问；会外读不出那一行照核心写好的摆`() =
        runTest {
            val core = Core().apply { out = merged(listOf("a" to session("a1"))) }
            val a = Machine()
            val b = Machine()
            val board = boardOf(core, "a" to a, "b" to b)
            assertEquals(1, core.got.size)
            a.pushes.emit(Changed.Needs("a1"))
            settle()
            assertEquals(2, core.got.size)
            a.pushes.emit(Changed.Needs())
            b.pushes.emit(Changed.Needs())
            settle()
            assertEquals("连着来的合成一圈", 3, core.got.size)
            for (c in listOf(Changed.Plan("/w"), Changed.Tasks("a1"))) {
                a.pushes.emit(c)
                settle()
            }
            assertEquals("别的主题不问", 3, core.got.size)
            core.out = merged(unreadable = listOf(Triple("b", "核心·gpu-01 读不出", "账号库无法解析")))
            b.pushes.emit(Changed.Needs())
            settle()
            val ov = overviewOf(board.state.value, hosts, current = "a")
            assertFalse(ov.sure)
            assertEquals(
                listOf(Triple("gpu-01", "核心·gpu-01 读不出", "账号库无法解析")),
                ov.unreadable.map { Triple(it.host.label, it.line, it.said) },
            )
        }

    @Test
    fun `那一圈有台没答上 ⇒ 不用谁再叫，过一会儿自己再问一圈；都答上了就不再自己问`() =
        runTest {
            val core = Core().apply { out = merged(down = listOf(Triple("a", "核心·没答上", ""))) }
            val board = boardOf(core, "a" to Machine())
            assertEquals(1, core.got.size)
            core.out = merged(listOf("a" to session("a1")))
            advanceTimeBy(NeedsBoard.RETRY_FIRST_MS + NeedsBoard.DEBOUNCE_MS + 1)
            runCurrent()
            assertEquals(2, core.got.size)
            assertEquals(
                1,
                board.state.value.merged
                    ?.manual
                    ?.total,
            )
            advanceTimeBy(60_000)
            runCurrent()
            assertEquals(2, core.got.size)
        }

    @Test
    fun `会话表里谁的点变了不另问：进出需手动由那台推 changed needs`() =
        runTest {
            val core = Core().apply { out = merged() }
            val a = Machine()
            boardOf(core, "a" to a)
            a.link.report("x", "/w", Faces.of("live-needs-you-tmux"))
            settle()
            assertEquals(1, core.got.size)
        }

    @Test
    fun `聊天屏那一块读合好那一份里本台那一条（别台同 sid 的不认）：打开它不另问`() =
        runTest {
            val other = session("s1").let { it + ("session" to ((it.getValue("session") as Map<*, *>) + ("needs" to mapOf("kind" to "approve", "text" to "别台", "head" to "别台那一条", "tone" to "need", "rank" to 0)))) }
            val core = Core().apply { out = merged(listOf("b" to other, "a" to session("s1")), ended = listOf(Triple("b", "s1", "别台收场"))) }
            val a = Machine()
            val board = boardOf(core, "a" to a, "b" to Machine())
            val before = core.got.size
            val desk = NeedsDesk(a.link, board.state.map { it.merged }, "a", board::reask, MutableStateFlow("s1"), "devbox", backgroundScope, now = { 0L })
            settle()
            assertEquals(
                "等批准 · Bash",
                desk.state.value.bar
                    ?.head,
            )
            assertEquals(before, core.got.size)
            assertEquals(0, a.asked("sessions-needs"))
        }

    @Test
    fun `板上少了一台（那条流收了）⇒ 再问一圈、只问剩下的；那台再推也不叫`() =
        runTest {
            val core = Core().apply { out = merged() }
            val a = Machine()
            val b = Machine()
            val sources = MutableStateFlow(mapOf("a" to NeedsSource(a.link, a.pushes) { 1L }, "b" to NeedsSource(b.link, b.pushes) { 2L }))
            NeedsBoard(sources, backgroundScope, core::ask)
            settle()
            assertEquals(1, core.got.size)
            sources.value = mapOf("a" to NeedsSource(a.link, a.pushes) { 1L })
            settle()
            assertEquals(listOf("""{"hosts":[{"handle":1,"origin":"a"}],"withinMs":15000}"""), core.got.drop(1))
            b.pushes.emit(Changed.Needs())
            settle()
            assertEquals("收了的那台不再叫", 2, core.got.size)
        }

    @Test
    fun `入口：说得准且 0 ⇒ 不画；别的照核心写好的头那一句`() {
        assertNull(entryText(head = "核心·头", total = 0, sure = true))
        assertEquals("核心·头", entryText(head = "核心·头", total = 3, sure = true))
        assertEquals("核心·头", entryText(head = "核心·头", total = 0, sure = false))
    }

    @Test
    fun `还没问完过一圈 ⇒ 板上那几台算读中、不出「无需手动事项」；核心说得准且 0 件 ⇒ 才说没有，并写读到了哪几台`() =
        runTest {
            val reading = overviewOf(NeedsNow(on = setOf("a")), hosts, current = "a")
            assertEquals(listOf("devbox"), reading.loading.map { it.label })
            assertFalse("没读到不当没有", reading.empty)
            val core = Core().apply { out = merged() }
            val board = boardOf(core, "a" to Machine(), "b" to Machine())
            val none = overviewOf(board.state.value, hosts, current = "a")
            assertTrue(none.empty)
            assertEquals(listOf("devbox", "gpu-01"), none.readFrom.map { it.label })
            assertTrue(none.loading.isEmpty())
        }

    @Test
    fun `通着却没答上那台 ⇒ 出［重试］、详情原样带着；点了再问一圈，问到了那一行就消失`() =
        runTest {
            val core = Core().apply { out = merged(listOf("b" to session("b1")), down = listOf(Triple("a", "核心·devbox · 记录读取失败", "原话"))) }
            val board = boardOf(core, "a" to Machine(), "b" to Machine())
            val ov = overviewOf(board.state.value, hosts, current = "a")
            assertEquals(listOf(Triple("devbox", "核心·devbox · 记录读取失败", true)), ov.down.map { Triple(it.host.label, it.said, it.retry) })
            assertEquals(listOf("原话"), ov.down.map { it.detail })
            assertEquals(listOf("gpu-01"), ov.readFrom.map { it.label })
            core.out = merged(listOf("b" to session("b1")))
            board.reask()
            settle()
            val after = overviewOf(board.state.value, hosts, current = "a")
            assertTrue(after.down.isEmpty())
            assertTrue(after.sure)
        }

    private fun name(l: NeedsLine): String =
        when (l) {
            is NeedsLine.Session -> l.row.sid
            is NeedsLine.Plan -> "${l.slice}@${l.cell?.id ?: "-"}"
            is NeedsLine.Manual -> "@${(l.item as ManualItem.AccountLogin).account}"
        }
}

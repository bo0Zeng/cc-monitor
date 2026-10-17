package com.ccmonitor.mobile.ui.needs

import com.ccmonitor.mobile.core.claude.link.Changed
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.ui.Faces
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
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

    private fun plan(
        slice: String,
        k: Int,
    ): Map<String, Any?> = mapOf("kind" to "plan", "rank" to 1.0, "workspace" to "/w", "slice" to slice, "k" to k.toDouble())

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
                if (cmd == "sessions-needs") Reply.Ok(mapOf("items" to items, "answered" to emptyList<Any>(), "manualUnreadable" to unreadable)) else Reply.LinkDown
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
    fun `两台各回一份 ⇒ 当前那台在前、每台照回包先后（计划同一片收一行）、总数相加；每台只问 sessions-needs、零次 manual-list`() =
        runTest {
            val a = Machine(listOf(session("a1"), plan("x", 0), plan("x", 1), manual("t")))
            val b = Machine(listOf(session("b1")))
            val board = boardOf("a" to a, "b" to b)
            val ov = overviewOf(board.state.value, emptyMap(), hosts, current = "b")
            assertEquals(listOf("b1", "a1", "x×2", "@t"), ov.needs.map(::name))
            assertEquals(5, ov.total)
            assertEquals(5, board.total())
            for (m in listOf(a, b)) {
                assertEquals("每台只问一次整份", 1, m.asked("sessions-needs"))
                assertEquals("会外那几项在整份里，不另问", 0, m.asked("manual-list"))
            }
            assertTrue("列表里不止一台 ⇒ 会外那一行写机器名", ov.manyHosts)
        }

    @Test
    fun `一台流断 ⇒ 那台不进数、板说它断着；回来（换了一条流）⇒ 重问那一台`() =
        runTest {
            val a = Machine(listOf(session("a1")))
            val b = Machine(listOf(session("b1"), manual("t")))
            val board = boardOf("a" to a, "b" to b)
            b.link.state.value = LinkState.Connecting
            settle()
            val down = overviewOf(board.state.value, emptyMap(), hosts, current = "a")
            assertEquals(listOf("a1"), down.needs.map(::name))
            assertEquals(1, down.total)
            assertEquals(listOf("gpu-01"), down.down.map { it.label })
            b.items = listOf(session("b2"))
            b.link.state.value = LinkState.Up()
            b.link.epoch.value = 1L
            settle()
            assertEquals(2, b.asked("sessions-needs"))
            assertEquals(1, a.asked("sessions-needs"))
            assertEquals(listOf("a1", "b2"), overviewOf(board.state.value, emptyMap(), hosts, current = "a").needs.map(::name))
        }

    @Test
    fun `那台推 changed needs · plan · manual ⇒ 只重问那一台；别的主题不问；会外读不出 ⇒ 数说不准、那台那一句`() =
        runTest {
            val a = Machine(listOf(session("a1")))
            val b = Machine(listOf())
            val board = boardOf("a" to a, "b" to b)
            for (topic in listOf("needs", "plan", "manual")) {
                a.pushes.emit(Changed(topic, null))
                settle()
            }
            a.pushes.emit(Changed("tasks", null))
            settle()
            assertEquals(4, a.asked("sessions-needs"))
            assertEquals(1, b.asked("sessions-needs"))
            b.unreadable = "账号库无法解析"
            b.pushes.emit(Changed("manual", null))
            settle()
            val ov = overviewOf(board.state.value, emptyMap(), hosts, current = "a")
            assertTrue(ov.uncertain)
            assertEquals(listOf("gpu-01" to "账号库无法解析"), ov.unreadable.map { (h, s) -> h.label to s })
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
    fun `入口：总数 0 ⇒ 不画；≥ 1 ⇒ 需手动 n；数说不准 ⇒ 不带数字`() {
        assertNull(entryText(0, uncertain = false))
        assertEquals(copyText("tabBar.needs.aria", "n" to 3), entryText(3, uncertain = false))
        assertEquals(copyText("tabBar.needs.label"), entryText(3, uncertain = true))
    }

    private fun name(l: NeedsLine): String =
        when (l) {
            is NeedsLine.Session -> l.row.sid
            is NeedsLine.Plan -> "${l.slice}×${l.n}"
            is NeedsLine.Manual -> "@${l.item.account}"
        }
}

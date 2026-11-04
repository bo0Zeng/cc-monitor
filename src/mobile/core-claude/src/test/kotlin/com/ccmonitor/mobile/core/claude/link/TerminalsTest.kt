package com.ccmonitor.mobile.core.claude.link

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 终端那几问（列 · 抓屏 · 送字 · 结束 · 进终端那一行）：请求照协议的格，应答照金样解。 */
class TerminalsTest {
    private val terminals = Fixtures.json("terminals.golden.json")

    @Test
    fun `terminals-list：请求与金样同形，自报是手机`() {
        assertEquals(terminals.obj("terminals-list")!!.obj("request"), TerminalsList.args())
    }

    @Test
    fun `terminals-list：每行解出句柄 · tmux 名 · 标题 · 有没有人连着 · 里面是哪一家`() {
        val rows = TerminalsList.of(terminals.obj("terminals-list")!!.obj("reply"))!!
        assertEquals(listOf("demo-cc", "other", "pipe-x"), rows.map { it.name })
        val demo = rows.first { it.name == "demo-cc" }
        assertEquals(TerminalsList.Row(terminal = "tmux-1-1", name = "demo-cc", title = "demo title", attached = true, agent = "claude", sid = "sid-a", tmux = true), demo)
        val other = rows.first { it.name == "other" }
        assertEquals("没人连着 ⇒ 不算附着", false, other.attached)
        assertNull("里面没跑会话 ⇒ 不知道是哪一家", other.agent)
        assertNull("缺 terminals ⇒ 解不出", TerminalsList.of(mapOf("complete" to true)))
    }

    @Test
    fun `托管那一行：名字 · 句柄照收，不是 tmux；持有者那一句照那台写好的（手机不接管、不交还，只摆那一句）`() {
        val reply = terminals.obj("terminals-list")!!.obj("reply")!!
        val tmuxRow = (reply["terminals"] as List<*>).first() as Map<*, *>
        val held = mapOf("kind" to "held", "holder" to mapOf("who" to "terminal-window", "mine" to false, "said" to "S-desk"))
        val managed = tmuxRow + mapOf("host" to "managed", "terminal" to "m-7f3a", "name" to "work", "input" to held)
        val row = TerminalsList.of(reply + mapOf("terminals" to listOf(managed)))!!.single()
        assertEquals("work", row.name)
        assertEquals("m-7f3a", row.terminal)
        assertEquals(false, row.tmux)
        assertEquals("S-desk", row.holder)
        assertEquals("结束按句柄交", TerminalEnd.args("m-7f3a"), mapOf("terminal" to "m-7f3a", "client" to "mobile"))
    }

    @Test
    fun `按 tmux 名找句柄：只认名字一模一样的那一行`() {
        val rows = TerminalsList.of(terminals.obj("terminals-list")!!.obj("reply"))!!
        assertEquals("tmux-1-1", TerminalsList.handleOf(rows, "demo-cc"))
        assertNull(TerminalsList.handleOf(rows, "demo"))
    }

    @Test
    fun `能不能结束照每行的 can_end：不能 ⇒ 那台那一句；能 ⇒ 没有`() {
        val rows = TerminalsList.of(terminals.obj("terminals-list")!!.obj("reply"))!!
        assertNull(rows.first { it.name == "demo-cc" }.endRefused)
        assertEquals("属于别的前端 · 只读", rows.first { it.name == "other" }.endRefused)
    }

    @Test
    fun `sessions-where：只问那一条、自报是手机；回包照解，问的那一条在第一个`() {
        assertEquals(mapOf("sids" to listOf("s1"), "client" to "mobile"), SessionsWhere.args("s1"))
        val reply =
            mapOf(
                "results" to
                    listOf(mapOf("sid" to "s1", "standing" to "ambiguous", "names" to listOf("a", "b"), "terminals" to listOf(mapOf("host" to "tmux", "terminal" to "h-a"), mapOf("host" to "managed", "terminal" to "m-b")), "run" to "tmux")),
            )
        val w = SessionsWhere.of(reply)!!
        assertEquals("s1", w.sid)
        assertEquals(listOf("a", "b"), w.names)
        assertNull("缺 results ⇒ 解不出", SessionsWhere.of(mapOf("x" to 1)))
    }

    @Test
    fun `terminal-preview：抓纯文字一屏；应答按行拼回`() {
        assertEquals(mapOf("terminal" to "tmux-1", "color" to false), TerminalPreview.args("tmux-1"))
        assertEquals("line one\nline two", TerminalPreview.of(terminals.obj("terminal-preview")!!.obj("reply")))
        assertNull(TerminalPreview.of(mapOf("rows" to 2)))
    }

    @Test
    fun `terminal-end：按句柄结束，自报是手机；应答认回的句柄`() {
        assertEquals(mapOf("terminal" to "m-7f3a", "client" to "mobile"), TerminalEnd.args("m-7f3a"))
        assertEquals("m-7f3a", TerminalEnd.of(mapOf("terminal" to "m-7f3a")))
        assertNull(TerminalEnd.of(mapOf("session" to "demo-cc")))
    }

    @Test
    fun `tmux 那一行没有持有者那一句`() {
        assertEquals(null, TerminalsList.of(terminals.obj("terminals-list")!!.obj("reply"))!!.first().holder)
    }
}

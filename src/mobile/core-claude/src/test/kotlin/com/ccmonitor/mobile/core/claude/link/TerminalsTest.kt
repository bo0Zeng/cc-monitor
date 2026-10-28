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
        assertEquals(TerminalsList.Row(terminal = "tmux-1-1", name = "demo-cc", title = "demo title", attached = true, agent = "claude", sid = "sid-a"), demo)
        val other = rows.first { it.name == "other" }
        assertEquals("没人连着 ⇒ 不算附着", false, other.attached)
        assertNull("里面没跑会话 ⇒ 不知道是哪一家", other.agent)
        assertNull("缺 terminals ⇒ 解不出", TerminalsList.of(mapOf("complete" to true)))
    }

    @Test
    fun `按 tmux 名找句柄：只认名字一模一样的那一行`() {
        val rows = TerminalsList.of(terminals.obj("terminals-list")!!.obj("reply"))!!
        assertEquals("tmux-1-1", TerminalsList.handleOf(rows, "demo-cc"))
        assertNull(TerminalsList.handleOf(rows, "demo"))
    }

    @Test
    fun `一条会话在哪个终端里：认名单上里面跑着它的那一行（那台认的）；没有 ⇒ 没有`() {
        val rows = TerminalsList.of(terminals.obj("terminals-list")!!.obj("reply"))!!
        assertEquals("demo-cc", TerminalsList.holding(rows, "sid-a")?.name)
        assertNull(TerminalsList.holding(rows, "sid-zz"))
    }

    @Test
    fun `terminal-preview：抓纯文字一屏；应答按行拼回`() {
        assertEquals(mapOf("terminal" to "tmux-1", "color" to false), TerminalPreview.args("tmux-1"))
        assertEquals("line one\nline two", TerminalPreview.of(terminals.obj("terminal-preview")!!.obj("reply")))
        assertNull(TerminalPreview.of(mapOf("rows" to 2)))
    }

    @Test
    fun `terminal-input：按句柄送字`() {
        assertEquals(mapOf("terminal" to "tmux-1", "text" to "/model opus"), TerminalInput.textInto("tmux-1", "/model opus"))
    }

    @Test
    fun `terminal-end：按句柄结束，自报是手机；应答认回的句柄`() {
        assertEquals(mapOf("terminal" to "m-7f3a", "client" to "mobile"), TerminalEnd.args("m-7f3a"))
        assertEquals("m-7f3a", TerminalEnd.of(mapOf("terminal" to "m-7f3a")))
        assertNull(TerminalEnd.of(mapOf("session" to "demo-cc")))
    }

    @Test
    fun `接管 · 交还：按句柄交，自报是手机；应答读出此刻的持有者（那台写好的那一句），tmux 那一形没有`() {
        assertEquals(mapOf("terminal" to "m-7f3a", "client" to "mobile"), TerminalHold.args("m-7f3a"))
        val held = mapOf("input" to mapOf("kind" to "held", "holder" to mapOf("who" to "mobile", "mine" to true, "said" to "S-here")))
        assertEquals(TerminalsList.Holder("S-here", true), TerminalHold.of(held))
        assertNull(TerminalHold.of(mapOf("input" to mapOf("kind" to "shared"))))
        assertEquals("tmux 那一行没有持有者", null, TerminalsList.of(terminals.obj("terminals-list")!!.obj("reply"))!!.first().holder)
    }
}

package com.ccmonitor.mobile.core.claude.link

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 起会话 · 送字 · 读正文那几问：请求照协议的格，应答照金样解。 */
class SessionsTest {
    private val terminals = Fixtures.json("terminals.golden.json")
    private val reads = Fixtures.json("record-reads.golden.json")
    private val frames = Fixtures.text("session-stream.golden.jsonl").lines().mapNotNull(Json::obj)

    @Test
    fun `session-new：在那台 tmux 里起 Claude，带票；不带模型 · 权限 · 账号（跟随默认）`() {
        val args = SessionNew.args(cwd = "~/proj", ticket = "t-1")
        assertEquals(mapOf("agent" to "claude", "cwd" to "~/proj", "place" to "tmux", "local" to false, "ticket" to "t-1"), args)
    }

    @Test
    fun `session-new：应答解出结局 · tmux 名 · 展开后的目录；新起的 sid 为空`() {
        val a = SessionNew.of(mapOf("outcome" to "started", "session" to "proj-cc", "sid" to null, "cmd" to null, "account" to null, "agent" to "claude", "cwd" to "/h/proj"))
        assertEquals(SessionNew.Answer(outcome = "started", session = "proj-cc", sid = null, cwd = "/h/proj"), a)
        assertNull("缺 cwd ⇒ 认不出报到的会话 ⇒ 解不出", SessionNew.of(mapOf("outcome" to "started", "agent" to "claude")))
    }

    @Test
    fun `session-new：报到的会话按展开后的目录认，起之前就在的不算`() {
        val before = setOf("old")
        val table =
            SessionTable(
                sessions =
                    mapOf(
                        "old" to LiveSession("old", cwd = "/h/proj", activity = Toned("空闲", null)),
                        "other" to LiveSession("other", cwd = "/h/else", activity = Toned("空闲", null)),
                        "new" to LiveSession("new", path = "/p/new.jsonl", cwd = "/h/proj", activity = Toned("运行中", null)),
                    ),
            )
        assertEquals("new", SessionNew.arrived(table, "/h/proj", before)?.sid)
        assertNull(SessionNew.arrived(table, "/h/none", before))
    }

    @Test
    fun `terminal-input：请求与金样同形（字 · 键），只带 sid`() {
        val golden = terminals.obj("terminal-input")!!.objs("requests")!!
        assertEquals(golden[1], TerminalInput.key("sid-a", "esc"))
        assertEquals(mapOf("sid" to "s", "text" to "/model opus"), TerminalInput.text("s", "/model opus"))
    }

    @Test
    fun `terminal-input：金样三种回话都解得出，拒的那句照抄`() {
        val replies = terminals.obj("terminal-input")!!.objs("replies")!!.map(TerminalInput::of)
        assertEquals(TerminalInput.Result.Delivered, replies[0])
        assertEquals(TerminalInput.Result.Refused(said = "画面已变", why = "screen_changed"), replies[1])
        assertEquals(TerminalInput.Result.Unsure, replies[2])
        assertNull(TerminalInput.of(mapOf("result" to "refused")))
        assertNull(TerminalInput.of(mapOf("result" to "maybe")))
    }

    @Test
    fun `history-tail · history-read：尾段起点与每行的记录照抄；不进界面的行不出`() {
        assertEquals(mapOf("path" to "/p/a.jsonl", "n" to 50L), HistoryTail.args("/p/a.jsonl", 50))
        assertEquals(HistoryTail.Answer(end = 900, splitAt = 300), HistoryTail.of(mapOf("end" to 900, "split_at" to 300, "tail_from" to 7, "total" to 9)))
        assertEquals(mapOf("path" to "/p/a.jsonl", "offset" to 3L, "until" to 9L), HistoryRead.args("/p/a.jsonl", 3, 9))
        val page = HistoryRead.of(reads["history-read"])!!
        assertEquals(273L, page.next)
        assertTrue(page.eof)
        assertEquals(listOf("r-1", "r-2"), page.records.map { it.str("id") })
        assertEquals(listOf(152L, 273L), page.ends)
    }

    @Test
    fun `line 帧：带记录的解出 sid · 偏移 · 记录；不带记录的照占号（记录为空）`() {
        val (full, least) = frames.filter { it.str("kind") == "line" }
        val a = RecordLine.of(full)!!
        assertEquals("s1", a.sid)
        assertEquals(120L, a.byteOffset)
        assertEquals("u1", a.record?.str("id"))
        assertNull(RecordLine.of(least)!!.record)
        assertNull(RecordLine.of(mapOf("kind" to "session_added", "sid" to "s1")))
    }
}

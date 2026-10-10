package com.ccmonitor.mobile.core.claude.link

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ProductsTest {
    private val frames = Fixtures.text("session-stream.golden.jsonl").lines().mapNotNull(Json::obj)

    private fun golden(kind: String) = frames.filter { it.str("kind") == kind }

    @Test
    fun `会话表：金样里 session_added 全格与最少格都折得进来，字与语气照抄`() {
        val breaks = mutableListOf<String>()
        val (full, least) = golden("session_added").take(2)
        val t = SessionTable().step(full, breaks::add)
        assertEquals(Toned("需手动", Tone.NEED), t.sessions.getValue("s1").activity)
        assertEquals("/p/s1.jsonl", t.sessions.getValue("s1").path)
        val t2 = SessionTable().step(least, breaks::add)
        assertEquals(Toned("运行中", Tone.NOW), t2.sessions.getValue("s1").activity)
        assertTrue(breaks.toString(), breaks.isEmpty())
    }

    @Test
    fun `会话表：status 改字、removed 拿掉、replayed 记下、丢了会话帧要重接`() {
        val breaks = mutableListOf<String>()
        var t = SessionTable().step(golden("session_added")[0], breaks::add)
        t = t.step(golden("session_status")[1], breaks::add)
        assertEquals(Toned("运行中", Tone.NOW), t.sessions.getValue("s1").activity)
        assertEquals("/p/s1.jsonl", t.sessions.getValue("s1").path) // 状态帧不抹掉宣告时的格
        t = t.step(golden("sessions_replayed")[0], breaks::add)
        assertTrue(t.replayed)
        val (lostOne, plain) = golden("overflow")
        assertTrue(t.step(lostOne, breaks::add).needsResync)
        assertFalse("只丢了内容帧 ⇒ 表照样可信", t.step(plain, breaks::add).needsResync)
        t = t.step(golden("session_removed")[1], breaks::add)
        assertTrue(t.sessions.isEmpty())
        assertTrue(breaks.toString(), breaks.isEmpty())
    }

    @Test
    fun `会话表：缺必填格报出来、表不动`() {
        val breaks = mutableListOf<String>()
        val t = SessionTable().step(mapOf("kind" to "session_added", "sid" to "s"), breaks::add)
        assertTrue(t.sessions.isEmpty())
        assertEquals(1, breaks.size)
    }

    @Test
    fun `history-list：金样整份解得出，标题 · 段头 · 时刻都是核心写好的`() {
        val a = HistoryList.of(Fixtures.json("history-list.golden.json"))
        assertNotNull(a)
        assertEquals(5, a!!.rows.size)
        val r = a.rows.first()
        assertEquals("从父会话分出来的", r.label)
        assertEquals("今天", r.sectionText)
        assertEquals("00:00", r.atText)
        assertEquals("/h/.claude/projects/-w/0000aaaa-0000-4000-8000-000000000005.jsonl", r.jsonlPath)
    }

    @Test
    fun `history-list：缺 label 的行 ⇒ 整份解不出，不拿 sid 顶标题`() {
        assertNull(HistoryList.of(mapOf("rows" to listOf(mapOf("sessionId" to "s")))))
    }

    @Test
    fun `sessions-needs：照核心的先后，不另排；字与已等照抄`() {
        val data =
            mapOf(
                "waiting" to
                    listOf(
                        mapOf("sid" to "b", "needs" to mapOf("kind" to "approve", "text" to "等批准 · Bash", "tone" to "need", "rank" to 0.0, "waitedText" to "3m")),
                        mapOf("sid" to "a", "needs" to mapOf("kind" to "answer", "text" to "等回答", "tone" to "need", "rank" to 1.0, "waitedText" to null)),
                    ),
            )
        val rows = SessionsNeeds.of(data)!!
        assertEquals(listOf("b", "a"), rows.map { it.sid })
        assertEquals("等批准 · Bash", rows[0].needs.text)
        assertEquals(null, rows[1].needs.waitedText)
        assertEquals(emptyList<SessionsNeeds.Row>(), SessionsNeeds.of(mapOf("waiting" to emptyList<Any>())))
        assertNull("缺 text ⇒ 契约对不上", SessionsNeeds.of(mapOf("waiting" to listOf(mapOf("sid" to "a", "needs" to mapOf("kind" to "x", "rank" to 0.0))))))
    }

    @Test
    fun `quota-read：每号的行、5h 那一格、开窗那一句照抄；出过数的在前`() {
        val cell = { t: String, tone: String -> mapOf("text" to t, "tone" to tone) }
        val data =
            mapOf(
                "state" to "present",
                "text" to null,
                "usableNow" to listOf("personal"),
                "accounts" to
                    listOf(
                        mapOf(
                            "account" to "personal",
                            "fiveHour" to "5h 63%",
                            "rows" to listOf(listOf(cell("personal", "plain"), cell("订阅", "plain")), listOf(cell("5h", "plain"), cell("63%", "warn"))),
                            "warm" to mapOf("act" to "wait", "text" to "5h 63% ↻18:30"),
                        ),
                    ),
                "unseen" to listOf(mapOf("account" to "work", "fiveHour" to null, "rows" to listOf(listOf(cell("work", "plain"))), "warm" to mapOf("act" to "send", "text" to "未计时"))),
            )
        val a = QuotaRead.of(data)!!
        assertEquals(listOf("personal", "work"), a.accounts.map { it.account })
        assertEquals(Toned("63%", Tone.WARN), a.accounts[0].rows[1][1])
        assertEquals("5h 63%", a.accounts[0].fiveHour)
        assertEquals("未计时", a.accounts[1].warmText)
        assertNull("行里一格缺 text ⇒ 解不出", QuotaRead.of(data + ("unseen" to listOf(mapOf("account" to "w", "rows" to listOf(listOf(mapOf("tone" to "plain"))))))))
    }
}

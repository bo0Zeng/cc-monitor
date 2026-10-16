package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.Hidden
import com.ccmonitor.mobile.core.claude.link.wire.Hide
import com.ccmonitor.mobile.core.claude.link.wire.NeedsRow
import com.ccmonitor.mobile.core.claude.link.wire.Tone
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
        assertFalse("需手动不算在跑", t.sessions.getValue("s1").running)
        val t2 = SessionTable().step(least, breaks::add)
        // 最少格：那台判不出在干什么（`activity` 缺席）⇒ 核心照实写「状态不明」、语气 plain；不算在跑（停止键不亮）。
        assertEquals(Toned("状态不明", Tone.PLAIN), t2.sessions.getValue("s1").activity)
        assertFalse("判不出不算在跑", t2.sessions.getValue("s1").running)
        assertTrue(breaks.toString(), breaks.isEmpty())
    }

    @Test
    fun `会话表：status 改字、removed 拿掉、replayed 记下、丢了会话帧要重接`() {
        val breaks = mutableListOf<String>()
        var t = SessionTable().step(golden("session_added")[0], breaks::add)
        t = t.step(golden("session_status")[1], breaks::add)
        assertEquals(Toned("状态不明", Tone.PLAIN), t.sessions.getValue("s1").activity)
        assertFalse("判不出不算在跑", t.sessions.getValue("s1").running)
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
    fun `会话表：状态帧带着记录路径（宣告以后记录才出现 · 换了一份）⇒ 那一条的路径换成它；不带就不动`() {
        val breaks = mutableListOf<String>()
        val bare = golden("session_added")[0] - "path"
        var t = SessionTable().step(bare, breaks::add)
        assertNull("宣告时记录还没有", t.sessions.getValue("s1").path)
        val withPath = golden("session_status").first { it.containsKey("path") }
        t = t.step(withPath, breaks::add)
        assertEquals(withPath.str("path"), t.sessions.getValue("s1").path)
        t = t.step(golden("session_status").first { !it.containsKey("path") }, breaks::add)
        assertEquals("不带路径的状态帧不抹掉已有的", withPath.str("path"), t.sessions.getValue("s1").path)
        assertTrue(breaks.toString(), breaks.isEmpty())
    }

    @Test
    fun `会话表：能不能换照帧上的 switch 抄（宣告 · 状态 · 离开活），被顶替的记下顶替它的那一条`() {
        val breaks = mutableListOf<String>()
        var t = SessionTable().step(golden("session_added")[0], breaks::add)
        assertEquals(Switches(CanSwitch("先答完需手动"), CanSwitch("先答完需手动")), t.sessions.getValue("s1").switch)
        t = t.step(golden("session_status")[1], breaks::add)
        assertEquals(Switches(CanSwitch(null), CanSwitch(null)), t.sessions.getValue("s1").switch)
        assertTrue(
            t.sessions
                .getValue("s1")
                .switch!!
                .model.can,
        )
        t = t.step(golden("session_state")[0], breaks::add)
        assertEquals(
            CanSwitch(null),
            t.fates
                .getValue("s1")
                .switch
                ?.model,
        )
        val (superseded, gone) = golden("session_removed")
        assertEquals(mapOf("s1" to "s2"), t.step(superseded, breaks::add).successors)
        assertTrue("不是顶替 ⇒ 不记", t.step(gone, breaks::add).successors.isEmpty())
        assertNull("认不出的形 ⇒ 不猜", Switches.of(mapOf("model" to mapOf("can" to "no"), "permission" to mapOf("can" to "yes"))))
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
    fun `history-list：行上隐藏那一格照收（后端写好的那一句）；入参只在「显示已隐藏」开着时带 hidden`() {
        val rows = listOf(mapOf("sessionId" to "s", "label" to "l", "atText" to "", "sectionText" to "", "projectPath" to "", "jsonlPath" to "", "agent" to "claude", "status" to "ended", "hidden" to mapOf("by" to "child", "said" to "由 a 启动 · 子会话默认隐藏")))
        assertEquals(
            Hidden("由 a 启动 · 子会话默认隐藏"),
            HistoryList
                .ofRows(rows)
                ?.rows
                ?.single()
                ?.hidden,
        )
        assertNull(
            HistoryList
                .of(Fixtures.json("history-list.golden.json"))!!
                .rows
                .first()
                .hidden,
        )
        assertEquals(mapOf("hidden" to true), HistoryList.args(showHidden = true))
        assertNull("关着 ⇒ 不带（缺省不出）", HistoryList.args(showHidden = false))
    }

    @Test
    fun `history-annotate：单个那一格交 on · off，null 交 rule（回到规则）；回包的 prev 原样读出，撤销照它交回去`() {
        assertEquals(mapOf("sid" to "s1", "patch" to mapOf("hide" to "on")), HistoryAnnotate.hide("s1", Hide.ON))
        assertEquals(mapOf("sid" to "s1", "patch" to mapOf("hide" to "off")), HistoryAnnotate.hide("s1", Hide.OFF))
        assertEquals(mapOf("sid" to "s1", "patch" to mapOf("hide" to "rule")), HistoryAnnotate.hide("s1", null))
        assertEquals(Hide.OFF, HistoryAnnotate.of(mapOf("entry" to mapOf("starred" to false), "prev" to "off"))?.prev)
        val none = HistoryAnnotate.of(mapOf("entry" to mapOf("starred" to false), "prev" to null))
        assertNotNull(none)
        assertNull(none!!.prev)
        assertNull("认不出的字 ⇒ 解不出", HistoryAnnotate.of(mapOf("entry" to emptyMap<String, Any>(), "prev" to "maybe")))
    }

    @Test
    fun `sessions-needs：照核心的先后，不另排；字照抄，已等读毫秒`() {
        val data =
            mapOf(
                "waiting" to
                    listOf(
                        mapOf("sid" to "b", "needs" to mapOf("kind" to "approve", "text" to "等批准 · Bash", "head" to "等批准 · Bash", "tone" to "need", "rank" to 0.0, "waitedMs" to 180000.0, "waitedText" to "3m")),
                        mapOf("sid" to "a", "needs" to mapOf("kind" to "answer", "text" to "等回答", "head" to "等回答", "tone" to "need", "rank" to 1.0, "waitedMs" to null, "waitedText" to null)),
                    ),
            )
        val rows = SessionsNeeds.of(data)!!
        assertEquals(listOf("b", "a"), rows.map { it.sid })
        assertEquals("等批准 · Bash", rows[0].needs.text)
        assertEquals("已等那一格读毫秒（手机按此刻走表），不抄收到那一刻写好的字", 180_000L, rows[0].needs.waitedMs)
        assertEquals(null, rows[1].needs.waitedMs)
        assertEquals(emptyList<NeedsRow>(), SessionsNeeds.of(mapOf("waiting" to emptyList<Any>())))
        assertNull("缺 text ⇒ 契约对不上", SessionsNeeds.of(mapOf("waiting" to listOf(mapOf("sid" to "a", "needs" to mapOf("kind" to "x", "head" to "需手动", "tone" to "need", "rank" to 0.0))))))
        assertNull("缺 head（钉条第一行）⇒ 契约对不上", SessionsNeeds.of(mapOf("waiting" to listOf(mapOf("sid" to "a", "needs" to mapOf("kind" to "x", "text" to "需手动", "tone" to "need", "rank" to 0.0))))))
    }
}

package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.Hidden
import com.ccmonitor.mobile.core.claude.link.wire.Hide
import com.ccmonitor.mobile.core.claude.link.wire.NeedsRow
import com.ccmonitor.mobile.core.claude.link.wire.SessionDot
import com.ccmonitor.mobile.core.claude.link.wire.SessionFace
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.claude.link.wire.Wire
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
    fun `会话表：金样里 session_added 全格与最少格都折得进来，脸照那台给的那一份`() {
        val breaks = mutableListOf<String>()
        val (full, least) = golden("session_added").take(2)
        val t = SessionTable().step(full, breaks::add)
        assertEquals(SessionDot.NEEDS_YOU, t.faces.getValue("s1").dot)
        assertEquals(Toned("需手动", Tone.NEED), t.faces.getValue("s1").let { Toned(it.name, it.tone) })
        assertEquals("/p/s1.jsonl", t.sessions.getValue("s1").path)
        assertFalse("需手动不算在跑", t.running("s1"))
        val t2 = SessionTable().step(least, breaks::add)
        // 最少格：那台判不出在干什么（脸上 `live` 不带 `activity`）⇒ 「状态不明」；不算在跑（停止键不亮）。
        assertEquals("状态不明", t2.faces.getValue("s1").name)
        assertNull(
            t2.faces
                .getValue("s1")
                .live.activity,
        )
        assertFalse("判不出不算在跑", t2.running("s1"))
        assertTrue(breaks.toString(), breaks.isEmpty())
    }

    @Test
    fun `会话表：同一条后到的脸覆盖先到的（宣告 → 状态 → 离开活 → 又宣告），摘掉了 ⇒ 脸一并拿掉`() {
        val breaks = mutableListOf<String>()
        var t = SessionTable().step(golden("session_added")[0], breaks::add)
        t = t.step(golden("session_status")[1], breaks::add)
        assertEquals("状态不明", t.faces.getValue("s1").name)
        assertEquals("/p/s1.jsonl", t.sessions.getValue("s1").path) // 状态帧不抹掉宣告时的格
        t = t.step(golden("sessions_replayed")[0], breaks::add)
        assertTrue(t.replayed)
        t = t.step(golden("session_removed")[1], breaks::add)
        assertTrue(t.sessions.isEmpty())
        assertNull("摘掉了、去向还没裁 ⇒ 流上没有它的脸", t.faces["s1"])
        t = t.step(golden("session_state")[0], breaks::add)
        assertEquals(SessionDot.EXITED, t.faces.getValue("s1").dot)
        assertFalse("离开活的不进活会话表", "s1" in t.sessions)
        t = t.step(golden("session_state")[1], breaks::add)
        assertEquals(SessionDot.ENDED, t.faces.getValue("s1").dot)
        t = t.step(golden("session_added")[0], breaks::add)
        assertEquals("又宣告 ⇒ 换回活的那一份", SessionDot.NEEDS_YOU, t.faces.getValue("s1").dot)
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
    fun `会话表：能不能换照脸上的 can 抄（宣告 · 状态 · 离开活），被顶替的记下顶替它的那一条`() {
        val breaks = mutableListOf<String>()
        var t = SessionTable().step(golden("session_added")[0], breaks::add)
        assertEquals(
            "先答完需手动",
            t.faces
                .getValue("s1")
                .can.model
                .blocked(),
        )
        assertEquals(
            "先答完需手动",
            t.faces
                .getValue("s1")
                .can.permission
                .blocked(),
        )
        t = t.step(golden("session_status")[1], breaks::add)
        assertNull(
            "能换 ⇒ 没有那一句",
            t.faces
                .getValue("s1")
                .can.model
                .blocked(),
        )
        t = t.step(golden("session_state")[0], breaks::add)
        assertNull(
            "离开「活」两样都能换（记在注解上、下次恢复用上）",
            t.faces
                .getValue("s1")
                .can.model
                .blocked(),
        )
        val (superseded, gone) = golden("session_removed")
        assertEquals(mapOf("s1" to "s2"), t.step(superseded, breaks::add).successors)
        assertTrue("不是顶替 ⇒ 不记", t.step(gone, breaks::add).successors.isEmpty())
        assertTrue(breaks.toString(), breaks.isEmpty())
    }

    @Test
    fun `整份脸金样每一形都装得进，点 · 短名 · 语气 · 看不见时作不作数照抄`() {
        val all = Fixtures.json("session-face.golden.json")
        assertTrue("金样里一形都没有 —— 本条在空转", all.isNotEmpty())
        for ((key, raw) in all) {
            val f = Wire.bind<SessionFace>(raw)
            assertNotNull("$key 装不进", f)
            @Suppress("UNCHECKED_CAST")
            val m = raw as Map<String, Any?>
            assertEquals(key, m["name"], f!!.name)
            assertEquals(key, m["settled"], f.settled)
        }
        assertEquals("线上那八颗点都在金样里", SessionDot.entries.toSet(), all.values.mapNotNull { Wire.bind<SessionFace>(it)?.dot }.toSet())
    }

    @Test
    fun `会话表：那份记录从头重读了 ⇒ 那一条换一代（读过的作废、整份重来），别的不动`() {
        val breaks = mutableListOf<String>()
        val reread = golden("session_file_reread")[0]
        var t = SessionTable().step(golden("session_added")[0], breaks::add)
        assertEquals(0L, t.generation("s1"))
        t = t.step(reread, breaks::add)
        assertEquals(1L, t.generation("s1"))
        t = t.step(reread, breaks::add)
        assertEquals(2L, t.generation("s1"))
        assertEquals(0L, t.generation("s2"))
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
    fun `history-list：行上隐藏那一格照收（后端写好的那一句）`() {
        val rows = listOf(mapOf("sessionId" to "s", "label" to "l", "at" to 0, "atText" to "", "sectionText" to "", "starred" to false, "projectName" to "", "projectPath" to "", "group" to "g", "can" to mapOf("delete" to "yes"), "jsonlPath" to "", "agent" to "claude", "face" to Fixtures.face("ended"), "hidden" to mapOf("by" to "child", "said" to "由 a 启动 · 子会话默认隐藏")))
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
                "answered" to emptyList<Any>(),
                "waiting" to
                    listOf(
                        mapOf("sid" to "b", "label" to "标题 b", "face" to Fixtures.face("live-needs-you-tmux"), "needs" to mapOf("kind" to "approve", "text" to "等批准 · Bash", "head" to "等批准 · Bash", "tone" to "need", "rank" to 0.0, "waitedMs" to 180000.0, "waitedText" to "3m")),
                        mapOf("sid" to "a", "label" to "标题 a", "face" to Fixtures.face("live-needs-you-tmux"), "needs" to mapOf("kind" to "answer", "text" to "等回答", "head" to "等回答", "tone" to "need", "rank" to 1.0, "waitedMs" to null, "waitedText" to null)),
                    ),
            )
        val rows = SessionsNeeds.of(data)!!
        assertEquals(listOf("b", "a"), rows.map { it.sid })
        assertEquals("等批准 · Bash", rows[0].needs.text)
        assertEquals("已等那一格读毫秒（手机按此刻走表），不抄收到那一刻写好的字", 180_000L, rows[0].needs.waitedMs)
        assertEquals(null, rows[1].needs.waitedMs)
        assertEquals(emptyList<NeedsRow>(), SessionsNeeds.of(mapOf("waiting" to emptyList<Any>(), "answered" to emptyList<Any>())))
        assertNull("缺 text ⇒ 契约对不上", SessionsNeeds.of(mapOf("waiting" to listOf(mapOf("sid" to "a", "label" to "标题 a", "face" to Fixtures.face("live-needs-you-tmux"), "needs" to mapOf("kind" to "x", "head" to "需手动", "tone" to "need", "rank" to 0.0))))))
        assertNull("缺 head（钉条第一行）⇒ 契约对不上", SessionsNeeds.of(mapOf("waiting" to listOf(mapOf("sid" to "a", "label" to "标题 a", "face" to Fixtures.face("live-needs-you-tmux"), "needs" to mapOf("kind" to "x", "text" to "需手动", "tone" to "need", "rank" to 0.0))))))
    }
}

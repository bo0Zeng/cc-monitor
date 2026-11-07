package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.rawPatch
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.Hidden
import com.ccmonitor.mobile.core.wire.gen.HiddenBy
import com.ccmonitor.mobile.core.wire.gen.Hide
import com.ccmonitor.mobile.core.wire.gen.LOCAL
import com.ccmonitor.mobile.core.wire.gen.Life
import com.ccmonitor.mobile.core.wire.gen.NeedsRow
import com.ccmonitor.mobile.core.wire.gen.SessionAdded
import com.ccmonitor.mobile.core.wire.gen.SessionBook
import com.ccmonitor.mobile.core.wire.gen.SessionDot
import com.ccmonitor.mobile.core.wire.gen.SessionFace
import com.ccmonitor.mobile.core.wire.gen.SessionLeft
import com.ccmonitor.mobile.core.wire.gen.Tone
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ProductsTest {
    /** 整份脸金样里的一形。 */
    private fun face(key: String): SessionFace = Wire.bind<SessionFace>(Fixtures.json("session-face.golden.json")[key])!!

    /** 核心折好交来的那一本（折法与它对着的 golden 在 Rust 那边，共享核心 `face-core` 的判据管）：这里只摆成品、钉读法。 */
    private val book: SessionBook =
        EMPTY_BOOK.copy(
            live =
                mapOf(
                    "a" to SessionAdded(sid = "a", agentKind = "claude", path = "/p/a.jsonl", face = face("live-needs-you-tmux"), pid = 1, label = "甲", title = "甲"),
                    "w" to SessionAdded(sid = "w", agentKind = "claude", face = face("live-working-plain"), pid = 2, label = "乙", title = "乙"),
                    "d" to SessionAdded(sid = "d", agentKind = "claude", face = face("live-unclear"), pid = 3, label = "丁", title = "丁"),
                ),
            left = mapOf("c" to SessionLeft("c", face("ended"))),
            rereads = mapOf("a" to 1L),
            fileSaid = mapOf("a" to "甲句"),
        )

    @Test
    fun `会话簿：记录文件那一句照核心记的原样读，没说过 ⇒ 没有`() {
        assertEquals("甲句", book.fileNotice("a"))
        assertNull(book.fileNotice("d"))
    }

    @Test
    fun `会话簿：脸照活的那一张 · 走了的照去向那一张，簿里没有 ⇒ 没有`() {
        assertEquals(Toned("需手动", Tone.NEED), book.face("a")?.let { Toned(it.name, it.tone) })
        assertEquals(SessionDot.ENDED, book.face("c")?.dot)
        assertNull(book.face("zz"))
        assertEquals(setOf("a", "w", "d", "c"), book.faces().keys)
        val needs = book.face("a")!!.can
        assertEquals("先答完需手动", needs.model.blocked())
    }

    @Test
    fun `会话簿：停止键只照那台说的停得下；重读过几回照核心记的`() {
        assertTrue(book.canStop("w"))
        assertFalse("需手动停不下", book.canStop("a"))
        assertNull((book.face("d")!!.life as? Life.Live)?.activity)
        assertFalse("判不出停不下", book.canStop("d"))
        assertFalse("走了的停不下", book.canStop("c"))
        assertFalse("不在 ⇒ 不算", book.canStop("zz"))
        assertEquals(1L, book.generation("a"))
        assertEquals(0L, book.generation("d"))
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
    fun `history-list：金样整份解得出，标题 · 段头 · 时刻都是核心写好的`() {
        val a = HistoryList.of(Fixtures.json("history-list.golden.json"))
        assertNotNull(a)
        assertEquals(5, a!!.rows.size)
        val r = a.rows.first()
        assertEquals("从父会话分出来的", r.label)
        assertEquals("今天", r.sectionText)
        assertEquals("00:00", r.atText)
        assertEquals("行照实写是哪台答的", "dev", r.origin)
        assertEquals("/h/.claude/projects/-w/0000aaaa-0000-4000-8000-000000000005.jsonl", r.jsonlPath)
    }

    @Test
    fun `history-list：缺 label 的行 ⇒ 整份解不出，不拿 sid 顶标题`() {
        assertNull(HistoryList.of(whole(listOf(mapOf("sessionId" to "s")))))
    }

    @Test
    fun `history-list：不是整份（带记号那一问的答）⇒ of 不当整份收；窗外还有 ⇒ truncated`() {
        val golden = Fixtures.json("history-list.golden.json") as Map<*, *>
        assertNull(HistoryList.of(golden + ("whole" to false)))
        val whole = rawPatch(golden)!!
        val head = whole.head!!
        assertEquals(false, HistoryList.page(whole)!!.truncated)
        assertEquals(true, HistoryList.page(whole.copy(head = head.copy(total = head.total + 1)))!!.truncated)
        assertEquals(mapOf("origin" to LOCAL, "held" to "h1"), HistoryList.args(ListAsk(), "h1"))
    }

    /** 不带记号那一问的整份回包（行照给的先后）。 */
    private fun whole(rows: List<Map<String, Any?>>): Map<String, Any?> =
        mapOf(
            "held" to "h",
            "whole" to true,
            "same" to false,
            "order" to rows.map { it["sessionId"] },
            "up" to rows.associateBy { it["sessionId"] },
            "head" to mapOf("total" to rows.size, "info" to mapOf("groups" to emptyList<Any?>(), "notice" to null)),
        )

    @Test
    fun `history-list：行上隐藏那一格照收（后端写好的那一句）`() {
        val rows = listOf(Fixtures.historyRow("hidden" to mapOf("by" to "child", "said" to "由 a 启动 · 子会话默认隐藏")))
        assertEquals(
            Hidden(HiddenBy.CHILD, "由 a 启动 · 子会话默认隐藏"),
            HistoryList
                .of(whole(rows))
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
        assertEquals(Hide.OFF, HistoryAnnotate.of(mapOf("entry" to mapOf("starred" to false, "customTitle" to null, "updatedAt" to 0), "prev" to "off"))?.prev)
        val none = HistoryAnnotate.of(mapOf("entry" to mapOf("starred" to false, "customTitle" to null, "updatedAt" to 0), "prev" to null))
        assertNotNull(none)
        assertNull(none!!.prev)
        assertNull("认不出的字 ⇒ 解不出", HistoryAnnotate.of(mapOf("entry" to emptyMap<String, Any>(), "prev" to "maybe")))
    }

    @Test
    fun `sessions-needs：照核心的先后，不另排；字照抄，已等读毫秒`() {
        val data =
            mapOf(
                "ended" to emptyList<Any>(),
                "items" to
                    sessionItems(
                        listOf(
                            mapOf("sid" to "b", "label" to "标题 b", "face" to Fixtures.face("live-needs-you-tmux"), "needs" to mapOf("kind" to "approve", "text" to "等批准 · Bash", "head" to "等批准 · Bash", "tone" to "need", "rank" to 0.0, "waitedMs" to 180000.0, "waitedText" to "3m")),
                            mapOf("sid" to "a", "label" to "标题 a", "face" to Fixtures.face("live-needs-you-tmux"), "needs" to mapOf("kind" to "answer", "text" to "等回答", "head" to "等回答", "tone" to "need", "rank" to 1.0, "waitedMs" to null, "waitedText" to null)),
                        ),
                    ),
            )
        val rows = SessionsNeeds.of(data)!!
        assertEquals(listOf("b", "a"), rows.map { it.sid })
        assertEquals("等批准 · Bash", rows[0].needs.text)
        assertEquals("已等那一格读毫秒（手机按此刻走表），不抄收到那一刻写好的字", 180_000L, rows[0].needs.waitedMs)
        assertEquals(null, rows[1].needs.waitedMs)
        assertEquals(emptyList<NeedsRow>(), SessionsNeeds.of(mapOf("items" to sessionItems(emptyList()), "ended" to emptyList<Any>())))
        assertNull("缺 text ⇒ 契约对不上", SessionsNeeds.of(mapOf("items" to sessionItems(listOf(mapOf("sid" to "a", "label" to "标题 a", "face" to Fixtures.face("live-needs-you-tmux"), "needs" to mapOf("kind" to "x", "head" to "需手动", "tone" to "need", "rank" to 0.0)))))))
        assertNull("缺 head（钉条第一行）⇒ 契约对不上", SessionsNeeds.of(mapOf("items" to sessionItems(listOf(mapOf("sid" to "a", "label" to "标题 a", "face" to Fixtures.face("live-needs-you-tmux"), "needs" to mapOf("kind" to "x", "text" to "需手动", "tone" to "need", "rank" to 0.0)))))))
    }

    @Test
    fun `groups-here：问这台自己那一片、不带参数；照线上那一形解得开 ⇒ 原样交回（交核心合），缺格解不开`() {
        assertEquals(emptyMap<String, Any?>(), Groups.ARGS)
        val member =
            mapOf(
                "sid" to "a",
                "groups" to listOf("g1"),
                "pinned" to false,
                "face" to Fixtures.face("ended"),
                "facts" to mapOf("title" to "标题 a", "cwd" to "/w", "record" to null, "account" to null, "bg" to false, "at" to 1L),
            )
        val slice = mapOf("groups" to listOf(mapOf("id" to "g1", "name" to "开发", "made" to 1L, "named" to 1L)), "members" to listOf(member))
        assertEquals(slice, Groups.of(slice))
        assertNull("缺 facts ⇒ 契约对不上", Groups.of(mapOf("groups" to emptyList<Any>(), "members" to listOf(member - "facts"))))
    }
}

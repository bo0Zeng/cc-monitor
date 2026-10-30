package com.ccmonitor.mobile.ui.chats

import com.ccmonitor.mobile.cache.ListCache
import com.ccmonitor.mobile.core.claude.link.RawJson
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.ui.Faces
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 会话屏那块板：勾上的每台各一份清单、筛选改一项各台重问、手指在滚时对外那一份不换。 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatsBoardTest {
    private fun rowMap(
        sid: String,
        at: Long,
    ) = mapOf(
        "sessionId" to sid,
        "label" to sid,
        "at" to at,
        "atText" to "09:00",
        "sectionText" to "今天",
        "starred" to false,
        "projectName" to "w",
        "projectPath" to "/w",
        "group" to "/w",
        "jsonlPath" to "/w/$sid.jsonl",
        "agent" to "claude",
        "agentTag" to null,
        "face" to Faces.raw["ended"],
        "hidden" to null,
        "forkedFromSessionId" to null,
        "forkedFrom" to null,
        "can" to mapOf("resume" to "yes", "accounts" to false, "fork" to false, "delete" to "yes"),
        "aiTitle" to null,
        "firstUserExcerpt" to "",
        "title" to sid,
        "startedAt" to at,
        "updatedAt" to at,
        "messageCountApprox" to 0,
        "isBg" to false,
        "customTitle" to null,
        "projectDir" to "/w",
        "untitled" to false,
        "spanText" to "09:00",
    )

    private class Host(
        val name: HostName,
    ) {
        val link = FakeCoreLink()
        var rows: List<Map<String, Any?>> = emptyList()

        /** `history-search` 的回包。 */
        var search: Reply = Reply.LinkDown

        /** `groups-read` 的回包（缺省：没有组、没有固定）。 */
        var groups: Reply = Reply.Ok(mapOf("groups" to emptyList<Any>(), "pinned" to emptyList<Any>(), "machines" to emptyList<Any>()))

        init {
            link.answer = { cmd, _ ->
                when (cmd) {
                    // 那台每次答整份（带不带记号都一样：判据只看屏上换没换）。
                    "history-list" ->
                        Reply.Ok(
                            mapOf(
                                "held" to "h${rows.size}",
                                "whole" to true,
                                "same" to false,
                                "order" to rows.map { it["sessionId"] },
                                "up" to rows.associateBy { it["sessionId"] },
                                "head" to mapOf("total" to rows.size, "info" to mapOf("groups" to emptyList<Any>(), "notice" to null)),
                            ),
                        )
                    "history-search" -> search
                    "groups-read" -> groups
                    else -> Reply.LinkDown
                }
            }
        }
    }

    /** 核心合一份那一步的替身：记下交进来的那一行，答 [out]（核心那一份另有 Rust 判据管）。 */
    private class Merge {
        val got = mutableListOf<String>()
        var out = ""

        fun run(input: String): String {
            got += input
            return out
        }
    }

    private fun TestScope.board(
        vararg hosts: Host,
        merge: Merge = Merge(),
    ): ChatsBoard {
        val by = hosts.associateBy { it.name.id }
        return ChatsBoard(hosts.first().name.id, MutableStateFlow(hosts.map { it.name }), { h, s ->
            HostChats(h.id, h.label, by.getValue(h.id).link, emptyFlow(), emptyFlow(), MutableStateFlow(null), {}, ListCache.None, s)
        }, merge::run, backgroundScope)
    }

    /** `groups-read` 回包里的一条组员：那台写好的标题 ＋ 那张脸（结构夹具）。 */
    private fun member(
        sid: String,
        pinned: Boolean = false,
    ) = mapOf(
        "origin" to "<local>",
        "sid" to sid,
        "pinned" to pinned,
        "face" to Faces.raw["ended"],
        "facts" to mapOf("title" to "标题 $sid", "cwd" to "/w", "record" to null, "account" to null, "bg" to false, "at" to 1L),
    )

    private fun groupsOf(
        groups: List<Map<String, Any?>>,
        pinned: List<Map<String, Any?>> = emptyList(),
        read: Map<String, Any?> = mapOf("is" to "ok", "notice" to null),
    ) = Reply.Ok(mapOf("groups" to groups, "pinned" to pinned, "machines" to listOf(mapOf("origin" to "<local>", "read" to read))))

    private fun group(
        id: String,
        title: String,
        members: List<Map<String, Any?>>,
        counts: List<String>,
    ) = mapOf(
        "id" to id,
        "name" to title,
        "title" to title,
        "made" to 1L,
        "named" to 1L,
        "known" to mapOf("is" to "now"),
        "members" to members,
        "counts" to counts.map { mapOf("dot" to "needs-you", "n" to 1L, "word" to "需手动", "text" to it) },
    )

    @Test
    fun `按组分段：固定的一段在最前，再一组一段（段头照那台写好的组名与计数），段里是组员；下面按天的不再重出`() =
        runTest {
            val a = Host(HostName("h1", "a"))
            a.rows = listOf(rowMap("s1", 3), rowMap("s2", 2), rowMap("s3", 1))
            a.groups = groupsOf(listOf(group("g1", "开发", listOf(member("s1"), member("s9")), listOf("需手动 1"))), pinned = listOf(member("s2", pinned = true)))
            val board = board(a)
            runCurrent()
            val ui = board.ui.value
            assertEquals(listOf("s2"), ui.pinned.map { it.sessionId })
            val band = ui.bands.single()
            assertEquals("开发", band.title)
            assertEquals(listOf("需手动 1"), band.heads)
            assertEquals(listOf("s1", "s9"), band.rows.map { it.sessionId })
            assertEquals("清单里没有的那一条照组员那一行写", listOf("s1", "标题 s9"), band.rows.map { it.label })
            assertEquals(listOf("s3"), ui.sections.flatMap { s -> s.rows.map { it.sessionId } })
            board.filters { it.copy(withinDays = 7) }
            runCurrent()
            assertEquals("筛着 ⇒ 不分段", emptyList<GroupBand>(), board.ui.value.bands)
            assertEquals(
                listOf("s1", "s2", "s3"),
                board.ui.value.rows
                    .map { it.sessionId },
            )
        }

    @Test
    fun `那台说组那一片读不到 ⇒ 那一句进注解行，不静默丢`() =
        runTest {
            val a = Host(HostName("h1", "a"))
            a.groups = groupsOf(emptyList(), read = mapOf("is" to "failed", "why" to "读不到注解"))
            val board = board(a)
            runCurrent()
            assertEquals(listOf("读不到注解"), board.ui.value.notices)
        }

    @Test
    fun `筛选改一项 ⇒ 勾上的每台照新的那几格重问；去掉一台 ⇒ 不再问它`() =
        runTest {
            val a = Host(HostName("h1", "a"))
            val b = Host(HostName("h2", "b"))
            val board = board(a, b)
            runCurrent()
            board.filters { it.copy(withinDays = 7) }
            runCurrent()
            assertEquals(mapOf("within_days" to 7), a.link.calls("history-list").last())
            assertEquals(mapOf("within_days" to 7), b.link.calls("history-list").last())
            board.filters { it.copy(unchecked = setOf("h2")) }
            runCurrent()
            val before = b.link.calls("history-list").size
            board.filters { it.copy(hidden = true) }
            runCurrent()
            assertEquals("去掉的那台不再问", before, b.link.calls("history-list").size)
            assertEquals(mapOf("within_days" to 7, "hidden" to true), a.link.calls("history-list").last())
            assertEquals(listOf("h1"), board.checked.value.map { it.hostId })
        }

    @Test
    fun `搜标题停手 300 ms 才问`() =
        runTest {
            val a = Host(HostName("h1", "a"))
            val board = board(a)
            runCurrent()
            board.query("登")
            board.query("登录")
            advanceTimeBy(ChatsBoard.QUERY_DEBOUNCE_MS - 1)
            runCurrent()
            assertEquals(null, a.link.calls("history-list").last())
            advanceTimeBy(2)
            runCurrent()
            assertEquals(mapOf("query" to "登录"), a.link.calls("history-list").last())
        }

    @Test
    fun `按着时对外那一份不换，松手换成最新的`() =
        runTest {
            val a = Host(HostName("h1", "a"))
            a.rows = listOf(rowMap("s1", 1))
            val board = board(a)
            runCurrent()
            assertEquals(
                listOf("s1"),
                board.ui.value.rows
                    .map { it.sessionId },
            )
            board.hold()
            a.rows = listOf(rowMap("s2", 2), rowMap("s1", 1))
            board.chats("h1")!!.fetch()
            runCurrent()
            assertEquals(
                "按着 ⇒ 还是旧的",
                listOf("s1"),
                board.ui.value.rows
                    .map { it.sessionId },
            )
            board.release()
            assertEquals(
                listOf("s2", "s1"),
                board.ui.value.rows
                    .map { it.sessionId },
            )
        }

    /**
     * 一台 `history-search` 的回包，照帧客户端读进来的那一形（[RawJson.read]：数 ⇒ `Double`）——真形里的整数格 `updatedAt` · `face.order` 都在；
     * 放一格手机的类不认的（`spanText`），交核心时要原样还在。
     */
    private fun hits(sid: String) =
        RawJson.read(
            """{"sessions":[{"sessionId":"$sid","updatedAt":1767225600123,"face":{"order":5},"hits":[],"moreHits":1,"spanText":"keep-$sid"}],"unreadable":2,"skipped":["Codex"]}""",
        )

    /** 核心合好的那一行（替身）。 */
    private fun mergedOf(vararg rows: Pair<String, String>) =
        RawJson.write(
            mapOf(
                "totalHits" to rows.size,
                "sessionCount" to rows.size,
                "truncated" to false,
                "unreadable" to 4,
                "skipped" to listOf("Codex"),
                "down" to emptyList<Any>(),
                "summary" to "核心·结果头",
                "sessions" to
                    rows.map { (origin, sid) ->
                        mapOf(
                            "origin" to origin,
                            "atText" to "09:00",
                            "spanText" to "09:00",
                            "label" to sid,
                            "starred" to false,
                            "hidden" to null,
                            "agent" to "claude",
                            "sessionId" to sid,
                            "projectPath" to "/w",
                            "projectName" to "w",
                            "jsonlPath" to "/w/$sid.jsonl",
                            "title" to sid,
                            "updatedAt" to 1,
                            "hits" to emptyList<Any>(),
                            "moreHits" to 1,
                            "hitsTruncated" to false,
                            "isBg" to false,
                            "can" to mapOf("resume" to "yes", "accounts" to false, "fork" to false, "delete" to "yes"),
                            "face" to Faces.raw["ended"],
                        )
                    },
            ),
        )

    @Test
    fun `搜内容：各台回包原样交核心合（每台写它的 hostId，整数照整数写），不交哪一台当信差；合好的行按 origin 对回哪台`() =
        runTest {
            val a = Host(HostName("h1", "a"))
            val b = Host(HostName("h2", "gpu"))
            a.search = Reply.Ok(hits("s1"))
            b.search = Reply.Ok(hits("s2"))
            val merge = Merge().apply { out = mergedOf("h2" to "s2", "h1" to "s1") }
            val board = board(a, b, merge = merge)
            runCurrent()
            board.query("登录")
            advanceTimeBy(ChatsBoard.QUERY_DEBOUNCE_MS + 1)
            runCurrent()
            board.searchContent()
            runCurrent()
            assertEquals(mapOf("query" to "登录"), b.link.calls("history-search").single())
            assertEquals("哪一台都没被当成信差", emptyList<Any?>(), a.link.calls("history-search-merge") + b.link.calls("history-search-merge"))
            val input = merge.got.single()
            assertEquals(
                """{"q":"登录","answered":[{"origin":"h1","reply":{"sessions":[{"sessionId":"s1","updatedAt":1767225600123,"face":{"order":5},"hits":[],"moreHits":1,"spanText":"keep-s1"}],"unreadable":2,"skipped":["Codex"]}},""" +
                    """{"origin":"h2","reply":{"sessions":[{"sessionId":"s2","updatedAt":1767225600123,"face":{"order":5},"hits":[],"moreHits":1,"spanText":"keep-s2"}],"unreadable":2,"skipped":["Codex"]}}]}""",
                input,
            )
            val c = board.ui.value.content!!
            assertEquals("结果头照核心写好的那一句", "核心·结果头", c.summary)
            assertEquals(listOf("h2" to "s2", "h1" to "s1"), c.rows.map { it.hostId to it.row.sessionId })
            assertEquals(listOf("gpu", null), c.rows.map { it.machineTag })
            assertEquals(
                listOf(copyText("history.search.unreadable", "n" to 4), copyText("history.search.skipped", "agent" to "Codex")),
                c.lines,
            )
        }

    @Test
    fun `搜内容：一台没答 ⇒ 那台离线 · 未搜索那一句、不交核心，别的台的行照出`() =
        runTest {
            val a = Host(HostName("h1", "a"))
            val b = Host(HostName("h2", "gpu"))
            a.search = Reply.TimedOut
            b.search = Reply.Ok(hits("s2"))
            val merge = Merge().apply { out = mergedOf("h2" to "s2") }
            val board = board(a, b, merge = merge)
            runCurrent()
            board.query("x")
            advanceTimeBy(ChatsBoard.QUERY_DEBOUNCE_MS + 1)
            runCurrent()
            board.searchContent()
            runCurrent()
            assertEquals(listOf("h2"), Regex(""""origin":"([^"]+)"""").findAll(merge.got.single()).map { it.groupValues[1] }.toList())
            val c = board.ui.value.content!!
            assertEquals(listOf("h2" to "s2"), c.rows.map { it.hostId to it.row.sessionId })
            assertEquals(copyText("history.search.machineDown", "machine" to "a"), c.lines.first())
            board.query("xy")
            runCurrent()
            assertNull("换了词 ⇒ 内容结果收掉", board.ui.value.content)
        }
}

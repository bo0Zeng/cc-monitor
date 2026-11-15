package com.ccmonitor.mobile.ui.chats

import com.ccmonitor.mobile.cache.ListCache
import com.ccmonitor.mobile.core.claude.link.ListSort
import com.ccmonitor.mobile.core.claude.link.RawJson
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.Failed
import com.ccmonitor.mobile.core.wire.gen.LOCAL
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
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

/** 会话屏那块板：勾上的每台各一份清单、筛选改一项各台重问、手指在滚时对外那一份不换。 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatsBoardTest {
    private fun rowMap(
        sid: String,
        at: Long,
        parent: String? = null,
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
        "forkedFromSessionId" to parent,
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
        "origin" to LOCAL,
        "reveal" to false,
        "deleteAsk" to "",
    )

    private class Host(
        val name: HostName,
    ) {
        val link = FakeCoreLink()
        var rows: List<Map<String, Any?>> = emptyList()

        /** `groups-here` 的回包（缺省：没有组、没有带组或固定的会话）。 */
        var groups: Reply = Reply.Ok(mapOf("groups" to emptyList<Any>(), "members" to emptyList<Any>()))

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
                    "groups-here" -> groups
                    else -> Reply.LinkDown
                }
            }
        }
    }

    /** 核心问一圈 ＋ 合那一步的替身：记下交进来的那一行，答 [out]（逐台问与合另有 Rust 判据管）。 */
    private class Merge {
        val got = mutableListOf<String>()
        var out = ""

        fun run(input: String): String {
            got += input
            return out
        }
    }

    /** 第 i 台那条流的编号 ＝ 10 ＋ i。 */
    private fun TestScope.board(
        vararg hosts: Host,
        merge: Merge = Merge(),
    ): ChatsBoard {
        val by = hosts.associateBy { it.name.id }
        val handle = hosts.mapIndexed { i, h -> h.name.id to 10L + i }.toMap()
        return ChatsBoard(hosts.first().name.id, MutableStateFlow(hosts.map { it.name }), { h, s ->
            HostChats(h.id, h.label, by.getValue(h.id).link, emptyFlow(), emptyFlow(), MutableStateFlow(null), {}, ListCache.None, s) { handle.getValue(h.id) }
        }, merge::run, backgroundScope)
    }

    /** `groups-here` 回包里一条带组或固定的会话：在哪几组 ＋ 那台写好的标题 ＋ 那张脸（结构夹具）。 */
    private fun member(
        sid: String,
        groups: List<String> = emptyList(),
        pinned: Boolean = false,
        face: String = "ended",
    ) = mapOf(
        "sid" to sid,
        "groups" to groups,
        "pinned" to pinned,
        "face" to Faces.raw[face],
        "facts" to mapOf("title" to "标题 $sid", "cwd" to "/w", "record" to null, "account" to null, "bg" to false, "at" to 1L),
    )

    /** 那台那一片：这几组的记录 ＋ 这几条会话。 */
    private fun slice(
        groups: List<Map<String, Any?>>,
        members: List<Map<String, Any?>>,
    ) = Reply.Ok(mapOf("groups" to groups, "members" to members))

    private fun group(
        id: String,
        name: String,
        named: Long = 1L,
    ) = mapOf("id" to id, "name" to name, "made" to 1L, "named" to named)

    @Test
    fun `按组分段：固定的一段在最前，再一组一段（段头照那台写好的组名与计数），段里是组员；下面按天的不再重出`() =
        runTest {
            val a = Host(HostName("h1", "a"))
            a.rows = listOf(rowMap("s1", 3), rowMap("s2", 2), rowMap("s3", 1))
            a.groups = slice(listOf(group("g1", "开发")), listOf(member("s1", listOf("g1"), face = "live-needs-you-tmux"), member("s9", listOf("g1")), member("s2", pinned = true)))
            val board = board(a)
            runCurrent()
            val ui = board.ui.value
            assertEquals(listOf("s2"), ui.pinned.map { it.sessionId })
            val band = ui.bands.single()
            assertEquals("开发", band.title)
            assertEquals("组头那几句是核心数好写好的", listOf(copyText("beGroups.count.needs", "n" to "1")), band.heads)
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
    fun `组头两颗照核心那两句：一条待答请求 · 一条需手动 ⇒ 待答请求 1 · 需手动 1（待答请求在前）；行的状态照脸的名字`() =
        runTest {
            val a = Host(HostName("h1", "a"))
            a.groups = slice(listOf(group("g1", "开发")), listOf(member("s1", listOf("g1"), face = "live-asks"), member("s2", listOf("g1"), face = "live-needs-you-tmux")))
            val board = board(a)
            runCurrent()
            val band =
                board.ui.value.bands
                    .single()
            assertEquals(
                "组头照核心数好写好的两句",
                listOf(copyText("beGroups.count.asks", "n" to "1"), copyText("beGroups.count.needs", "n" to "1")),
                band.heads,
            )
            assertEquals(
                "行的状态照那张脸的名字：答得了的叫待答请求、答不了的叫需手动",
                listOf(copyText("needs.asks.title"), copyText("beSession.needs.unknown")),
                band.rows.map { it.status.text },
            )
        }

    @Test
    fun `删除确认那一句是核心照那台的称呼写好的：按天 · 固定 · 组里每行都点名，清单里没有的组员多一句说不准`() =
        runTest {
            val a = Host(HostName("h1", "a"))
            val b = Host(HostName("h2", "gpu"))
            a.rows = listOf(rowMap("s1", 2))
            b.rows = listOf(rowMap("s2", 3), rowMap("s3", 1))
            b.groups = slice(listOf(group("g1", "开发")), listOf(member("s3", pinned = true), member("s9", listOf("g1"))))
            val board = board(a, b)
            runCurrent()
            val ui = board.ui.value
            val named = { m: String -> copyText("history.delete.bodyRemote", "machine" to m) }
            assertEquals(listOf(named("gpu"), named("a")), ui.sections.flatMap { s -> s.rows.map { it.deleteAsk } })
            assertEquals(listOf(named("gpu")), ui.pinned.map { it.deleteAsk })
            val unsure = named("gpu") + "\n" + copyText("history.delete.unsure")
            val band = ui.bands.single()
            assertEquals(listOf(unsure), band.rows.map { it.deleteAsk })
        }

    @Test
    fun `那台说组那一片读不到 ⇒ 那一句进注解行，不静默丢`() =
        runTest {
            val a = Host(HostName("h1", "a"))
            a.groups = Reply.Failed(Failed("failed", "读不到注解", "d"))
            val board = board(a)
            runCurrent()
            assertEquals(listOf("读不到注解"), board.ui.value.notices)
        }

    @Test
    fun `同一组在两台各有一片 ⇒ 核心合成一段：名字取起得晚的那片，组员照台接、各带那台的机器小标签`() =
        runTest {
            val c = Host(HostName("c", "C"))
            val e = Host(HostName("e", "E"))
            c.groups = slice(listOf(group("g1", "甲", named = 1L)), listOf(member("c1", listOf("g1"))))
            e.groups = slice(listOf(group("g1", "乙", named = 2L)), listOf(member("e1", listOf("g1"))))
            val board = board(c, e)
            runCurrent()
            val band =
                board.ui.value.bands
                    .single()
            assertEquals("g1", band.groupId)
            assertEquals("乙", band.title)
            assertEquals(listOf("c" to "c1", "e" to "e1"), band.rows.map { it.hostId to it.sessionId })
            assertEquals(listOf(null, "E"), band.rows.map { it.machineTag })
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
            assertEquals(mapOf("origin" to LOCAL, "within_days" to 7), a.link.calls("history-list").last())
            assertEquals(mapOf("origin" to LOCAL, "within_days" to 7), b.link.calls("history-list").last())
            board.filters { it.copy(unchecked = setOf("h2")) }
            runCurrent()
            val before = b.link.calls("history-list").size
            board.filters { it.copy(hidden = true) }
            runCurrent()
            assertEquals("去掉的那台不再问", before, b.link.calls("history-list").size)
            assertEquals(mapOf("origin" to LOCAL, "within_days" to 7, "hidden" to true), a.link.calls("history-list").last())
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
            assertEquals(mapOf("origin" to LOCAL), a.link.calls("history-list").last())
            advanceTimeBy(2)
            runCurrent()
            assertEquals(mapOf("origin" to LOCAL, "query" to "登录"), a.link.calls("history-list").last())
        }

    @Test
    fun `点开合那一格 ⇒ 分叉就地出在父下、不重问那台；换筛选不清；再点收起`() =
        runTest {
            val a = Host(HostName("h1", "a"))
            a.rows = listOf(rowMap("p", 3), rowMap("f", 2, parent = "p"), rowMap("q", 1))
            val board = board(a)
            val shown = {
                board.ui.value.rows
                    .map { it.sessionId }
            }
            runCurrent()
            assertEquals(listOf("p", "q"), shown())
            val asked = a.link.calls("history-list").size
            board.toggleForks("h1", "p")
            runCurrent()
            assertEquals(listOf("p", "f", "q"), shown())
            assertEquals("开合不重问那台", asked, a.link.calls("history-list").size)
            board.filters { it.copy(sort = ListSort.CREATED) }
            runCurrent()
            assertEquals("换筛选不清", listOf("p", "f", "q"), shown())
            board.toggleForks("h1", "p")
            runCurrent()
            assertEquals(listOf("p", "q"), shown())
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

    /** 核心合好的那一行（替身）；[down]：没答上的那几台（hostId ⇒ 核心写好的那一句）；[summary] 缺 ⇒ 核心没写结果头。 */
    private fun mergedOf(
        vararg rows: Pair<String, String>,
        down: List<Pair<String, String>> = emptyList(),
        summary: String? = "核心·结果头",
        empty: String? = null,
    ) =
        RawJson.write(
            mapOf(
                "totalHits" to rows.size,
                "truncated" to false,
                "notes" to listOf("核心·读不动 4 份", "核心·Codex 只按标题"),
                "down" to down.map { (origin, said) -> mapOf("origin" to origin, "said" to said) },
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
                            "reveal" to false,
                            "deleteAsk" to "",
                        )
                    },
            ) + listOfNotNull(summary?.let { "summary" to it }, empty?.let { "empty" to it }).toMap(),
        )

    @Test
    fun `搜内容：只问核心一次（交词与勾上那几台的编号 · hostId），各台的流上不问、不交哪一台当信差；合好的行按 origin 对回哪台`() =
        runTest {
            val a = Host(HostName("h1", "a"))
            val b = Host(HostName("h2", "gpu"))
            val merge = Merge().apply { out = mergedOf("h2" to "s2", "h1" to "s1") }
            val board = board(a, b, merge = merge)
            runCurrent()
            board.query("登录")
            advanceTimeBy(ChatsBoard.QUERY_DEBOUNCE_MS + 1)
            runCurrent()
            board.searchContent()
            runCurrent()
            for (h in listOf(a, b)) {
                assertEquals("各台的流上不问", emptyList<Any?>(), h.link.calls("history-search") + h.link.calls("history-search-merge"))
            }
            assertEquals(
                listOf("""{"q":"登录","hosts":[{"handle":10,"origin":"h1"},{"handle":11,"origin":"h2"}],"withinMs":15000}"""),
                merge.got,
            )
            val c = board.ui.value.content!!
            assertEquals("结果头照核心写好的那一句", "核心·结果头", c.summary)
            assertEquals(listOf("h2" to "s2", "h1" to "s1"), c.rows.map { it.hostId to it.row.sessionId })
            assertEquals(listOf("gpu", null), c.rows.map { it.machineTag })
            assertEquals("头底下那几句照核心写好的摆、不自己按数取字", listOf("核心·读不动 4 份", "核心·Codex 只按标题"), c.lines)
        }

    @Test
    fun `搜内容：一台没答 ⇒ 那台那一句照核心写好的摆，别的台的行照出；换词收掉`() =
        runTest {
            val a = Host(HostName("h1", "a"))
            val b = Host(HostName("h2", "gpu"))
            val merge = Merge().apply { out = mergedOf("h2" to "s2", down = listOf("h1" to "核心·a 没答上")) }
            val board = board(a, b, merge = merge)
            runCurrent()
            board.query("x")
            advanceTimeBy(ChatsBoard.QUERY_DEBOUNCE_MS + 1)
            runCurrent()
            board.searchContent()
            runCurrent()
            assertEquals(1, merge.got.size)
            val c = board.ui.value.content!!
            assertEquals(listOf("h2" to "s2"), c.rows.map { it.hostId to it.row.sessionId })
            assertEquals("核心·a 没答上", c.lines.first())
            assertEquals("有台答了 ⇒ 结果头照出", "核心·结果头", c.summary)
            board.query("xy")
            runCurrent()
            assertNull("换了词 ⇒ 内容结果收掉", board.ui.value.content)
        }

    @Test
    fun `搜内容：出不出结果头只照核心那一格，不自己数没答上的台`() =
        runTest {
            val a = Host(HostName("h1", "a"))
            val b = Host(HostName("h2", "gpu"))
            val down = listOf("h1" to "核心·a 没答上", "h2" to "核心·gpu 没答上")
            val merge = Merge().apply { out = mergedOf(down = down, summary = null) }
            val board = board(a, b, merge = merge)
            runCurrent()
            board.query("x")
            advanceTimeBy(ChatsBoard.QUERY_DEBOUNCE_MS + 1)
            runCurrent()
            board.searchContent()
            runCurrent()
            val none = board.ui.value.content!!
            assertNull("核心没写结果头 ⇒ 不出", none.summary)
            assertEquals(listOf("核心·a 没答上", "核心·gpu 没答上"), none.lines.take(2))
            merge.out = mergedOf(down = down)
            board.query("xy")
            advanceTimeBy(ChatsBoard.QUERY_DEBOUNCE_MS + 1)
            runCurrent()
            board.searchContent()
            runCurrent()
            val some = board.ui.value.content!!
            assertEquals("核心写了 ⇒ 照出（台数不在这里数）", "核心·结果头", some.summary)
        }

    @Test
    fun `搜内容：一行都没有时空态那一句只照核心那一格（没答上一台与没中各是核心写的那一句）`() =
        runTest {
            val a = Host(HostName("h1", "a"))
            val b = Host(HostName("h2", "gpu"))
            val down = listOf("h1" to "核心·a 没答上", "h2" to "核心·gpu 没答上")
            val merge = Merge().apply { out = mergedOf(down = down, summary = null, empty = "核心·没有一台答上") }
            val board = board(a, b, merge = merge)
            runCurrent()
            board.query("x")
            advanceTimeBy(ChatsBoard.QUERY_DEBOUNCE_MS + 1)
            runCurrent()
            board.searchContent()
            runCurrent()
            val ui = board.ui
            val empty = { ui.value.content?.empty }
            assertEquals("一台都没答上 ⇒ 照核心那一句", "核心·没有一台答上", empty())
            merge.out = mergedOf(empty = "核心·没有匹配")
            board.query("xy")
            advanceTimeBy(ChatsBoard.QUERY_DEBOUNCE_MS + 1)
            runCurrent()
            board.searchContent()
            runCurrent()
            assertEquals("答了没中 ⇒ 照核心那一句", "核心·没有匹配", empty())
            merge.out = mergedOf("h1" to "s1")
            board.query("xyz")
            advanceTimeBy(ChatsBoard.QUERY_DEBOUNCE_MS + 1)
            runCurrent()
            board.searchContent()
            runCurrent()
            assertNotNull("合好了", board.ui.value.content)
            assertNull("有行 ⇒ 没有空态", empty())
        }
}

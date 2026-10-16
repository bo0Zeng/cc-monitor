package com.ccmonitor.mobile.ui.chats

import com.ccmonitor.mobile.cache.ListCache
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
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
        "status" to "ended",
        "hidden" to null,
        "forkedFromSessionId" to null,
        "forkedFrom" to null,
        "can" to mapOf("delete" to "yes"),
    )

    private class Host(
        val name: HostName,
    ) {
        val link = FakeCoreLink()
        var rows: List<Map<String, Any?>> = emptyList()

        init {
            link.answer = { cmd, _ -> if (cmd == "history-list") Reply.Ok(mapOf("rows" to rows, "groups" to emptyList<Any>(), "notice" to null, "truncated" to false)) else Reply.LinkDown }
        }
    }

    private fun TestScope.board(vararg hosts: Host): ChatsBoard {
        val by = hosts.associateBy { it.name.id }
        return ChatsBoard(hosts.first().name.id, MutableStateFlow(hosts.map { it.name }), { h, s ->
            HostChats(h.id, h.label, by.getValue(h.id).link, emptyFlow(), MutableStateFlow(null), {}, {}, ListCache.None, s)
        }, backgroundScope)
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
            assertEquals(listOf("s1"), board.ui.value.rows.map { it.sessionId })
            board.hold()
            a.rows = listOf(rowMap("s2", 2), rowMap("s1", 1))
            board.chats("h1")!!.fetch()
            runCurrent()
            assertEquals("按着 ⇒ 还是旧的", listOf("s1"), board.ui.value.rows.map { it.sessionId })
            board.release()
            assertEquals(listOf("s2", "s1"), board.ui.value.rows.map { it.sessionId })
        }
}

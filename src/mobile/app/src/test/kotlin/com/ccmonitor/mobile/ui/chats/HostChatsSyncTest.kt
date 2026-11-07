package com.ccmonitor.mobile.ui.chats

import com.ccmonitor.mobile.cache.ListCache
import com.ccmonitor.mobile.core.claude.link.ListAsk
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.wire.RawPatch
import com.ccmonitor.mobile.core.claude.link.wire.rawPatch
import com.ccmonitor.mobile.core.wire.gen.LOCAL
import com.ccmonitor.mobile.link.Fetched
import com.ccmonitor.mobile.ui.Faces
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 没筛选那一问带着手上那一份的记号问、答交核心换（那台只给变了的那几行）；带筛选的不带记号、不交核心。
 * 缓存里有上次那一份 ⇒ 开屏先画它、头一问就带它的记号。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class HostChatsSyncTest {
    private fun row(
        sid: String,
        label: String = sid,
    ) = mapOf(
        "sessionId" to sid,
        "label" to label,
        "at" to 1L,
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
        "title" to label,
        "startedAt" to 1L,
        "updatedAt" to 1L,
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

    private val head = mapOf("total" to 2L, "info" to mapOf("groups" to emptyList<Any>(), "notice" to null))

    private fun data(
        held: String,
        whole: Boolean = false,
        same: Boolean = false,
        order: List<String>? = null,
        up: Map<String, Any?> = emptyMap(),
        head: Map<String, Any?>? = null,
    ) = mapOf("held" to held, "whole" to whole, "same" to same, "order" to order, "up" to up, "head" to head)

    private fun patch(
        held: String,
        whole: Boolean = false,
        same: Boolean = false,
        order: List<String>? = null,
        up: Map<String, Any?> = emptyMap(),
        head: Map<String, Any?>? = null,
    ) = Reply.Ok(data(held, whole, same, order, up, head))

    /** 核心交回的整份那一形（换法在核心里，这里照剧本给）。 */
    private fun whole(
        held: String,
        vararg rows: Pair<String, String>,
    ): RawPatch = rawPatch(data(held, whole = true, order = rows.map { it.first }, up = rows.associate { (k, l) -> k to row(k, l) }, head = head))!!

    /** 替核心那一份：手上那一份 [held]；交来的答记下，换好的那一份照 [next] 给（`null` ⇒ 读不动）。 */
    private class Kept(
        var held: RawPatch? = null,
    ) : ListCache {
        val got = mutableListOf<Any?>()
        var next: RawPatch? = null

        override suspend fun held(): RawPatch? = held

        override suspend fun keep(reply: Any?): RawPatch? {
            got += reply
            return next?.also { held = it }
        }
    }

    private fun TestScope.chats(
        link: FakeCoreLink,
        cache: ListCache,
        groupsChanged: Flow<Unit> = emptyFlow(),
    ) = HostChats("h1", "a", link, emptyFlow(), groupsChanged, MutableStateFlow(null), {}, cache, backgroundScope)

    private fun HostChats.labels() = ((state.value.list as? Fetched.Got)?.value?.rows ?: state.value.cached?.rows).orEmpty().map { it.label }

    @Test
    fun `没筛选那一问带着手上那一份的记号问、答原样交核心、画核心换好的整份；带筛选的不带记号、不交核心`() =
        runTest {
            val link = FakeCoreLink()
            val cache = Kept()
            var next: Reply = patch("e.1.1", whole = true, order = listOf("a", "b"), up = mapOf("a" to row("a"), "b" to row("b")), head = head)
            link.answer = { _, _ -> next }
            cache.next = whole("e.1.1", "a" to "a", "b" to "b")
            val c = chats(link, cache)
            runCurrent()
            assertEquals(listOf("a", "b"), c.labels())
            assertEquals(listOf<Map<String, Any?>?>(mapOf("origin" to LOCAL)), link.calls("history-list"))
            next = patch("e.1.2", up = mapOf("b" to row("b", "b 改了名")))
            cache.next = whole("e.1.2", "a" to "a", "b" to "b 改了名")
            c.fetch()
            runCurrent()
            assertEquals(mapOf("origin" to LOCAL, "held" to "e.1.1"), link.calls("history-list").last())
            assertEquals("那台的答原样交核心", (next as Reply.Ok).data, cache.got.last())
            assertEquals(listOf("a", "b 改了名"), c.labels())
            // 核心读不动那一答 ⇒ 说读不出，手上那一份不动、下一问照旧带它的记号。
            next = patch("e.1.3", same = true)
            cache.next = null
            c.fetch()
            runCurrent()
            assertTrue(c.state.value.list is Fetched.Failed)
            assertEquals("e.1.2", cache.held?.held)
            assertEquals(3, cache.got.size)
            // 带筛选那一问：不带记号、答的整份不交核心。
            next = patch("e.2.1", whole = true, order = listOf("a"), up = mapOf("a" to row("a")), head = head)
            c.ask(ListAsk(query = "a"))
            runCurrent()
            assertEquals(mapOf("origin" to LOCAL, "query" to "a"), link.calls("history-list").last())
            assertEquals(3, cache.got.size)
            assertEquals(listOf("a"), c.labels())
        }

    @Test
    fun `缓存里有上次那一份 ⇒ 开屏先画它，头一问就带它的记号`() =
        runTest {
            val link = FakeCoreLink()
            link.answer = { _, _ -> Reply.LinkDown }
            val c = chats(link, Kept(whole("e.9.4", "a" to "上次的 a")))
            runCurrent()
            assertEquals(listOf("上次的 a"), c.labels())
            assertEquals(mapOf("origin" to LOCAL, "held" to "e.9.4"), link.calls("history-list").single())
        }

    @Test
    fun `接上问一次 groups-here（这台自己那一片，原样留着交核心合）；那台连着推 changed {groups} ⇒ 合成一问重问`() =
        runTest {
            val link = FakeCoreLink()
            val slice = mapOf("groups" to emptyList<Any>(), "members" to emptyList<Any>())
            link.answer = { cmd, _ ->
                if (cmd == "groups-here") {
                    Reply.Ok(slice)
                } else {
                    patch("e.1.1", whole = true, order = emptyList(), head = head)
                }
            }
            val pushed = MutableSharedFlow<Unit>(extraBufferCapacity = 8)
            val c = chats(link, Kept(), pushed)
            runCurrent()
            assertEquals(listOf<Map<String, Any?>?>(emptyMap()), link.calls("groups-here"))
            assertEquals(slice, c.state.value.groups)
            pushed.emit(Unit)
            pushed.emit(Unit)
            advanceTimeBy(HostChats.LIST_DEBOUNCE_MS + 1)
            runCurrent()
            assertEquals("连着两帧合成一问", 2, link.calls("groups-here").size)
        }
}

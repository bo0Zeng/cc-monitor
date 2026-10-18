package com.ccmonitor.mobile.ui.chats

import com.ccmonitor.mobile.cache.ListCache
import com.ccmonitor.mobile.core.claude.link.ListAsk
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.wire.SyncHand
import com.ccmonitor.mobile.link.Fetched
import com.ccmonitor.mobile.ui.Faces
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * 没筛选那一问带着手上那一份的记号问、照答换了存回（那台只给变了的那几行）；带筛选的不带记号、不存。
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
        "can" to mapOf("delete" to "yes"),
    )

    private val head = mapOf("total" to 2L, "info" to mapOf("groups" to emptyList<Any>(), "notice" to null))

    private fun patch(
        held: String,
        whole: Boolean = false,
        same: Boolean = false,
        order: List<String>? = null,
        up: Map<String, Any?> = emptyMap(),
        head: Map<String, Any?>? = null,
    ) = Reply.Ok(mapOf("held" to held, "whole" to whole, "same" to same, "order" to order, "up" to up, "head" to head))

    /** 存在内存里的那一份（存了几回也记着）。 */
    private class Kept(
        var hand: SyncHand = SyncHand(),
    ) : ListCache {
        var keeps = 0

        override suspend fun hand(): SyncHand = hand

        override suspend fun keep(hand: SyncHand) {
            this.hand = hand
            keeps++
        }
    }

    private fun TestScope.chats(
        link: FakeCoreLink,
        cache: ListCache,
    ) = HostChats("h1", "a", link, emptyFlow(), MutableStateFlow(null), {}, cache, backgroundScope)

    private fun HostChats.labels() = ((state.value.list as? Fetched.Got)?.value?.rows ?: state.value.cached?.rows).orEmpty().map { it.label }

    @Test
    fun `头一问整份；再问带着记号，没变 ⇒ 不动；只给一行 ⇒ 只换那一行`() =
        runTest {
            val link = FakeCoreLink()
            val cache = Kept()
            var next: Reply = patch("e.1.1", whole = true, order = listOf("a", "b"), up = mapOf("a" to row("a"), "b" to row("b")), head = head)
            link.answer = { _, _ -> next }
            val c = chats(link, cache)
            runCurrent()
            assertEquals(listOf("a", "b"), c.labels())
            assertEquals(listOf<Map<String, Any?>?>(null), link.calls("history-list"))
            next = patch("e.1.1", same = true)
            c.fetch()
            runCurrent()
            assertEquals(mapOf("held" to "e.1.1"), link.calls("history-list").last())
            assertEquals(listOf("a", "b"), c.labels())
            next = patch("e.1.2", up = mapOf("b" to row("b", "b 改了名")))
            c.fetch()
            runCurrent()
            assertEquals(listOf("a", "b 改了名"), c.labels())
            assertEquals("e.1.2", cache.hand.held)
            assertEquals(listOf("a", "b"), cache.hand.order)
            assertEquals(3, cache.keeps)
            // 带筛选那一问：不带记号、答的整份不进缓存。
            next = patch("e.2.1", whole = true, order = listOf("a"), up = mapOf("a" to row("a")), head = head)
            c.ask(ListAsk(query = "a"))
            runCurrent()
            assertEquals(mapOf("query" to "a"), link.calls("history-list").last())
            assertEquals(3, cache.keeps)
            assertEquals("e.1.2", cache.hand.held)
        }

    @Test
    fun `缓存里有上次那一份 ⇒ 开屏先画它，头一问就带它的记号`() =
        runTest {
            val link = FakeCoreLink()
            val had = SyncHand("e.9.4", listOf("a"), mapOf("a" to row("a", "上次的 a")), null)
            link.answer = { _, _ -> Reply.LinkDown }
            val c = chats(link, Kept(had))
            runCurrent()
            assertEquals(listOf("上次的 a"), c.labels())
            assertEquals(mapOf("held" to "e.9.4"), link.calls("history-list").single())
        }
}

package com.ccmonitor.mobile.ui.chat.attach

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.wire.gen.Failed
import com.ccmonitor.mobile.ui.chat.ChatSession
import com.ccmonitor.mobile.ui.chat.ChatTarget
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import com.ccmonitor.mobile.ui.chat.Sendable
import com.ccmonitor.mobile.ui.chat.linkDown
import com.ccmonitor.mobile.ui.chat.linkUp
import com.ccmonitor.mobile.ui.chat.sendable
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import kotlin.coroutines.EmptyCoroutineContext

/** ➤ 那一下：那一排全落好了才发，编号随那一句交 `session-queue-put`（按先后），排清空；那台拒了重发照带；新建会话没有那一排。 */
@OptIn(ExperimentalCoroutinesApi::class)
class AttachSendTest {
    @get:Rule val dir = TemporaryFolder()

    private val staged = mapOf("key" to "k1", "chunks" to 1.0, "bytes" to 5.0, "sha256" to "ab".repeat(32))

    private fun FakeCoreLink.there(say: () -> Reply) {
        var n = 0
        answer = { cmd, args ->
            when (cmd) {
                "session-attachment-check" -> Reply.Ok(mapOf("items" to List((args!!["items"] as List<*>).size) { OK }))
                "session-attachment-commit" -> Reply.Ok(mapOf("id" to "id-${++n}", "name" to "n", "path" to "/w/n", "rel" to "n", "bytes" to 5.0, "sha256" to "ab", "class" to "image"))
                "session-queue-put" -> say()
                "history-tail" -> Reply.Ok(FakeCoreLink.tail(0, 0, 0))
                else -> Reply.Ok(mapOf("rows" to emptyList<Any>(), "next" to 0, "eof" to true))
            }
        }
        transferred = { Reply.Ok(staged) }
    }

    private fun TestScope.session(
        link: FakeCoreLink,
        target: ChatTarget = ChatTarget.Existing("s1"),
    ) = ChatSession(
        link,
        link,
        target,
        "box",
        backgroundScope,
        attachOf = { scope, sid -> AttachTray(link, "box", AttachStore(dir.root), scope, sid, { 7 }, EmptyCoroutineContext) },
    ).also {
        it.open()
        runCurrent()
    }

    private fun says(link: FakeCoreLink) = link.calls("session-queue-put").map { it!!["text"] to it["attachments"] }

    private val queued = Reply.Ok(mapOf("outcome" to "queued", "seq" to 1L, "place" to 1L, "said" to "q-1"))

    @Test
    fun send_hands_the_ids_in_order_with_the_line_and_clears_the_row() =
        runTest {
            val link = FakeCoreLink().apply { there { queued } }
            val s = session(link)
            val tray = s.attach!!
            tray.add("a.png", true) { ByteArray(5).inputStream() }
            tray.add("b.png", true) { ByteArray(5).inputStream() }
            runCurrent()
            s.send("两张都是 360 宽的")
            runCurrent()
            assertEquals(listOf<Pair<Any?, Any?>>("两张都是 360 宽的" to listOf("id-1", "id-2")), says(link))
            assertEquals(emptyList<AttachCard>(), tray.state.value.cards)
        }

    @Test
    fun nothing_goes_out_until_every_card_has_landed_and_a_bare_row_can_go_alone() =
        runTest {
            val link = FakeCoreLink().apply { there { queued } }
            link.state.value = linkDown("down")
            val s = session(link)
            s.attach!!.add("a.png", true) { ByteArray(5).inputStream() }
            runCurrent()
            s.send("看这张")
            runCurrent()
            assertEquals(emptyList<Any>(), says(link))

            link.state.value = linkUp()
            runCurrent()
            s.send("")
            runCurrent()
            assertEquals(listOf<Pair<Any?, Any?>>("" to listOf("id-1")), says(link))
        }

    @Test
    fun a_retry_after_a_failed_send_carries_the_same_ids() =
        runTest {
            var first = true
            val refused = Reply.Failed(Failed("queue_full", "队满了", "", null))
            val link =
                FakeCoreLink().apply {
                    there {
                        if (first) {
                            first = false
                            refused
                        } else {
                            queued
                        }
                    }
                }
            val s = session(link)
            s.attach!!.add("a.png", true) { ByteArray(5).inputStream() }
            runCurrent()
            s.send("这张")
            runCurrent()
            val local =
                s.state.value.outbox
                    .single { it.failed && it.retry }
                    .id
            s.retry(local)
            runCurrent()
            assertEquals(List(2) { "这张" to listOf("id-1") }, says(link))
            assertEquals(emptyList<String>(), AttachStore(dir.root).sentOf(local))
        }

    @Test
    fun a_new_session_has_no_row() =
        runTest {
            val link = FakeCoreLink()
            assertNull(session(link, ChatTarget.Draft("t", "~/p")).attach)
        }

    /** ➤ 灰到全传完；只有附件没字也能发；没字没附件 ⇒ 运行中那一颗是 ■。 */
    @Test
    fun the_send_button_waits_for_every_card() {
        fun row(vararg p: AttachPhase) = AttachUi(p.mapIndexed { i, ph -> AttachCard("$i", "n", 1L, true, dir.root, ph) })
        assertEquals(Sendable.EMPTY, sendable(" ", null))
        assertEquals(Sendable.EMPTY, sendable("", row()))
        assertEquals(Sendable.READY, sendable("hi", null))
        assertEquals(Sendable.READY, sendable("", row(AttachPhase.Landed("a"))))
        assertEquals(Sendable.WAITING, sendable("hi", row(AttachPhase.Landed("a"), AttachPhase.Moving(1, 2))))
        assertEquals(Sendable.WAITING, sendable("hi", row(AttachPhase.Refused("超上限"))))
    }
}

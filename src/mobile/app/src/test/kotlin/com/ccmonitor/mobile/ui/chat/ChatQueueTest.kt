package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.Changed
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * 聊天屏的输入队那一份：只照会话那台推来的名单画（`changed {session_queue, key: 这条}`），不各自记；每条可撤（`session-queue-cancel`）。
 * 名单里每一条的那一句（`said`）是会话那台写好的，这里照摆；不摆哪一端。
 */
class ChatQueueTest {
    private fun item(
        id: String,
        said: String,
    ) = mapOf("seq" to 1, "id" to id, "end" to "monitor", "text" to "text of $id", "state" to "queued", "said" to said)

    private fun list(vararg items: Map<String, Any?>) = mapOf("items" to items.toList(), "sent" to emptyList<String>())

    private fun pushed(
        sid: String,
        body: Map<String, Any?>?,
    ): Changed = Wire.bind<Changed>(mapOf("topic" to "session_queue", "key" to sid) + listOfNotNull(body?.let { "body" to it }))!!

    @Test
    fun `★ 桌面撤掉第 1 条 ⇒ 手机那份名单当场少一条、原第 2 条排第 1`() =
        runTest(UnconfinedTestDispatcher()) {
            val link = FakeCoreLink()
            link.answer = { cmd, _ -> if (cmd == "session-queue-read") Reply.Ok(list(item("a", "place-1"), item("b", "place-2"))) else Reply.LinkDown }
            val changed = MutableSharedFlow<Changed>()
            val q = ChatQueue(link, backgroundScope) {}
            q.watch(changed) { "s1" }
            q.read("s1")
            assertEquals(listOf("a" to "place-1", "b" to "place-2"), q.lines().map { it.id to it.said })
            changed.emit(pushed("s1", list(item("b", "place-1"))))
            assertEquals(listOf("b" to "place-1"), q.lines().map { it.id to it.said })
            assertEquals("text of b", q.lines().single().text)
        }

    @Test
    fun `别的会话那一份不动；没带名单 ⇒ 重问`() =
        runTest(UnconfinedTestDispatcher()) {
            val link = FakeCoreLink()
            link.answer = { cmd, _ -> if (cmd == "session-queue-read") Reply.Ok(list(item("a", "place-1"))) else Reply.LinkDown }
            val changed = MutableSharedFlow<Changed>()
            val q = ChatQueue(link, backgroundScope) {}
            q.watch(changed) { "s1" }
            changed.emit(pushed("s-other", list()))
            assertEquals(0, link.calls("session-queue-read").size)
            changed.emit(pushed("s1", null))
            assertEquals(listOf("a"), q.lines().map { it.id })
            assertEquals(mapOf("machine" to "<local>", "sid" to "s1"), link.calls("session-queue-read").single())
        }

    @Test
    fun `撤 ⇒ 照编号请那台撤，照回的整份名单画`() =
        runTest(UnconfinedTestDispatcher()) {
            val link = FakeCoreLink()
            link.answer = { cmd, _ ->
                when (cmd) {
                    "session-queue-read" -> Reply.Ok(list(item("a", "place-1"), item("b", "place-2")))
                    "session-queue-cancel" -> Reply.Ok(mapOf("cancelled" to true, "queue" to list(item("b", "place-1"))))
                    else -> Reply.LinkDown
                }
            }
            val q = ChatQueue(link, backgroundScope) {}
            q.read("s1")
            var back: String? = null
            q.cancel("a", "s1") { back = it }
            assertEquals(mapOf("machine" to "<local>", "sid" to "s1", "id" to "a"), link.calls("session-queue-cancel").single())
            assertEquals(listOf("b"), q.lines().map { it.id })
            assertEquals("★ 撤成了 ⇒ 那一句的字交回（回到输入框）", "text of a", back)
        }

    @Test
    fun `★ 名单与待发排成同一种行：一条一行只摆那一句与开头那一行，不写哪一端；没送到 · 拒了 ⇒ 错误色；没回执 ⇒ 淡着`() =
        runTest(UnconfinedTestDispatcher()) {
            val link = FakeCoreLink()
            val gone = item("b", "未送达") + ("state" to "undelivered") + ("text" to "第一行\n第二行")
            link.answer = { cmd, _ -> if (cmd == "session-queue-read") Reply.Ok(list(item("a", "等需手动"), gone)) else Reply.LinkDown }
            val q = ChatQueue(link, backgroundScope) {}
            q.read("s1")
            val outbox =
                listOf(
                    OutboxLine("o1", "发着的", "等确认", failed = false, held = false),
                    OutboxLine("o2", "没回执的", "离线", failed = false, held = true),
                    OutboxLine("o3", "被拒的", "队满", failed = true, held = false),
                )
            val rows = queueRows(q.lines(), outbox).rows
            assertEquals(listOf("a", "b", "o1", "o2", "o3"), rows.map { it.id })
            assertEquals(listOf("等需手动", "未送达", "等确认", "离线", "队满"), rows.map { it.state })
            assertEquals("一条只占一行：开头只取第一行", "第一行", rows[1].head)
            assertEquals(listOf(false, true, false, false, true), rows.map { it.error })
            assertEquals(listOf(false, false, false, true, false), rows.map { it.held })
            assertEquals(listOf(true, true, false, false, false), rows.map { it.listed })
        }

    @Test
    fun `★ 待发里那一句名单里已有（回执还没到、那台已排进队）⇒ 只照名单画一行（同桌面 queue-row）`() {
        val listed = listOf(QueueLine("o1", "排着", "那一句", undelivered = false))
        val pending =
            listOf(
                OutboxLine("o1", "那一句", "等确认", failed = false, held = false),
                OutboxLine("o2", "另一句", "等确认", failed = false, held = false),
            )
        val rows = queueRows(listed, pending).rows
        assertEquals(listOf("o1" to true, "o2" to false), rows.map { it.id to it.listed })
        assertEquals("排着", rows.first().state)
    }

    @Test
    fun `★ 那台离线 ⇒ 那一排排头单独一行「名单为上次所知」、不拼离线那句；连着 · 一行都没有 ⇒ 不说（同桌面 queue-row）`() {
        val one = listOf(QueueLine("a", "等确认", "那一句", undelivered = false))
        val stale = queueRows(one, emptyList(), offline = true)
        assertEquals(copyText("terminal.queue.stale"), stale.stale)
        assertEquals(listOf("a"), stale.rows.map { it.id })
        assertNull("连着 ⇒ 照常", queueRows(one, emptyList(), offline = false).stale)
        assertNull("没有一行 ⇒ 那一排收起、不说", queueRows(emptyList(), emptyList(), offline = true).stale)
        val pending = listOf(OutboxLine("o1", "发着的", "等确认", failed = false, held = false))
        assertEquals("只有待发也算有行", stale.stale, queueRows(emptyList(), pending, offline = true).stale)
    }
}

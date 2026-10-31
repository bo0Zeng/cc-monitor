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
 * 名单里每一条的那一句（`said`）与发自哪一端（`from`）是会话那台写好的，这里照摆。
 */
class ChatQueueTest {
    private fun item(
        id: String,
        said: String,
    ) = mapOf("seq" to 1, "id" to id, "end" to "monitor", "text" to "text of $id", "state" to "queued", "said" to said, "from" to "from-word")

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
    fun `撤 ⇒ 照编号请那台撤，照回的整份名单画；见过、名单里没了也不在已送进 ⇒ 算撤掉了`() =
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
            q.cancel("a", "s1")
            assertEquals(mapOf("machine" to "<local>", "sid" to "s1", "id" to "a"), link.calls("session-queue-cancel").single())
            assertEquals(listOf("b"), q.lines().map { it.id })
            assertEquals(true, q.gone("a"))
            assertEquals(false, q.gone("b"))
            assertEquals(false, q.gone("never-seen"))
            assertEquals(true, q.holds("b"))
        }

    @Test
    fun `名单与待发排成同一种行：一条一行只摆开头那一行；没送到 · 拒了 ⇒ 状态错误色；拒了的才有［重试］`() =
        runTest(UnconfinedTestDispatcher()) {
            val link = FakeCoreLink()
            val gone = item("b", "未送达") + ("state" to "undelivered") + ("text" to "第一行\n第二行")
            link.answer = { cmd, _ -> if (cmd == "session-queue-read") Reply.Ok(list(item("a", "排第 1"), gone)) else Reply.LinkDown }
            val q = ChatQueue(link, backgroundScope) {}
            q.read("s1")
            val outbox = listOf(OutboxLine("o1", "发着的", "发送中", failed = false, retry = false), OutboxLine("o2", "被拒的", "队满", failed = true, retry = true))
            val rows = queueRows(q.lines(), outbox).rows
            assertEquals(listOf("a", "b", "o1", "o2"), rows.map { it.id })
            assertEquals(listOf("排第 1", "未送达", "发送中", "队满"), rows.map { it.state })
            assertEquals(listOf("from-word", "from-word", null, null), rows.map { it.from })
            assertEquals("一条只占一行：开头只取第一行", "第一行", rows[1].head)
            assertEquals(listOf(false, true, false, true), rows.map { it.error })
            assertEquals(listOf(false, false, false, true), rows.map { it.retry })
            assertEquals(listOf(true, true, false, false), rows.map { it.listed })
        }

    @Test
    fun `★ 那台离线 ⇒ 那一排头上一行「离线那句 · 名单为上次所知」、淡着；连着 · 一行都没有 ⇒ 不说（同桌面 queue-row）`() {
        val one = listOf(QueueLine("a", "排第 1", "from-word", "那一句", undelivered = false))
        val stale = queueRows(one, emptyList(), offline = "X 离线")
        assertEquals(listOf("X 离线", copyText("terminal.queue.stale")).joinToString(copyText("kit.text.sep")), stale.stale)
        assertEquals(listOf("a"), stale.rows.map { it.id })
        assertNull("连着 ⇒ 照常", queueRows(one, emptyList(), offline = null).stale)
        assertNull("没有一行 ⇒ 那一排收起、不说", queueRows(emptyList(), emptyList(), offline = "X 离线").stale)
        val pending = listOf(OutboxLine("o1", "发着的", "发送中", failed = false, retry = false))
        assertEquals("只有待发也算有行", stale.stale, queueRows(emptyList(), pending, offline = "X 离线").stale)
    }
}

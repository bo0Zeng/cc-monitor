package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.Tone
import com.ccmonitor.mobile.core.claude.model.DeliveryState
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.ui.copy.copyText
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.failed
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.human
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.reply
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 聊天屏的会话：起会话 · 送字 · 正文全走那台核心。
 * 判据：手机起的会话是那台 tmux 里的（`session-new` 的 `place: tmux`，桌面那一侧看得到、接得上）；手机打的字经 `terminal-input` 进那条会话。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatSessionTest {
    private fun TestScope.draft(link: FakeCoreLink) = ChatSession(link, ChatTarget.Draft(ticket = "t-1", cwd = "~/proj"), "box", backgroundScope)

    private fun TestScope.existing(link: FakeCoreLink) = ChatSession(link, ChatTarget.Existing("s1"), "box", backgroundScope).also { it.open() }

    private fun ok(data: Any?) = Reply.Ok(data)

    private val started = mapOf("outcome" to "started", "session" to "proj-cc", "sid" to null, "agent" to "claude", "cwd" to "/h/proj")

    private fun ChatSession.local() =
        state.value.units
            .filterIsInstance<RenderUnit.UserText>()
            .filter { it.sourceUuid == null }

    @Test
    fun `新建会话：第一句 ⇒ 请那台在 tmux 里起（带票）⇒ 认出报到的那条 ⇒ 字经 terminal-input 进那条会话`() =
        runTest {
            val link = FakeCoreLink()
            link.report("old", "/h/proj") // 起之前就在的同目录会话不算
            link.answer = { cmd, _ ->
                when (cmd) {
                    "session-new" -> ok(started)
                    "terminal-input" -> ok(mapOf("result" to "delivered"))
                    "history-tail" -> ok(mapOf("end" to 0, "split_at" to 0))
                    else -> ok(mapOf("rows" to emptyList<Any>(), "next" to 0, "eof" to true))
                }
            }
            val s = draft(link)
            s.send("你好")
            runCurrent()
            assertEquals(listOf(mapOf("agent" to "claude", "cwd" to "~/proj", "place" to "tmux", "local" to false, "ticket" to "t-1")), link.calls("session-new"))
            assertTrue("报到之前不送字", link.calls("terminal-input").isEmpty())
            assertEquals(DeliveryState.SENDING, s.local().single().delivery)

            link.report("new", "/h/proj")
            runCurrent()
            assertEquals("new", s.state.value.sid)
            assertEquals(listOf(mapOf("sid" to "new", "text" to "你好")), link.calls("terminal-input"))
            assertNull("送到了的不挂标", s.local().single().delivery)

            link.line("new", 10, human("u1", "你好"))
            runCurrent()
            assertEquals("记录里出现了这句人话 ⇒ 本地那句换成记录那一条", emptyList<RenderUnit.UserText>(), s.local())
            assertEquals(
                listOf("u1"),
                s.state.value.units
                    .map { it.key },
            )
        }

    @Test
    fun `新建会话：第二句等同一次起，不起第二个`() =
        runTest {
            val link = FakeCoreLink()
            link.answer = { cmd, _ -> if (cmd == "session-new") ok(started) else ok(mapOf("result" to "delivered")) }
            val s = draft(link)
            s.send("一")
            s.send("二")
            runCurrent()
            link.report("new", "/h/proj")
            runCurrent()
            assertEquals(1, link.calls("session-new").size)
            assertEquals(listOf("一", "二"), link.calls("terminal-input").map { it?.get("text") })
        }

    @Test
    fun `起不来：会话那一层写核心那一句；那句话留着、可再发一次`() =
        runTest {
            val link = FakeCoreLink()
            link.answer = { _, _ -> failed("目录不在") }
            val s = draft(link)
            s.send("你好")
            runCurrent()
            assertEquals("目录不在", s.state.value.failedWhy)
            assertEquals(DeliveryState.FAILED, s.local().single().delivery)
        }

    @Test
    fun `报到等不到：说「会话未报到」，不送字`() =
        runTest {
            val link = FakeCoreLink()
            link.answer = { _, _ -> ok(started) }
            val s = draft(link)
            s.send("你好")
            advanceTimeBy(ChatSession.ARRIVAL_MS + 1)
            runCurrent()
            assertEquals(copyText("launchArrival.missed.title"), s.state.value.failedWhy)
            assertTrue(link.calls("terminal-input").isEmpty())
        }

    @Test
    fun `已有的那条：读最后一屏，再跟流上这条的新行；读到之前的旧行不重一份`() =
        runTest {
            val link = FakeCoreLink()
            link.report("s1", "/w", Tone.NOW)
            link.answer = { cmd, args ->
                when (cmd) {
                    "history-tail" -> ok(mapOf("end" to 200, "split_at" to 100))
                    "history-read" -> {
                        assertEquals(mapOf("path" to "/p/s1.jsonl", "offset" to 100L, "until" to 200L), args)
                        ok(mapOf("rows" to listOf(mapOf("end" to 150, "record" to human("u1", "问")), mapOf("end" to 200, "record" to reply("a1", "答"))), "next" to 200, "eof" to true))
                    }
                    else -> Reply.LinkDown
                }
            }
            val s = existing(link)
            runCurrent()
            assertEquals(
                listOf("u1", "a1#0"),
                s.state.value.units
                    .map { it.key },
            )
            assertTrue("会话帧语气 now ⇒ 在跑", s.state.value.running)
            assertTrue("还有更早的", s.state.value.canLoadOlder)
            link.line("s1", 200, reply("a1", "答")) // 读到的那一行，流上又报一次
            link.line("other", 300, reply("x", "别的会话"))
            link.line("s1", 260, reply("a2", "又一句"))
            runCurrent()
            assertEquals(
                listOf("u1", "a1#0", "a2#0"),
                s.state.value.units
                    .map { it.key },
            )
        }

    @Test
    fun `送达未知 · 画面变了 · 会话已结束：各是各的标，字照文案表`() =
        runTest {
            val link = FakeCoreLink()
            val results = ArrayDeque(listOf(mapOf("result" to "unsure"), mapOf("result" to "refused", "why" to "screen_changed", "said" to "画面已变"), mapOf("result" to "refused", "why" to "ended", "said" to "已结束")))
            link.answer = { cmd, _ -> if (cmd == "terminal-input") ok(results.removeFirst()) else Reply.LinkDown }
            val s = ChatSession(link, ChatTarget.Existing("s1"), "box", backgroundScope)
            s.send("a")
            runCurrent()
            s.send("b")
            runCurrent()
            s.send("c")
            runCurrent()
            val (a, b, c) = s.local()
            assertEquals(DeliveryState.UNSURE, a.delivery)
            assertEquals(copyText("terminal.input.unsure", "machine" to "box"), a.deliveryText)
            assertEquals(DeliveryState.FAILED, b.delivery)
            assertEquals(copyText("terminal.input.failed", "why" to "画面已变"), b.deliveryText)
            assertEquals(DeliveryState.FAILED_PERMANENT, c.delivery)
        }

    @Test
    fun `停：往那条会话送 Esc`() =
        runTest {
            val link = FakeCoreLink()
            link.answer = { _, _ -> ok(mapOf("result" to "delivered")) }
            ChatSession(link, ChatTarget.Existing("s1"), "box", backgroundScope).stop()
            runCurrent()
            assertEquals(listOf(mapOf("sid" to "s1", "key" to "esc")), link.calls("terminal-input"))
        }
}

package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.Tone
import com.ccmonitor.mobile.core.claude.model.DeliveryState
import com.ccmonitor.mobile.core.claude.model.RecordUnits
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
    private fun TestScope.draft(
        link: FakeCoreLink,
        agent: String = "claude",
    ) = ChatSession(link, link, ChatTarget.Draft(ticket = "t-1", cwd = "~/proj", agent = agent), "box", backgroundScope)

    private fun TestScope.existing(link: FakeCoreLink) = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope).also { it.open() }

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
                    "history-tail" -> ok(mapOf("end" to 0, "split_at" to 0, "tail_from" to 0, "total" to 0))
                    else -> ok(mapOf("rows" to emptyList<Any>(), "next" to 0, "eof" to true))
                }
            }
            val s = draft(link)
            s.send("你好")
            runCurrent()
            assertEquals(listOf(mapOf("agent" to "claude", "cwd" to "~/proj", "place" to "tmux", "local" to false, "ticket" to "t-1", "prompt" to "你好")), link.calls("session-new"))
            assertTrue("报到之前不送字", link.calls("terminal-input").isEmpty())
            assertEquals(DeliveryState.SENDING, s.local().single().delivery)

            link.report("new", "/h/proj")
            runCurrent()
            assertEquals("new", s.state.value.sid)
            assertEquals(listOf(mapOf("sid" to "new", "text" to "你好")), link.calls("terminal-input"))
            assertNull("送到了的不挂标", s.local().single().delivery)

            link.line("new", 0, human("u1", "你好"))
            runCurrent()
            assertEquals("记录里出现了这句人话 ⇒ 本地那句换成记录那一条", emptyList<RenderUnit.UserText>(), s.local())
            assertEquals(
                listOf("u1"),
                s.state.value.units
                    .map { it.key },
            )
        }

    /** 那台先定好 sid、第一句随起交进去了（`session-new` 的 `prompt`）。 */
    private fun said(prompt: Map<String, Any?>) = started + mapOf("sid" to "pre", "prompt" to prompt)

    @Test
    fun `新建会话：第一句随 session-new 交（那台作为启动参数交），送没送到照回包里的 prompt；不另走 terminal-input`() =
        runTest {
            val link = FakeCoreLink()
            link.answer = { cmd, _ ->
                when (cmd) {
                    "session-new" -> ok(said(mapOf("result" to "delivered", "why" to null, "said" to null)))
                    else -> Reply.LinkDown
                }
            }
            val s = draft(link)
            s.send("你好")
            runCurrent()
            assertEquals("你好", link.calls("session-new").single()?.get("prompt"))
            assertEquals("session-new 撤不动（阻塞档）：按阻塞档问，不带 within_ms、不撤单", listOf("session-new"), link.blockingCalls)
            assertEquals("pre", s.state.value.sid)
            assertTrue("第一句已随起交了，不再送一遍", link.calls("terminal-input").isEmpty())
            assertNull("送到了的不挂标", s.local().single().delivery)
        }

    @Test
    fun `新建会话（兜底）：那台说第一句没交到（refused，能改的）⇒ 那句标未送达、字照那台写的；再发一次走 terminal-input`() =
        runTest {
            val link = FakeCoreLink()
            link.answer = { cmd, _ ->
                when (cmd) {
                    "session-new" -> ok(said(mapOf("result" to "refused", "why" to "not_known", "said" to "终端已不在")))
                    "terminal-input" -> ok(mapOf("result" to "delivered"))
                    else -> Reply.LinkDown
                }
            }
            val s = draft(link)
            s.send("你好")
            runCurrent()
            val o = s.local().single()
            assertEquals(DeliveryState.FAILED, o.delivery)
            assertEquals(copyText("terminal.input.failed", "why" to "终端已不在"), o.deliveryText)
            s.retry(o.key)
            runCurrent()
            assertEquals(listOf(mapOf("sid" to "pre", "text" to "你好")), link.calls("terminal-input"))
            assertEquals(1, link.calls("session-new").size)
        }

    @Test
    fun `新起的会话记录还没落盘：不报找不到，等那台推来这条会话的记录路径再读；不按点重问`() =
        runTest {
            val link = FakeCoreLink()
            val file = FakeCoreLink.File()
            link.answer = { cmd, args ->
                when (cmd) {
                    "session-new" -> ok(said(mapOf("result" to "delivered", "why" to null, "said" to null)))
                    else -> file.answer(cmd, args) ?: Reply.LinkDown
                }
            }
            val s = draft(link)
            s.visible(true)
            s.send("你好")
            link.report("pre", "/h/proj", path = null) // 那台宣告这条会话时记录还没有
            advanceTimeBy(ChatSession.ARRIVAL_MS * 3)
            runCurrent()
            assertNull("记录还没有不是「找不到」", s.state.value.failedWhy)
            assertTrue("不去清单里按点找", link.calls("history-list").isEmpty())
            assertEquals(setOf("pre"), link.watching())
            file.records += human("u1", "你好")
            link.report("pre", "/h/proj") // 记录出现：那台推来带路径的状态帧（在那一行的 line 帧之前）
            runCurrent()
            link.line("pre", 0, human("u1", "你好"))
            runCurrent()
            file.records += reply("a1", "替身收到")
            link.line("pre", 1, reply("a1", "替身收到"))
            runCurrent()
            assertNull(s.state.value.failedWhy)
            assertEquals(listOf("u1", "a1#0"), s.state.value.units.map { it.key })
        }

    @Test
    fun `Codex 那台也落聊天屏：新建会话起的是 Codex，字同样经 terminal-input 进那条会话`() =
        runTest {
            val link = FakeCoreLink()
            link.answer = { cmd, _ ->
                when (cmd) {
                    "session-new" -> ok(started + ("agent" to "codex"))
                    "terminal-input" -> ok(mapOf("result" to "delivered"))
                    "history-tail" -> ok(mapOf("end" to 0, "split_at" to 0, "tail_from" to 0, "total" to 0))
                    else -> ok(mapOf("rows" to emptyList<Any>(), "next" to 0, "eof" to true))
                }
            }
            val s = draft(link, agent = "codex")
            s.send("看一下")
            runCurrent()
            assertEquals("codex", link.calls("session-new").single()?.get("agent"))
            link.report("cx", "/h/proj")
            runCurrent()
            assertEquals(listOf(mapOf("sid" to "cx", "text" to "看一下")), link.calls("terminal-input"))
            assertNull("送到了的不挂标", s.local().single().delivery)
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
                    "history-tail" -> ok(mapOf("end" to 200, "split_at" to 100, "tail_from" to 0, "total" to 2))
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
            link.line("s1", 1, reply("a1", "答")) // 读到的那一行，流上又报一次
            link.line("other", 2, reply("x", "别的会话"))
            link.line("s1", 2, reply("a2", "又一句"))
            runCurrent()
            assertEquals(
                listOf("u1", "a1#0", "a2#0"),
                s.state.value.units
                    .map { it.key },
            )
        }

    @Test
    fun `进出会话 · 断线重接以后，看到的行和全看时逐条一样；在屏上才进 stream-watch 名单`() =
        runTest {
            val link = FakeCoreLink()
            val file = FakeCoreLink.File()

            fun add(n: Int) = (file.records.size until file.records.size + n).forEach { i -> file.records += if (i % 2 == 0) human("u$i", "问$i") else reply("a$i", "答$i") }

            suspend fun write(n: Int) = repeat(n) { add(1).also { link.line("s1", file.records.lastIndex.toLong(), file.records.last()) } }

            link.answer = { cmd, args -> file.answer(cmd, args) ?: Reply.LinkDown }
            link.report("s1", "/w")
            add(5)
            val s = existing(link)
            s.visible(true)
            runCurrent()
            assertEquals("在屏上 ⇒ 进名单", setOf("s1"), link.watching())
            write(1)
            runCurrent()

            s.visible(false) // 退出会话：名单里没它，它的行不上流
            runCurrent()
            assertEquals(emptySet<String>(), link.watching())
            add(2)

            s.visible(true) // 再进：from 说从第 7 行起上流，[6, 7) 按行号补
            runCurrent()
            link.from("s1", 7)
            write(1)
            runCurrent()
            link.line("s1", 4, file.records[4]) // 重叠处按行号去重
            runCurrent()

            add(2) // 断开那一段的两行没收到
            link.epoch.value += 1 // 换了一条流
            runCurrent()
            add(1) // 接上以后先来的一行比已有的新、中间缺着
            link.line("s1", file.records.lastIndex.toLong(), file.records.last())
            runCurrent()
            assertEquals(RecordUnits.of(file.records), s.state.value.units)
            assertTrue("补段走的是按行号取", link.calls("history-lines").isNotEmpty())
            assertNull(s.state.value.failedWhy)
        }

    @Test
    fun `点进会话那一刻流不通：不报离线，接上以后照常出第一屏`() =
        runTest {
            val link = FakeCoreLink()
            val file = FakeCoreLink.File(mutableListOf(human("u0", "问"), reply("a1", "答")))
            var up = false
            link.answer = { cmd, args -> if (up) file.answer(cmd, args) ?: Reply.LinkDown else Reply.TimedOut }
            link.report("s1", "/w")
            val s = existing(link)
            runCurrent()
            up = true
            link.epoch.value += 1
            runCurrent()
            assertEquals(RecordUnits.of(file.records), s.state.value.units)
            assertNull("接上以后那一句撤掉", s.state.value.failedWhy)
        }

    @Test
    fun `送达未知 · 画面变了 · 会话已结束：各是各的标，字照文案表`() =
        runTest {
            val link = FakeCoreLink()
            val results = ArrayDeque(listOf(mapOf("result" to "unsure"), mapOf("result" to "refused", "why" to "screen_changed", "said" to "画面已变"), mapOf("result" to "refused", "why" to "ended", "said" to "已结束")))
            link.answer = { cmd, _ -> if (cmd == "terminal-input") ok(results.removeFirst()) else Reply.LinkDown }
            val s = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope)
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
            ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope).stop()
            runCurrent()
            assertEquals(listOf(mapOf("sid" to "s1", "key" to "esc")), link.calls("terminal-input"))
        }
}

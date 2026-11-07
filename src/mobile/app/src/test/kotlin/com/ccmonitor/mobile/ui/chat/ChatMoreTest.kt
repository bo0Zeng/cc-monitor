package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.HistoryAnnotate
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionsStop
import com.ccmonitor.mobile.ui.Faces
import com.ccmonitor.mobile.ui.Wires
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.human
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.reply
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 聊天屏 ⋯ 里的四面：会话里找 · 大纲 · 结束会话 · 无人答时自动答。判定都在那台（命中 · 清单 · `can.end` · 几档），这里只看手机照摆、交的形对得上。
 * 替身记录第 i 行占字节 `[10i, 10i+10)`（[FakeCoreLink.File]）。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatMoreTest {
    private val file = FakeCoreLink.File((0 until 300).map { if (it % 2 == 0) human("u$it", "问 $it") else reply("a$it", "答 $it") }.toMutableList(), pageLines = 50)

    private class Opened(
        val link: FakeCoreLink,
        val session: ChatSession,
        val more: ChatMore,
    ) {
        val ui: MoreUi get() = more.state.value
        val found: FindUi get() = ui.find!!
        val outline: OutlineUi get() = ui.outline!!
        val auto: AutoUi get() = ui.auto!!
        val chat: ChatUiState get() = session.state.value

        fun readAt(): Any? = link.calls("history-read").first()!!["offset"]
    }

    /** 那台怎么答：命中那条命令 ⇒ 那一份；别的照替身记录答。 */
    private fun answers(vararg by: Pair<String, Reply>): (String, Map<String, Any?>?) -> Reply? = { cmd, _ -> by.toMap()[cmd] }

    private fun TestScope.opened(
        face: String = "live-idle",
        extra: (String, Map<String, Any?>?) -> Reply? = { _, _ -> null },
    ): Opened {
        val link = FakeCoreLink()
        link.report("s1", "/h/proj", Faces.of(face))
        link.answer = { cmd, args -> extra(cmd, args) ?: file.answer(cmd, args) ?: Reply.LinkDown }
        val s = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope)
        val more = ChatMore(link, "box", backgroundScope, { "s1" }, s::jump)
        s.open()
        runCurrent()
        return Opened(link, s, more)
    }

    private fun found(
        total: Long,
        hits: List<Map<String, Any?>>,
        head: String,
        widen: Boolean = false,
    ) = Reply.Ok(mapOf("total" to total, "hits" to hits, "head" to head, "widen" to widen))

    private fun hit(line: Long) = mapOf("uuid" to "u$line", "line" to line, "kind" to "user", "before" to "", "matched" to "问", "after" to " $line", "turn" to 1L, "tsMs" to 0L, "tsText" to "", "tool" to null)

    @Test
    fun `⋯ 的清单：收到脸 · 有运行表 · 知道目录 ⇒ 七项照这个顺序；缺哪样少哪一项`() =
        runTest {
            val o = opened()
            val all =
                listOf(
                    ChatMoreItem.Agents,
                    ChatMoreItem.Rename,
                    ChatMoreItem.Rotation,
                    ChatMoreItem.OpenDir,
                    ChatMoreItem.FindOutline,
                    ChatMoreItem.AutoAnswer,
                    ChatMoreItem.End,
                )
            assertEquals(all, chatMoreItems(agents = true, openDir = true, more = o.more))
            assertEquals("没运行表 ⇒ 没有 agent", all - ChatMoreItem.Agents, chatMoreItems(agents = false, openDir = true, more = o.more))
            assertEquals("不知道目录 ⇒ 没有打开工作目录", all - ChatMoreItem.OpenDir, chatMoreItems(agents = true, openDir = false, more = o.more))
            val faceless = ChatMore(o.link, "box", backgroundScope, { "no-face" }, o.session::jump)
            assertEquals(
                "没收到脸 ⇒ 没有自动答与结束会话",
                all - ChatMoreItem.AutoAnswer - ChatMoreItem.End,
                chatMoreItems(agents = true, openDir = true, more = faceless),
            )
        }

    @Test
    fun `找：交那条记录的路径与查询串；命中照那台的先后摆，总数照那台`() =
        runTest {
            val o = opened(extra = { cmd, _ -> if (cmd == "history-find") found(9L, listOf(hit(100), hit(120)), "前 2 · 共 9") else null })
            o.more.find("问")
            runCurrent()
            assertEquals(listOf(mapOf("path" to "/p/s1.jsonl", "query" to "问")), o.link.calls("history-find"))
            val f = o.more.state.value.find!!
            assertEquals(listOf(100L, 120L), f.hits.map { it.line })
            assertEquals("结果头是那台写好的那一句", "前 2 · 共 9", f.head)
            assertEquals(false, f.widen)
        }

    @Test
    fun `范围选用户 ⇒ 交 scope user；那台说可放宽 ⇒ 给搜全部，点了回全部、词不变、再问一次`() =
        runTest {
            val o = opened(extra = { cmd, args -> if (cmd == "history-find") (if (args?.get("scope") == "user") found(0L, emptyList(), "无匹配「问」 · 仅用户", widen = true) else found(1L, listOf(hit(100)), "匹配 1")) else null })
            o.more.find("问")
            runCurrent()
            o.more.scope(user = true)
            runCurrent()
            assertEquals(mapOf("path" to "/p/s1.jsonl", "query" to "问", "scope" to "user"), o.link.calls("history-find").last())
            assertEquals("无匹配「问」 · 仅用户", o.found.head)
            assertTrue("那台说可放宽 ⇒ 给［搜全部］", o.found.widen)
            assertTrue(o.found.user)
            o.more.widen()
            runCurrent()
            assertEquals(mapOf("path" to "/p/s1.jsonl", "query" to "问"), o.link.calls("history-find").last())
            assertEquals("匹配 1", o.found.head)
            assertEquals(false, o.found.user)
            assertEquals(false, o.found.widen)
        }

    @Test
    fun `找到 ⇒ 点一条跳到那一行：先问那一行的起点，再从那里读`() =
        runTest {
            val o = opened(extra = { cmd, _ -> if (cmd == "history-find") found(1L, listOf(hit(100)), "匹配 1") else null })
            o.more.find("问")
            runCurrent()
            o.link.calls.clear()
            o.more.pickHit(o.found.hits.single())
            runCurrent()
            assertEquals("问的是一行都不交的那一段", listOf(mapOf("path" to "/p/s1.jsonl", "from" to 100L, "until" to 100L)), o.link.calls("history-lines").take(1))
            assertEquals("从那一行的起点读", 1000L, o.readAt())
            val first = o.chat.units.first()
            assertEquals("落点是那一行那张卡", first.key, o.chat.landing)
            assertNull("点了就收起", o.ui.find)
        }

    @Test
    fun `大纲：问人说过的话那份清单，点一条照它的起点跳`() =
        runTest {
            val rows = listOf(mapOf("at" to 0L, "line" to 0L, "uuid" to "u0", "timestamp" to "", "excerpt" to "问 0"), mapOf("at" to 2000L, "line" to 200L, "uuid" to "u200", "timestamp" to "", "excerpt" to "问 200"))
            val o = opened(extra = { cmd, _ -> if (cmd == "history-user-inputs") Reply.Ok(mapOf("from" to 0L, "end" to 3000L, "next" to null, "entries" to rows)) else null })
            o.more.openOutline()
            runCurrent()
            assertEquals(listOf(mapOf("path" to "/p/s1.jsonl")), o.link.calls("history-user-inputs"))
            assertEquals(listOf("问 0", "问 200"), o.outline.rows.map { it.excerpt })
            o.link.calls.clear()
            o.more.pickSaid(o.outline.rows[1])
            runCurrent()
            assertEquals(2000L, o.readAt())
            assertNull(o.ui.outline)
        }

    @Test
    fun `can_end 是 no ⇒ 没有结束：灰着写那台那一句，点了也不交`() =
        runTest {
            val o = opened(face = "ended")
            assertEquals(EndEntry.Blocked("会话已结束"), o.more.endEntry())
            o.more.askEnd("t")
            o.more.confirmEnd()
            runCurrent()
            assertNull("不开确认框", o.more.state.value.ending)
            assertTrue("不交 sessions-stop", o.link.calls(SessionsStop.COMMAND).isEmpty())
        }

    @Test
    fun `can_end 是 yes ⇒ 确认框列这条的名字；确认了交 sids 与自报的前端`() =
        runTest {
            val o = opened(extra = { cmd, _ -> if (cmd == "sessions-stop") Reply.Ok(mapOf("results" to listOf(outcome("done", null)))) else null })
            assertEquals(EndEntry.Ready, o.more.endEntry())
            o.more.askEnd("proj · t")
            assertEquals(EndAsk(listOf("proj · t")), o.ui.ending)
            o.more.confirmEnd()
            runCurrent()
            assertEquals(listOf(mapOf("sids" to listOf("s1"), "client" to "mobile")), o.link.calls(SessionsStop.COMMAND))
            assertNull(o.more.state.value.ending)
            assertNull("做成了不说话（变已结束由会话流兜）", o.more.state.value.note)
        }

    @Test
    fun `结束没成 ⇒ 那台写好的那一句`() =
        runTest {
            val o = opened(extra = { cmd, _ -> if (cmd == "sessions-stop") Reply.Ok(mapOf("results" to listOf(outcome("failed", "那台那一句")))) else null })
            o.more.askEnd("t")
            o.more.confirmEnd()
            runCurrent()
            assertEquals("那台那一句", o.more.state.value.note)
        }

    @Test
    fun `自动答：开时照会话事实列几档、勾在此刻那一档；设了 ⇒ 交的那一份与拼入参的口同形`() =
        runTest {
            val facts = Wires.facts("path" to "/p/s1.jsonl", "autoAnswer" to 5, "autoAnswerChoices" to listOf(mapOf("min" to 0, "text" to "关"), mapOf("min" to 5, "text" to "5 分钟")))
            val o = opened(extra = answers("history-facts" to Reply.Ok(facts), "history-annotate" to Reply.Ok(mapOf("prev" to emptyMap<String, Any?>()))))
            o.more.openAuto()
            runCurrent()
            val a = o.auto
            assertEquals(listOf("关", "5 分钟"), a.choices.map { it.text })
            assertEquals(5, a.now)
            o.more.setAuto(0)
            runCurrent()
            assertEquals(listOf(HistoryAnnotate.autoAnswer("s1", 0)), o.link.calls(HistoryAnnotate.COMMAND))
            assertNull("交成了就收起", o.more.state.value.auto)
        }

    @Test
    fun `自动答没交成 ⇒ 那一句，几档照旧摆着`() =
        runTest {
            val facts = Wires.facts("path" to "/p/s1.jsonl", "autoAnswer" to null, "autoAnswerChoices" to listOf(mapOf("min" to 0, "text" to "关"), mapOf("min" to 5, "text" to "5 分钟")))
            val o = opened(extra = answers("history-facts" to Reply.Ok(facts), "history-annotate" to FakeCoreLink.failed("那台不认")))
            o.more.openAuto()
            runCurrent()
            assertEquals("没设 ⇒ 勾在关", 0, o.auto.now)
            o.more.setAuto(5)
            runCurrent()
            assertEquals("那台不认", o.ui.note)
            assertEquals(2, o.auto.choices.size)
        }

    private fun outcome(
        state: String,
        said: String?,
    ) = mapOf("machine" to "local", "sid" to "s1", "outcome" to state, "why" to said?.let { "x" }, "said" to said, "copyDetail" to "", "title" to "t", "session" to null)
}

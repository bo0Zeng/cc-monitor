package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.AutoAnswerSet
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
    )

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

    private fun hit(line: Long) = mapOf("uuid" to "u$line", "line" to line, "kind" to "user", "before" to "", "matched" to "问", "after" to " $line", "turn" to 1L, "tsMs" to 0L, "tsText" to "", "tool" to null)

    @Test
    fun `找：交那条记录的路径与查询串；命中照那台的先后摆，总数照那台`() =
        runTest {
            val o = opened(extra = { cmd, _ -> if (cmd == "history-find") Reply.Ok(mapOf("total" to 9L, "hits" to listOf(hit(100), hit(120)))) else null })
            o.more.find("问")
            runCurrent()
            assertEquals(listOf(mapOf("path" to "/p/s1.jsonl", "query" to "问")), o.link.calls("history-find"))
            val f = o.more.state.value.find!!
            assertEquals(listOf(100L, 120L), f.hits.map { it.line })
            assertEquals(9L, f.total)
        }

    @Test
    fun `找到 ⇒ 点一条跳到那一行：先问那一行的起点，再从那里读`() =
        runTest {
            val o = opened(extra = { cmd, _ -> if (cmd == "history-find") Reply.Ok(mapOf("total" to 1L, "hits" to listOf(hit(100)))) else null })
            o.more.find("问")
            runCurrent()
            o.link.calls.clear()
            o.more.pickHit(o.more.state.value.find!!.hits.single())
            runCurrent()
            assertEquals("问的是一行都不交的那一段", listOf(mapOf("path" to "/p/s1.jsonl", "from" to 100L, "until" to 100L)), o.link.calls("history-lines").take(1))
            assertEquals("从那一行的起点读", 1000L, o.link.calls("history-read").first()!!["offset"])
            assertEquals("落点是那一行那张卡", o.session.state.value.units.first().key, o.session.state.value.landing)
            assertNull("点了就收起", o.more.state.value.find)
        }

    @Test
    fun `大纲：问人说过的话那份清单，点一条照它的起点跳`() =
        runTest {
            val rows = listOf(mapOf("at" to 0L, "line" to 0L, "uuid" to "u0", "timestamp" to "", "excerpt" to "问 0"), mapOf("at" to 2000L, "line" to 200L, "uuid" to "u200", "timestamp" to "", "excerpt" to "问 200"))
            val o = opened(extra = { cmd, _ -> if (cmd == "history-user-inputs") Reply.Ok(mapOf("from" to 0L, "end" to 3000L, "next" to null, "entries" to rows)) else null })
            o.more.openOutline()
            runCurrent()
            assertEquals(listOf(mapOf("path" to "/p/s1.jsonl")), o.link.calls("history-user-inputs"))
            assertEquals(listOf("问 0", "问 200"), o.more.state.value.outline!!.rows.map { it.excerpt })
            o.link.calls.clear()
            o.more.pickSaid(o.more.state.value.outline!!.rows[1])
            runCurrent()
            assertEquals(2000L, o.link.calls("history-read").first()!!["offset"])
            assertNull(o.more.state.value.outline)
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
            assertEquals(listOf("proj · t"), o.more.state.value.ending!!.titles)
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
            val o = opened(extra = { cmd, _ -> if (cmd == "history-facts") Reply.Ok(facts) else if (cmd == "history-annotate") Reply.Ok(mapOf("prev" to emptyMap<String, Any?>())) else null })
            o.more.openAuto()
            runCurrent()
            val a = o.more.state.value.auto!!
            assertEquals(listOf("关", "5 分钟"), a.choices.map { it.text })
            assertEquals(5, a.now)
            o.more.setAuto(0)
            runCurrent()
            assertEquals(listOf(AutoAnswerSet.args("s1", 0)), o.link.calls(AutoAnswerSet.COMMAND))
            assertNull("交成了就收起", o.more.state.value.auto)
        }

    @Test
    fun `自动答没交成 ⇒ 那一句，几档照旧摆着`() =
        runTest {
            val facts = Wires.facts("path" to "/p/s1.jsonl", "autoAnswer" to null, "autoAnswerChoices" to listOf(mapOf("min" to 0, "text" to "关"), mapOf("min" to 5, "text" to "5 分钟")))
            val o = opened(extra = { cmd, _ -> if (cmd == "history-facts") Reply.Ok(facts) else if (cmd == "history-annotate") FakeCoreLink.failed("那台不认") else null })
            o.more.openAuto()
            runCurrent()
            assertEquals("没设 ⇒ 勾在关", 0, o.more.state.value.auto!!.now)
            o.more.setAuto(5)
            runCurrent()
            assertEquals("那台不认", o.more.state.value.note)
            assertEquals(2, o.more.state.value.auto!!.choices.size)
        }

    private fun outcome(
        state: String,
        said: String?,
    ) = mapOf("machine" to "local", "sid" to "s1", "outcome" to state, "why" to said?.let { "x" }, "said" to said, "copyDetail" to "", "title" to "t", "session" to null)
}

package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.model.RecordUnits
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.LineRecord
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.human
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.reply
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 从搜内容的一处命中打开：先读命中那一页，再往下读到尾、往上翻照旧；落点是命中那张卡。
 * 替身记录第 i 行占字节 `[10i, 10i+10)`（[FakeCoreLink.File]）。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatJumpTest {
    /** 300 行：一问一答轮着来；一页 50 行（往下读要几页）。 */
    private val file = FakeCoreLink.File((0 until 300).map { if (it % 2 == 0) human("u$it", "问 $it") else reply("a$it", "答 $it") }.toMutableList(), pageLines = 50)

    private fun TestScope.session(link: FakeCoreLink): ChatSession {
        link.report("s1", "/h/proj")
        link.answer = { cmd, args -> file.answer(cmd, args) ?: Reply.LinkDown }
        return ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope)
    }

    /** 读正文的那几问（`history-facts` 那一问是 chip 的，不算）。 */
    private fun FakeCoreLink.reads() = calls.filter { it.first in READS }

    private fun units(from: Int) = RecordUnits.of(file.records.drop(from).mapNotNull { Wire.bind<LineRecord>(it) })

    @Test
    fun `给了 at ⇒ 第一问是带这个 offset 的 history-read；读到尾、落点是命中那一张`() =
        runTest {
            val link = FakeCoreLink()
            val s = session(link)
            s.jump(1000, null)
            s.open()
            runCurrent()
            val first = link.reads().first()
            assertEquals("history-read", first.first)
            assertEquals(1000L, first.second!!["offset"])
            assertEquals("从命中那一行读到尾", units(100), s.state.value.units)
            assertEquals(units(100).first().key, s.state.value.landing)
            assertEquals("交读到的行数问尾段", listOf(200L), link.calls("history-tail").map { it!!["n"] })
            assertTrue(s.state.value.canLoadOlder)
        }

    @Test
    fun `往上翻接在命中那一段前面；之后流上的行照常接在后面`() =
        runTest {
            val link = FakeCoreLink()
            val s = session(link)
            s.jump(1000, null)
            s.open()
            s.visible(true)
            runCurrent()
            s.loadOlder()
            runCurrent()
            assertEquals("再往上一屏", units(40), s.state.value.units)
            assertEquals("落点不变", units(100).first().key, s.state.value.landing)
            file.records += human("u300", "新的一句")
            link.line("s1", 300, file.records.last(), 3010)
            runCurrent()
            assertEquals(units(40), s.state.value.units)
        }

    @Test
    fun `命中落在一次工具调用的卡里 ⇒ 落点是那张工具卡`() =
        runTest {
            val call = mapOf("agent" to "claude", "id" to "a101", "t" to "reply", "autoReply" to false, "blocks" to listOf(mapOf("type" to "tool_use", "id" to "c1", "name" to "Read", "input" to emptyMap<String, Any>())))
            file.records[101] = call
            val link = FakeCoreLink()
            val s = session(link)
            s.jump(1000, "c1")
            s.open()
            runCurrent()
            val card = s.state.value.units.filterIsInstance<RenderUnit.ToolCall>().single { it.toolUseId == "c1" }
            assertEquals(card.key, s.state.value.landing)
        }

    @Test
    fun `已开着的那条跳到一处命中 ⇒ 从命中处重读`() =
        runTest {
            val link = FakeCoreLink()
            val s = session(link)
            s.open()
            runCurrent()
            assertEquals("先是最后一屏", units(240), s.state.value.units)
            link.calls.clear()
            s.jump(500, null)
            runCurrent()
            assertEquals("history-read", link.reads().first().first)
            assertEquals(500L, link.reads().first().second!!["offset"])
            assertEquals(units(50), s.state.value.units)
            assertEquals(units(50).first().key, s.state.value.landing)
        }

    private companion object {
        val READS = setOf("history-read", "history-tail", "history-lines")
    }
}

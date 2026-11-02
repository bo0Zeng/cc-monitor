package com.ccmonitor.mobile.ui.agents

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.claude.model.StreamItem
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.human
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.reply
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * agent 屏的正文：按运行读（`history-run`）。收场的从头读（`from=0`），在跑的从尾巴读（`tail`）；往上翻拿这一页的 `from` 当 `before`、
 * 到了 0 不再问；往下读拿 `end` 当 `from`。栈里同一个运行只出现一次。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class AgentRunTest {
    private fun page(
        from: Long,
        end: Long,
        more: Boolean,
        vararg records: Map<String, Any?>,
    ) = mapOf("run" to "r1", "path" to "/p/s1/subagents/r1.jsonl", "rows" to records.map { mapOf("record" to it) }, "from" to from, "end" to end, "more" to more)

    private fun TestScope.open(
        link: FakeCoreLink,
        running: Boolean,
    ) = AgentRun(link, "box", "/p/s1.jsonl", "r1", running, backgroundScope).also {
        it.open()
        runCurrent()
    }

    private fun texts(stream: List<StreamItem>): List<String> =
        stream.mapNotNull { (it as? StreamItem.Card)?.unit }.map {
            when (it) {
                is RenderUnit.UserText -> it.text
                is RenderUnit.AssistantMarkdown -> it.text
                else -> it.key
            }
        }

    @Test
    fun `收场的那一个首屏读第一页（from=0），在跑的读最后一页（tail）`() =
        runTest {
            val ended = FakeCoreLink().apply { answer = { _, _ -> Reply.Ok(page(0, 20, false, human("u1", "派的话"))) } }
            open(ended, running = false)
            assertEquals(listOf(mapOf("parent" to "/p/s1.jsonl", "run" to "r1", "from" to 0L)), ended.calls("history-run"))

            val running = FakeCoreLink().apply { answer = { _, _ -> Reply.Ok(page(100, 200, false, reply("a1", "在做"))) } }
            val r = open(running, running = true)
            assertEquals(listOf(mapOf("parent" to "/p/s1.jsonl", "run" to "r1", "tail" to true)), running.calls("history-run"))
            assertEquals(listOf("在做"), texts(r.state.value.stream))
        }

    @Test
    fun `往上翻拿这一页的 from 当 before、更早的接在前面；到了 0 不再问`() =
        runTest {
            val link = FakeCoreLink()
            link.answer = { _, args ->
                if (args?.get("before") == 100L) Reply.Ok(page(0, 100, true, human("u1", "派的话"))) else Reply.Ok(page(100, 200, false, reply("a1", "在做")))
            }
            val r = open(link, running = true)
            assertTrue(r.state.value.canOlder)
            r.older()
            runCurrent()
            assertEquals(mapOf("parent" to "/p/s1.jsonl", "run" to "r1", "before" to 100L), link.calls("history-run").last())
            assertEquals(listOf("派的话", "在做"), texts(r.state.value.stream))
            assertFalse(r.state.value.canOlder)
            r.older()
            runCurrent()
            assertEquals(2, link.calls("history-run").size)
        }

    @Test
    fun `往下读拿 end 当 from、新的接在后面`() =
        runTest {
            val link = FakeCoreLink()
            link.answer = { _, args ->
                if (args?.get("from") == 20L) Reply.Ok(page(20, 40, false, reply("a1", "做完了"))) else Reply.Ok(page(0, 20, true, human("u1", "派的话")))
            }
            val r = open(link, running = false)
            r.newer()
            runCurrent()
            assertEquals(mapOf("parent" to "/p/s1.jsonl", "run" to "r1", "from" to 20L), link.calls("history-run").last())
            assertEquals(listOf("派的话", "做完了"), texts(r.state.value.stream))
        }

    @Test
    fun `读不成 ⇒ 那一句上屏，正文不动`() =
        runTest {
            val link = FakeCoreLink().apply { answer = { _, _ -> FakeCoreLink.failed("读不动那份记录") } }
            val r = open(link, running = false)
            assertEquals("读不动那份记录", r.state.value.failed)
            assertTrue(
                r.state.value.stream
                    .isEmpty(),
            )
        }

    @Test
    fun `栈里同一个运行只出现一次：已在 ⇒ 退回到它，就在顶上 ⇒ 不动，不在 ⇒ 压一层`() {
        val stack = mutableListOf<String>()
        val popTo = { r: String ->
            val i = stack.lastIndexOf(r)
            if (i >= 0) {
                while (stack.size > i + 1) stack.removeAt(stack.lastIndex)
            }
            i >= 0
        }

        fun open(r: String) = AgentStack.open(stack.lastOrNull(), r, popTo) { stack += it }
        open("r1")
        open("r2")
        open("r3")
        assertEquals(listOf("r1", "r2", "r3"), stack)
        open("r3")
        assertEquals(listOf("r1", "r2", "r3"), stack)
        open("r1")
        assertEquals(listOf("r1"), stack)
        open("r2")
        assertEquals(listOf("r1", "r2"), stack)
    }
}

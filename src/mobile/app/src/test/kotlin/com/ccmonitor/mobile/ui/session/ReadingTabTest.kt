package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.model.RecordUnits
import com.ccmonitor.mobile.ui.chat.ChatController
import com.ccmonitor.mobile.ui.chat.ChatTarget
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.human
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.reply
import com.ccmonitor.mobile.ui.chat.chatKey
import com.ccmonitor.mobile.ui.chat.existingChat
import com.ccmonitor.mobile.ui.chat.sessionOf
import com.ccmonitor.mobile.ui.overview.HistoryItem
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test

/**
 * 终端屏的「阅读」子页：读哪一条照核心的清单挑；读出来的是聊天屏那一条 [com.ccmonitor.mobile.ui.chat.ChatSession]（同一个 key、同一份数据）。
 * 判据：同一个会话，子页和聊天屏渲染出的单元逐条一样。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ReadingTabTest {
    private fun row(
        sid: String,
        dir: String?,
    ) = HistoryItem(sessionId = sid, label = sid, atText = null, agentTag = null, projectPath = dir, status = null)

    private val rows = listOf(row("a", "/h/x"), row("b", "/h/y"), row("c", "/h/y"))

    @Test
    fun `读哪一条：钉住的 ＞ resume 的那条 ＞ 这个目录最近的一条（清单的先后照核心）`() {
        assertEquals("p", readingSid(rows, pinned = "p", resumed = "r", cwd = "/h/y"))
        assertEquals("r", readingSid(rows, pinned = null, resumed = "r", cwd = "/h/y"))
        assertEquals("b", readingSid(rows, pinned = null, resumed = null, cwd = "/h/y"))
        assertNull("这个目录没有会话 ⇒ 不拿别的目录顶", readingSid(rows, pinned = null, resumed = null, cwd = "/h/z"))
        assertEquals("不知道目录 ⇒ 这台最近的一条", "a", readingSid(rows, pinned = null, resumed = null, cwd = null))
        assertNull(readingSid(emptyList(), pinned = null, resumed = null, cwd = null))
    }

    @Test
    fun `同一个会话：子页与聊天屏取到同一条、渲染出的单元逐条一样`() =
        runTest {
            val records = listOf(human("u1", "看一下"), reply("r1", "好的"))
            val link = FakeCoreLink()
            link.answer = { cmd, _ ->
                when (cmd) {
                    "history-list" -> Reply.Ok(mapOf("rows" to listOf(mapOf("sessionId" to "s1", "label" to "s1", "jsonlPath" to "/p/s1.jsonl"))))
                    "history-tail" -> Reply.Ok(mapOf("end" to 20, "split_at" to 0))
                    "history-read" -> Reply.Ok(mapOf("rows" to records.mapIndexed { i, r -> mapOf("record" to r, "end" to (i + 1) * 10) }, "next" to 20, "eof" to true))
                    else -> Reply.LinkDown
                }
            }
            val controller = ChatController(scopeFactory = { CoroutineScope(backgroundScope.coroutineContext + SupervisorJob()) })
            val chat = existingChat("h", "s1", "box")
            assertEquals(chatKey("h", "s1"), chat.key)
            assertEquals(ChatTarget.Existing("s1"), chat.target)

            val fromChat = controller.sessionOf(chat) { link }.also { it.open() }
            val fromReading = controller.sessionOf(readingChat("h", "s1", "box")) { link }.also { it.open() }
            runCurrent()

            assertSame("同一个 key ⇒ 同一条会话，不另读一份", fromChat, fromReading)
            assertEquals(RecordUnits.of(records), fromReading.state.value.units)
            assertEquals(fromChat.state.value.units, fromReading.state.value.units)
            assertEquals("只读了一趟", 1, link.calls("history-tail").size)
        }
}

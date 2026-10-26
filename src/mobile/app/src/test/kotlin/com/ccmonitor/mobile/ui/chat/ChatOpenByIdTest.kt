package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.CoreFailure
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.ui.Wires
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.human
import com.ccmonitor.mobile.ui.chat.FakeCoreLink.Companion.reply
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 打开一条会话表里没有的会话（不在跑了）：记录路径与目录都问那台的 `history-facts {sid}`（那台按 sid 找记录、回包带它找到的 `path`），
 * 不整份问 `history-list` 再自己按 sid 找那一行。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatOpenByIdTest {
    private val file = FakeCoreLink.File(mutableListOf(human("u1", "你好"), reply("a1", "在")))

    @Test
    fun `会话表里没有 ⇒ 问 history-facts 的 sid、照回包的 path 读正文、目录取 projectDir；不问 history-list`() =
        runTest {
            val link = FakeCoreLink()
            link.answer = { cmd, args ->
                when {
                    cmd == "history-facts" && args == mapOf("sid" to "s1") -> Reply.Ok(Wires.facts("path" to "/p/s1.jsonl", "projectDir" to "/h/proj"))
                    cmd == "history-facts" -> Reply.Ok(Wires.facts("projectDir" to "/h/proj"))
                    else -> file.answer(cmd, args) ?: Reply.LinkDown
                }
            }
            val s = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope).also { it.open() }
            runCurrent()
            assertTrue("认一条会话不整份问清单", link.calls("history-list").isEmpty())
            assertEquals(mapOf("sid" to "s1"), link.calls("history-facts").first())
            assertEquals("/p/s1.jsonl", link.calls("history-tail").single()!!["path"])
            val units = s.state.value.units
            assertEquals(listOf("u1", "a1#0"), units.map { it.key })
            assertEquals("/h/proj", s.state.value.cwd)
        }

    @Test
    fun `那台找不到这条会话的记录 ⇒ 上屏那台写好的那一句`() =
        runTest {
            val link = FakeCoreLink()
            link.answer = { cmd, _ ->
                if (cmd == "history-facts") Reply.Failed(CoreFailure("no_such_session", "未找到此会话的记录", "")) else Reply.LinkDown
            }
            val s = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope).also { it.open() }
            runCurrent()
            assertTrue(link.calls("history-list").isEmpty())
            assertEquals("未找到此会话的记录", s.state.value.failedWhy)
        }
}

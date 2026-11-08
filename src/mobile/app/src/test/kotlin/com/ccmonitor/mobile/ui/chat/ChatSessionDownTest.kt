package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.MachineStateKind
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 新建会话发第一句时那台断着（核心那一行是 Down）：不起会话 —— 不照默认选项起在家目录 · 默认那一家；
 * 字回到输入框，照核心那一句说那台断着；那台连上以后再发，照正常选项起。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatSessionDownTest {
    @Test
    fun `断着发第一句 ⇒ 不起会话、字回到输入框、照核心那一句说；接回再发才起`() =
        runTest {
            val link = FakeCoreLink()
            link.state.value = linkDown("x")
            val s = ChatSession(link, link, ChatTarget.Draft(ticket = "t-1", cwd = "~/proj"), "box", backgroundScope)
            s.send("你好")
            runCurrent()
            assertEquals("断着不许起会话", 0, link.calls("session-open").size)
            val ui = s.state.value
            assertEquals("字回到输入框", "你好", ui.recall?.text)
            assertTrue("待发那一排里没有它", ui.outbox.isEmpty())
            assertEquals("照核心那一句说", link.offline.value, ui.failedWhy)
            assertTrue("还是新建会话的草稿（选项在）", ui.drafting)

            link.state.value = LinkLine.Up(1, machineOf(MachineStateKind.UP))
            link.epoch.value = 1
            runCurrent()
            assertEquals("接回不自己起", 0, link.calls("session-open").size)
            link.answer = { _, _ -> Reply.LinkDown }
            s.send("你好")
            runCurrent()
            assertEquals("接回之后再发照常起", 1, link.calls("session-open").size)
        }
}

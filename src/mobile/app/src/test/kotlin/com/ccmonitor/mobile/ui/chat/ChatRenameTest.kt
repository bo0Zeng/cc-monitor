package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.ui.Faces
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * 会话页叫这条会话什么 · 在会话页改标题：名字是那台定的（会话簿那一格 `label`，状态帧来了就换），手机不自己挑；
 * 改标题交那台 `history-annotate {customTitle}`（历史清单「改标题」同一格），名字等会话簿换。夹具只造结构（sid · 占位名）。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatRenameTest {
    private fun TestScope.existing(link: FakeCoreLink) = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope).also { it.open() }

    private fun FakeCoreLink.named(label: String) {
        val seat = table.value.live.getValue("s1")
        table.value = table.value.copy(live = table.value.live + ("s1" to seat.copy(label = label)))
    }

    @Test
    fun `顶栏的名字照会话簿那一格，状态帧来了就换`() =
        runTest {
            val link = FakeCoreLink()
            link.report("s1", "/w", Faces.of("live-idle"))
            val s = existing(link)
            runCurrent()
            assertEquals("s1", s.state.value.label)
            link.named("修登录")
            runCurrent()
            assertEquals("修登录", s.state.value.label)
        }

    @Test
    fun `改标题交那台 customTitle（去掉首尾空白）；名字不先改；没交成 ⇒ chip 上方那一句`() =
        runTest {
            val link = FakeCoreLink()
            link.report("s1", "/w", Faces.of("live-idle"))
            link.answer = { cmd, _ -> if (cmd == "history-annotate") Reply.Ok(mapOf("entry" to emptyMap<String, Any?>(), "prev" to null, "prev_title" to null)) else Reply.LinkDown }
            val s = existing(link)
            runCurrent()
            s.rename("  修登录 ")
            runCurrent()
            assertEquals(listOf(mapOf("sid" to "s1", "patch" to mapOf("customTitle" to "修登录"))), link.calls("history-annotate"))
            assertEquals("名字等会话簿换", "s1", s.state.value.label)
            assertNull(s.state.value.note)
            link.answer = { _, _ -> Reply.LinkDown }
            s.rename("")
            runCurrent()
            assertEquals(mapOf("sid" to "s1", "patch" to mapOf("customTitle" to "")), link.calls("history-annotate").last())
            assertNotNull("没交成 ⇒ 说一句", s.state.value.note)
        }
}

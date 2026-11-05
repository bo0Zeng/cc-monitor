package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.wire.gen.AgentPick
import com.ccmonitor.mobile.ui.Wires
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * 聊天屏上几格各只照一个源：chip 的模型 / 权限字照会话事实（换的回包不写字）· 工作目录照会话事实的 `projectDir` ·
 * 输入队头上离线那一行只在断着时说。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatOneSourceTest {
    private fun TestScope.existing(
        link: FakeCoreLink,
        bookDir: String = "/book/dir",
        answer: (String, Map<String, Any?>?) -> Reply?,
    ): ChatSession {
        link.report("s1", bookDir)
        link.answer = { cmd, args -> answer(cmd, args) ?: Reply.LinkDown }
        return ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope).also {
            it.open()
            it.visible(true)
        }
    }

    @Test
    fun `换过一次模型后会话事实再来 ⇒ chip 上是事实那一个，不是回包那一个`() =
        runTest {
            val link = FakeCoreLink()
            var model = "事实·Opus 4.5"
            var pick = "opus"
            val s =
                existing(link) { cmd, _ ->
                    when (cmd) {
                        "history-facts" -> Reply.Ok(Wires.facts("modelText" to model, "modelPick" to pick, "permissionModeText" to "事实·档"))
                        "session-model" -> Reply.Ok(mapOf("result" to "done", "model" to "回包·Sonnet", "said" to null))
                        else -> null
                    }
                }
            runCurrent()
            assertEquals("事实·Opus 4.5", s.state.value.model.text)
            // 那台换成了、可终端那边随后又换了一次：会话事实是此刻那一个。
            model = "事实·Haiku"
            pick = "haiku"
            s.switchModel(AgentPick("sonnet", "Sonnet"))
            runCurrent()
            assertNull(s.state.value.model.pending)
            assertEquals("字照会话事实", "事实·Haiku", s.state.value.model.text)
            assertEquals("抽屉勾哪一项也照会话事实", "haiku", s.state.value.model.value)
        }

    @Test
    fun `工作目录只照会话事实的 projectDir：会话簿那一格先到也不算`() =
        runTest {
            val link = FakeCoreLink()
            val s =
                existing(link, bookDir = "/book/dir") { cmd, _ ->
                    if (cmd == "history-facts") Reply.Ok(Wires.facts("projectDir" to "/facts/dir")) else null
                }
            runCurrent()
            assertEquals("/facts/dir", s.state.value.cwd)
        }

    @Test
    fun `那台连着 ⇒ 输入队头上不说离线那一句`() =
        runTest {
            val link = FakeCoreLink()
            val s = existing(link) { _, _ -> null }
            runCurrent()
            assertNull("连着 ⇒ 没有离线那一句", s.state.value.offline)
            link.state.value = linkDown("断了")
            runCurrent()
            assertEquals(FakeCoreLink.OFFLINE, s.state.value.offline)
        }
}

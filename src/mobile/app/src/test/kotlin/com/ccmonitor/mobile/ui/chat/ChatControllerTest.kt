package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.Tone
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 应用级的对话持有者：同一个 key 同一条对话、离屏不断；持有的连接成对还回去；逐出不碰在跑的。
 * 「在跑」是那台判的（会话帧语气 `now`），不是手机自己数的。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ChatControllerTest {
    private val link = FakeCoreLink()

    /** 每条对话一个作用域，挂在 `backgroundScope` 下：挂在测试 scope 上 `runTest` 会一直等它们；共用一个的话收一条就全收了。 */
    private fun TestScope.controller(holder: ConnectionHolder? = null) =
        ChatController(scopeFactory = { CoroutineScope(backgroundScope.coroutineContext + Job(backgroundScope.coroutineContext[Job])) }, connections = holder)

    private fun ChatController.open(
        key: String,
        hostId: String? = null,
    ): ChatSession = sessionFor(key, hostId) { scope -> ChatSession(link, ChatTarget.Existing(key), "box", scope).also { it.open() } }

    private class Log : ConnectionHolder {
        val log = mutableListOf<String>()

        override fun retain(
            hostId: String,
            holder: String,
        ) {
            log += "retain $hostId/$holder"
        }

        override fun release(
            hostId: String,
            holder: String,
        ) {
            log += "release $hostId/$holder"
        }
    }

    @Test
    fun `同一个 key 拿到同一条对话，不同 key 不同`() =
        runTest {
            val c = controller()
            assertSame(c.open("a"), c.open("a"))
            assertNotSame(c.open("a"), c.open("b"))
        }

    @Test
    fun `建对话持有那条连接，同 key 再取不重复持有；收掉才放手，holder 逐字一致`() =
        runTest {
            val h = Log()
            val c = controller(h)
            c.open("h1/s1", "h1")
            c.open("h1/s1", "h1")
            assertEquals(listOf("retain h1/chat-h1/s1"), h.log)
            assertTrue(c.release("h1/s1"))
            assertFalse("收过一次就没有了", c.release("h1/s1"))
            assertEquals(listOf("retain h1/chat-h1/s1", "release h1/chat-h1/s1"), h.log)
        }

    @Test
    fun `全部收掉：每条持有都还回去`() =
        runTest {
            val h = Log()
            val c = controller(h)
            c.open("h1/a", "h1")
            c.open("h1/b", "h1")
            c.releaseAll()
            assertEquals(2, h.log.count { it.startsWith("release") })
        }

    @Test
    fun `在跑跟着那台的会话帧；逐出只挑不在跑的，逐出的那条也还持有`() =
        runTest {
            val h = Log()
            val c = controller(h)
            link.report("busy", "/w", Tone.NOW)
            link.answer = { _, _ -> Reply.LinkDown }
            val busy = c.open("busy", "h")
            runCurrent()
            assertTrue("那台说在跑 ⇒ 有在飞的", c.hasInFlightTurn.value)
            repeat(ChatController.MAX_SESSIONS) { c.open("idle-$it", "h") }
            runCurrent()
            assertTrue("前提：确实逐出过", c.evictedCount > 0)
            assertSame("在跑的不逐", busy, c.open("busy", "h"))
            assertEquals("持有数与还活着的对话数一致", ChatController.MAX_SESSIONS, h.log.count { it.startsWith("retain") } - h.log.count { it.startsWith("release") })
            link.report("busy", "/w", Tone.PLAIN)
            runCurrent()
            assertFalse("那台说停了 ⇒ 没有在飞的", c.hasInFlightTurn.value)
        }
}

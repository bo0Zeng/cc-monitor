package com.ccmonitor.mobile.ui.chat.newsession

import com.ccmonitor.mobile.core.claude.link.CoreFailure
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.wire.PasteKind
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 登录抽屉：起 ⇒ 链接与粘什么照回包；交 ⇒ login-finish；没成 ⇒ 那台那一句；收起 ⇒ login-cancel。 */
@OptIn(ExperimentalCoroutinesApi::class)
class LoginFlowTest {
    private val started = mapOf("job" to "j1", "url" to "https://example.com/oauth", "paste" to "code", "auto" to false)

    @Test
    fun `起 · 粘错一回（那一格下面那一句、作业还在）· 再交成了 ⇒ done；成了之后收起不再撤`() =
        runTest {
            val l = FakeCoreLink()
            var finish: Reply = Reply.Failed(CoreFailure("bad_paste", "粘贴的内容对不上本次登录 · 可重新粘贴", "d"))
            l.answer = { cmd, _ ->
                when (cmd) {
                    "login-start" -> Reply.Ok(started)
                    "login-finish" -> finish
                    else -> Reply.Ok(mapOf("was" to true))
                }
            }
            val f = LoginFlow(l, "smokebox", "claude", "team", backgroundScope)
            f.start()
            runCurrent()
            assertEquals(listOf(mapOf("kind" to "account", "agent" to "claude", "account" to "team")), l.calls("login-start"))
            assertEquals(
                PasteKind.CODE,
                f.state.value.started
                    ?.paste,
            )
            f.submit("  abc  ")
            runCurrent()
            assertEquals(mapOf("job" to "j1", "pasted" to "abc"), l.calls("login-finish").single())
            assertEquals("粘贴的内容对不上本次登录 · 可重新粘贴", f.state.value.pasteError)
            finish = Reply.Ok(mapOf("target" to mapOf("kind" to "account")))
            f.submit("abd")
            runCurrent()
            assertTrue(f.state.value.done)
            f.cancel()
            runCurrent()
            assertTrue(l.calls("login-cancel").isEmpty())
        }

    @Test
    fun `没起成 ⇒ 那台那一句、没有作业；收起不发撤单`() =
        runTest {
            val l = FakeCoreLink().also { it.answer = { _, _ -> Reply.Failed(CoreFailure("unsupported", "codex · 不支持在 app 里登录", "d")) } }
            val f = LoginFlow(l, "smokebox", "codex", "cx", backgroundScope)
            f.start()
            runCurrent()
            assertEquals("codex · 不支持在 app 里登录", f.state.value.failed)
            assertNull(f.state.value.started)
            f.cancel()
            runCurrent()
            assertTrue(l.calls("login-cancel").isEmpty())
        }

    @Test
    fun `起好了中途收起 ⇒ login-cancel 交那份作业`() =
        runTest {
            val l = FakeCoreLink().also { it.answer = { cmd, _ -> if (cmd == "login-start") Reply.Ok(started) else Reply.Ok(mapOf("was" to true)) } }
            val f = LoginFlow(l, "smokebox", "claude", "team", backgroundScope)
            f.start()
            runCurrent()
            f.cancel()
            runCurrent()
            assertEquals(listOf(mapOf("job" to "j1")), l.calls("login-cancel"))
        }
}

package com.ccmonitor.mobile.ui.login

import com.ccmonitor.mobile.core.claude.link.CoreFailure
import com.ccmonitor.mobile.core.claude.link.Login
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.wire.ManualItem
import com.ccmonitor.mobile.core.claude.link.wire.PasteKind
import com.ccmonitor.mobile.core.claude.link.wire.Wire
import com.ccmonitor.mobile.core.copy.copyText
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
            var finish: Reply = Reply.Failed(CoreFailure("bad_paste", "粘贴的内容对不上本次登录 · 可重新粘贴", "d", mapOf("again" to "paste")))
            l.answer = { cmd, _ ->
                when (cmd) {
                    "login-start" -> Reply.Ok(started)
                    "login-finish" -> finish
                    else -> Reply.Ok(mapOf("was" to true))
                }
            }
            val f = LoginFlow(l, "smokebox", Login.account("claude", "team"), backgroundScope)
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
            assertNull("接着粘：抽屉不换成失败那一态", f.state.value.failed)
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
            val f = LoginFlow(l, "smokebox", Login.account("codex", "cx"), backgroundScope)
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
            val f = LoginFlow(l, "smokebox", Login.account("claude", "team"), backgroundScope)
            f.start()
            runCurrent()
            f.cancel()
            runCurrent()
            assertEquals(listOf(mapOf("job" to "j1")), l.calls("login-cancel"))
        }

    /** 交了没成：怎么办照那台的 `again` —— 重新开始 ⇒ 失败那一态 ＋［重新开始］；没法再来 ⇒ 只那一句。 */
    @Test
    fun `交了没成 ⇒ 照 again：restart 给重新开始、none 不给，作业都已收`() =
        runTest {
            for ((again, can) in listOf("restart" to true, "none" to false)) {
                val l = FakeCoreLink()
                l.answer = { cmd, _ ->
                    when (cmd) {
                        "login-start" -> Reply.Ok(started)
                        "login-finish" -> Reply.Failed(CoreFailure("expired", "这次登录已过期", "d", mapOf("again" to again)))
                        else -> Reply.Ok(mapOf("was" to true))
                    }
                }
                val f = LoginFlow(l, "smokebox", Login.account("claude", "team"), backgroundScope)
                f.start()
                runCurrent()
                f.submit("abc")
                runCurrent()
                val s = f.state.value
                assertEquals(again, "这次登录已过期", s.failed)
                assertEquals(again, can, s.canRestart)
                assertNull(again, s.pasteError)
                assertNull(again, s.started)
            }
        }

    /** 没起成：那台说没法再来 ⇒ 不给［重新开始］；没说（超时 · 断线）⇒ 给。 */
    @Test
    fun `没起成 ⇒ again none 不给重新开始；回包里没有 again ⇒ 给`() =
        runTest {
            for ((reply, can) in listOf(
                Reply.Failed(CoreFailure("unsupported", "不支持", "d", mapOf("again" to "none"))) to false,
                Reply.TimedOut to true,
            )) {
                val l = FakeCoreLink().also { it.answer = { _, _ -> reply } }
                val f = LoginFlow(l, "smokebox", Login.account("codex", "cx"), backgroundScope)
                f.start()
                runCurrent()
                assertEquals(reply.toString(), can, f.state.value.canRestart)
            }
        }

    /** 会话外那一项（MCP 那一形），照那台回包原样。 */
    private val mcpItem: Map<String, Any?> =
        mapOf(
            "kind" to "mcpLogin",
            "account" to null,
            "server" to "notes",
            "scope" to "user",
            "said" to "MCP notes 需登录",
            "fix" to mapOf("command" to "login-start", "args" to mapOf("kind" to "mcp", "agent" to "claude", "account" to null, "server" to "notes", "projectDir" to null)),
        )

    @Test
    fun `MCP 那一项：目标就是那一项的 fix args，login-start 入参与它逐格相等（缺的那几格不交，后端照缺省收）；抽屉标题照目标取 MCP 那一句`() =
        runTest {
            val l = FakeCoreLink().also { it.answer = { _, _ -> Reply.Ok(started) } }
            val item = Wire.bind<ManualItem>(mcpItem)!!
            val f = LoginFlow(l, "smokebox", item.fix!!.args, backgroundScope)
            f.start()
            runCurrent()
            @Suppress("UNCHECKED_CAST")
            val want = ((mcpItem["fix"] as Map<String, Any?>)["args"] as Map<String, Any?>).filterValues { it != null }
            assertEquals(listOf(want), l.calls("login-start"))
            assertEquals(copyText("login.sheet.mcp"), titleOf(f.state.value.target))
            assertEquals(copyText("login.sheet.account"), titleOf(Login.account("claude", "team")))
        }
}

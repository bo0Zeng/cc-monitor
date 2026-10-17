package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.wire.AgentPick
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 终端屏里要那台做的事，手机一家都不认识、一条 shell 命令都不拼：进哪儿一问 `session-open`（只交意图），起哪一家 · 能换哪些模型 ·
 * 有没有 tmux 问那台，换模型走 `session-model`（不往终端里敲 `/model`），推公钥问 `authorized-keys-add`。
 */
class TerminalOpsAgentTest {
    private val facts =
        mapOf(
            "recent" to emptyList<Any>(),
            "tmux" to true,
            "agents" to
                listOf(
                    mapOf("kind" to "claude", "text" to "Claude Code", "models" to listOf(mapOf("value" to "opus", "text" to "Opus")), "permissionModes" to emptyList<Any>()),
                    mapOf("kind" to "codex", "text" to "Codex", "models" to emptyList<Any>(), "permissionModes" to emptyList<Any>()),
                ),
        )

    private fun terminal(
        name: String,
        sid: String?,
        agent: String?,
    ) = mapOf(
        "terminal" to "h-$name",
        "tmux_name" to name,
        "title" to name,
        "clients" to emptyList<Any>(),
        "session" to sid?.let { mapOf("sid" to it, "agent" to agent) },
    )

    private fun opened(landed: String) =
        mapOf("landed" to landed, "session" to "w", "line" to "ccm -- --attach w", "sid" to null, "agent" to null, "cwd" to null, "account" to null, "note" to null, "prompt" to null)

    private fun link(
        vararg rows: Map<String, Any?>,
        tmux: Boolean = true,
    ) = FakeCoreLink().apply {
        answer = { cmd, _ ->
            when (cmd) {
                "session-open-facts" -> Reply.Ok(facts + ("tmux" to tmux))
                "terminals-list" -> Reply.Ok(mapOf("terminals" to rows.toList(), "complete" to true))
                "session-model" -> Reply.Ok(mapOf("result" to "done", "model" to "opus", "said" to "已换成 opus"))
                "session-open" -> Reply.Ok(opened("idle"))
                "authorized-keys-add" -> Reply.Ok(mapOf("outcome" to "already"))
                else -> Reply.LinkDown
            }
        }
    }

    private fun Map<String, Any?>?.what(): Map<*, *>? = this?.get("what") as? Map<*, *>

    /** 进一条终端没了的会话：恰好一问 `session-open {resume}`（不另问名单 · 不另要那一行）。 */
    @Test
    fun enteringAGoneSessionIsOneResumeAsk() =
        runTest {
            val l = link()
            val a = TerminalOps(l, { "box" }).enter(attachTo = null, resumeSid = "s1", tmux = "main", cwd = "/w")!!.getOrThrow()
            assertEquals("ccm -- --attach w", a.line)
            assertEquals(listOf("session-open"), l.calls.map { it.first })
            assertEquals(mapOf("kind" to "resume", "sid" to "s1"), l.calls("session-open").single().what())
        }

    /** 管理窗点一个会话附着：新 tab 一问 `session-open {look}`。 */
    @Test
    fun attachingFromTheManagerIsOneLookAsk() =
        runTest {
            val l = link()
            TerminalOps(l, { "box" }).enter(attachTo = "work", resumeSid = null, tmux = "work", cwd = null)!!.getOrThrow()
            assertEquals(listOf("session-open"), l.calls.map { it.first })
            assertEquals(mapOf("kind" to "look", "session" to "work"), l.calls("session-open").single().what())
        }

    /** 主机开着「连上就进 tmux」：一问 `session-open {shell, session: main}`（建不建 · 方言都在那台）；只有目录 ⇒ 只交目录；都没有 ⇒ 不问。 */
    @Test
    fun connectingIntoTmuxOrADirAsksForAShell() =
        runTest {
            val l = link()
            val ops = TerminalOps(l, { "box" })
            ops.enter(attachTo = null, resumeSid = null, tmux = "main", cwd = "~/w")!!.getOrThrow()
            ops.enter(attachTo = null, resumeSid = null, tmux = null, cwd = "/srv")!!.getOrThrow()
            assertNull("没有要进的 ⇒ 普通 shell、不问", ops.enter(null, null, null, null))
            assertEquals(
                listOf(mapOf("kind" to "shell", "session" to "main", "cwd" to "~/w"), mapOf("kind" to "shell", "cwd" to "/srv")),
                l.calls("session-open").map { it.what() },
            )
            assertEquals(listOf("session-open", "session-open"), l.calls.map { it.first })
        }

    /** 管理窗「新建」：请那台建（`shell {session}`），不进入。 */
    @Test
    fun creatingASessionAsksTheMachine() =
        runTest {
            val l = link()
            assertNull(TerminalOps(l, { "box" }).create("n1"))
            assertEquals(mapOf("kind" to "shell", "session" to "n1"), l.calls("session-open").single().what())
        }

    /** 推公钥：一问 `authorized-keys-add {key}`，结局照回包说；没有一次 exec。 */
    @Test
    fun pushingAKeyAsksTheMachine() =
        runTest {
            val l = link()
            val said = TerminalOps(l, { "box" }).pushKey("ssh-ed25519 AAAA k").getOrThrow()
            assertEquals(copyText("mobKeyPush.result.already"), said)
            assertEquals(listOf("authorized-keys-add" to mapOf("key" to "ssh-ed25519 AAAA k")), l.calls)
        }

    /** 那台说没有 tmux ⇒ tmux 那几样灰、推公钥照亮；后端问不到 ⇒ 都灰、写那一句。 */
    @Test
    fun whatTheMachineCanDoComesFromTheMachine() =
        runTest {
            val none = TerminalOps(link(tmux = false), { "box" }).reach()
            assertEquals(Reach(noBackend = null, noTmux = copyText("mobTerm.tmux.none", "host" to "box")), none)
            val palette = terminalPaletteActions(none, listOf(AgentPick("opus", "Opus")), {}, {}, {}, {})
            assertEquals("没 tmux ⇒ 模型 · 抓屏 · tmux 会话都不出，只剩推公钥（能点）", listOf(copyText("mobPalette.item.key")), palette.map { it.label })
            assertNotNull(palette.single().run)
            assertEquals(Reach(), TerminalOps(link(tmux = true), { "box" }).reach())

            val down = TerminalOps(FakeCoreLink(), { "box" }).reach()
            assertNotNull(down.noBackend)
            assertEquals(down.noBackend, down.noTmux)
            val key = terminalPaletteActions(down, emptyList(), {}, {}, {}, {}).single()
            assertNull("后端没连上 ⇒ 推公钥灰着", key.run)
            assertEquals("灰着写原因", down.noBackend, key.subtitle)
        }

    @Test
    fun withNothingStoredTheFirstAgentTheMachineListsIsUsed() =
        runTest {
            val l = link()
            val ops = TerminalOps(l, { "box" }, { "ClaudeCode" }) // 旧值：那台认不出 ⇒ 当没选
            ops.startAgent("w")
            val what = l.calls("session-open").single().what()
            assertEquals(mapOf("kind" to "new", "agent" to "claude", "cwd" to "~", "tmuxName" to "w"), what)
        }

    @Test
    fun modelsAndTheSwitchComeFromTheMachine() =
        runTest {
            val l = link(terminal("work", "s1", "claude"), terminal("bare", null, null))
            val ops = TerminalOps(l, { "box" })
            assertEquals(listOf("opus"), ops.modelsIn("work").map { it.value })
            assertTrue("没跑会话 ⇒ 没有可换的", ops.modelsIn("bare").isEmpty())
            assertEquals("已换成 opus", ops.switchModel("work", "opus").getOrThrow())
            assertEquals(listOf(mapOf("sid" to "s1", "model" to "opus", "client" to "mobile")), l.calls("session-model"))
            assertTrue("不再往终端里送字", l.calls("terminal-input").isEmpty())
        }
}

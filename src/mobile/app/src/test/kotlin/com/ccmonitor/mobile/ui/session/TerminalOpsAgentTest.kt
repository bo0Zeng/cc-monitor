package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 终端屏里要那台做的事，手机一家都不认识：进终端那一句只交意图（不交启动命令），起哪一家 · 能换哪些模型问那台，
 * 换模型走 `session-model`（不往终端里敲 `/model`）。
 */
class TerminalOpsAgentTest {
    private val facts =
        mapOf(
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

    private fun link(vararg rows: Map<String, Any?>) =
        FakeCoreLink().apply {
            answer = { cmd, _ ->
                when (cmd) {
                    "session-new-facts" -> Reply.Ok(facts)
                    "terminals-list" -> Reply.Ok(mapOf("terminals" to rows.toList(), "complete" to true))
                    "launch-render-cli" -> Reply.Ok(mapOf("cmd" to "ccm -- --attach x", "account" to null))
                    "session-model" -> Reply.Ok(mapOf("result" to "done", "model" to "opus", "said" to "已换成 opus"))
                    "session-new" -> Reply.Ok(mapOf("outcome" to "started", "session" to "w", "sid" to null, "cwd" to "/h", "prompt" to null))
                    else -> Reply.LinkDown
                }
            }
        }

    @Test
    fun theAttachLineAsksOnlyTheIntentAndTakesTheAgentRunningInThatTerminal() =
        runTest {
            val l = link(terminal("work", "s1", "codex"), terminal("bare", null, null))
            val ops = TerminalOps(l, { "box" }, { "claude" })
            assertEquals("ccm -- --attach x", ops.attachLine("work").getOrThrow())
            ops.attachLine("bare").getOrThrow()
            val asked = l.calls("launch-render-cli")
            assertEquals(listOf("codex", "claude"), asked.map { it?.get("agent") })
            asked.forEach { a ->
                assertEquals("只交意图：${a?.keys}", setOf("agent", "action", "container", "account"), a?.keys)
            }
        }

    @Test
    fun withNothingStoredTheFirstAgentTheMachineListsIsUsed() =
        runTest {
            val l = link()
            val ops = TerminalOps(l, { "box" }, { "ClaudeCode" }) // 旧值：那台认不出 ⇒ 当没选
            ops.startAgent("w")
            assertEquals("claude", l.calls("session-new").single()?.get("agent"))
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

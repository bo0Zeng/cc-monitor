package com.ccmonitor.mobile.agent

import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.core.claude.command.CodexInvocation
import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.ssh.ResumeSpec
import com.ccmonitor.mobile.ssh.TmuxBackend
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxResumeCommand
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * `TmuxBackend.resume` 按种类问档案，把对的档案交给 resume 脚手架。
 *
 * 防的是两支接错线：Codex 会话拿到 Claude 的 `--resume` 载荷（远端当场报错），
 * 或 Claude 会话拿到 Codex 的 `find` 深度（定位不到 jsonl，resume 静默退化成一个空 shell）。
 * 命令串本身的定值由 `TmuxGatewayGoldenTest` 钉。
 *
 * 判别力边界：对拍的两边都是本仓自己的函数，证的是「交对了档案」，不证「这串命令在真机上跑得对」。
 */
class AgentResumeGoldenTest {
    private val claudeSid = "2f1a8c30-7c1e-4b2a-9f11-abc123def456"
    private val codexSid = "0198f3aa-1111-7c9d-8e02-0badc0ffee11"

    /** Claude 那支：命令串 + 三个字段逐字等于直调 [tmuxResumeCommand]。 */
    @Test
    fun theClaudeResumeCommandComesFromTheClaudeProfile() {
        val spec =
            ResumeSpec(
                sessionId = claudeSid,
                launchCandidates = listOf("cct"),
                claudeDir = "/home/u/.claude",
                fallbackCwd = "/w",
                alreadyInTmux = false,
                agentKind = AgentKind.ClaudeCode,
            )
        val plan = TmuxBackend.resume(spec)!!
        val name = ClaudeInvocation.resumeSessionName(claudeSid)
        assertEquals(
            "Claude resume 命令串必须逐字等于 Claude 档案拼出的那串",
            tmuxResumeCommand(AgentProfile.of(AgentKind.ClaudeCode), name, claudeSid, "/home/u/.claude", "/w", false, "cct"),
            plan.command,
        )
        assertEquals("会话名走 Claude 那支（`cc-` 前缀）", name, plan.sessionName)
        assertEquals("cct", plan.launchLabel)
        assertNull("干净命令没被替换", plan.substitutedFrom)
    }

    /** Codex 那支：命令串 + 三个字段逐字等于直调 [tmuxResumeCommand]。 */
    @Test
    fun theCodexResumeCommandComesFromTheCodexProfile() {
        val spec =
            ResumeSpec(
                sessionId = codexSid,
                launchCandidates = listOf("codex"),
                claudeDir = "/home/u/.codex", // 字段名沿用 claudeDir，语义是 agent-dir
                fallbackCwd = "/w",
                alreadyInTmux = true,
                agentKind = AgentKind.Codex,
            )
        val plan = TmuxBackend.resume(spec)!!
        val name = CodexInvocation.resumeSessionName(codexSid)
        assertEquals(
            "Codex resume 命令串必须逐字等于 Codex 档案拼出的那串",
            tmuxResumeCommand(AgentProfile.of(AgentKind.Codex), name, codexSid, "/home/u/.codex", "/w", true, "codex"),
            plan.command,
        )
        assertEquals("会话名走 Codex 那支（`cx-` 前缀）", name, plan.sessionName)
        assertEquals("codex", plan.launchLabel)
        assertNull("干净命令没被替换", plan.substitutedFrom)
    }

    /**
     * 两支不许接成同一支。
     *
     * 上面两条各自对拍时，一个「不管什么 kind 都走 Claude 那支」的实现只会红掉 Codex 那条；
     * 但如果哪天脚手架不再读档案（两家走同一份脚手架，差别全靠档案那几格），
     * 对拍会同时跟着变、两条一起绿。本条从输出本身再钉一次：两支的命令串与会话名必须真的不同。
     */
    @Test
    fun theTwoArchetypesDoNotCollapseIntoOne() {
        // 同一个 sid、同一条候选链，只换 agent 档 ⇒ 输出的任何差异都只可能来自档案
        val claude = planFor(AgentKind.ClaudeCode)
        val codex = planFor(AgentKind.Codex)
        assertNotEquals("同一个 sid 换个 agent 档，命令串必须不同（相同 = 档案没被读）", claude.command, codex.command)
        assertNotEquals("会话名也必须不同（`cc-` ↔ `cx-`）", claude.sessionName, codex.sessionName)
        // 各自的指纹：Claude 用 `--resume` flag，Codex 用 `resume` 子命令 —— 接错线时这两句直接点名
        assertTrue("Claude 那支必须发 `--resume`：${claude.command}", claude.command.contains("--resume $claudeSid"))
        assertTrue("Codex 那支必须发 `resume` 子命令：${codex.command}", codex.command.contains("cct resume $claudeSid"))
        assertFalse("Codex 那支不许混进 Claude 的 `--resume`：${codex.command}", codex.command.contains("--resume"))
    }

    private fun planFor(kind: AgentKind) =
        TmuxBackend.resume(
            ResumeSpec(
                sessionId = claudeSid,
                launchCandidates = listOf("cct"),
                claudeDir = "/home/u/.claude",
                fallbackCwd = "/w",
                alreadyInTmux = false,
                agentKind = kind,
            ),
        )!!

    /** sid 非法时两支都回 null（校验走档案的 `invocation`，不是各写一遍）。 */
    @Test
    fun bothArchetypesRejectAnInvalidSessionId() {
        for (kind in AgentKind.entries) {
            val spec = ResumeSpec("bad id with spaces", listOf("cct"), "/home/u/.claude", fallbackCwd = null, agentKind = kind)
            assertNull("$kind：非法 sid 必须回 null（别把它拼进 shell）", TmuxBackend.resume(spec))
        }
    }
}

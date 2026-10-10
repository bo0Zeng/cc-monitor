package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.ssh.TmuxCommands.PANE_PROBE_HAS_CHILD
import com.ccmonitor.mobile.ssh.TmuxCommands.PANE_PROBE_NO_CHILD
import com.ccmonitor.mobile.ssh.TmuxCommands.isResumeForegroundShell
import com.ccmonitor.mobile.ssh.TmuxCommands.paneConfirmedNoChild
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxCapturePaneCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxForegroundProbeCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxKillCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxListCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxNewDetachedCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxNewDetachedRunning
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxNewOrAttach
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxPaneChildProbeCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxResumeCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxSendModelCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxSessionCwdCommand
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * [TmuxBackend] 的委托一致性：每个方法逐字节等于对应的 [TmuxCommands] 纯函数，
 * 外加 capabilities 的值与 resume 计划的各字段。
 */
class SessionBackendTest {
    private val name = "main"
    private val sid = "abc12345-def6-7890-1234-567890abcdef"
    private val claudeDir = "/home/u/.claude"

    @Test
    fun capabilitiesTmuxAllTrue() {
        val c = TmuxBackend.capabilities
        assertTrue(c.supportsSendKeys)
        assertTrue(c.supportsCapture)
        assertTrue(c.supportsMultiClient)
        assertTrue(c.supportsMultiWindow)
    }

    @Test
    fun execCommandParity() {
        assertEquals(tmuxListCommand(), TmuxBackend.listCommand())
        assertEquals(tmuxKillCommand(name), TmuxBackend.killCommand(name))
        assertEquals(tmuxNewDetachedCommand(name), TmuxBackend.newDetachedCommand(name))
        assertEquals(tmuxNewDetachedRunning(name, "claude"), TmuxBackend.newDetachedRunningCommand(name, "claude"))
        assertEquals(tmuxSessionCwdCommand(name), TmuxBackend.sessionCwdCommand(name))
        assertEquals(tmuxSendModelCommand(name, "opus"), TmuxBackend.sendModelCommand(name, "opus"))
        assertEquals(tmuxCapturePaneCommand(name), TmuxBackend.captureCommand(name))
    }

    @Test
    fun ptyAndProbeParity() {
        assertEquals(tmuxNewOrAttach(name, "/tmp"), TmuxBackend.enter(name, "/tmp"))
        assertEquals(tmuxNewOrAttach(name, null), TmuxBackend.enter(name, null))
        assertEquals(tmuxForegroundProbeCommand(name), TmuxBackend.foregroundProbeCommand(name))
        assertEquals(tmuxPaneChildProbeCommand(name), TmuxBackend.paneChildProbeCommand(name))
    }

    @Test
    fun watchdogPredicateParity() {
        assertEquals(isResumeForegroundShell("bash"), TmuxBackend.isForegroundShell("bash"))
        assertEquals(isResumeForegroundShell("node"), TmuxBackend.isForegroundShell("node"))
        assertEquals(paneConfirmedNoChild(PANE_PROBE_NO_CHILD), TmuxBackend.confirmedNoChild(PANE_PROBE_NO_CHILD))
        assertEquals(paneConfirmedNoChild(PANE_PROBE_HAS_CHILD), TmuxBackend.confirmedNoChild(PANE_PROBE_HAS_CHILD))
    }

    @Test
    fun resumeCleanCommandParityAndFields() {
        val spec = ResumeSpec(sid, listOf("cct"), claudeDir, fallbackCwd = "/w", alreadyInTmux = false)
        val plan = TmuxBackend.resume(spec)!!
        // command 逐字等于直调 tmuxResumeCommand（launch=cct 过白名单原样）
        val expected = tmuxResumeCommand(AgentProfile.DEFAULT, ClaudeInvocation.resumeSessionName(sid), sid, claudeDir, "/w", false, "cct")
        assertEquals(expected, plan.command)
        assertEquals(ClaudeInvocation.resumeSessionName(sid), plan.sessionName)
        assertEquals("cct", plan.launchLabel)
        assertNull(plan.substitutedFrom) // 干净命令未被替换
    }

    @Test
    fun resumeSubstitutionNoticed() {
        // 含注入元字符 → fail-closed 回退 claude；substitutedFrom = 原意图（供提示）。
        val spec = ResumeSpec(sid, listOf("cc; rm -rf /"), claudeDir, fallbackCwd = null, alreadyInTmux = false)
        val plan = TmuxBackend.resume(spec)!!
        assertEquals("claude", plan.launchLabel)
        assertEquals("cc; rm -rf /", plan.substitutedFrom)
    }

    @Test
    fun resumeFirstCandidatePriority() {
        // 候选链取首个过白名单原样通过者（cct），而非之后的。
        val spec = ResumeSpec(sid, listOf(null, "cct", "cc"), claudeDir, fallbackCwd = null)
        assertEquals("cct", TmuxBackend.resume(spec)!!.launchLabel)
    }

    @Test
    fun resumeNullOnInvalidSid() {
        val spec = ResumeSpec("bad id with spaces", listOf("cct"), claudeDir, fallbackCwd = null)
        assertNull(TmuxBackend.resume(spec))
    }
}

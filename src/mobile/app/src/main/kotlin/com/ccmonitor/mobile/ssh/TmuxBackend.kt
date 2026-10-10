package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.claude.agent.AgentProfile

/*
 * [SessionBackend] 的 tmux 实现。命令串全由 [TmuxCommands] 网关造，本对象只把串摆成 [SessionBackend] 的形状
 * （能力位、[ResumePlan] 的几格、把 agent 档案交给网关）。本文件里不许出现 `"tmux …"` 字面。
 */

/** tmux 会话后端。 */
object TmuxBackend : SessionBackend {
    override val capabilities =
        SessionCapabilities(
            supportsSendKeys = true,
            supportsCapture = true,
            supportsMultiClient = true,
            supportsMultiWindow = true,
        )

    override fun enter(
        sessionName: String,
        workingDir: String?,
    ): String = TmuxCommands.tmuxNewOrAttach(sessionName, workingDir)

    /**
     * 一条路，全部问 agent 档案：载荷（`invocation`）、按 sid 定位（`sessionLocator`）、打不打 `@ccm_sid`
     * 都由 [TmuxCommands.tmuxResumeCommand] 向档案要，这里不按种类分支。
     */
    override fun resume(spec: ResumeSpec): ResumePlan? {
        if (!capabilities.supportsSendKeys) return null
        val profile = AgentProfile.of(spec.agentKind)
        val invocation = profile.invocation
        if (!invocation.isValidSessionId(spec.sessionId)) return null
        val launch = invocation.resolveLaunchCommand(spec.launchCandidates)
        val intended = spec.launchCandidates.firstNotNullOfOrNull { it?.trim()?.takeIf(String::isNotEmpty) }
        val name = invocation.resumeSessionName(spec.sessionId)
        val command =
            TmuxCommands.tmuxResumeCommand(
                profile = profile,
                name = name,
                sessionId = spec.sessionId,
                agentDir = spec.claudeDir,
                fallbackCwd = spec.fallbackCwd,
                alreadyInTmux = spec.alreadyInTmux,
                launchCommand = launch,
            )
        return ResumePlan(
            command = command,
            sessionName = name,
            launchLabel = launch,
            substitutedFrom = intended?.takeIf { it != launch },
        )
    }

    override fun listCommand(): String = TmuxCommands.tmuxListCommand()

    override fun killCommand(sessionName: String): String = TmuxCommands.tmuxKillCommand(sessionName)

    override fun newDetachedCommand(sessionName: String): String = TmuxCommands.tmuxNewDetachedCommand(sessionName)

    override fun newDetachedRunningCommand(
        sessionName: String,
        command: String,
    ): String = TmuxCommands.tmuxNewDetachedRunning(sessionName, command)

    override fun startOnceCommand(
        sessionName: String,
        command: String,
    ): String = TmuxCommands.tmuxStartOnceCommand(sessionName, command)

    override fun sessionCwdCommand(sessionName: String): String = TmuxCommands.tmuxSessionCwdCommand(sessionName)

    override fun sendModelCommand(
        sessionName: String,
        model: String,
    ): String = TmuxCommands.tmuxSendModelCommand(sessionName, model)

    override fun captureCommand(sessionName: String): String = TmuxCommands.tmuxCapturePaneCommand(sessionName)

    override fun foregroundProbeCommand(sessionName: String): String = TmuxCommands.tmuxForegroundProbeCommand(sessionName)

    override fun paneChildProbeCommand(sessionName: String): String = TmuxCommands.tmuxPaneChildProbeCommand(sessionName)

    override fun isForegroundShell(probeOutput: String): Boolean = TmuxCommands.isResumeForegroundShell(probeOutput)

    override fun confirmedNoChild(probeOutput: String): Boolean = TmuxCommands.paneConfirmedNoChild(probeOutput)
}

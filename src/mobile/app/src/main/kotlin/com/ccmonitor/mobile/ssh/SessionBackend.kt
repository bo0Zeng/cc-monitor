package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.model.AgentKind

/*
 * 会话后端：调用方经它拿 tmux 命令串，自己不写 tmux 字面、不知道底下是 tmux 还是哪种 agent。
 *
 * 方法只返回命令串、不执行。enter/resume 必须注进 pty，tmux 才扶得住会话（断线不死靠它）；
 * 后端若握着 channel 或 pty 直接执行，就有了会话句柄，换实现或误 kill 就能弄死活会话。
 * 只返回串，后端无句柄无状态，结构上弄不死会话；在 pty 还是 exec 旁路执行由调用方定。
 */

/** 后端能做什么（不是 app 当前用了什么）。 */
data class SessionCapabilities(
    val supportsSendKeys: Boolean, // resume、切模型用 send-keys
    val supportsCapture: Boolean, // capture-pane 抓屏
    val supportsMultiClient: Boolean, // 一端起的会话另一端能接，断线回来 attach 即续
    val supportsMultiWindow: Boolean, // 多窗口；app 只读窗口数，不创建
)

/** resume 意图。调用方给候选链和定位信息，后端解析启动命令并包好。 */
data class ResumeSpec(
    val sessionId: String,
    val launchCandidates: List<String?>, // 选定 > 发起 tab launcher > 主机 launchers（调用方按优先级拼）
    val claudeDir: String, // agent 配置目录前缀，按种类：Claude ~/.claude，Codex ~/.codex
    val fallbackCwd: String?,
    val alreadyInTmux: Boolean = false,
    // agent 种类：决定 resume 载荷（Claude `--resume <sid>`，Codex `resume <uuid>` 子命令）和按 sid 找会话的路径。
    val agentKind: AgentKind = AgentProfile.DEFAULT.kind,
)

/** resume 计划。[command] 注进 pty；[sessionName] 先乐观置名并给 watchdog 用；[launchLabel] 失败时点名；[substitutedFrom] 启动命令被换掉时提示。 */
data class ResumePlan(
    val command: String,
    val sessionName: String,
    val launchLabel: String,
    val substitutedFrom: String?,
)

/**
 * 会话后端的唯一入口，只有 [TmuxBackend] 一个实现。调用方按 [capabilities] 和 `ShellDialect.sessionBackend`
 * 是否存在来门控依赖 tmux 的动作。
 */
interface SessionBackend {
    val capabilities: SessionCapabilities

    // —— 注进 pty：靠这些命令把会话交给 tmux 扶住 ——

    /** 连接首发的「进入/建立会话」命令（有则 attach 无则建；含移动端设置）。 */
    fun enter(
        sessionName: String,
        workingDir: String?,
    ): String

    /**
     * resume 一个 agent 会话；sid 不合法或没有 send-keys 能力时返回 null，调用方落回普通 shell。
     * 乐观会话名就是 [ResumePlan.sessionName]，按 agent 种类起。
     */
    fun resume(spec: ResumeSpec): ResumePlan?

    // —— exec 旁路，一次性，不打扰 pty ——

    /** 列会话命令；输出由调用方 `parseTmuxList` 解析，格式同源。 */
    fun listCommand(): String

    fun killCommand(sessionName: String): String

    fun newDetachedCommand(sessionName: String): String

    fun newDetachedRunningCommand(
        sessionName: String,
        command: String,
    ): String

    fun sessionCwdCommand(sessionName: String): String

    fun sendModelCommand(
        sessionName: String,
        model: String,
    ): String

    fun captureCommand(sessionName: String): String

    // —— watchdog 探测与判定 ——
    fun foregroundProbeCommand(sessionName: String): String

    fun paneChildProbeCommand(sessionName: String): String

    fun isForegroundShell(probeOutput: String): Boolean

    /** 不叫 `paneConfirmedNoChild`：同名的话委托会变成递归调自己。 */
    fun confirmedNoChild(probeOutput: String): Boolean
}

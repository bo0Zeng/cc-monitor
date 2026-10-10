package com.ccmonitor.mobile.core.claude.command

/**
 * 一个 agent 的启动 / 续接载荷，`AgentProfile.invocation` 那一格的类型。调用方拿 `profile.invocation`，不按种类分派。
 *
 * 只覆盖 resume 的载荷那一半；外面那层 tmux 脚手架在 app 层，core-claude 不知 tmux。
 * 脚手架里随 agent 变的另两样由档案回答：定位命令（`SessionLocator.findBySessionIdCommand`）与身份标记（`AgentProfile.hasCcmIdentity`）。
 */
interface AgentInvocation {
    /** sessionId 是否可安全用于 resume 命令（防注入的白名单正则）。 */
    fun isValidSessionId(id: String): Boolean

    /** 净化主机自定义启动命令；null/空白/含注入元字符 → 回退该 agent 的内置命令名（fail-closed）。 */
    fun sanitizeLaunchCommand(command: String?): String

    /** 候选链取首个原样通过 [sanitizeLaunchCommand] 的；全被拒/无候选 → 内置命令名。 */
    fun resolveLaunchCommand(candidates: List<String?>): String

    /** resume 载荷（send-keys 进交互 shell 的那串，不含 tmux）。 */
    fun resumeInvocation(
        launchCommand: String?,
        sessionId: String,
    ): String

    /** resume 会话名（按 sessionId 稳定，调用方用作 tmux 会话名）。 */
    fun resumeSessionName(sessionId: String): String
}

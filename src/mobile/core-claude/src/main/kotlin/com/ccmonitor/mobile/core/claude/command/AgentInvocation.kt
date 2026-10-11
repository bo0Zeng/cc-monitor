package com.ccmonitor.mobile.core.claude.command

/**
 * 一个 agent 的启动 / 续接载荷，`AgentProfile.invocation` 那一格的类型。调用方拿 `profile.invocation`，不按种类分派。
 *
 * 只剩启动命令那一半（净化 · 按候选链取）；起会话 · 接回会话都是那台核心的事。
 */
interface AgentInvocation {
    /** 净化主机自定义启动命令；null/空白/含注入元字符 → 回退该 agent 的内置命令名（fail-closed）。 */
    fun sanitizeLaunchCommand(command: String?): String

    /** 候选链取首个原样通过 [sanitizeLaunchCommand] 的；全被拒/无候选 → 内置命令名。 */
    fun resolveLaunchCommand(candidates: List<String?>): String
}

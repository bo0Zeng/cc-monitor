package com.ccmonitor.mobile.core.claude.command

/**
 * Claude 会话启动 / 续接 / 切模型的远端命令载荷。纯 Claude 知识，POSIX shell，不含 tmux 语法；
 * 怎么跑（tmux 编排）是调用方的事。
 */
object ClaudeInvocation : AgentInvocation {
    /**
     * 启动命令的注入元字符 denylist。命令是主机上自配的（cc/cct/带参），放行引号、括号、星号
     * （要交给交互 shell 解释，quote 会弄坏别名），只挡串联/展开/重定向：`;` `|` `&` `$` 反引号 `>` `<` 换行。
     */
    private val LAUNCH_UNSAFE = Regex("[;|&\$`><\\n\\r]")

    /** 净化主机自定义 Claude 启动命令；null/空白/含注入元字符 → 回退 `claude`（fail-closed）。 */
    override fun sanitizeLaunchCommand(command: String?): String {
        val c = command?.trim().orEmpty()
        return if (c.isNotEmpty() && !LAUNCH_UNSAFE.containsMatchIn(c)) c else "claude"
    }

    /**
     * 从按优先级排好的 [candidates] 里取第一个原样通过 [sanitizeLaunchCommand] 的；
     * 前一个非法就看下一个，全被拒才兜底 `claude`。
     */
    override fun resolveLaunchCommand(candidates: List<String?>): String {
        val cs = candidates.mapNotNull { it?.trim()?.takeIf(String::isNotEmpty) }
        return cs.firstOrNull { sanitizeLaunchCommand(it) == it } ?: "claude"
    }

    /** 切模型载荷 `/model <model>`（Claude 斜杠命令；由调用方 send-keys 投递）。 */
    fun modelCommand(model: String): String = "/model $model"
}

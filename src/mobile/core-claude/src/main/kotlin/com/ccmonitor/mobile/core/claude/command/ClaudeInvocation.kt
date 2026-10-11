package com.ccmonitor.mobile.core.claude.command

/**
 * Claude 会话启动 / 续接 / 切模型的远端命令载荷。纯 Claude 知识，POSIX shell，不含 tmux 语法；
 * 怎么跑（tmux 编排）是调用方的事。
 */
object ClaudeInvocation : AgentInvocation {
    /** sessionId 白名单（UUID 及变体，限长）。sid 要插值进 shell 载荷，先验再用。 */
    private val SESSION_ID_RE = Regex("^[A-Za-z0-9_-]{1,128}$")

    /**
     * 启动命令的注入元字符 denylist。命令是主机上自配的（cc/cct/带参），放行引号、括号、星号
     * （要交给交互 shell 解释，quote 会弄坏别名），只挡串联/展开/重定向：`;` `|` `&` `$` 反引号 `>` `<` 换行。
     */
    private val LAUNCH_UNSAFE = Regex("[;|&\$`><\\n\\r]")

    /**
     * 嵌套 Claude 环境标记（空格分隔，喂 `unset`）。shell 继承了这些时 Claude 自认嵌套子会话，
     * 不写 JSONL、不注册 pidfile，所以起之前先 unset。`CLAUDE_CONFIG_DIR` 刻意不在里面。
     */
    const val NESTED_ENV_VARS = "CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION"

    /** sessionId 是否可安全用于 resume 命令（`^[A-Za-z0-9_-]{1,128}$`）。 */
    override fun isValidSessionId(id: String): Boolean = SESSION_ID_RE.matches(id)

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

    /**
     * resume 载荷（send-keys 进交互 shell 的那串）：`unset <嵌套标记>; <cc/cct> --resume <sid>`。
     * [sessionId] 不合法直接 require 失败，调用方应先验并给出反馈。
     */
    override fun resumeInvocation(
        launchCommand: String?,
        sessionId: String,
    ): String {
        require(isValidSessionId(sessionId)) { "非法 Claude sessionId: $sessionId" }
        return "unset $NESTED_ENV_VARS; ${sanitizeLaunchCommand(launchCommand)} --resume $sessionId"
    }

    /**
     * resume 会话名 `cc-<sid前8>`，调用方拿它当 tmux 会话名。
     *
     * `cc-` 前缀是和桌面端约定好的：桌面端按这个名字加 `@ccm_sid` 认出这条会话，改了名就认不出。
     * 所以要 `cc-<sid8>` 只许调这里，别处不再拼一份。
     */
    override fun resumeSessionName(sessionId: String): String = "cc-${sessionId.take(8)}"

    /** 切模型载荷 `/model <model>`（Claude 斜杠命令；由调用方 send-keys 投递）。 */
    fun modelCommand(model: String): String = "/model $model"
}

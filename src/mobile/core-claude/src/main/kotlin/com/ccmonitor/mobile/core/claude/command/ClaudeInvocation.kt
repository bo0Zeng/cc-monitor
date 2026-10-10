package com.ccmonitor.mobile.core.claude.command

import com.ccmonitor.mobile.core.claude.transport.ClaudePaths
import com.ccmonitor.mobile.core.remote.shellQuote

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

    // ---- 常驻管道 --------------------------------------------------------

    /*
     * 管道命令的管子那一半（会话目录、文件名、信封、契约首行）在 [PipeCommands]。
     * 这里只出 Claude 知识那一半：跑哪个程序、带哪些 flag、要不要 cd、要不要 unset。
     */

    /**
     * 常驻管道载荷：
     *
     * ```sh
     * tail -n 0 -f in.ndjson | claude --input-format stream-json --output-format stream-json \
     *                                 --include-partial-messages --verbose >> out.ndjson
     * ```
     *
     * `tail -f` 让 stdin 永不 EOF，进程常驻。上行就是往 `in.ndjson` 追加一行
     * `{"type":"user","message":{…}}`；同一条 stdin 也是 CLI 的控制通道，追一行
     * `control_request`/`interrupt` 就能让这一轮停下，不打掉管道。
     *
     * 不用「每条消息起一次 `claude -p --resume`」：那样每次都要重读整个会话记录，开销随历史体量涨。
     *
     * 约束：嵌套环境标记必须 unset（[NESTED_ENV_VARS]）；启动命令过白名单（[sanitizeLaunchCommand]）。
     * 只造命令串，不起进程。
     */
    fun pipeInvocation(
        launchCommand: String?,
        sessionId: String,
        workdir: String? = null,
        permissionMode: String? = null,
        /**
         * 接着这条已有对话跑（`claude --resume <sid>`）；null = 起新对话。
         *
         * - `--resume` 按 cwd 划分作用域：从别的目录跑会报 `No conversation found`，所以要和 `workdir` 一起传对。
         * - 它不回放历史（只有 `system` 与 `result`），历史得另从会话记录读。
         * - 它常驻等 stdin，EOF 才退，与常驻管道相容。
         */
        resumeSessionId: String? = null,
        /**
         * 新对话：把这个编号指定给 Claude（见 [sessionIdFlag]），我们的编号就是它的编号。
         * 与 [resumeSessionId] 互斥；两个都给当场失败，那只可能是接线错了。
         */
        newSessionId: String? = null,
        /**
         * 用户显式覆盖过的 Claude 配置目录（`CLAUDE_CONFIG_DIR`）；null = 没覆盖，什么都不设。
         *
         * 读那侧走 [ClaudePaths.resolveClaudeDir]，起那侧必须设同一个值，否则读 A 写 B；
         * 远端做了账号隔离时，不设还会起出一个未登录的 Claude。
         * 没覆盖时默认值本身是 `${'$'}{CLAUDE_CONFIG_DIR:-…}`，语义是「听远端自己的」，硬塞值反而覆盖远端。
         */
        claudeDir: String? = null,
    ): String {
        require(newSessionId == null || resumeSessionId == null) {
            "同一条对话不能既接着已有的跑又指定新编号：resume=$resumeSessionId new=$newSessionId"
        }
        // 一律经 `env` 起：
        // 1. 启动命令白名单不禁 `=`，`FOO=1 claude` 能过校验；而配了 workdir 那支用 `exec`，
        //    `exec FOO=1 claude` 会把赋值当程序名报 not found。`exec env …` 两种形态都成立。
        // 2. [claudeDir] 也靠它传给 Claude。
        // 代价：`env` 只找可执行文件，不认 shell 函数/别名；非交互 shell 本来也不展开别名。
        val envPrefix = claudeDir?.let { "env CLAUDE_CONFIG_DIR=${shellQuote(it)} " } ?: "env "
        // 新对话的 flag 在远端当场决定，见 [newConversationFlagExpr]。
        val (decideFlag, sidFlag) =
            newSessionId?.let { newConversationFlagExpr(it, workdir, claudeDir) } ?: ("" to "")
        val cmd =
            envPrefix + sanitizeLaunchCommand(launchCommand) + permissionModeFlag(permissionMode) +
                resumeFlag(resumeSessionId) + sidFlag
        // 只有 claude 这个进程换 cwd：子 shell 的 `cd` 不影响外层，`tail -f` 与 `>>` 仍按相对路径
        // 指向家目录下的会话通道。`exec` 让 claude 顶掉子 shell。
        // cwd 还决定会话 JSONL 落在 `~/.claude/projects/<编码后的 cwd>/` 哪个目录，和会话发现对得上。
        // `workdir` 是字面路径，走单引号，`~` 不展开，要填绝对路径。
        val claude =
            workdir?.trim()?.ifEmpty { null }?.let { "( cd ${shellQuote(it)} && exec $cmd $PIPE_FLAGS )" }
                ?: "$cmd $PIPE_FLAGS"
        // 两段原样嵌进网关，网关不解释，所以这里给出去的必须已经是能跑的 shell。
        return PipeCommands.startCommand(
            sessionId = sessionId,
            agentCommand = claude,
            prelude = "unset $NESTED_ENV_VARS; $decideFlag",
        )
    }

    /**
     * 起会话时可选的权限模式（`claude --permission-mode <值>`）。取值来自 `claude --help`，只提供其中三个：
     * `bypassPermissions` 刻意不给（官方移动端也不给）；`auto` / `dontAsk` 语义没核实过，不上。
     * aterm 不做权限判定，只把用户选的值传给 claude。
     */
    val PERMISSION_MODES = listOf("manual", "acceptEdits", "plan")

    /**
     * 把用户选的模式变成命令行片段。
     *
     * @return 非法/未选 ⇒ 空串，不把原值拼进去。claude 不认的值会让它当场退出，
     *   表现成「管道起来了但永远没有下行」。
     */
    fun permissionModeFlag(mode: String?): String =
        if (mode in PERMISSION_MODES) " --permission-mode $mode" else ""

    /**
     * `--resume <sid>` 片段。
     *
     * @return 非法/未给 ⇒ 空串（起新对话）。理由同 [permissionModeFlag]，而且 sid 进命令行，不过白名单就是注入向量。
     */
    fun resumeFlag(sessionId: String?): String =
        if (sessionId != null && isValidSessionId(sessionId)) " --resume $sessionId" else ""

    /** Claude 的对话编号必须是合法 UUID（`--session-id` 的硬要求）。 */
    private val UUID_RE = Regex("^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")

    /** [id] 是不是合法 UUID。新对话的编号要过这一关才能交给 Claude。 */
    fun isValidUuid(id: String): Boolean = UUID_RE.matches(id)

    /**
     * 把编号指定给 Claude（`claude --session-id <uuid>`）。不指定的话 Claude 自己另发一个编号，
     * 和我们的远端目录名对不上，新开的对话下次不一定找得回来；指定后从第一帧起 `session_id` 就是这个。
     *
     * 非法 UUID 当场失败，与 [resumeFlag] 相反：这里静默丢掉会让两个编号重新分叉。
     */
    fun sessionIdFlag(sessionId: String?): String {
        if (sessionId == null) return ""
        require(isValidUuid(sessionId)) { "新对话编号必须是合法 UUID（Claude 的 --session-id 只认这个形状）：$sessionId" }
        return " --session-id $sessionId"
    }

    /** 起管道时临时装那个 flag 的 shell 变量名（见 [newConversationFlagExpr]）。 */
    private const val SID_FLAG_VAR = "ATERM_SID_FLAG"

    /**
     * 新对话用哪个 flag 在远端当场决定：已有记录 ⇒ `--resume`，否则 `--session-id`。
     *
     * 给已有记录的编号传 `--session-id`，CLI 会报 `Session ID … is already in use` 并退出（记录不受影响）。
     * 常驻命令死了之后从返回栈回到新对话那一屏就会走到这条路，表现为界面永远空白。
     * 记录在不在只有远端知道，于是在同一条命令里问一句，零额外往返。
     *
     * 注意：[workdir] 为 null 时问不了（不知道记录落在哪个项目目录），退回直接传 `--session-id`；
     * 那种情形下 `--resume` 本来也不可靠。
     */
    private fun newConversationFlagExpr(
        newSessionId: String,
        workdir: String?,
        claudeDir: String?,
    ): Pair<String, String> {
        // 记录路径只有 [ClaudePaths.sessionRecordPath] 一处答案，翻历史那侧读的是同一个文件。
        val record =
            ClaudePaths.sessionRecordPath(claudeDir ?: ClaudePaths.DEFAULT_CLAUDE_DIR, workdir, newSessionId)
                ?: return "" to sessionIdFlag(newSessionId)
        // `$newSessionId` 已过 UUID 校验、`projectDirName` 的产物恒为 `[A-Za-z0-9-]` ⇒ 拼进去是安全的。
        val decide =
            "if [ -e $record ]; then $SID_FLAG_VAR='--resume $newSessionId'; " +
                "else $SID_FLAG_VAR='--session-id $newSessionId'; fi; "
        return decide to " \$$SID_FLAG_VAR"
    }

    /**
     * 管道 flag。`--include-partial-messages` 不能省：没有它就没有 `stream_event`，
     * 也就没有增量帧，手机上整段整段地蹦而不是逐字流式。
     */
    const val PIPE_FLAGS =
        "--input-format stream-json --output-format stream-json --include-partial-messages --verbose"

    /**
     * 文件通道的根，相对登录 cwd（见 [PipeCommands.sessionDir]）。不写 `$HOME`，
     * 让「两端指同一个目录」不依赖引号用法。
     *
     * 这是 app 往远端写东西的唯一前缀：凡写远端的命令（`mkdir` `touch` `>>` `mv` `cp`），
     * 路径前缀只许是本常量或用户显式配置的值；`~/.cc-monitor/` 与 tmux `cc-*` 属于桌面端，只读。
     * 越界时两边都以为自己成功了，不会报错。别处不再写 `".aterm"` 字面，引用本常量。
     */
    const val ATERM_HOME = ".aterm"

    /** 切模型载荷 `/model <model>`（Claude 斜杠命令；由调用方 send-keys 投递）。 */
    fun modelCommand(model: String): String = "/model $model"
}

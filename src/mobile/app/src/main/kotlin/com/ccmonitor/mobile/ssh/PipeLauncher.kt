package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.claude.bridge.PipeSession
import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.core.claude.command.PipeCommands
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.ui.session.runOnce

/**
 * 起那条常驻管道：
 *
 * ```sh
 * tail -n 0 -f in.ndjson | claude --input-format stream-json --output-format stream-json \
 *                                 --include-partial-messages --verbose >> out.ndjson
 * ```
 *
 * 管道死了只由用户动作触发重起：开屏时、点「重新接上内容」时、每次发消息之前，各做一次幂等重起。
 * 不做后台轮询探活，也不自动重连。
 *
 * 放在 app 而不是 core-claude：管道必须跑在 tmux 里，进程不能挂在 SSH 通道上（断线不丢当前这一轮），
 * 而 core-claude 不知道 tmux。载荷由 `ClaudeInvocation.pipeInvocation` 造，包进 tmux 在这里，
 * 且只经 [SessionBackend]，不自己拼 tmux 命令。
 *
 * 不设 `@ccm_sid`：设它的前提是 pidfile 写了 `attachable:false`，而管道下 pidfile 由 `claude` 自己写，
 * 这边没有位置写这个字段；缺席即 `true`，设了就是向 cc-monitor 声明「可 attach」，那是错的。
 * 注意：不设也挡不住 cc-monitor 对这个会话 attach / kill / preview，它看的是 pane 前台命令。
 */
object PipeLauncher {
    /**
     * 起管道的完整命令串（纯函数，不执行；同 [SessionBackend]，无句柄就弄不死会话）。
     *
     * @return `null` = sessionId 不合法（它要进路径和会话名，就在这里拦）。
     */
    fun startCommand(
        backend: SessionBackend,
        sessionId: String,
        launchCommand: String?,
        workdir: String? = null,
        permissionMode: String? = null,
        /** 接着这条已有对话跑（见 `ClaudeInvocation.pipeInvocation` 的 `resumeSessionId`）。 */
        resumeSessionId: String? = null,
        /** 新对话：把编号指定给 Claude（见 `ClaudeInvocation.sessionIdFlag`）。与 [resumeSessionId] 互斥。 */
        newSessionId: String? = null,
        /** 用户显式覆盖过的 Claude 配置目录（见 `ClaudeInvocation.pipeInvocation` 的 `claudeDir`）。 */
        claudeDir: String? = null,
    ): String? {
        if (!ClaudeInvocation.isValidSessionId(sessionId)) return null
        val payload =
            ClaudeInvocation.pipeInvocation(
                launchCommand,
                sessionId,
                workdir,
                permissionMode,
                resumeSessionId,
                newSessionId,
                claudeDir,
            )
        // 走幂等那条：会话已在跑就什么都不做。非幂等那条的几段用 `;` 分隔，会话已存在时建失败、send-keys 照发，
        // 会把整条管道命令敲进正在跑的 pane。
        return backend.startOnceCommand(tmuxNameFor(sessionId), payload)
    }

    /**
     * 管道那条 tmux 会话的名字：[PIPE_SESSION_PREFIX] 加完整 sid。
     *
     * 必须与 resume 的 `cc-<sid8>` 不同名：那个名字跑的是官方 TUI。起会话命令里 `;` 不是 `&&`，
     * 会话已存在时建失败、send-keys 照发，整条 `unset …; mkdir -p … | claude … >> events.ndjson` 会被当输入
     * 敲进 TUI 并回车；碰上模态弹窗就是替人点了「是，我信任」。
     *
     * 不许截断 sid：目录 `.aterm/s/<完整 sid>` 不撞，tmux 名会撞，起 B 的管道时命中 A 的会话，跨对话串流。
     * `isValidSessionId` 的白名单 `[A-Za-z0-9_-]{1,128}` 全是 tmux 合法字符，没理由截。
     */
    fun tmuxNameFor(sessionId: String): String = "$PIPE_SESSION_PREFIX$sessionId"

    /**
     * 管道会话的专属前缀，必须与 resume 的 `cc-` 不同（见 [tmuxNameFor]）。
     *
     * 这是 app 自己起 tmux 会话的唯一前缀，只在这里定义。tmux 会话名只有三种来源：
     * 本前缀加完整 sid（管道会话）；`AgentInvocation.resumeSessionName` 的 `cc-<sid8>` / `cx-`（resume，
     * cc-monitor 定下的约定前缀）；调用方透传的用户显式值（`LaunchSpec.tmuxSession`、`DEFAULT_TMUX_SESSION`、
     * tmux 管理器里手敲的名）。别处不许再写 `"atermpipe-"` 字面，也不许自己拼会话名前缀。
     * `~/.cc-monitor/` 与 `cc-*` 那一层归 cc-monitor，只读不写。
     */
    const val PIPE_SESSION_PREFIX = "atermpipe-"

    /**
     * 起管道，并要一个肯定的成功证据。
     *
     * `exec` 拿不到退出码，「stdout 空」既可能是成功，也可能是 tmux 没装或建不出来。成功证据绑在会话真的存在上：
     * `tmuxStartOnceCommand` 打的「已在跑」与「刚建起来」两个标记都算成功，两者都没有才算失败。
     * 注意：会话存在不等于里面那条命令还活着，这里不声称管道在跑。
     *
     * 幂等：在跑就什么都不做，不在就重建，正是每次发消息前要的动作，所以上行侧不另造探针。
     * 不区分「本来在跑」和「刚重建」：上行侧先探活后写，刚重建之后写进去的行照样读得到，区分了调用方也没事可做。
     *
     * 注意：`tmux new-session -d` 返回时里面的 `tail -n 0 -f` 未必已经打开文件，紧接着追进去的那一行
     * 可能被 `-n 0` 跳过。
     *
     * @param workdir Claude 干活的目录（用户的项目目录）。null 时根本不 cd，cwd 就是登录目录。
     * @return `null` = 起成功；非 null = 人可读的失败原因。用返回类型表达，调用方没法忽略。
     */
    suspend fun start(
        channel: RemoteCommandChannel,
        backend: SessionBackend,
        sessionId: String,
        launchCommand: String?,
        workdir: String? = null,
        /** 起会话时的权限模式（白名单；不合法的值就不带这个参数）。 */
        permissionMode: String? = null,
        /** 接着这条已有对话跑。 */
        resumeSessionId: String? = null,
        /** 新对话：把编号指定给 Claude，两边编号从此一致。与 [resumeSessionId] 互斥。 */
        newSessionId: String? = null,
        /** 用户显式覆盖过的 Claude 配置目录；不传的话设置里填了、起出来的 Claude 也不认。 */
        claudeDir: String? = null,
    ): String? {
        // 两种失败走同一个出口：编号不合法（`startCommand` 给 null），接线错了（`pipeInvocation` 的 `require` 抛，
        // 如新编号不是 UUID 或两个都给了）。后者不许逃出去，否则会从 `LaunchedEffect` 里掀掉整个 app。
        val cmd =
            runCatching {
                startCommand(backend, sessionId, launchCommand, workdir, permissionMode, resumeSessionId, newSessionId, claudeDir)
                    ?: error("这个对话的编号不合法")
            }.getOrElse { e -> return e.message ?: e.javaClass.simpleName }
        val stdout =
            runCatching { runOnce(channel, cmd) }
                .getOrElse { e -> return e.message ?: e.javaClass.simpleName }
        // 已在跑与刚建起来都算成功。
        if (stdout.contains(TmuxCommands.TMUX_ALREADY_MARKER) || stdout.contains(TmuxCommands.TMUX_STARTED_MARKER)) return null
        return stdout.trim().ifBlank { "没起来，也没说为什么" }
    }

    /** 会话的文件通道目录，转发 [PipeCommands.sessionDir]（供提示和排查；读写走 [PipeSession]）。 */
    fun sessionDir(sessionId: String): String = PipeCommands.sessionDir(sessionId)

    // ---- 收掉管道会话之前的凭据闸 ----

    /**
     * 这个 tmux 会话名是不是这条管道可以动的那一类。
     *
     * 注意：它证明的是「在我们的名字空间里」，不是「我们起的」。用户可以手敲 `tmux new -s atermpipe-<某个 UUID>`，
     * tmux 不记谁建的，这边也没有「起过哪些」的落盘账本。把它读成作者证明，错的方向正好是杀错东西。
     *
     * 三道凭据缺一不可：名字以 [PIPE_SESSION_PREFIX] 开头（排掉用户会话、resume 的 `cc-` / `cx-`、`cc-*` 一层）；
     * 去掉前缀后非空；余下是合法 UUID（排掉借前缀随手起的名）。
     *
     * 第三道要 UUID 而不是 [ClaudeInvocation.isValidSessionId]：生产上的 sid 恒是 UUID（新对话用
     * `UUID.randomUUID()`，接着跑的用 Claude 的会话编号），收紧不会漏掉真起过的；更要紧的是，它让碰到 `cc-*`
     * 在结构上不可能。`TmuxCommands.isCcmTmuxName` 还认 `<X>-cc` / `<X>-cc-<N>` 后缀，`atermpipe-proj-cc`
     * 既带我们的前缀又被 cc-monitor 认领；UUID 末段是 12 位十六进制，不可能以这两种后缀收尾，交集为空。
     *
     * 只认不造，造名只许 [tmuxNameFor]。
     */
    fun isOurPipeSessionName(name: String): Boolean {
        if (!name.startsWith(PIPE_SESSION_PREFIX)) return false
        val sid = name.substring(PIPE_SESSION_PREFIX.length)
        return ClaudeInvocation.isValidUuid(sid)
    }

    /**
     * 收掉 [sessionName] 那条管道会话的命令；过不了 [isOurPipeSessionName] 就拿不到命令。
     *
     * 闸和命令同一次返回，拿到非 null 就是闸已过；不留「调用方记得先判」的余地，忘了判的后果是杀掉别人的会话。
     *
     * 只判可不可以碰，不判该不该收。目前没有生产调用方：管道里的 claude 多半还活着，而防误杀的政策
     * （`TmuxCommands.paneConfirmedNoChild`：只有明确无子进程才 kill）正拒绝杀这一档，绕过它等于给同一个判定
     * 立第二套更松的政策；安全的触发点是退出这条对话时，那在聊天界面里，还没接。
     *
     * @return `null` = 这个名字不能碰，一条命令都不该发；非 null = 经 [SessionBackend] 拿到的 `tmux kill-session`。
     */
    fun reapCommandFor(
        backend: SessionBackend,
        sessionName: String,
    ): String? = if (isOurPipeSessionName(sessionName)) backend.killCommand(sessionName) else null
}

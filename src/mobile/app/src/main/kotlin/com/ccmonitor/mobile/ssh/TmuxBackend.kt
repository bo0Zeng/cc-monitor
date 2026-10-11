package com.ccmonitor.mobile.ssh

/*
 * [SessionBackend] 的 tmux 实现。命令串全由 [TmuxCommands] 造，本文件里不许出现 `"tmux …"` 字面。
 */

/** tmux 会话后端。 */
object TmuxBackend : SessionBackend {
    override fun enter(
        sessionName: String,
        workingDir: String?,
    ): String = TmuxCommands.tmuxNewOrAttach(sessionName, workingDir)

    override fun newDetachedCommand(sessionName: String): String = TmuxCommands.tmuxNewDetachedCommand(sessionName)
}

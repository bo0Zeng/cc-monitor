package com.ccmonitor.mobile.ssh

/**
 * 用户在 ssh 里自己会敲的那两条 tmux 命令，原样敲进终端（不另拼 tmux 管理逻辑）：主机设置里的
 * 「连上就进某个会话」与 tmux 管理窗里的「新建」。纯函数，只造串不执行。带 `-u`：exec 通道与新开的 shell 里常常
 * 没有 UTF-8 的 locale，tmux 会把多字节字换成 `_`。
 *
 * 不在这里的：找会话、附着到 cc-monitor 认得的会话、结束、抓屏、往会话里送字 —— 都是那台核心的命令
 * （`terminals-list` · `launch-render-cli` 的 attach · `kill` · `terminal-preview` · `terminal-input`），手机不拼。
 * 也不改会话选项（状态栏、鼠标）：那是会话级的，一改桌面那侧接着的也跟着变；手机的滑动照桌面终端处理滚轮的规矩走（termlib）。
 */
object TmuxCommands {
    /** `tmux -u new-session -A -s <name> [-c <wd>]`：有则接、无则建（`-c` 只在新建时生效）。 */
    fun tmuxNewOrAttach(
        name: String,
        workingDir: String? = null,
    ): String {
        val wd = workingDir?.let { " -c ${shQuote(it)}" } ?: ""
        return "tmux -u new-session -A -s ${shQuote(name)}$wd"
    }

    /** `tmux -u new-session -d -s <name>`：后台新建一个会话，不进入。 */
    fun tmuxNewDetachedCommand(name: String): String = "tmux -u new-session -d -s ${shQuote(name)}"
}

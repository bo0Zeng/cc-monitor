package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.remote.shellQuote

/*
 * 不属于 tmux 面的远端命令小件：全项目唯一的 quote、默认 tmux 会话名、推公钥命令、「连接后进入」的意图。
 * tmux 命令族都在 [TmuxCommands]。
 */

/** POSIX 单引号转义（路径/会话名含空格或特殊字符也安全）。全项目唯一 quote。 */
fun shQuote(s: String): String = shellQuote(s)

/** auto-tmux 的默认会话名。 */
const val DEFAULT_TMUX_SESSION = "main"

/**
 * 把 OpenSSH 公钥幂等追加到远端 `~/.ssh/authorized_keys`（在已认证的连接上 exec）。
 *
 * 防第二行注入有两道：所有空白（含换行、tab）折叠成单空格；写入用 `printf '%s\n'` 而不用 `echo`，
 * 因为 dash/busybox 的 `echo` 会把字面 `\n` 展开成换行。整串再经 [shQuote] 包裹。
 * `grep -qxF` 整行匹配去重，输出 `ATERM_ADDED` 或 `ATERM_ALREADY`。
 * 顺带建 `~/.ssh` 并设 700/600，权限过宽时 sshd 不认这个文件。
 */
fun appendAuthorizedKeyCommand(publicKeyOpenSsh: String): String {
    val key = publicKeyOpenSsh.trim().replace(Regex("\\s+"), " ") // 净化：真实空白折叠为单空格
    val q = shQuote(key)
    return "mkdir -p ~/.ssh && chmod 700 ~/.ssh && touch ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys && " +
        "if grep -qxF $q ~/.ssh/authorized_keys; then echo ATERM_ALREADY; else printf '%s\\n' $q >> ~/.ssh/authorized_keys && echo ATERM_ADDED; fi"
}

/** 一次「连接后进入」的意图；重连时原样重放。全空 = 只开 shell。 */
data class LaunchSpec(
    /** 进入后 cd 的目录，优先于 host.defaultWorkingDir（如「在 SFTP 目录打开终端」）。 */
    val workingDir: String? = null,
    /** tmux 会话名（host 为 tmux 时用）；null → [DEFAULT_TMUX_SESSION]。 */
    val tmuxSession: String? = null,
    /** 进入后要跑的命令；null → 不追加。 */
    val command: String? = null,
)

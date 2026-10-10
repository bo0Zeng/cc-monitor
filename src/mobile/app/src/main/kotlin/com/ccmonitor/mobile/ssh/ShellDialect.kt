package com.ccmonitor.mobile.ssh

/**
 * 远端 OS 命令方言（[com.ccmonitor.mobile.core.data.db.Host.os] 由用户手动标）。
 * 命令的 quote、cd 等差异集中在这里分派，别在各处写 `if (windows)`。tmux 只有 POSIX 有，
 * [ShellDialect.supportsTmux] 为 false 时 [initialCommands] 不进 tmux 分支。
 */
enum class HostOs {
    POSIX,
    WINDOWS,
    ;

    companion object {
        /** null/"posix"/其它 → POSIX（默认）；"windows"（忽略大小写）→ WINDOWS。 */
        fun of(os: String?): HostOs = if (os?.equals("windows", ignoreCase = true) == true) WINDOWS else POSIX
    }
}

sealed interface ShellDialect {
    /** 该 OS 有没有 tmux。false 时不生成 tmux 命令，UI 关掉 tmux 会话、model、抓屏、launcher、推公钥。 */
    val supportsTmux: Boolean

    /** 会话后端，tmux 命令族的唯一入口；null = 该 OS 没有（Windows）。 */
    val sessionBackend: SessionBackend?

    /**
     * 该 shell 有没有 Claude 阅读面要的 POSIX 工具（`tail -F`、`ls`、glob）。
     * 与 [supportsTmux] 分开：阅读面依赖的是这些工具，不是 tmux。
     */
    val supportsClaudeReading: Boolean

    /** 把参数安全引用（防注入/含空格）。 */
    fun quote(s: String): String

    /** 切工作目录命令。 */
    fun cd(path: String): String

    /**
     * SFTP 路径（sshj 恒用 `/`，Windows 上常是 `/C:/Users/...`）转成本 shell `cd` 能用的路径。
     * POSIX 原样；Windows 去掉盘符前的 `/`。
     */
    fun sftpCwdToShell(sftpPath: String): String
}

/** POSIX（bash/sh）：单引号 `'...'`（内嵌 `'` → `'\''`）。复用既有 [shQuote]。 */
data object PosixDialect : ShellDialect {
    override val supportsTmux = true
    override val supportsClaudeReading = true
    override val sessionBackend: SessionBackend = TmuxBackend

    override fun quote(s: String): String = shQuote(s)

    override fun cd(path: String): String = "cd ${quote(path)}"

    override fun sftpCwdToShell(sftpPath: String): String = sftpPath
}

/**
 * Windows，按 PowerShell 生成：单引号内嵌 `'` 写成 `''`；`cd` 是 Set-Location 的别名。
 *
 * 注意：Windows OpenSSH 默认交互 shell 常是 cmd.exe（除非服务端把 `DefaultShell` 改成 PowerShell），
 * cmd.exe 里单引号是字面量，这里生成的 `cd` 会失败。
 */
data object PowerShellDialect : ShellDialect {
    override val supportsTmux = false
    override val supportsClaudeReading = false
    override val sessionBackend: SessionBackend? = null

    override fun quote(s: String): String = "'" + s.replace("'", "''") + "'"

    override fun cd(path: String): String = "cd ${quote(path)}"

    // `/C:/Users` → `C:/Users`；盘符根 `/C:`（面包屑能走到，没有尾斜杠）→ `C:`；`/foo` 这类原样。
    override fun sftpCwdToShell(sftpPath: String): String =
        if (Regex("""^/[A-Za-z]:(/|$)""").containsMatchIn(sftpPath)) sftpPath.substring(1) else sftpPath
}

/** 按 [com.ccmonitor.mobile.core.data.db.Host.os] 取方言。 */
fun dialectFor(os: String?): ShellDialect =
    when (HostOs.of(os)) {
        HostOs.WINDOWS -> PowerShellDialect
        HostOs.POSIX -> PosixDialect
    }

/** 该 os 是不是 Windows。UI 门控用的是能力位（[ShellDialect.supportsTmux] 等），不用这个。 */
fun isWindows(os: String?): Boolean = HostOs.of(os) == HostOs.WINDOWS

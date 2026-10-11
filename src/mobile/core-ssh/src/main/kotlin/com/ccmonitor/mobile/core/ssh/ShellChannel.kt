package com.ccmonitor.mobile.core.ssh

import net.schmizz.sshj.connection.channel.direct.Session
import java.io.Closeable
import java.io.InputStream
import java.io.OutputStream

/** 交互 shell 通道：原始字节流与 PTY resize，供终端桥接。 */
interface ShellChannel : Closeable {
    val input: InputStream
    val output: OutputStream

    /** PTY 尺寸变化 → 远端 SIGWINCH。 */
    fun resize(
        cols: Int,
        rows: Int,
    )
}

/** sshj 那一份（[SshConnection] 用）。 */
class SshjShellChannel(
    private val session: Session,
    private val shell: Session.Shell,
) : ShellChannel {
    override val input: InputStream get() = shell.inputStream
    override val output: OutputStream get() = shell.outputStream

    override fun resize(
        cols: Int,
        rows: Int,
    ) {
        shell.changeWindowDimensions(cols, rows, 0, 0)
    }

    override fun close() {
        runCatching { shell.close() }
        runCatching { session.close() }
    }
}

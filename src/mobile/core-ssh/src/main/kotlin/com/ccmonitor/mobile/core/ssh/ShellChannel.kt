package com.ccmonitor.mobile.core.ssh

import net.schmizz.sshj.connection.channel.direct.Session
import java.io.Closeable
import java.io.InputStream
import java.io.OutputStream

/** 交互 shell 通道：原始字节流与 PTY resize，供终端桥接。 */
class ShellChannel(
    private val session: Session,
    private val shell: Session.Shell,
) : Closeable {
    val input: InputStream get() = shell.inputStream
    val output: OutputStream get() = shell.outputStream

    /** PTY 尺寸变化 → 远端 SIGWINCH（sshj 在 Shell 上：`changeWindowDimensions`）。 */
    fun resize(
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

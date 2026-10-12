package com.ccmonitor.mobile.core.ssh

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

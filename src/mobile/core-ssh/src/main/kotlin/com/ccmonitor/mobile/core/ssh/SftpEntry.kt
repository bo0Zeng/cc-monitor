package com.ccmonitor.mobile.core.ssh

/** SFTP 目录项。[path] 为远端全路径，[size] 为字节数（目录无意义）。 */
data class SftpEntry(
    val name: String,
    val path: String,
    val isDir: Boolean,
    val size: Long,
)

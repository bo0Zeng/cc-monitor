package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.remote.RemoteExecutor
import kotlinx.coroutines.flow.Flow
import java.io.Closeable
import java.io.File

/**
 * SSH 连接能力抽象。[SshConnection] 是唯一的生产实现；[SshConnectionManager] 的竞速、代号与重连逻辑靠它能在 JVM 上测。
 *
 * 默认参数只能写在接口声明上（override 不能再带默认值），经接口类型调用即可用默认值。
 */
interface SshTransport :
    Closeable,
    // execCapture（带退出码与 stderr）也是传输能力：放进本接口，测试替身也必须实现「退出码 null 不算成功」。
    RemoteExecutor {
    /** 已连接且已认证。 */
    val isConnected: Boolean

    /**
     * 活性探测。[isConnected] 只读 sshj 本地标志，死 TCP 在内核超时前仍是 true，复用连接前要真走一次往返。
     * 默认实现就是 [isConnected]；[SshConnection] 实现为开关一次 session，超过 [timeoutMs] 判死并关整条连接。
     * [timeoutMs] 不设默认值，由调用方显式传。
     */
    suspend fun probeAlive(timeoutMs: Long): Boolean = isConnected

    /**
     * 连接并认证。[configs] 是多个地址，首个成功者胜；各地址共用 `configs.first().jumpVia` 的同一条跳板，跳板只连一次。
     * 直连多地址或单地址并发竞速；跳板 + 多地址串行（共享跳板上并发拆活隧道会拖垮整条跳板）。
     * [onEvent] 非空时按阶段发 [SshConnectEvent]。
     */
    suspend fun connect(
        configs: List<ConnectionConfig>,
        onEvent: ((SshConnectEvent) -> Unit)? = null,
    )

    suspend fun openShell(
        term: String = "xterm-256color",
        cols: Int = 80,
        rows: Int = 24,
    ): ShellChannel

    fun execStream(command: String): Flow<ByteArray>

    suspend fun sftpList(path: String = "."): List<SftpEntry>

    suspend fun sftpRealPath(path: String): String

    suspend fun sftpDownload(
        remotePath: String,
        local: File,
    )

    suspend fun sftpUpload(
        local: File,
        remotePath: String,
    )

    suspend fun sftpRename(
        from: String,
        to: String,
    )

    suspend fun sftpDelete(
        path: String,
        isDir: Boolean,
    )

    suspend fun sftpMkdir(path: String)

    suspend fun sftpReadText(
        path: String,
        maxBytes: Int = 256 * 1024,
    ): String

    /** 读供编辑的全量文本。过大（超过 maxBytes）或疑似二进制（含 NUL）返回 null：拒绝编辑，免得截断或乱码写坏。 */
    suspend fun sftpReadTextForEdit(
        path: String,
        maxBytes: Int = 256 * 1024,
    ): String?

    /** 以 UTF-8 覆写远端文件（WRITE|CREAT|TRUNC）。 */
    suspend fun sftpWriteText(
        remotePath: String,
        text: String,
    )
}

/** 单地址便捷入口：委托给多地址 [connect]，单元素列表不竞速。 */
suspend fun SshTransport.connect(
    config: ConnectionConfig,
    onEvent: ((SshConnectEvent) -> Unit)? = null,
) = connect(listOf(config), onEvent)

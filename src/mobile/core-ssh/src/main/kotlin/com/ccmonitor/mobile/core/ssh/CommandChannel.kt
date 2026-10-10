package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.remote.ConnectionDeadException
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.remote.RemoteDuplexChannel
import com.ccmonitor.mobile.core.remote.RemoteExecutor
import kotlinx.coroutines.flow.flow

/**
 * 由 [SshConnectionManager] 造 [RemoteCommandChannel] 的唯一工厂；每次订阅开一条 SSH exec，Flow 取消即杀远端命令。
 *
 * 无活连接（连接不存在或 `!isConnected`）时，收集 exec 流抛 [ConnectionDeadException]（IOException 子类），不返回空流：
 * 空流与「命令成功、输出为空」分不开，断连会被当成可信的「没有会话」。
 */
fun SshConnectionManager.commandChannel(id: String): RemoteCommandChannel =
    RemoteCommandChannel { cmd ->
        val conn = connection(id)
        if (conn != null && conn.isConnected) {
            conn.execStream(cmd)
        } else {
            flow { throw ConnectionDeadException("SSH 连接不可用（尚未建立或已断开）") }
        }
    }

/**
 * [RemoteExecutor] 的工厂，与 [commandChannel] 并列（一次性 vs 流式）。
 *
 * 无活连接时同样抛 [ConnectionDeadException]，不返回「退出码 0、输出为空」的假成功。
 * 注意：必须和 [commandChannel] 用同一个 `connection(id)` + `isConnected` 判据。
 */
fun SshConnectionManager.commandExecutor(id: String): RemoteExecutor = liveExecutor { connection(id) }

/** [RemoteExecutor]：每次调用现取 [conn]；没有活连接 ⇒ 抛 [ConnectionDeadException]（与上面同一个判据）。 */
fun liveExecutor(conn: () -> SshTransport?): RemoteExecutor =
    RemoteExecutor { cmd -> alive(conn()).execCapture(cmd) }

private fun alive(conn: SshTransport?): SshTransport =
    conn?.takeIf { it.isConnected } ?: throw ConnectionDeadException("SSH 连接不可用（尚未建立或已断开）")

/**
 * [RemoteDuplexChannel] 的工厂（常驻流那条双向长 exec）。无活连接时抛 [ConnectionDeadException]，判据与上面两个同一个。
 */
fun SshConnectionManager.duplexChannel(id: String): RemoteDuplexChannel = liveDuplex { connection(id) }

/** [RemoteDuplexChannel]：每次打开现取 [conn]；没有活连接 ⇒ 抛 [ConnectionDeadException]。 */
fun liveDuplex(conn: () -> SshTransport?): RemoteDuplexChannel = RemoteDuplexChannel { cmd -> alive(conn()).execDuplex(cmd) }

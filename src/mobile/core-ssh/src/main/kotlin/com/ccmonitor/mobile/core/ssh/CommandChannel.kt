package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.remote.ConnectionDeadException
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
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
fun SshConnectionManager.commandExecutor(id: String): RemoteExecutor =
    RemoteExecutor { cmd ->
        val conn = connection(id)
        if (conn != null && conn.isConnected) {
            conn.execCapture(cmd)
        } else {
            throw ConnectionDeadException("SSH 连接不可用（尚未建立或已断开）")
        }
    }

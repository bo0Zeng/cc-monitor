package com.ccmonitor.mobile.core.ssh

// 没有 DISCONNECTED：断开就是把会话条目整个移除，连接存活另由 isConnected 与流 EOF 反映。
enum class SessionStatus { CONNECTING, CONNECTED, RECONNECTING, ERROR }

/** 一个 SSH 会话的名义状态。 */
data class SessionState(
    val id: String,
    val hostId: String,
    val label: String,
    val status: SessionStatus,
    val error: String? = null,
)

/**
 * 计入活跃会话数：CONNECTED。保活计数与后台 watcher 门控共用这一个谓词。
 * 注意：这是会话的名义状态，不是底层连接的存活判据；底层断连由 exec 抛 `ConnectionDeadException` 等反应式入口反映。
 */
fun SessionState.countsAsActive(): Boolean = status == SessionStatus.CONNECTED

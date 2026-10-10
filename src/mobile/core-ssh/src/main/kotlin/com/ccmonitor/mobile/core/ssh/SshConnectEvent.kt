package com.ccmonitor.mobile.core.ssh

/**
 * 一次连接过程中的阶段事件，供连接页实时滚动显示。
 *
 * 每个事件带 [host]:[port]，多地址竞速和跳板时能分清是哪个地址在进展。
 * sshj 的 `client.connect` 一步里含 TCP、协议协商、密钥交换与主机密钥校验，没有更细的钩子，
 * 所以分为 [Stage.TCP]、[Stage.NEGOTIATE]，再单列 [Stage.HOSTKEY] 与 [Stage.AUTH]。
 */
data class SshConnectEvent(
    val host: String,
    val port: Int,
    val stage: Stage,
    val message: String,
) {
    enum class Stage { TCP, NEGOTIATE, HOSTKEY, AUTH, ESTABLISHED, ERROR }
}

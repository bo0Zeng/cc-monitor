package com.ccmonitor.mobile.core.ssh

/**
 * 一次连接过程中的阶段事件，供连接页实时滚动显示。
 *
 * 每个事件带 [host]:[port]，多地址竞速和跳板时能分清是哪个地址在进展。
 * 由 Rust 核心报的阶段行换来（[RustTransport]）：起拨 ⇒ [Stage.TCP]，校验 host key ⇒ [Stage.HOSTKEY]，竞速胜出 ⇒ [Stage.NEGOTIATE]，
 * 鉴权 ⇒ [Stage.AUTH]，连上 ⇒ [Stage.ESTABLISHED]，某个地址失败 ⇒ [Stage.ERROR]。
 */
data class SshConnectEvent(
    val host: String,
    val port: Int,
    val stage: Stage,
    val message: String,
) {
    enum class Stage { TCP, NEGOTIATE, HOSTKEY, AUTH, ESTABLISHED, ERROR }
}

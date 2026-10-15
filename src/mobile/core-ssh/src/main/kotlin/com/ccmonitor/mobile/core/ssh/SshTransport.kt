package com.ccmonitor.mobile.core.ssh

import com.ccmonitor.mobile.core.remote.RemoteExecutor
import kotlinx.coroutines.flow.Flow
import java.io.Closeable

/**
 * SSH 连接能力抽象。[RustTransport] 是唯一的生产实现；[SshConnectionManager] 的代号 · 持有 · 跟着共用循环换连接靠它能在 JVM 上测。
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

    /**
     * 这条连接的拨号身份那一份请求（JSON，不带用法）：常驻流（共用那条重连循环）在嵌的核心里照它拨、在同一条连接上开自己的链路。
     * 连上过就有（握着的那条死了也还在：共用那条照它自己再拨）；没连上过 · 不是走核心的传输 ⇒ `null`。
     */
    val dialRequest: String? get() = null

    /**
     * 同一份拨号身份上的一条新的（还没握）：[connect] 它照原来那份请求再握一条（池里有活的就复用），[dialRequest] 一字不差。
     * SSH 那层跟着共用那条重连循环换连接时用。做不到 ⇒ `null`（调用方另造一条）。
     */
    fun again(): SshTransport? = null

    /** 握着的那条被核心收了（对端断 · 保活判死）时叫 [f] 一次；自己 [close] 的不叫。 */
    fun onDrop(f: () -> Unit) = Unit
}

/** 单地址便捷入口：委托给多地址 [connect]，单元素列表不竞速。 */
suspend fun SshTransport.connect(
    config: ConnectionConfig,
    onEvent: ((SshConnectEvent) -> Unit)? = null,
) = connect(listOf(config), onEvent)

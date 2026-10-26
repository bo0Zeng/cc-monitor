package com.ccmonitor.mobile.core.ssh

import java.io.Closeable
import java.io.IOException

/**
 * SSH 连接能力抽象。[RustTransport] 是唯一的生产实现；[SshConnectionManager] 的代号 · 持有 · 跟着共用循环换连接靠它能在 JVM 上测。
 *
 * 默认参数只能写在接口声明上（override 不能再带默认值），经接口类型调用即可用默认值。
 */
interface SshTransport : Closeable {
    /** 已连接且已认证。 */
    val isConnected: Boolean

    /**
     * 跑一条一次性命令并等它结束（带退出码与 stderr 尾巴）。实现方必须守的两条：
     * 1. 无条件排空 stderr：stderr 与 stdout 共用同一个通道窗口，不读 stderr 会耗尽窗口、把 stdout 一起拖停。
     * 2. 绝不分配 PTY：PTY 的 ONLCR 会污染字节流。
     *
     * @return 永远返回结果对象；**命令失败不抛异常** —— 调用方常常需要区分「失败了但可以保守降级」与「失败了必须告诉用户」。
     *   连接不可用等**传输层**故障仍然抛（[ConnectionDeadException]）。
     */
    suspend fun execCapture(command: String): ExecResult

    /**
     * 照 [config] 那一份请求连接并认证（多地址竞速 · 跳板都在核心里）。[onEvent] 非空时按阶段发 [SshConnectEvent]。
     */
    suspend fun connect(
        config: ConnectionConfig,
        onEvent: ((SshConnectEvent) -> Unit)? = null,
    )

    suspend fun openShell(
        term: String = "xterm-256color",
        cols: Int = 80,
        rows: Int = 24,
    ): ShellChannel

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

/**
 * 一次远端命令的完整结果。
 *
 * [stderrTail] 只保尾部若干字节（错误定位够用，防大 stderr 占内存）。
 */
data class ExecResult(
    /** `null` = 远端**没回**退出码（连接中断、命令被杀）。**不要**把它当成 0。 */
    val exitStatus: Int?,
    val stdout: String,
    val stderrTail: String,
)

/**
 * 底层连接不可用（连接对象不存在 / 未连上）时抛这个异常。是 IOException 子类，只 `catch (IOException)` 的地方行为不变；
 * 要区分「连接死了」与「其他传输/命令错」的消费方单独 catch 它。
 */
class ConnectionDeadException(
    message: String,
) : IOException(message)

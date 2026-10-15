package com.ccmonitor.mobile.core.ssh

import kotlinx.coroutines.sync.Mutex
import java.util.concurrent.atomic.AtomicBoolean

/**
 * 单个 id（会话或连接）的全部连接状态，由 [lock] 统一守护。
 *
 * 并发规矩：
 * - [connection] / [configs] / [reached] 有无锁的跨线程读，所以是 `@Volatile`。
 * - [connecting] / [generation] / [holders] 只在 [lock] 内访问，不要加无锁读点。
 * - [following] 用 [AtomicBoolean] 的 `compareAndSet(false, true)` 做原子去重。
 * - 本对象永不从状态表移除：[lock]、[connectLock]、[generation] 须跨 disconnect 稳定，disconnect 只清字段。
 */
internal class HostConnectionState(
    // 没有逻辑读者，只作调试与日志里的身份标识。
    val id: String,
) {
    /** 短临界区锁。同 id 恒为同一把锁。 */
    val lock = Any()

    /** 单飞连接锁：同 id 并发连接串行化。 */
    val connectLock = Mutex()

    /** 活 transport。有无锁读。 */
    @Volatile
    var connection: SshTransport? = null

    /** 在飞连接。只在 [lock] 内访问。 */
    var connecting: SshTransport? = null

    /** 多地址配置；null 表示没有。有无锁读。 */
    @Volatile
    var configs: List<ConnectionConfig>? = null

    /** 代号。只在 [lock] 内访问；只有 disconnect 自增它，用来作废在飞 connect。 */
    var generation: Int = 0

    /** 持有者集。只在 [lock] 内访问；非空即有持有者。 */
    val holders = mutableSetOf<String>()

    /** 共用那条重连循环最近一次报的：这台够没够着（[SshConnectionManager.loopReached]）。 */
    @Volatile
    var reached: Boolean = false

    /** 跟着共用循环换连接那一次在飞（去重）。 */
    val following = AtomicBoolean(false)
}

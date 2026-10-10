package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.ssh.SshConnectEvent
import com.ccmonitor.mobile.core.ssh.SshConnectionManager
import com.ccmonitor.mobile.core.ssh.SshTransport

/**
 * 主机连接的生与死都经这里，别处只拿句柄。
 *
 * - 生：[connect]，解析认证、跳板、多地址，再交给 [SshConnectionManager.connect]（同主机 single-flight）。
 * - 持有与放手：[retain] / [release]。holder token 由调用方自己铸，本类不持状态。
 * - 句柄：[connection]。
 * - 强制全断：[disconnectAll]。
 *
 * 不提供单机强断 `disconnect(id)`：生产代码里没人要。
 * 别的文件仍会碰 [SshConnectionManager]，要的是通道工厂（`commandChannel` / `commandExecutor`）、
 * 会话计数和 `probeAndReconnectStale`，不是连接的生死，所以不收进这里。
 *
 * 收口是因为调用方在哪个线程、有几个，靠人记会记错：在主线程上连接时，失败路径的同步 `close()` 会成为
 * 主线程网络写，异常再被外层吞掉，连接就静默泄漏。
 */
class HostConnector(
    private val manager: SshConnectionManager,
    private val hostRepo: HostRepository,
    private val identityRepo: IdentityRepository,
) {
    /**
     * 连接一台主机，得到 transport（多地址竞速在 [SshConnectionManager] 里）。
     * @param host 调用方已取的主机，这里不重取。
     * @param pendingIdentityId 本次选定但还没存下的身份；没有传 null。
     * @param onEvent 逐阶段的连接日志；不需要传 null。
     * @param expectedGen 调用方在 connect 前 [retain] 拿到的重连纪元，透传给 manager 守门：
     *   retain 与建连之间若来了一次 force-disconnect，这次连接作废，不留孤儿。null = 不守门。
     */
    suspend fun connect(
        host: Host,
        pendingIdentityId: String? = null,
        onEvent: ((SshConnectEvent) -> Unit)? = null,
        expectedGen: Int? = null,
    ): SshTransport =
        manager.connect(
            host.id,
            host.id,
            host.label,
            host.toConnectionConfigs(
                resolveHostAuth(host, pendingIdentityId, identityRepo, hostRepo),
                resolveJumpChain(host, hostRepo, identityRepo),
            ),
            onEvent = onEvent,
            expectedGen = expectedGen,
        )

    /**
     * 登记一个持有者，返回当前重连纪元（语义同 [SshConnectionManager.retain]，按 holder 幂等）。
     *
     * 注意：必须在 [connect] 之前调，返回值原样交给 [connect] 的 `expectedGen`；
     * 否则 retain 之后、建连之前的一次 force-disconnect 会留下一条没人持有的孤儿连接。
     */
    fun retain(
        hostId: String,
        holder: String,
    ): Int = manager.retain(hostId, holder)

    /**
     * 放手一个持有者；这台主机没有持有者了才真断开（语义同 [SshConnectionManager.release]）。
     * 同主机多 tab 共享一条连接；物理 `close` 由 manager 派到后台，绝不落在调用方线程上。
     */
    fun release(
        hostId: String,
        holder: String,
    ) = manager.release(hostId, holder)

    /**
     * 这台主机当前的活 transport，没有就 `null`（纯 get，不建连；断开后同步变 `null`）。
     *
     * 注意：这是瞬时快照，拿到后随时可能因别的持有者放手而断。要复用先探活：死 TCP 上裸 `isConnected`
     * 会骗人约 75 秒。
     */
    fun connection(hostId: String): SshTransport? = manager.connection(hostId)

    /**
     * 不管持有者，断开全部连接：清持有者集、纪元加一作废在飞的 connect，物理 `close` 派到后台。
     *
     * 唯一的调用方是 `SshKeepAliveService.onTaskRemoved`（用户把任务划走），跑在主线程上，
     * 所以 close 必须在后台。别在别处调：会把别的 tab 正在用的连接一起断掉。
     */
    fun disconnectAll() = manager.disconnectAll()
}

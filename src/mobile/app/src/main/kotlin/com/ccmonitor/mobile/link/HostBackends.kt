package com.ccmonitor.mobile.link

import android.util.Log
import com.ccmonitor.mobile.core.claude.link.BackendFeed
import com.ccmonitor.mobile.core.claude.link.OneShot
import com.ccmonitor.mobile.core.claude.link.ResidentLink
import com.ccmonitor.mobile.core.ssh.liveDuplex
import com.ccmonitor.mobile.core.ssh.liveExecutor
import com.ccmonitor.mobile.ssh.HostConnector
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

/**
 * 每台一条常驻流（[BackendFeed]），进程级（聊天 · 列表 · 用量 · 完成通知共用同一条）。
 * 这里只接三根线：SSH 那层此刻的连接（[HostConnector.linkOf]）· 在那条连接上开流（[ResidentLink]）·
 * 流断了叫 SSH 那层探活（[HostConnector.probeStale]）。
 */
class HostBackends(
    private val connector: HostConnector,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val byHost = ConcurrentHashMap<String, BackendFeed>()

    fun of(hostId: String): BackendFeed = byHost.getOrPut(hostId) { feed(hostId) }

    /** App 回到前台：每台断着的当场再接（手机会被杀后台，不是常驻）。 */
    fun onForeground() = byHost.values.forEach(BackendFeed::onForeground)

    private fun feed(hostId: String): BackendFeed {
        val reported = ConcurrentHashMap.newKeySet<String>()

        // 两端契约对不上：一种报一次（`架构.md` D4），进日志。
        fun onBreak(what: String) {
            if (reported.add(what.substringBefore('：'))) Log.w(TAG, "$hostId：$what")
        }
        val conn = { connector.connection(hostId) }
        return BackendFeed(
            scope = scope,
            ssh = connector.linkOf(hostId),
            open = { s -> ResidentLink(OneShot(liveExecutor(conn)), liveDuplex(conn)).open(s, nonce(), TIMEOUTS, ::onBreak) },
            onLost = { connector.probeStale() },
            onBreak = ::onBreak,
        )
    }

    private fun nonce(): String = UUID.randomUUID().toString().take(NONCE_LEN)

    private companion object {
        const val TAG = "HostBackend"
        const val NONCE_LEN = 8

        /** 握手（读 hello · 等 attach 回话）的期限：冷启要起常驻、读盘，给足。 */
        val TIMEOUTS = ResidentLink.Timeouts(handshakeMs = 20_000)
    }
}

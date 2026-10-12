package com.ccmonitor.mobile.link

import android.util.Log
import com.ccmonitor.mobile.core.claude.link.EmbeddedBuild
import com.ccmonitor.mobile.core.claude.link.ResidentCore
import com.ccmonitor.mobile.core.claude.link.ResidentFeed
import com.ccmonitor.mobile.core.claude.link.SshLink
import com.ccmonitor.mobile.ssh.HostConnector
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.map
import java.time.ZoneId
import java.util.concurrent.ConcurrentHashMap

/**
 * 每台一条常驻流（[ResidentFeed]），进程级（聊天 · 列表 · 用量 · 完成通知共用同一条）。流本身在嵌的核心里；
 * 这里只接几根线：SSH 那层此刻的拨号请求（[HostConnector.linkOf]）· 这台叫什么 · 流断了叫 SSH 那层探活（[HostConnector.probeStale]）。
 */
class HostBackends(
    private val connector: HostConnector,
    private val labelOf: suspend (hostId: String) -> String,
    private val core: ResidentCore = CoreResident,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val byHost = ConcurrentHashMap<String, ResidentFeed>()

    fun of(hostId: String): ResidentFeed = byHost.getOrPut(hostId) { feed(hostId) }

    /** App 回到前台 · 网络变了：每台断着的当场再接（手机会被杀后台，不是常驻）。 */
    fun onForeground() = byHost.values.forEach(ResidentFeed::onForeground)

    private fun feed(hostId: String): ResidentFeed {
        val reported = ConcurrentHashMap.newKeySet<String>()

        // 两端契约对不上：一种报一次（`架构.md` D4），进日志。
        fun onBreak(what: String) {
            if (reported.add(what.substringBefore('：'))) Log.w(TAG, "$hostId：$what")
        }
        return ResidentFeed(
            scope = scope,
            core = core,
            machine = { labelOf(hostId) },
            ssh = connector.linkOf(hostId).map { t -> t?.dialRequest?.let(::SshLink) },
            mine = EmbeddedBuild.ID,
            tz = ZoneId.systemDefault().id,
            onLost = { connector.probeStale() },
            onBreak = ::onBreak,
        )
    }

    private companion object {
        const val TAG = "HostBackend"
    }
}

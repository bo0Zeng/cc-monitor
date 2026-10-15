package com.ccmonitor.mobile.link

import android.util.Log
import com.ccmonitor.mobile.core.claude.link.EmbeddedBuild
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.ResidentCore
import com.ccmonitor.mobile.core.claude.link.ResidentFeed
import com.ccmonitor.mobile.ssh.HostConnector
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import java.time.ZoneId
import java.util.concurrent.ConcurrentHashMap

/**
 * 每台一条常驻流（[ResidentFeed]），进程级（聊天 · 列表 · 用量 · 完成通知共用同一条）。流本身在嵌的核心里，
 * **它就是手机上唯一那条重连循环**；SSH 那层（终端 · 文件）跟着它走。这里只接几根线：
 * - 这台的拨号请求（[HostConnector.dialOf]，SSH 那层连上过就一直在）⇒ 交核心；有拨号请求的每一台都起一条（[HostConnector.dialing]）。
 * - 它报的通不通 ⇒ 原样转给 SSH 那层（[HostConnector.loopReached]：通着，或停在要人动手的那一步，都是够着了那一台）。
 * - 这台叫什么。
 */
class HostBackends(
    private val connector: HostConnector,
    private val labelOf: suspend (hostId: String) -> String,
    private val core: ResidentCore = CoreResident,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val byHost = ConcurrentHashMap<String, ResidentFeed>()

    init {
        scope.launch { connector.dialing.collect { ids -> ids.forEach { of(it).ensure() } } }
    }

    // computeIfAbsent：原子，一台只起一条（多起一条的那份也会往 SSH 那层报「没够着」）。
    fun of(hostId: String): ResidentFeed = byHost.computeIfAbsent(hostId) { feed(it) }

    /** App 回到前台 · 网络回来了 / 变了：每台交给核心（[ResidentFeed.wake]：连着的当场问活，断着的当场再接）。 */
    fun wake() = byHost.values.forEach(ResidentFeed::wake)

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
            ssh = connector.dialOf(hostId),
            mine = EmbeddedBuild.ID,
            tz = ZoneId.systemDefault().id,
            onBreak = ::onBreak,
        ).also { f -> scope.launch { f.state.collect { connector.loopReached(hostId, reached(it)) } } }
    }

    /** 共用那条够没够着那一台：通着 · 停在要人动手的那一步（没装 · 不是 Unix…：登上了那一台才知道）。 */
    private fun reached(s: LinkState): Boolean = s is LinkState.Up || (s is LinkState.Down && s.needsHand)

    private companion object {
        const val TAG = "HostBackend"
    }
}

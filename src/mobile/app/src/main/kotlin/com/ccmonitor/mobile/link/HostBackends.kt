package com.ccmonitor.mobile.link

import android.util.Log
import com.ccmonitor.mobile.core.claude.link.EmbeddedBuild
import com.ccmonitor.mobile.core.claude.link.ResidentCore
import com.ccmonitor.mobile.core.claude.link.ResidentFeed
import com.ccmonitor.mobile.ssh.HostConnector
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.time.ZoneId
import java.util.concurrent.ConcurrentHashMap

/**
 * 每台一条常驻流（[ResidentFeed]），进程级（聊天 · 列表 · 用量 · 完成通知共用同一条）。流本身在嵌的核心里，
 * **它就是手机上唯一那条重连循环**；SSH 那层（终端 · 文件）跟着它走。这里只接几根线：
 * - 这台的拨号请求（[HostConnector.dialOf]，SSH 那层连上过就一直在）⇒ 交核心；有拨号请求的每一台都起一条（[HostConnector.dialing]）。
 * - 它报的 SSH 够没够着（与流通不通分开：常驻后端起不来时也照样够着）⇒ 原样转给 SSH 那层（[HostConnector.loopReached]）；
 *   SSH 那层握着的那条死了 ⇒ 叫醒它（[HostConnector.holdLost]）。终端是逃生口，不跟着后端卡住。
 * - 这台叫什么。
 */
class HostBackends(
    private val connector: HostConnector,
    private val labelOf: suspend (hostId: String) -> String,
    /** 包里那几格后端解到私有目录、交核心的那一段（装 / 更新后端用；解压在 IO 线程上）。 */
    private val backendSlots: () -> String = { "[]" },
    private val core: ResidentCore = CoreResident,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val byHost = ConcurrentHashMap<String, ResidentFeed>()

    init {
        scope.launch { connector.dialing.collect { ids -> ids.forEach { of(it).ensure() } } }
        // SSH 那层握着的那条死了 ⇒ 交给这台的常驻流当一件事（当场再拨；停在要人动手那一步的也叫起来）。
        scope.launch { connector.holdLost.collect { of(it).wake() } }
    }

    // computeIfAbsent：原子，一台只起一条（多起一条的那份也会往 SSH 那层报「没够着」）。
    fun of(hostId: String): ResidentFeed = byHost.computeIfAbsent(hostId) { feed(it) }

    /** 给这台装 / 更新后端（判定与做法在核心；走到哪 · 结局经 [ResidentFeed.deploying]）。 */
    fun deploy(hostId: String) = of(hostId).deploy { withContext(Dispatchers.IO) { backendSlots() } }

    /** App 回到前台 · 网络回来了 / 变了：每台交给核心（[ResidentFeed.wake]：连着的当场问活，断着的当场再接）。 */
    fun wake() = byHost.values.forEach(ResidentFeed::wake)

    private fun feed(hostId: String): ResidentFeed {
        val reported = ConcurrentHashMap.newKeySet<String>()

        // 两端契约对不上：一种报一次，进日志。
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
        ).also { f -> scope.launch { f.sshReached.collect { connector.loopReached(hostId, it) } } }
    }

    private companion object {
        const val TAG = "HostBackend"
    }
}

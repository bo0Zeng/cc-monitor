package com.ccmonitor.mobile.link

import android.util.Log
import com.ccmonitor.mobile.core.claude.link.FrameClient
import com.ccmonitor.mobile.core.claude.link.LinkOutcome
import com.ccmonitor.mobile.core.claude.link.OneShot
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.ResidentLink
import com.ccmonitor.mobile.core.claude.link.SessionTable
import com.ccmonitor.mobile.core.claude.link.str
import com.ccmonitor.mobile.core.ssh.liveDuplex
import com.ccmonitor.mobile.core.ssh.liveExecutor
import com.ccmonitor.mobile.ssh.HostConnector
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.io.IOException
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

/** 一台的常驻流此刻通不通。 */
sealed interface LinkState {
    data object Connecting : LinkState

    data object Up : LinkState

    /** 走不通：[outcome] 是那条路断在哪（[LinkOutcome] 之一）；`null` ＝ 接上过、流断了。 */
    data class Down(
        val outcome: LinkOutcome?,
        val transport: String? = null,
    ) : LinkState
}

/**
 * 一台机器的后端：一条常驻流（`ccm -- --resident-attach`）＋ 上面折出来的会话表。手机对这台的所有一问一答都经 [call]。
 * SSH 连接由别处（`HostConnector`）建好、持有；这里只经它拿那条连接、在上面开那条双向 exec。
 */
class HostBackend internal constructor(
    private val hostId: String,
    private val connector: HostConnector,
    private val scope: CoroutineScope,
) {
    private val stateFlow = MutableStateFlow<LinkState>(LinkState.Connecting)
    val state: StateFlow<LinkState> = stateFlow.asStateFlow()

    private val tableFlow = MutableStateFlow(SessionTable())

    /** 流上的会话帧折出来的会话表（此刻有哪些会话、各自的状态字与语气）。 */
    val table: StateFlow<SessionTable> = tableFlow.asStateFlow()

    private val changeFlow = MutableSharedFlow<String>(extraBufferCapacity = CHANGE_BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    /** 每来一帧发它的 `kind`：出口据此重问（`quota_changed` ⇒ 重问 `quota-read` …）。 */
    val changes: SharedFlow<String> = changeFlow.asSharedFlow()

    @Volatile private var client: FrameClient? = null

    @Volatile private var run: Job? = null
    private val reported = ConcurrentHashMap.newKeySet<String>()

    /** 没在接 / 没接上 ⇒ 接一次；已经通着 ⇒ 什么都不做。 */
    fun ensure() {
        if (run?.isActive == true) return
        run = scope.launch { connectAndPump() }
    }

    /** 重接：断开这一条（有的话）、整份重来（后端会把会话整份重报一遍）。 */
    fun reconnect() {
        run?.cancel()
        client?.close()
        client = null
        run = scope.launch { connectAndPump() }
    }

    /** 一问一答。流没通 ⇒ [Reply.LinkDown]。 */
    suspend fun call(
        cmd: String,
        args: Map<String, Any?>? = null,
        withinMs: Long = DEFAULT_CALL_MS,
    ): Reply = client?.call(cmd, args, withinMs) ?: Reply.LinkDown

    private suspend fun connectAndPump() {
        stateFlow.value = LinkState.Connecting
        tableFlow.value = SessionTable()
        val runScope = CoroutineScope(scope.coroutineContext + Job(scope.coroutineContext[Job]))
        try {
            val conn = { connector.connection(hostId) }
            val link = ResidentLink(OneShot(liveExecutor(conn)), liveDuplex(conn))
            when (val got = link.open(runScope, nonce(), TIMEOUTS, ::onBreak)) {
                is LinkOutcome.Up -> pump(got.client)
                else -> stateFlow.value = LinkState.Down(got)
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: IOException) {
            stateFlow.value = LinkState.Down(null, e.message ?: e.javaClass.simpleName)
        } finally {
            runScope.cancel()
        }
    }

    private suspend fun pump(c: FrameClient) {
        client = c
        stateFlow.value = LinkState.Up
        for (frame in c.frames) {
            val next = tableFlow.value.step(frame, ::onBreak)
            tableFlow.value = next
            frame.str("kind")?.let(changeFlow::tryEmit)
            if (next.needsResync) {
                // 丢了会话帧：这张表不可信了 ⇒ 重接，后端整份重报。
                c.close()
                break
            }
        }
        client = null
        if (stateFlow.value == LinkState.Up) stateFlow.value = LinkState.Down(null)
        if (tableFlow.value.needsResync) reconnect()
    }

    /** 两端契约对不上：一种报一次（`架构.md` D4），进日志。 */
    private fun onBreak(what: String) {
        if (reported.add(what.substringBefore('：'))) Log.w(TAG, "$hostId：$what")
    }

    private fun nonce(): String = UUID.randomUUID().toString().take(NONCE_LEN)

    private companion object {
        const val TAG = "HostBackend"
        const val CHANGE_BUFFER = 64
        const val NONCE_LEN = 8
        const val DEFAULT_CALL_MS = 15_000L

        /** 握手（读 hello · 等 attach 回话）与格目录那一问的期限：冷启要起常驻、读盘，给足。 */
        val TIMEOUTS = ResidentLink.Timeouts(handshakeMs = 20_000, callMs = 15_000)
    }
}

/** 每台一个 [HostBackend]，进程级（聊天 · 列表 · 用量共用同一条流）。 */
class HostBackends(
    private val connector: HostConnector,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val byHost = ConcurrentHashMap<String, HostBackend>()

    fun of(hostId: String): HostBackend = byHost.getOrPut(hostId) { HostBackend(hostId, connector, scope) }
}

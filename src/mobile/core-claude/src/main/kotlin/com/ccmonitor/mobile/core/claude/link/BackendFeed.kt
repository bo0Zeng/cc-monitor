package com.ccmonitor.mobile.core.claude.link

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import java.io.IOException

/** 一台的常驻流此刻通不通。 */
sealed interface LinkState {
    data object Connecting : LinkState

    data object Up : LinkState

    /** 走不通：[outcome] 是那条路断在哪（[LinkOutcome] 之一）；`null` ＝ 接上过、流断了（或 SSH 没通）。 */
    data class Down(
        val outcome: LinkOutcome?,
        val transport: String? = null,
    ) : LinkState
}

/**
 * 一台机器的常驻流（`ccm -- --resident-attach`）＋ 上面折出来的会话表，断了自己接回去。手机对这台的一问一答都经 [call]。
 *
 * 接回去只走一条路：[open]（probe → ensure → attach，带 `--tail-only`）。什么时候走：
 * - [ssh] 换了一条（SSH 那层重连过）⇒ 旧流关掉、当场在新连接上接；[ssh] 是 `null`（SSH 没通）⇒ 不接、停在断开。
 * - SSH 还通、流断了 ⇒ 叫 [onLost]（让 SSH 那层去探活：多半是 SSH 先死了），再按 [Backoff] 接。
 * - 走不通、要人动手的那几种（另一版 · 没装 · 起不了常驻）不自己反复试：等 [reconnect]（［重试］）或 [onForeground]。
 *
 * 每次接上，那台整份重报会话（`session_added` … `sessions_replayed`），会话表从空表重新折，不留旧的、不重一份。
 * SSH 连接本身不归这里：由别处（`HostConnector`）建好、持有，这里只在它上面开那条双向 exec。
 */
class BackendFeed(
    private val scope: CoroutineScope,
    /** SSH 那层此刻的连接（`null` ＝ 没通）；值换了（按身份比）＝ 换了一条。 */
    private val ssh: Flow<Any?>,
    /** 走一遍 probe → ensure → attach；给的 scope 是这一条流活多久。 */
    private val open: suspend (CoroutineScope) -> LinkOutcome,
    private val onLost: () -> Unit = {},
    private val onBreak: (String) -> Unit = {},
    private val backoff: Backoff = Backoff(),
) {
    private val stateFlow = MutableStateFlow<LinkState>(LinkState.Connecting)
    val state: StateFlow<LinkState> = stateFlow.asStateFlow()

    private val tableFlow = MutableStateFlow(SessionTable())

    /** 流上的会话帧折出来的会话表（此刻有哪些会话、各自的状态字与语气）。 */
    val table: StateFlow<SessionTable> = tableFlow.asStateFlow()

    private val changeFlow = MutableSharedFlow<String>(extraBufferCapacity = BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    /** 每来一帧发它的 `kind`：出口据此重问（`session_added` ⇒ 重问 `history-list` …）。 */
    val changes: SharedFlow<String> = changeFlow.asSharedFlow()

    private val turnEndFlow = MutableSharedFlow<TurnEnd>(extraBufferCapacity = BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    /** 流上的 `turn_end` 帧（逐帧，未折；折成一轮一条在 [TurnEnds]）。 */
    val turnEnds: SharedFlow<TurnEnd> = turnEndFlow.asSharedFlow()

    @Volatile private var client: FrameClient? = null

    @Volatile private var supervisor: Job? = null

    /** ［重试］/ 回前台：别等退避，马上再接一次。 */
    private val kick = Channel<Unit>(Channel.CONFLATED)

    /** 开始看着这台（只起一次；之后断了自己接）。 */
    fun ensure() {
        if (supervisor?.isActive == true) return
        supervisor = scope.launch { ssh.distinctUntilChanged { a, b -> a === b }.collectLatest { conn -> follow(conn) } }
    }

    /** ［重试］：断开这一条（有的话）、马上整份重来。 */
    fun reconnect() {
        ensure()
        client?.close()
        kick.trySend(Unit)
    }

    /** App 回到前台：通着不动；断着马上再接（手机会被杀后台，不是常驻）。 */
    fun onForeground() {
        if (stateFlow.value is LinkState.Down) reconnect() else ensure()
    }

    /** 一问一答。流没通 ⇒ [Reply.LinkDown]。 */
    suspend fun call(
        cmd: String,
        args: Map<String, Any?>? = null,
        withinMs: Long = DEFAULT_CALL_MS,
    ): Reply = client?.call(cmd, args, withinMs) ?: Reply.LinkDown

    /** 跟着 SSH 的这一条连接：没通 ⇒ 停在断开；通着 ⇒ 接、断了再接，直到它换一条（被 `collectLatest` 取消）。 */
    private suspend fun follow(conn: Any?) {
        if (conn == null) {
            stateFlow.value = LinkState.Down(null)
            return
        }
        kick.tryReceive()
        var attempt = 0
        while (true) {
            val run = connectAndPump()
            if (run.wasUp) {
                attempt = 0
                onLost()
            }
            if (run.retryable) {
                withTimeoutOrNull(backoff.delayMs(attempt++)) { kick.receive() }
            } else {
                kick.receive()
            }
        }
    }

    private class Run(
        val wasUp: Boolean,
        val retryable: Boolean,
    )

    private suspend fun connectAndPump(): Run {
        stateFlow.value = LinkState.Connecting
        tableFlow.value = SessionTable()
        val runScope = CoroutineScope(scope.coroutineContext + Job(scope.coroutineContext[Job]))
        try {
            return when (val got = open(runScope)) {
                is LinkOutcome.Up -> pump(got.client)
                else -> {
                    stateFlow.value = LinkState.Down(got)
                    Run(wasUp = false, retryable = retryable(got))
                }
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: IOException) {
            stateFlow.value = LinkState.Down(null, e.message ?: e.javaClass.simpleName)
            return Run(wasUp = false, retryable = true)
        } finally {
            client?.close()
            client = null
            runScope.cancel()
        }
    }

    private suspend fun pump(c: FrameClient): Run {
        client = c
        stateFlow.value = LinkState.Up
        for (frame in c.frames) {
            val next = tableFlow.value.step(frame, onBreak)
            tableFlow.value = next
            frame.str("kind")?.let(changeFlow::tryEmit)
            TurnEnd.of(frame)?.let(turnEndFlow::tryEmit)
            if (next.needsResync) {
                // 丢了会话帧：这张表不可信了 ⇒ 重接，那台整份重报。
                kick.trySend(Unit)
                break
            }
        }
        stateFlow.value = LinkState.Down(null)
        return Run(wasUp = true, retryable = true)
    }

    /** 断在路上的（接流时被掐 · 小中继连不上常驻）再试会好；其余要人动手。 */
    private fun retryable(o: LinkOutcome): Boolean =
        o is LinkOutcome.Attach && (o.outcome is AttachOutcome.Cut || o.outcome is AttachOutcome.Refused)

    /** 流断了、SSH 还通时多久再接：从 [firstMs] 起翻倍，封顶 [capMs]。 */
    class Backoff(
        private val firstMs: Long = 1_000L,
        private val capMs: Long = 30_000L,
    ) {
        fun delayMs(attempt: Int): Long = (firstMs shl attempt.coerceIn(0, MAX_SHIFT)).coerceAtMost(capMs)

        private companion object {
            const val MAX_SHIFT = 20
        }
    }

    private companion object {
        const val BUFFER = 64
        const val DEFAULT_CALL_MS = 15_000L
    }
}

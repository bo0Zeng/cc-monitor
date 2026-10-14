package com.ccmonitor.mobile.core.claude.link

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean

/**
 * 嵌的 Rust 核心里那一台常驻流的几个口（`Core.resident*`，手写 JNI、走消息）。生产上直通 JNI；JVM 单测给假的。
 * 交回来的事经 [open] 时给的 `events` 回（`state` · `frame` · `watch_from` · `lost` · `reply`）。
 */
interface ResidentCore {
    fun open(
        machine: String,
        mine: String,
        tz: String,
        events: (kind: String, body: String) -> Unit,
    ): Long

    fun ssh(
        handle: Long,
        request: String?,
    )

    fun call(
        handle: Long,
        cmd: String,
        args: String,
        withinMs: Long,
    ): Long

    fun watch(
        handle: Long,
        sid: String,
        holding: Boolean,
    )

    fun kick(
        handle: Long,
        retry: Boolean,
    )
}

/** SSH 那层的一条连接：它的拨号请求（JSON，不带用法）。按身份比 —— 重连过就是新的一个，哪怕请求一字不差。 */
class SshLink(
    val request: String,
)

/**
 * 一台机器的常驻流：接 · 断了再接 · 握手 · 版本门 · 在看名单 · 一问一答**整条在核心里**（`resident-client`，桌面同一份）。
 * 这里只把核心交回来的事落成手机这一侧的几条流：会话表（流上的会话帧折出来）· 记录行 · 轮次结束 · 每帧的 `kind` · 此刻通不通。
 *
 * - [ssh] 是 SSH 那层此刻的那一条（`null` ＝ 没通）；换了一条就把它的拨号请求交给核心（它在新的上重接）。
 * - 核心说流断了、SSH 还在（`lost`）⇒ 叫 [onLost]（让 SSH 那层去探活：多半是 SSH 先死了）。
 * - 丢了会话帧（[SessionTable.needsResync]）⇒ 叫核心整份重来。
 * - 退避的数（2 → 30 s，活过 30 s 才清零）只在核心里，这里一个都不写。
 */
class ResidentFeed(
    private val scope: CoroutineScope,
    private val core: ResidentCore,
    /** 这台在屏上的名字（核心的句子里称呼它）。 */
    private val machine: suspend () -> String,
    private val ssh: Flow<SshLink?>,
    /** 手上这一版的身份（版本门的对照物）。 */
    private val mine: String,
    /** 看的这一台的时区（IANA 名）。 */
    private val tz: String,
    private val onLost: () -> Unit = {},
    private val onBreak: (String) -> Unit = {},
) : CoreLink,
    SessionWatch {
    private val stateFlow = MutableStateFlow<LinkState>(LinkState.Connecting)
    val state: StateFlow<LinkState> = stateFlow.asStateFlow()

    private val tableFlow = MutableStateFlow(SessionTable())
    override val table: StateFlow<SessionTable> = tableFlow.asStateFlow()

    private val epochFlow = MutableStateFlow(0L)
    override val epoch: StateFlow<Long> = epochFlow.asStateFlow()

    private val changeFlow = MutableSharedFlow<String>(extraBufferCapacity = BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    /** 每来一帧发它的 `kind`：出口据此重问（`session_added` ⇒ 重问 `history-list` …）。 */
    val changes: SharedFlow<String> = changeFlow.asSharedFlow()

    private val turnEndFlow = MutableSharedFlow<TurnEnd>(extraBufferCapacity = BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    /** 流上的 `turn_end` 帧（逐帧，未折；折成一轮一条在 [TurnEnds]）。 */
    val turnEnds: SharedFlow<TurnEnd> = turnEndFlow.asSharedFlow()

    private val lineFlow = MutableSharedFlow<RecordLine>(extraBufferCapacity = LINE_BUFFER)

    /** 流上的 `line` 帧。满了 ⇒ 这里停收（核心那一侧跟着停读、SSH 窗口停发），不丢。 */
    override val lines: SharedFlow<RecordLine> = lineFlow.asSharedFlow()

    private val liveFlow = MutableSharedFlow<LiveFrame>(extraBufferCapacity = LINE_BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)
    override val live: SharedFlow<LiveFrame> = liveFlow.asSharedFlow()

    private val fromFlow = MutableSharedFlow<WatchFrom>(extraBufferCapacity = BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)
    override val from: SharedFlow<WatchFrom> = fromFlow.asSharedFlow()

    private val started = AtomicBoolean(false)
    private val opened = CompletableDeferred<Long>()
    private val replies = ConcurrentHashMap<Long, CompletableDeferred<Reply>>()

    /** 核心交回来的事按到达顺序在这里排队、在 [scope] 里一件一件落（核心的工作线程上不等任何人）。 */
    private val inbox = Channel<Pair<String, String>>(Channel.UNLIMITED)

    /** 开始看着这台（只起一次；之后断了核心自己接）。 */
    fun ensure() {
        if (!started.compareAndSet(false, true)) return
        scope.launch { for ((kind, body) in inbox) onEvent(kind, body) }
        scope.launch {
            val h = core.open(machine(), mine, tz) { kind, body -> inbox.trySend(kind to body) }
            opened.complete(h)
            // 按身份比：SSH 那层重连过（网络换了 · 探活换上新的一条）⇒ 新的一个，核心当场在它上面重接，不等旧流自己断。
            ssh.distinctUntilChanged { a, b -> a === b }.collect { core.ssh(h, it?.request) }
        }
    }

    /** ［重试］：断开这一条（有的话）、马上整份重来。 */
    fun reconnect() {
        ensure()
        scope.launch { core.kick(opened.await(), true) }
    }

    /** App 回到前台 · 网络变了：通着不动；断着马上再接。 */
    fun onForeground() {
        ensure()
        scope.launch { core.kick(opened.await(), false) }
    }

    override suspend fun call(
        cmd: String,
        args: Map<String, Any?>?,
        withinMs: Long,
    ): Reply {
        ensure()
        val h = withTimeoutOrNull(withinMs) { opened.await() } ?: return Reply.LinkDown
        val answer = CompletableDeferred<Reply>()
        // 编号由核心回，结局也由核心交（结局可能先于这里拿到编号 ⇒ 谁先到谁占那一格）。核心到期限必回一句；兜底多等一会儿。
        val id = core.call(h, cmd, Json.line(args ?: emptyMap<String, Any?>()), withinMs)
        if (id == 0L) return Reply.LinkDown
        val slot = replies.computeIfAbsent(id) { answer }
        return (withTimeoutOrNull(withinMs + REPLY_GRACE_MS) { slot.await() } ?: Reply.LinkDown).also { replies.remove(id) }
    }

    override fun hold(sid: String): AutoCloseable {
        ensure()
        scope.launch { core.watch(opened.await(), sid, true) }
        val closed = AtomicBoolean(false)
        return AutoCloseable {
            if (closed.compareAndSet(false, true)) scope.launch { core.watch(opened.await(), sid, false) }
        }
    }

    /** 两端契约对不上（核心交的那一行认不出）：经 [onBreak] 只进日志，一种报一次由调用方管。 */
    private fun broke(
        kind: String,
        body: String,
    ) = onBreak("核心交的 $kind 认不出：${body.take(BREAK_SNIPPET)}")

    private suspend fun onEvent(
        kind: String,
        body: String,
    ) {
        when (kind) {
            "state" -> Json.obj(body)?.let(::onState) ?: broke(kind, body)
            "frame" -> Json.obj(body)?.let { onFrame(it) } ?: broke(kind, body)
            "watch_from" -> watchFromOf(body)?.forEach { fromFlow.tryEmit(it) } ?: broke(kind, body)
            "lost" -> onLost()
            "reply" -> onReply(body)
            else -> broke(kind, body)
        }
    }

    private fun onState(m: Map<String, Any?>) {
        val said = { key: String -> m.obj(key)?.let { Said(it.str("text").orEmpty(), it.str("detail").orEmpty()) } }
        when (m.str("state")) {
            "connecting" -> {
                tableFlow.value = SessionTable()
                stateFlow.value = LinkState.Connecting
            }
            "up" -> {
                val e = m.num("epoch") ?: 0L
                if (e != epochFlow.value) epochFlow.value = e
                stateFlow.value = LinkState.Up(said("notice"))
            }
            "down" -> stateFlow.value = LinkState.Down(said("problem") ?: Said("", ""), m.bool("needs_hand") == true)
            else -> broke("state", m.toString())
        }
    }

    private suspend fun onFrame(frame: Map<String, Any?>) {
        val next = tableFlow.value.step(frame, onBreak)
        tableFlow.value = next
        frame.str("kind")?.let(changeFlow::tryEmit)
        TurnEnd.of(frame)?.let(turnEndFlow::tryEmit)
        if (frame.str("kind") == LiveFrame.KIND) LiveFrame.of(frame)?.let(liveFlow::tryEmit) ?: broke("frame", frame.toString())
        // 满了就在这里等（停收），不丢行。
        RecordLine.of(frame)?.let { lineFlow.emit(it) }
        if (next.needsResync) reconnect()
    }

    private fun onReply(body: String) {
        val m = Json.obj(body) ?: return broke("reply", body)
        val id = m.num("call") ?: return broke("reply", body)
        val reply =
            when {
                m.bool("ok") == true -> Reply.Ok(m["data"])
                m.bool("ok") == false ->
                    m.str("code")?.let { code -> Reply.Failed(CoreFailure(code, m.str("message").orEmpty(), m.str("detail").orEmpty(), m["data"])) }
                        ?: Reply.Unreadable(body)
                m.bool("timeout") == true -> Reply.TimedOut
                m.bool("cancelled") == true -> Reply.Cancelled
                m.bool("down") == true -> Reply.LinkDown
                // 核心按 `cmd-wire` 那一份类型解回包没解开（形状对不上）：原样带回，详情里是 serde 那一句。
                m.bool("unreadable") == true -> Reply.Unreadable(body)
                else -> Reply.Unreadable(body)
            }
        replies.computeIfAbsent(id) { CompletableDeferred() }.complete(reply)
    }

    private companion object {
        const val BUFFER = 64
        const val LINE_BUFFER = 256
        const val BREAK_SNIPPET = 200

        /** 核心到期限必回一句；它没回（不该有）时这边多等这么久就当断了。 */
        const val REPLY_GRACE_MS = 5_000L

        /** `watch_from` 那一段：`[{sid, path, seq}]`。 */
        fun watchFromOf(body: String): List<WatchFrom>? =
            Json.obj("""{"from":$body}""")?.let { m ->
                decoding { m.objs("from").need().map { WatchFrom(it.str("sid").need(), it.str("path").need(), it.num("seq").need()) } }
            }
    }
}

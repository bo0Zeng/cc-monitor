package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.MachineState
import com.ccmonitor.mobile.core.claude.link.wire.Wire
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
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
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean

/**
 * 嵌的 Rust 核心里那一台常驻流的几个口（`Core.resident*`，手写 JNI、走消息）。生产上直通 JNI；JVM 单测给假的。
 * 交回来的事经 [open] 时给的 `events` 回（`state` · `frame` · `unread` · `watch_from` · `ssh` · `reply` · `moved` · `deploy` · `batch` · `batch_keep`）。
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

    /** 分块传那一半（文件描述符交给核心）：回问答编号，走到哪经 `moved`、结局经 `reply`；这台不在 ⇒ 0。 */
    fun stage(
        handle: Long,
        fd: Int,
        name: String,
        withinMs: Long,
    ): Long

    /** 下载那一半（[args] 是 `{path}`）：同 [stage]。 */
    fun pull(
        handle: Long,
        fd: Int,
        args: String,
        shown: String,
        withinMs: Long,
    ): Long

    /** 撤那一趟（[stage] · [pull] 回的编号）：本机那一份当场不再读写。 */
    fun transferStop(id: Long)

    /** 起一组（[want] 是 JSON；`{kept}` ⇒ 接回落盘那一份）：回编号，事经 `batch` 那一种交回、落盘那一份经 `batch_keep`；这台不在 · 读不动 ⇒ 0。 */
    fun batchStart(
        handle: Long,
        want: String,
    ): Long

    fun batchAnswer(
        id: Long,
        answer: String?,
    )

    fun batchStop(id: Long)

    /** 文件页那一问（`files-ls` 的原行，JSON 数组）⇒ 核心那一行（[PickMenu.of]）；[handle] 还没开 ⇒ 0（当断着）。 */
    fun pickMenu(
        handle: Long,
        rows: String,
    ): String

    /** 同名清单（JSON）⇒ 抽屉那几行（JSON 数组）。 */
    fun clashRows(data: String): String

    /** 下载一组那几格（JSON）⇒ `batch` 那一形的一行（[BatchEvent.of]）；读不动 ⇒ 空串。 */
    fun downloadEvent(want: String): String

    /** 手机这一侧问的同名那一答（JSON；`null` ＝ 取消）⇒ [keys]（JSON 数组）每个键一格（JSON）。 */
    fun answerOf(
        keys: String,
        answer: String?,
    ): String

    /** 给那台装 / 更新后端（[slots] = 包里那几格解到的文件，JSON）；事经 `deploy` 那一种交回。那台不在 · 正在装 ⇒ `false`。 */
    fun deploy(
        handle: Long,
        slots: String,
    ): Boolean
}

/**
 * 一台机器的常驻流：接 · 断了再接 · 握手 · 版本门 · 在看名单 · 一问一答**整条在核心里**（`resident-client`，桌面同一份）。
 * 这里只把核心交回来的事落成手机这一侧的几条流：会话表（流上的会话帧折出来）· 记录行 · 每帧的 `kind` · 此刻通不通。
 *
 * - [ssh] 是这台的拨号请求（JSON，不带用法；`null` ＝ 没连上过 · 已断开）：按值比，变了才交给核心（它照新的重拨）。
 *   这条流就是手机上唯一那条重连循环：池里那条死了它在核心里自己拨一条新的，SSH 那层跟着它的 [state] 换连接。
 * - 退避的数（2 → 30 s，活过 30 s 才清零）只在核心里，这里一个都不写。
 */
class ResidentFeed(
    private val scope: CoroutineScope,
    private val core: ResidentCore,
    /** 这台在屏上的名字（核心的句子里称呼它）。 */
    private val machine: suspend () -> String,
    private val ssh: Flow<String?>,
    /** 手上这一版的身份（版本门的对照物）。 */
    private val mine: String,
    /** 看的这一台的时区（IANA 名）。 */
    private val tz: String,
    /** 这一台在做的几组落盘的那一份（核心交 `batch_keep` 时写 · 删；开起来交回核心再问）。 */
    private val kept: KeptBatches,
    private val onBreak: (String) -> Unit = {},
) : CoreLink,
    SessionWatch {
    private val stateFlow = MutableStateFlow<LinkState>(LinkState.Connecting)
    override val state: StateFlow<LinkState> = stateFlow.asStateFlow()

    private val sshFlow = MutableStateFlow(false)

    /** SSH 够没够着那一台（拨通 · 认证过），与 [state] 分开：那台常驻后端起不来时照样是 `true`。SSH 那层（终端 · 文件）跟它走。 */
    val sshReached: StateFlow<Boolean> = sshFlow.asStateFlow()

    private val tableFlow = MutableStateFlow(SessionTable())
    override val table: StateFlow<SessionTable> = tableFlow.asStateFlow()

    private val epochFlow = MutableStateFlow(0L)
    override val epoch: StateFlow<Long> = epochFlow.asStateFlow()

    private val changeFlow = MutableSharedFlow<String>(extraBufferCapacity = BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    /** 每来一帧发它的 `kind`：出口据此重问（`session_added` ⇒ 重问 `history-list` …）。 */
    val changes: SharedFlow<String> = changeFlow.asSharedFlow()

    private val changedFlow = MutableSharedFlow<Changed>(extraBufferCapacity = BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    /** 流上的 `changed` 帧（哪一样变了 · 哪一个）：出口照它重读那一份。 */
    val changed: SharedFlow<Changed> = changedFlow.asSharedFlow()

    private val lineFlow = MutableSharedFlow<RecordLine>(extraBufferCapacity = LINE_BUFFER)

    /** 流上的 `line` 帧。满了 ⇒ 这里停收（核心那一侧跟着停读、SSH 窗口停发），不丢。 */
    override val lines: SharedFlow<RecordLine> = lineFlow.asSharedFlow()

    private val liveFlow = MutableSharedFlow<LiveFrame>(extraBufferCapacity = LINE_BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)
    override val live: SharedFlow<LiveFrame> = liveFlow.asSharedFlow()

    private val fromFlow = MutableSharedFlow<WatchFrom>(extraBufferCapacity = BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)
    override val from: SharedFlow<WatchFrom> = fromFlow.asSharedFlow()

    private val deployFlow = MutableStateFlow<Deploying?>(null)

    /** 装 / 更新后端这一趟（没装过 ⇒ `null`）：每一步 · 结局 · 失败，原样是核心交回的那一句。 */
    val deploying: StateFlow<Deploying?> = deployFlow.asStateFlow()

    /** SSH 那层此刻有没有交给核心（装后端要在那条连接上做：交过了再叫核心）。 */
    private val sshGiven = MutableStateFlow(false)

    private val started = AtomicBoolean(false)
    private val opened = CompletableDeferred<Long>()
    private val replies = ConcurrentHashMap<Long, CompletableDeferred<Reply>>()

    /** 在走的分块传：问答编号 ⇒ 报走到哪的那一口。 */
    private val moving = ConcurrentHashMap<Long, (Long, Long) -> Unit>()

    /** 核心交回来的事按到达顺序在这里排队、在 [scope] 里一件一件落（核心的工作线程上不等任何人）。 */
    private val inbox = Channel<Pair<String, String>>(Channel.UNLIMITED)

    /** 开始看着这台（只起一次；之后断了核心自己接）。 */
    fun ensure() {
        if (!started.compareAndSet(false, true)) return
        scope.launch { for ((kind, body) in inbox) onEvent(kind, body) }
        scope.launch {
            val h = core.open(machine(), mine, tz) { kind, body -> inbox.trySend(kind to body) }
            opened.complete(h)
            // 落过盘的那几组交回核心、照同一张票再问；核心读不动的那一份不留。
            for ((ticket, plan) in kept.all()) {
                if (core.batchStart(h, Json.args(mapOf("kept" to plan))) == 0L) kept.drop(ticket)
            }
            // 按值比：SSH 那层在同一份身份上换一条（跟着这条流换的那种）请求一字不差 ⇒ 不交，流不被打断。
            ssh.distinctUntilChanged().collect {
                core.ssh(h, it)
                sshGiven.value = it != null
            }
        }
    }

    /** ［重试］：断开这一条（有的话）、马上整份重来。 */
    fun reconnect() {
        ensure()
        scope.launch { core.kick(opened.await(), true) }
    }

    /** App 回到前台 · 网络回来了 / 变了：通着 ⇒ 核心当场问一句活（没回话就判死重接）；断着 ⇒ 马上再接（卡在一拨没人回话的拨号里也丢下重拨）。 */
    fun wake() {
        ensure()
        scope.launch { core.kick(opened.await(), false) }
    }

    /** 给这台装 / 更新后端：[slots] 交出包里那几格解到的文件（解压可能要一会儿，在调用方的线程上做）。 */
    fun deploy(slots: suspend () -> String) {
        ensure()
        scope.launch {
            val h = opened.await()
            // SSH 那层还没交到核心 ⇒ 等一会儿（等不到也照叫：核心照实说离线）。
            withTimeoutOrNull(SSH_WAIT_MS) { sshGiven.first { it } }
            core.deploy(h, slots())
        }
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
        val id = core.call(h, cmd, Json.args(args ?: emptyMap()), withinMs)
        if (id == 0L) return Reply.LinkDown
        val slot = replies.computeIfAbsent(id) { answer }
        return (withTimeoutOrNull(withinMs + REPLY_GRACE_MS) { slot.await() } ?: Reply.LinkDown).also { replies.remove(id) }
    }

    /** 结局由核心交：每一块都有期限，最后必回一句 ⇒ 这里不另设整趟的期限（大文件要多久就多久）。 */
    override suspend fun transfer(
        how: Transfer,
        fd: () -> Int,
        withinMs: Long,
        moved: (Long, Long) -> Unit,
    ): Reply {
        ensure()
        val h = withTimeoutOrNull(withinMs) { opened.await() } ?: return Reply.LinkDown
        val id =
            when (how) {
                is Transfer.Stage -> core.stage(h, fd(), how.name, withinMs)
                is Transfer.Pull -> core.pull(h, fd(), Json.args(mapOf("path" to how.path.wire)), how.shown, withinMs)
            }
        if (id == 0L) return Reply.LinkDown
        moving[id] = moved
        val slot = replies.computeIfAbsent(id) { CompletableDeferred() }
        return try {
            slot.await()
        } catch (ce: CancellationException) {
            core.transferStop(id)
            throw ce
        } finally {
            replies -= id
            moving -= id
        }
    }

    private val batchFlow = MutableSharedFlow<BatchEvent>(extraBufferCapacity = BUFFER)
    override val batches: SharedFlow<BatchEvent> = batchFlow.asSharedFlow()

    /** 接回来的那几组各自最后那一格（[resumed]）。 */
    private val resumedLast = LinkedHashMap<Long, BatchEvent>()

    override fun resumed(): List<BatchEvent> =
        synchronized(resumedLast) { resumedLast.values.toList().also { resumedLast.values.removeAll(BatchEvent::over) } }

    override suspend fun batch(want: Map<String, Any?>): Long {
        ensure()
        val h = withTimeoutOrNull(DEFAULT_CALL_MS) { opened.await() } ?: return 0L
        return core.batchStart(h, Json.args(want))
    }

    override fun batchAnswer(
        id: Long,
        answer: BatchAnswer?,
    ) = core.batchAnswer(id, BatchAnswer.wire(answer)?.let(Json::args))

    override fun batchStop(id: Long) = core.batchStop(id)

    @OptIn(ExperimentalCoroutinesApi::class)
    override fun picks(rows: List<Any?>): PickMenu {
        ensure()
        val h = if (opened.isCompleted) opened.getCompleted() else 0L
        return PickMenu.of(core.pickMenu(h, Json.line(rows)))
    }

    override fun clashes(data: Any?): List<BatchEvent.Clash> =
        (Json.value(core.clashRows(Json.line(data))) as? List<*>).orEmpty().mapNotNull { m ->
            @Suppress("UNCHECKED_CAST")
            (m as? Map<String, Any?>)?.let(BatchEvent.Clash::of)
        }

    @Suppress("UNCHECKED_CAST")
    override fun decided(
        keys: List<String>,
        answer: BatchAnswer?,
    ): Map<String, String>? = Json.value(core.answerOf(Json.line(keys), BatchAnswer.wire(answer)?.let(Json::args))) as? Map<String, String>

    override fun downloadEvent(want: Map<String, Any?>): BatchEvent? {
        @Suppress("UNCHECKED_CAST")
        return (Json.value(core.downloadEvent(Json.args(want))) as? Map<String, Any?>)?.let(BatchEvent::of)
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
            "frame" -> onFrameLine(body)
            // 核心按 `cmd-wire` 那一份类型解不开的一帧（两端版本对不上），原样那一行。
            "unread" -> broke("frame", body)
            "watch_from" -> onWatchFrom(body)
            "ssh" -> sshFlow.value = body == "true"
            "reply" -> onReply(body)
            "moved" -> onMoved(body)
            "deploy" -> onDeployLine(body)
            "batch" -> onBatchLine(body)
            "batch_keep" -> onKeepLine(body)
            else -> broke(kind, body)
        }
    }

    /** `state` 那一行：通没通（`state` · `epoch`）＋ 那台的状态成品（`machine`，按线上类型装）。 */
    private fun onState(m: Map<String, Any?>) {
        val machine = Wire.bind<MachineState>(m["machine"]) ?: return broke("state", m.toString())
        val said = Said(machine.said, machine.detail.orEmpty())
        when (m.str("state")) {
            "connecting" -> {
                tableFlow.value = SessionTable()
                stateFlow.value = LinkState.Connecting
            }
            "up" -> {
                val e = m.num("epoch") ?: 0L
                if (e != epochFlow.value) epochFlow.value = e
                stateFlow.value = LinkState.Up(said.takeIf { it.text.isNotEmpty() }, machine.fixes)
            }
            "down" -> stateFlow.value = LinkState.Down(said, machine.fixes)
            else -> broke("state", m.toString())
        }
        // 「装好了」那一句只留到下一次出事（断了 · 又说要更新）；没装上那一句留着，等下一趟。
        val now = stateFlow.value
        if (deployFlow.value is Deploying.Done && (now is LinkState.Down || (now as? LinkState.Up)?.notice != null)) deployFlow.value = null
    }

    private suspend fun onFrameLine(body: String) {
        Json.obj(body)?.let { onFrame(it) } ?: broke("frame", body)
    }

    private suspend fun onFrame(frame: Map<String, Any?>) {
        val next = tableFlow.value.step(frame, onBreak)
        tableFlow.value = next
        frame.str("kind")?.let(changeFlow::tryEmit)
        Changed.of(frame)?.let(changedFlow::tryEmit)
        if (frame.str("kind") == LiveFrame.KIND) LiveFrame.of(frame)?.let(liveFlow::tryEmit) ?: broke("frame", frame.toString())
        // 满了就在这里等（停收），不丢行。
        RecordLine.of(frame)?.let { lineFlow.emit(it) }
    }

    /** `batch` 那一行落进 [batches]（接回来的那几组另记最后那一格：收场时有人在看 ⇒ 算交过）；认不出 ⇒ 报一次契约对不上。 */
    private suspend fun onBatchLine(body: String) {
        val e = Json.obj(body)?.let(BatchEvent::of) ?: return broke("batch", body)
        if (e.kept) {
            synchronized(resumedLast) {
                if (e.over && batchFlow.subscriptionCount.value > 0) resumedLast.remove(e.id) else resumedLast[e.id] = e
            }
        }
        batchFlow.emit(e)
    }

    /** `batch_keep {ticket, plan}`：那一串原样落盘；`plan` 是 `null` ⇒ 删。 */
    private suspend fun onKeepLine(body: String) {
        val m = Json.obj(body) ?: return broke("batch_keep", body)
        val ticket = m.str("ticket") ?: return broke("batch_keep", body)
        when (val plan = m.str("plan")) {
            null -> kept.drop(ticket)
            else -> kept.put(ticket, plan)
        }
    }

    /** `deploy` 那一行落进 [deploying]；认不出 ⇒ 报一次契约对不上、不动。 */
    private fun onDeployLine(body: String) {
        Json.obj(body)?.let(::deployOf)?.let { deployFlow.value = it } ?: broke("deploy", body)
    }

    /** `deploy` 那一行 ⇒ 一步 · 结局 · 失败；认不出 ⇒ `null`。 */
    private fun deployOf(m: Map<String, Any?>): Deploying? {
        val text = m.str("text") ?: return null
        return when {
            m.str("step") != null -> Deploying.Step(text, m.bool("bar") == true)
            m.str("done") != null -> Deploying.Done(text)
            m.str("failed") != null -> Deploying.Failed(Said(text, m.str("detail").orEmpty()))
            else -> null
        }
    }

    /** `moved {call, done, total}`：交给在等那一趟的那一口（总数不知道 ⇒ `-1`）。 */
    private fun onWatchFrom(body: String) {
        watchFromOf(body)?.forEach { fromFlow.tryEmit(it) } ?: broke("watch_from", body)
    }

    private fun onMoved(body: String) {
        val m = Json.obj(body) ?: return broke("moved", body)
        val id = m.num("call") ?: return broke("moved", body)
        moving[id]?.invoke(m.num("done") ?: 0L, m.num("total") ?: -1L)
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

        /** 装后端之前等 SSH 那层交到核心，最多等这么久。 */
        const val SSH_WAIT_MS = 15_000L

        /** `watch_from` 那一段：`[{sid, path, seq}]`。 */
        fun watchFromOf(body: String): List<WatchFrom>? =
            Json.obj("""{"from":$body}""")?.let { m ->
                decoding { m.objs("from").need().map { WatchFrom(it.str("sid").need(), it.str("path").need(), it.num("seq").need()) } }
            }
    }
}

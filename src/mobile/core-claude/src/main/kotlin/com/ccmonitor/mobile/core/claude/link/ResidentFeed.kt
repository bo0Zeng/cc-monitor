package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.BatchAnswer
import com.ccmonitor.mobile.core.wire.gen.BatchKeep
import com.ccmonitor.mobile.core.wire.gen.BatchLine
import com.ccmonitor.mobile.core.wire.gen.CallEnd
import com.ccmonitor.mobile.core.wire.gen.Changed
import com.ccmonitor.mobile.core.wire.gen.ClashRow
import com.ccmonitor.mobile.core.wire.gen.Decide
import com.ccmonitor.mobile.core.wire.gen.DeployEnd
import com.ccmonitor.mobile.core.wire.gen.EmptyFace
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.LiveCards
import com.ccmonitor.mobile.core.wire.gen.LocalAsk
import com.ccmonitor.mobile.core.wire.gen.LocalOpened
import com.ccmonitor.mobile.core.wire.gen.Moved
import com.ccmonitor.mobile.core.wire.gen.PhoneMenu
import com.ccmonitor.mobile.core.wire.gen.SessionBook
import com.ccmonitor.mobile.core.wire.gen.arr
import com.ccmonitor.mobile.core.wire.gen.dict
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
import java.io.IOException
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean

/**
 * 嵌的 Rust 核心里那一台常驻流的几个口（`Core.resident*`，手写 JNI、走消息）。生产上直通 JNI；JVM 单测给假的。
 * 交回来的事经 [open] 时给的 `events` 回（`state` · `empty` · `book` · `live` · `frame` · `unread` · `watch_from` · `ssh` · `reply` · `moved` · `deploy` · `batch` · `batch_keep` · `batch_local` · `busy`）：
 * 每一样的形状是 `cmd-wire` 的 `phone` 那一份（会话簿 · 活卡是端核心 `resident-client` 那一份）—— 生成的类照它解，缺一格就是两端契约对不上，不补默认值。
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

    /** 撤那一趟（[stage] 回的编号）：本机那一份当场不再读。 */
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

    /** 文件页那一问（`files-ls` 的原行，JSON 数组）⇒ 核心那一行（[PhoneMenu]）；[handle] 还没开 ⇒ 0（当断着）。 */
    fun pickMenu(
        handle: Long,
        rows: String,
    ): String

    /** 同名清单（JSON）⇒ 抽屉那几行（[ClashRow] 的 JSON 数组）。 */
    fun clashRows(data: String): String

    /** 几台搜内容的结果（`[{origin, reply}]`，JSON）⇒ 合好的那一份（[SearchMerge.of]）；读不动 ⇒ 空串。 */
    fun searchMerge(data: String): String

    /** 手机这一侧问的同名那一答（JSON；`null` ＝ 取消）⇒ [keys]（JSON 数组）每个键一格（键 ⇒ [Decide] 的 JSON）。 */
    fun answerOf(
        keys: String,
        answer: String?,
    ): String

    /** 给那台装 / 更新后端（[slots] = 包里那几格解到的文件，JSON）；结局经 `deploy` 那一种交回，走到哪一步在 `state` 那一行的成品里。那台不在 · 正在装 ⇒ `false`。 */
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
    /** 下到本机那一侧（核心经 `batch_local` 要本机那一份时在这里开 · 删）。 */
    private val saves: LocalSaves,
    private val onBreak: (String) -> Unit = {},
    /** 核心跟着 `state` 交的首屏那一句（手上看着的每台合的一份，不只这一台；最新那一行算数）。 */
    private val onEmpty: (EmptyFace) -> Unit = {},
) : CoreLink,
    SessionWatch {
    private val stateFlow = MutableStateFlow<LinkLine?>(null)
    override val state: StateFlow<LinkLine?> = stateFlow.asStateFlow()
    private val offlineFlow = MutableStateFlow<String?>(null)
    override val offline: StateFlow<String?> = offlineFlow.asStateFlow()

    private val sshFlow = MutableStateFlow(false)

    /** SSH 够没够着那一台（拨通 · 认证过），与 [state] 分开：那台常驻后端起不来时照样是 `true`。SSH 那层（终端 · 文件）跟它走。 */
    val sshReached: StateFlow<Boolean> = sshFlow.asStateFlow()

    private val tableFlow = MutableStateFlow(EMPTY_BOOK)
    override val table: StateFlow<SessionBook> = tableFlow.asStateFlow()

    private val epochFlow = MutableStateFlow(0L)
    override val epoch: StateFlow<Long> = epochFlow.asStateFlow()

    private val changedFlow = MutableSharedFlow<Changed>(extraBufferCapacity = BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    /** 流上的 `changed` 帧（哪一样变了 · 哪一个）：出口照它重读那一份。 */
    val changed: SharedFlow<Changed> = changedFlow.asSharedFlow()

    private val lineFlow = MutableSharedFlow<RecordLine>(extraBufferCapacity = LINE_BUFFER)

    /** 流上的 `line` 帧。满了 ⇒ 这里停收（核心那一侧跟着停读、SSH 窗口停发），不丢。 */
    override val lines: SharedFlow<RecordLine> = lineFlow.asSharedFlow()

    private val liveFlow = MutableSharedFlow<LiveCards>(extraBufferCapacity = LINE_BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)
    override val live: SharedFlow<LiveCards> = liveFlow.asSharedFlow()

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

    override fun reconnect() {
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

    private val batchFlow = MutableSharedFlow<BatchLine>(extraBufferCapacity = BUFFER)
    override val batches: SharedFlow<BatchLine> = batchFlow.asSharedFlow()

    private val workingFlow = MutableStateFlow(false)
    override val working: StateFlow<Boolean> = workingFlow.asStateFlow()

    /** 下到本机那几组开给各项的那一份（组编号 · 键 ⇒ 那一份），没下成时照它删。 */
    private val saved = ConcurrentHashMap<Pair<Long, String>, LocalFile>()

    /** 接回来的那几组各自最后那一格（[resumed]）。 */
    private val resumedLast = LinkedHashMap<Long, BatchLine>()

    override fun resumed(): List<BatchLine> =
        synchronized(resumedLast) { resumedLast.values.toList().also { resumedLast.values.removeAll { e -> e.over } } }

    override suspend fun batch(want: Map<String, Any?>): Long {
        ensure()
        val h = withTimeoutOrNull(DEFAULT_CALL_MS) { opened.await() } ?: return 0L
        return core.batchStart(h, Json.args(want))
    }

    override fun batchAnswer(
        id: Long,
        answer: BatchAnswer?,
    ) = core.batchAnswer(id, answer?.let { Wire.line(it) })

    override fun batchStop(id: Long) = core.batchStop(id)

    @OptIn(ExperimentalCoroutinesApi::class)
    override fun picks(rows: List<Any?>): PhoneMenu? {
        ensure()
        val h = if (opened.isCompleted) opened.getCompleted() else 0L
        val line = core.pickMenu(h, Json.line(rows))
        return Wire.read<PhoneMenu>(line) ?: null.also { broke("pick_menu", line) }
    }

    override fun clashes(data: Any?): List<ClashRow> {
        val line = core.clashRows(Json.line(data))
        return Wire.read(arr(ClashRow.ADAPTER), line) ?: emptyList<ClashRow>().also { broke("clash_rows", line) }
    }

    override fun decided(
        keys: List<String>,
        answer: BatchAnswer?,
    ): Map<String, Decide>? {
        val line = core.answerOf(Json.line(keys), answer?.let { Wire.line(it) })
        // 取消 ⇒ 核心回 `null`。
        if (line == "null") return null
        return Wire.read(dict(Decide.ADAPTER), line) ?: null.also { broke("answer_of", line) }
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
            "state" -> Wire.read<LinkLine>(body)?.let(::onState) ?: broke(kind, body)
            "empty" -> Wire.read<EmptyFace>(body)?.let(onEmpty) ?: broke(kind, body)
            "frame", "unread", "book", "live" -> onStream(kind, body)
            "watch_from" -> onWatchFrom(body)
            "ssh" -> sshFlow.value = body == "true"
            "reply" -> onReply(body)
            "moved" -> onMoved(body)
            "deploy" -> onDeployLine(body)
            "batch", "batch_keep", "batch_local", "busy" -> onWork(kind, body)
            else -> broke(kind, body)
        }
    }

    /** 核心里在走的活那几样：一组走到哪 · 落盘那一份 · 下到本机那一侧要做的 · 有没有活在走。 */
    private suspend fun onWork(
        kind: String,
        body: String,
    ) {
        when (kind) {
            "batch" -> onBatchLine(body)
            "batch_keep" -> onKeepLine(body)
            "batch_local" -> onLocalLine(body)
            else -> workingFlow.value = body == "true"
        }
    }

    /** `state` 那一行：通没通（[LinkLine] 那几支）＋ 那台的状态成品（部署走到哪一步也在它里面）。 */
    private fun onState(line: LinkLine) {
        offlineFlow.value = line.machine.offlineSaid
        // 会话簿随旧连接作废（连接中那一行之前核心交了空簿）；换了一条流 ⇒ epoch 跟着换。
        if (line is LinkLine.Up && line.epoch != epochFlow.value) epochFlow.value = line.epoch
        stateFlow.value = line
        // 「装好了」那一句只留到下一次出事（断了 · 通着又有问题行）；没装上那一句留着，等下一趟。
        val trouble = line is LinkLine.Down || (line is LinkLine.Up && line.machine.said.isNotEmpty())
        if (deployFlow.value is Deploying.Done && trouble) deployFlow.value = null
    }

    /** 流上来的那几样：别的帧 · 核心解不开的一帧 · 核心折好的整份（会话簿换掉手上那一本；活卡是那一处此刻整份的卡）。 */
    private suspend fun onStream(
        kind: String,
        body: String,
    ) {
        when (kind) {
            "frame" -> onFrameLine(body)
            // 核心按 `cmd-wire` 那一份类型解不开的一帧（两端版本对不上），原样那一行。
            "unread" -> broke("frame", body)
            "book" -> Wire.read<SessionBook>(body)?.let { tableFlow.value = it } ?: broke(kind, body)
            else -> Wire.read<LiveCards>(body)?.let(liveFlow::tryEmit) ?: broke(kind, body)
        }
    }

    private suspend fun onFrameLine(body: String) {
        Json.obj(body)?.let { onFrame(it) } ?: broke("frame", body)
    }

    private suspend fun onFrame(frame: Map<String, Any?>) {
        if (frame["kind"] == CHANGED) Wire.bind<Changed>(frame)?.let(changedFlow::tryEmit) ?: broke("frame", frame.toString())
        // 满了就在这里等（停收），不丢行。
        RecordLine.of(frame)?.let { lineFlow.emit(it) }
    }

    /** `batch` 那一行落进 [batches]（接回来的那几组另记最后那一格：收场时有人在看 ⇒ 算交过）；认不出 ⇒ 报一次契约对不上。 */
    private suspend fun onBatchLine(body: String) {
        val e = Wire.read<BatchLine>(body) ?: return broke("batch", body)
        // 收场了：开给那几项的那几份不再要删（下成的留着）。
        if (e.over) saved.keys.removeIf { it.first == e.id }
        if (e.kept) {
            synchronized(resumedLast) {
                if (e.over && batchFlow.subscriptionCount.value > 0) resumedLast.remove(e.id) else resumedLast[e.id] = e
            }
        }
        batchFlow.emit(e)
    }

    /** `batch_keep {ticket, plan}`：那一串原样落盘；`plan` 是 `null` ⇒ 删。 */
    private suspend fun onKeepLine(body: String) {
        val k = Wire.read<BatchKeep>(body) ?: return broke("batch_keep", body)
        when (val plan = k.plan) {
            null -> kept.drop(k.ticket)
            else -> kept.put(k.ticket, plan)
        }
    }

    /**
     * `batch_local`：下到本机那一组要本机那一侧做的 —— 开一份（开好的描述符 · 开不了的系统原话经 `batchAnswer` 交回，那一句核心写）·
     * 删开给那一项的那一份。照到达先后一件一件做：撤在开的路上时，删那一件排在开那一件后面，删得到。
     */
    private fun onLocalLine(body: String) {
        when (val a = Wire.read<LocalAsk>(body) ?: return broke("batch_local", body)) {
            is LocalAsk.Open -> {
                val answer: LocalOpened =
                    try {
                        val f = saves.create(a.folder, a.name, a.over)
                        val fd =
                            try {
                                f.fd()
                            } catch (e: IOException) {
                                f.drop()
                                throw e
                            }
                        saved[a.id to a.key] = f
                        LocalOpened.Fd(a.key, fd.toLong())
                    } catch (e: IOException) {
                        LocalOpened.Refused(a.key, e.toString())
                    }
                core.batchAnswer(a.id, Wire.line(answer))
            }
            is LocalAsk.Drop -> saved.remove(a.id to a.key)?.drop?.invoke()
        }
    }

    /** `deploy` 那一行落进 [deploying]；认不出 ⇒ 报一次契约对不上、不动。 */
    private fun onDeployLine(body: String) {
        deployFlow.value =
            when (val e = Wire.read<DeployEnd>(body) ?: return broke("deploy", body)) {
                is DeployEnd.Done -> Deploying.Done(e.text)
                is DeployEnd.Failed -> Deploying.Failed(Said(e.text, e.detail))
            }
    }

    /** `watch_from [{sid, path, seq}]`：每一条从哪一行接着读。 */
    private fun onWatchFrom(body: String) {
        Wire.read(arr(WatchFrom.ADAPTER), body)?.forEach { fromFlow.tryEmit(it) } ?: broke("watch_from", body)
    }

    /** `moved {call, done, total}`：交给在等那一趟的那一口（总数不知道 ⇒ `-1`）。 */
    private fun onMoved(body: String) {
        val m = Wire.read<Moved>(body) ?: return broke("moved", body)
        moving[m.call]?.invoke(m.done, m.total ?: -1L)
    }

    /** `reply` 那一行（[CallEnd] 那几支）落到在等那一问；那一行本身认不出 ⇒ 报一次契约对不上（在等的那一问到期限当断了）。 */
    private fun onReply(body: String) {
        val e = Wire.read<CallEnd>(body) ?: return broke("reply", body)
        val reply =
            when (e) {
                is CallEnd.Ok -> Reply.Ok(e.data.value)
                is CallEnd.Failed -> Reply.Failed(CoreFailure(e.code, e.message, e.detail, e.data.value))
                is CallEnd.Timeout -> Reply.TimedOut
                is CallEnd.Cancelled -> Reply.Cancelled
                is CallEnd.Down -> Reply.LinkDown
                // 传文件那一趟的回包核心照类型解不开（形状对不上）：原样带回，详情里是 serde 那一句。
                is CallEnd.Unreadable -> Reply.Unreadable(body)
            }
        replies.computeIfAbsent(e.call) { CompletableDeferred() }.complete(reply)
    }

    private companion object {
        const val BUFFER = 64
        const val LINE_BUFFER = 256
        const val BREAK_SNIPPET = 200

        /** 核心到期限必回一句；它没回（不该有）时这边多等这么久就当断了。 */
        const val REPLY_GRACE_MS = 5_000L

        /** 装后端之前等 SSH 那层交到核心，最多等这么久。 */
        const val SSH_WAIT_MS = 15_000L

        /** 流上 `changed` 那种帧的 `kind`。 */
        const val CHANGED = "changed"
    }
}

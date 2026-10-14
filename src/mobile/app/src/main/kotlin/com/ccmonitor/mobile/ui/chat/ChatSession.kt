package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.HistoryLines
import com.ccmonitor.mobile.core.claude.link.HistoryList
import com.ccmonitor.mobile.core.claude.link.HistoryRead
import com.ccmonitor.mobile.core.claude.link.HistoryTail
import com.ccmonitor.mobile.core.claude.link.LiveBoard
import com.ccmonitor.mobile.core.claude.link.LiveCardState
import com.ccmonitor.mobile.core.claude.link.RecordLine
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionCommandList
import com.ccmonitor.mobile.core.claude.link.SessionModel
import com.ccmonitor.mobile.core.claude.link.SessionNew
import com.ccmonitor.mobile.core.claude.link.SessionNewFacts
import com.ccmonitor.mobile.core.claude.link.SessionPermission
import com.ccmonitor.mobile.core.claude.link.SessionSay
import com.ccmonitor.mobile.core.claude.link.SessionWatch
import com.ccmonitor.mobile.core.claude.link.TerminalInput
import com.ccmonitor.mobile.core.claude.link.wire.LineRecord
import com.ccmonitor.mobile.core.claude.link.wire.ModeSwitchResult
import com.ccmonitor.mobile.core.claude.link.wire.NewSessionPick
import com.ccmonitor.mobile.core.claude.link.wire.RecordKind
import com.ccmonitor.mobile.core.claude.link.wire.SessionCommands
import com.ccmonitor.mobile.core.claude.link.wire.TailPlan
import com.ccmonitor.mobile.core.claude.model.DeliveryState
import com.ccmonitor.mobile.core.claude.model.RecordUnits
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.onSubscription
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull

/** 聊天面的一帧 UI 状态。 */
data class ChatUiState(
    /** 记录 ＋ 手机这边还没在记录里出现的那几句。 */
    val units: List<RenderUnit> = emptyList(),
    /** 这条会话此刻在跑（会话帧的语气 `now`）：右端是 ■。 */
    val running: Boolean = false,
    /** 会话那一层的失败（起不来 · 读不出记录）那一句；`null` ＝ 没有。 */
    val failedWhy: String? = null,
    val canLoadOlder: Boolean = false,
    val loadingOlder: Boolean = false,
    /** 往上翻没读成的那一句（不崩页面、再上滑一次重试）。 */
    val historyError: String? = null,
    /** 已经起成、认出来的那条会话；新建会话发第一条之前是 `null`。 */
    val sid: String? = null,
    /** 这条会话主运行此刻的活卡（那台的 `live_card` 帧照画；记录到了那台撤卡）。 */
    val live: List<LiveCardState> = emptyList(),
    /** 模型 chip。 */
    val model: SwitchChip = SwitchChip(),
    /** 权限档 chip。 */
    val permission: SwitchChip = SwitchChip(),
    /** chip 上方那一行（换的结局没成 · ［+］面板里的动作没送到）：那台写好的那一句；`null` ＝ 不画。 */
    val note: String? = null,
    /** 两个 chip 抽屉里列的（那台那一家给的）；还没问 ⇒ `null`。 */
    val picks: Picks? = null,
    /** ［+］面板里的命令与 Skill（那台给的这条会话那一份）。 */
    val commands: CommandsView = CommandsView.Idle,
)

/**
 * 一颗「换」的 chip：模型 · 权限档。字只写那台读回来的；判「此刻能不能换」也归那台（它拒了就照它那句说），
 * 界面只管「正在换」这一态（两颗都不可点、换的那颗原位转圈）。
 */
data class SwitchChip(
    /** 那台读回来的显示字；还没读回过 ⇒ `null`（chip 写这颗是什么）。 */
    val text: String? = null,
    /** 上一次换成了的那一项（原值）：抽屉里勾它。 */
    val value: String? = null,
    /** 正在换成的那一项（写它的字、转圈）；不在换 ⇒ `null`。 */
    val pending: NewSessionPick? = null,
)

/** 两个 chip 抽屉里列的：那台 `session-new-facts` 给这条会话那一家的模型与权限档；问不到 ⇒ [error] 是那一句。 */
data class Picks(
    val models: List<NewSessionPick> = emptyList(),
    val modes: List<NewSessionPick> = emptyList(),
    val error: String? = null,
)

/** ［+］面板里那份命令与 Skill 的清单此刻怎样。 */
sealed interface CommandsView {
    data object Idle : CommandsView

    data object Loading : CommandsView

    data class Ready(
        val list: SessionCommands,
    ) : CommandsView

    data class Failed(
        val why: String,
    ) : CommandsView
}

/** 聊天屏对着的是哪条会话。 */
sealed interface ChatTarget {
    /** 已有的那一条（从列表点进来）。 */
    data class Existing(
        val sid: String,
    ) : ChatTarget

    /** 新建会话：发第一条时请那台起。[ticket] 一个草稿一张。 */
    data class Draft(
        val ticket: String,
        /** 在哪个目录起（开头的 `~` 那台按它的家目录读）。 */
        val cwd: String,
        /** 这台存的那一家（线上 kind，主机设置里选的）；没选 ⇒ `null`。起的时候照那台 `session-new-facts` 认：认得 ⇒ 它，否则那台列的第一家。 */
        val agent: String?,
    ) : ChatTarget
}

/**
 * 一条会话的聊天状态。起会话、送字、正文都走那台的核心（[CoreLink]）：
 * - 新建会话：第一条按 ➤ ⇒ `session-new`（在那台 tmux 里起，桌面那一侧看得到、接得上），那一句随起交（`prompt`：那台作为启动参数交给 agent，
 *   作为启动参数交，送没送到看回包）；那台没交（回包没有 `prompt`）⇒ 等它报到（会话表里那个目录新出现的一条）再经 `terminal-input` 送。
 *   新起的会话记录要等第一句进去才落盘 ⇒ 读正文之前先等那台推来它的记录路径（[freshPath]）。
 * - 送字 / 停：`terminal-input`（字 · `esc`）；送没送到是那台判的（`delivered` · `unsure` · `refused`）。
 * - 正文：`history-tail` ＋ `history-read` 读最后一屏，之后跟流上这条会话的 `line` 帧；往上翻再读上一段。记录 ⇒ 渲染单元见 [RecordUnits]。
 * - 在屏上（[visible]）才进这台的 `stream-watch` 名单；进名单 · 断线重接之后，`[已有, seq)` 那一段按行号用 `history-lines` 补上
 *   （`from[]` 给的 seq，或来了一行比已有的新、中间缺着），缺得多就整份重读最后一屏；重叠处按行号去重。
 * 手机这边只记「送出去、记录里还没出现」的那几句；记录里出现了同一句人话 ⇒ 那一句的标消失。
 *
 * 它不是 `ViewModel`：持有者是应用级的 [ChatController]，离开聊天屏照样跟着。
 */
class ChatSession(
    private val link: CoreLink,
    /** 这台的 `stream-watch` 名单：在屏上时才看这一条（不在名单里的会话，它的行不上流）。 */
    private val watch: SessionWatch,
    target: ChatTarget,
    /** 这台在手机上的名字（「{machine} 无应答」那几句）。 */
    private val machine: String,
    private val scope: CoroutineScope,
    private val arrivalMs: Long = ARRIVAL_MS,
) {
    private val _state = MutableStateFlow(ChatUiState(sid = (target as? ChatTarget.Existing)?.sid))
    val state: StateFlow<ChatUiState> = _state.asStateFlow()

    private val draft = target as? ChatTarget.Draft
    private val records = mutableListOf<LineRecord>()
    private val outgoing = mutableListOf<Outgoing>()
    private val lock = Mutex()

    @Volatile private var sid: String? = (target as? ChatTarget.Existing)?.sid
    private var path: String? = null

    /** 已读到的最早一行的字节起点；已有的最新一行之后那一行的行号；尾段要了几行。 */
    private var head = 0L
    private var nextSeq = 0L
    private var tailLines = PAGE_LINES

    /** 在屏上 ⇒ 这一条在名单里（[holding]）。sid 还没有（新建会话起好之前）⇒ 起好再进。 */
    @Volatile private var shown = false
    private var holding: AutoCloseable? = null

    private var follow: Job? = null
    private val board = LiveBoard()
    private var starting: Job? = null
    private var seq = 0

    private class Outgoing(
        val localId: String,
        val text: String,
        var delivery: DeliveryState?,
        var deliveryText: String?,
    ) {
        /** 这一句已随 `session-new` 交出去了（结局已标上）。 */
        var handed = false
    }

    /** 开屏：已有的那条读最后一屏、跟上；新建会话什么都不问。 */
    fun open() {
        val s = sid ?: return
        if (follow == null) follow = scope.launch { attach(s) }
    }

    fun send(text: String) {
        if (text.isBlank()) return
        val o = Outgoing("local-${seq++}", text, DeliveryState.SENDING, copyText("mobChat.delivery.sending"))
        outgoing += o
        publish()
        scope.launch { deliver(o) }
    }

    /** 再发一次没送到的那一句（只有能改的原因才有这颗）。 */
    fun retry(localId: String) {
        val o = outgoing.firstOrNull { it.localId == localId && it.delivery == DeliveryState.FAILED } ?: return
        o.delivery = DeliveryState.SENDING
        o.deliveryText = copyText("mobChat.delivery.sending")
        publish()
        scope.launch { deliver(o) }
    }

    /**
     * 换模型：那台送命令、从记录里读回换成了什么（`session-model`）。读回 `done` 才换字；没读回（`unsure` · 超时）· 被拒 ⇒ 回原字 ＋ chip 上方那一句。
     * 已经在换（模型或权限档）⇒ 不再起一趟。
     */
    fun switchModel(pick: NewSessionPick) =
        switching(pick, { it.model }, { st, c -> st.copy(model = c) }) { s ->
            val r = link.call(SessionModel.COMMAND, SessionModel.args(s, pick.value), SWITCH_MS)
            when {
                r is Reply.Ok ->
                    SessionModel.of(r.data)?.let { Switched(it.result, it.model, it.said) }
                        ?: Switched.failed(unreadableReply(machine, SessionModel.COMMAND, r.data.toString()).text)
                r == Reply.TimedOut -> Switched.failed(copyText("session.model.unsure"))
                else -> Switched.failed(r.problem(machine, SessionModel.COMMAND)?.text)
            }.orUnsure(copyText("session.model.unsure"))
        }

    /** 切权限档（`session-permission`）：同 [switchModel]，读回的字是那一档写给人看的字。 */
    fun switchPermission(pick: NewSessionPick) =
        switching(pick, { it.permission }, { st, c -> st.copy(permission = c) }) { s ->
            val r = link.call(SessionPermission.COMMAND, SessionPermission.args(s, pick.value), SWITCH_MS)
            when {
                r is Reply.Ok ->
                    SessionPermission.of(r.data)?.let { Switched(it.result, it.modeText, it.said) }
                        ?: Switched.failed(unreadableReply(machine, SessionPermission.COMMAND, r.data.toString()).text)
                r == Reply.TimedOut -> Switched.failed(copyText("session.permission.unsure"))
                else -> Switched.failed(r.problem(machine, SessionPermission.COMMAND)?.text)
            }.orUnsure(copyText("session.permission.unsure"))
        }

    /** 一趟换的结局：[text] 读回的字，[said] 那台那一句。 */
    private data class Switched(
        val result: ModeSwitchResult,
        val text: String?,
        val said: String?,
    ) {
        /** 没成却没那一句 ⇒ 补上「结果未知」那一句（unsure 时那台多半带着自己的那一句）。 */
        fun orUnsure(unsure: String): Switched = if (result != ModeSwitchResult.DONE && said == null) copy(said = unsure) else this

        companion object {
            fun failed(said: String?) = Switched(ModeSwitchResult.UNSURE, null, said)
        }
    }

    private fun switching(
        pick: NewSessionPick,
        chipOf: (ChatUiState) -> SwitchChip,
        put: (ChatUiState, SwitchChip) -> ChatUiState,
        ask: suspend (String) -> Switched,
    ) {
        val s = sid ?: return
        val st = _state.value
        if (st.model.pending != null || st.permission.pending != null) return
        _state.value = put(st, chipOf(st).copy(pending = pick)).copy(note = null)
        scope.launch {
            val r = ask(s)
            val now = _state.value
            val was = chipOf(now).copy(pending = null)
            val chip = if (r.result == ModeSwitchResult.DONE && r.text != null) was.copy(text = r.text, value = pick.value) else was
            _state.value = put(now, chip).copy(note = if (r.result == ModeSwitchResult.DONE) r.said.takeIf { r.text == null } else r.said)
        }
    }

    /** 抽屉里列什么：那台给这条会话那一家的模型与权限档（会话表里认那一家；认不出 ⇒ 那台列的第一家）。问过一次就不再问。 */
    fun loadPicks() {
        if (_state.value.picks?.error == null && _state.value.picks != null) return
        val agent =
            sid?.let {
                link.table.value.sessions[it]
                    ?.agentKind
            } ?: draft?.agent
        scope.launch {
            val r = link.call(SessionNewFacts.COMMAND, emptyMap(), READ_MS)
            val picks =
                when (r) {
                    is Reply.Ok ->
                        SessionNewFacts.of(r.data)?.let { all -> SessionNewFacts.pick(all, agent).let { Picks(it?.models.orEmpty(), it?.permissionModes.orEmpty()) } }
                            ?: Picks(error = unreadableReply(machine, SessionNewFacts.COMMAND, r.data.toString()).text)
                    else -> Picks(error = r.problem(machine, SessionNewFacts.COMMAND)?.text.orEmpty())
                }
            _state.value = _state.value.copy(picks = picks)
        }
    }

    /** ［+］面板打开：问那台这条会话能用的命令与 Skill（每开一次问一次：项目里的命令文件可能刚改过）。 */
    fun loadCommands() {
        val s = sid ?: return
        _state.value = _state.value.copy(commands = CommandsView.Loading)
        scope.launch {
            val r = link.call(SessionCommandList.COMMAND, SessionCommandList.args(s), READ_MS)
            val view =
                when (r) {
                    is Reply.Ok ->
                        SessionCommandList.of(r.data)?.let { CommandsView.Ready(it) }
                            ?: CommandsView.Failed(unreadableReply(machine, SessionCommandList.COMMAND, r.data.toString()).text)
                    else -> CommandsView.Failed(r.problem(machine, SessionCommandList.COMMAND)?.text.orEmpty())
                }
            _state.value = _state.value.copy(commands = view)
        }
    }

    /** ［+］面板里的一次性动作（压缩上下文 · 清空会话）：经 `session-say` 说那一句，结果回消息流；没送到 ⇒ chip 上方那一句。 */
    fun say(text: String) {
        val s = sid ?: return
        _state.value = _state.value.copy(note = null)
        scope.launch {
            val (delivery, said) = outcomeOf(link.call(SessionSay.COMMAND, SessionSay.args(s, text), INPUT_MS))
            if (delivery != null) _state.value = _state.value.copy(note = said)
        }
    }

    /** ■：往那条会话送 Esc；中断那一条看记录。 */
    fun stop() {
        val s = sid ?: return
        scope.launch { link.call(TerminalInput.COMMAND, TerminalInput.key(s, "esc"), INPUT_MS) }
    }

    fun loadOlder() {
        val p = path ?: return
        if (_state.value.loadingOlder || head <= 0L) return
        _state.value = _state.value.copy(loadingOlder = true, historyError = null)
        scope.launch {
            val want = tailLines + PAGE_LINES
            val got = readTail(p, want)?.let { t -> readRange(p, t.splitAt, head)?.let { t to it } }
            lock.withLock {
                if (got == null) {
                    _state.value = _state.value.copy(loadingOlder = false)
                    return@withLock
                }
                val (t, page) = got
                tailLines = want
                head = t.splitAt
                records.addAll(0, page)
                _state.value = _state.value.copy(loadingOlder = false, canLoadOlder = head > 0L)
                publishLocked()
            }
        }
    }

    /** 聊天屏进 / 出：进了名单这条会话的行才上流，退了就停（`stream-watch`）。 */
    fun visible(on: Boolean) {
        shown = on
        syncHold()
    }

    @Synchronized
    private fun syncHold() {
        val s = sid
        if (shown && s != null && attached) {
            if (holding == null) holding = watch.hold(s)
        } else {
            holding?.close()
            holding = null
        }
    }

    @Volatile private var attached = false

    fun close() {
        shown = false
        syncHold()
        scope.cancel()
    }

    /** ➤：经 `session-say` 往那条会话说（那台判：在等人不送 · 终端没了按记下的号恢复再交）。 */
    private suspend fun deliver(o: Outgoing) {
        val s = sid ?: started(o)?.also { if (o.handed) return } ?: return fail(o, permanent = false, text = null)
        val (delivery, text) = outcomeOf(link.call(SessionSay.COMMAND, SessionSay.args(s, o.text), INPUT_MS))
        mark(o, delivery, text)
    }

    /** `session-say` 的回话 ⇒ 气泡下的标与那一句（送到了 ⇒ 没有标）。 */
    private fun outcomeOf(r: Reply): Pair<DeliveryState?, String?> =
        when (r) {
            is Reply.Ok ->
                SessionSay.of(r.data)?.let(::outcomeOf)
                    ?: failed(unreadableReply(machine, SessionSay.COMMAND, r.data.toString()).text)
            // 期限到了：发出去了、不知道送没送到 ⇒ 不重发。
            Reply.TimedOut -> unsure()
            else -> failed(r.problem(machine, SessionSay.COMMAND)?.text)
        }

    /** 那台判的送没送到（`terminal-input` 的回话 · `session-new` 回包里第一句的结局）⇒ 标与那一句。 */
    private fun outcomeOf(res: TerminalInput.Result): Pair<DeliveryState?, String?> =
        when (res) {
            TerminalInput.Result.Delivered -> null to null
            TerminalInput.Result.Unsure -> unsure()
            is TerminalInput.Result.Refused ->
                (if (res.why in RETRYABLE) DeliveryState.FAILED else DeliveryState.FAILED_PERMANENT) to
                    copyText("terminal.input.failed", "why" to res.said)
        }

    private fun unsure() = DeliveryState.UNSURE to copyText("terminal.input.unsure", "machine" to machine)

    private fun failed(why: String?) = DeliveryState.FAILED to copyText("terminal.input.failed", "why" to why.orEmpty())

    private fun mark(
        o: Outgoing,
        delivery: DeliveryState?,
        text: String?,
    ) {
        o.delivery = delivery
        o.deliveryText = text
        publish()
    }

    private fun fail(
        o: Outgoing,
        permanent: Boolean,
        text: String?,
    ) = mark(o, if (permanent) DeliveryState.FAILED_PERMANENT else DeliveryState.FAILED, text ?: copyText("newSession.start.failed"))

    /** 新建会话起好、认出来了 ⇒ 它的 sid；起不来 ⇒ `null`（那一句已在 [ChatUiState.failedWhy]）。几句同时等同一次起，头一句（[o]）随起交；
     * 那台说没交到（`prompt` 是 `refused`）⇒ 那句标未送达、能改的给［再发一次］（走 `terminal-input`）：兜底，今天两家都作为启动参数交、起好就是交到了。 */
    private suspend fun started(o: Outgoing): String? {
        val d = draft ?: return null
        val job = lock.withLock { starting?.takeIf { it.isActive } ?: scope.launch { start(d, o) }.also { starting = it } }
        job.join()
        return sid
    }

    private suspend fun start(
        d: ChatTarget.Draft,
        first: Outgoing,
    ) {
        _state.value = _state.value.copy(failedWhy = null)
        val before = link.table.value.sessions.keys
        // 起哪一家问那台（存的那一家认得 ⇒ 它，否则第一家）；问不到 ⇒ 照存的交，认不出由那台那一句说。
        val facts = (link.call(SessionNewFacts.COMMAND, emptyMap(), READ_MS) as? Reply.Ok)?.let { SessionNewFacts.of(it.data) }
        val agent = facts?.let { SessionNewFacts.pick(it, d.agent)?.kind } ?: d.agent.orEmpty()
        val answer =
            when (val r = link.call(SessionNew.COMMAND, SessionNew.args(d.cwd, d.ticket, agent, first.text), NEW_MS)) {
                is Reply.Ok -> SessionNew.of(r.data) ?: return failSession(unreadableReply(machine, SessionNew.COMMAND, r.data.toString()).text)
                else -> return failSession(r.problem(machine, SessionNew.COMMAND)?.text)
            }
        val arrived =
            answer.sid ?: withTimeoutOrNull(arrivalMs) {
                link.table.mapNotNull { SessionNew.arrived(it, answer.cwd, before)?.sid }.first()
            } ?: return failSession(copyText("launchArrival.missed.title"))
        answer.prompt?.let { said ->
            first.handed = true
            val (delivery, text) = outcomeOf(said)
            mark(first, delivery, text)
        }
        sid = arrived
        _state.value = _state.value.copy(sid = arrived)
        syncHold()
        follow = scope.launch { attach(arrived, fresh = true) }
    }

    private fun failSession(text: String?) {
        _state.value = _state.value.copy(failedWhy = text)
    }

    /** 一条会话流上来的两样：一行 · 进名单那一格（从第几行起上流）。 */
    private sealed interface Event {
        data class Line(
            val line: RecordLine,
        ) : Event

        data class From(
            val seq: Long,
        ) : Event

        /** 换了一条流（`CoreLink.epoch` 变了）：断开那段的行没收到。 */
        data object Reconnected : Event
    }

    /** 先订上这条会话的 `line` 帧与 `from`（攒着），再读最后一屏；读完按行号接上攒着的，之后来一样接一样。[fresh]：刚起的那条（见 [pathOf]）。 */
    private suspend fun attach(
        s: String,
        fresh: Boolean = false,
    ) {
        val events = Channel<Event>(Channel.UNLIMITED)
        val subscribed = CompletableDeferred<Unit>()
        val fromSubscribed = CompletableDeferred<Unit>()
        scope.launch {
            link.lines.onSubscription { subscribed.complete(Unit) }.collect { if (it.sid == s) events.send(Event.Line(it)) }
        }
        scope.launch {
            watch.from.onSubscription { fromSubscribed.complete(Unit) }.collect { if (it.sid == s) events.send(Event.From(it.seq)) }
        }
        subscribed.await()
        fromSubscribed.await()
        scope.launch { link.epoch.drop(1).collect { events.send(Event.Reconnected) } }
        followLive(s)
        attached = true
        syncHold()
        watchRunning(s)
        val p = (if (fresh) freshPath(s) else pathOf(s)) ?: return
        path = p
        // 读没成（那一句已上屏）⇒ 等换了一条流再整份重读；成了之后断开再接上 ⇒ 从断处补。
        var loaded = lock.withLock { reloadLocked(p) }

        suspend fun take(e: Event) =
            lock.withLock {
                if (!loaded) {
                    if (e == Event.Reconnected) loaded = reloadLocked(p)
                } else if (stepLocked(p, e)) {
                    publishLocked()
                }
            }
        for (e in events) take(e)
    }

    /**
     * 刚起的那条的记录路径：第一句进去之前记录还没落盘，那台宣告这条会话时不带路径；记录出现时那台先推一帧带路径的状态帧
     * （在那一行的 line 帧之前）⇒ 等会话表里那一条有了路径。不设期限、不去清单里按点找：会话在，记录迟早来。
     */
    private suspend fun freshPath(s: String): String = link.table.mapNotNull { it.sessions[s]?.path }.first()

    /** 接上一样；记录有变 ⇒ `true`。 */
    private suspend fun stepLocked(
        p: String,
        e: Event,
    ): Boolean =
        when (e) {
            is Event.Line -> lineLocked(p, e.line)
            is Event.From -> e.seq <= nextSeq || fillLocked(p, e.seq)
            Event.Reconnected -> {
                val t = readTail(p, 1)
                val filled = t != null && (t.total <= nextSeq || fillLocked(p, t.total))
                if (filled) _state.value = _state.value.copy(failedWhy = null, historyError = null)
                filled
            }
        }

    /** 一行：比已有的旧 ⇒ 重叠、不要；比已有的新、中间缺着 ⇒ 先补上。 */
    private suspend fun lineLocked(
        p: String,
        line: RecordLine,
    ): Boolean {
        if (line.seq > nextSeq && !fillLocked(p, line.seq)) return false
        if (line.seq < nextSeq) return false
        line.record?.let { records += it }
        nextSeq = line.seq + 1
        return true
    }

    /** 整份重来：读最后一屏（[tailLines] 行）。没读成 ⇒ `false`（那一句已上屏）。 */
    private suspend fun reloadLocked(p: String): Boolean {
        val t = readTail(p, tailLines) ?: return false
        val page = readRange(p, t.splitAt, t.end) ?: return false
        head = t.splitAt
        nextSeq = t.total
        records.clear()
        records.addAll(page)
        _state.value = _state.value.copy(canLoadOlder = head > 0L, failedWhy = null)
        publishLocked()
        return true
    }

    /**
     * 补上 `[nextSeq, until)`：缺得不多按行号取（`history-lines`）；缺得多（超过一屏）⇒ 整份重读最后一屏、更早的往上翻再取。
     * 没补成 ⇒ `false`，已有的不动（下一行来了再补）。
     */
    private suspend fun fillLocked(
        p: String,
        until: Long,
    ): Boolean {
        if (until - nextSeq > PAGE_LINES) {
            tailLines = PAGE_LINES
            return reloadLocked(p)
        }
        var more = true
        while (more && nextSeq < until) {
            val r = link.call(HistoryLines.COMMAND, HistoryLines.args(p, nextSeq, until), READ_MS)
            val page =
                (r as? Reply.Ok)?.let { HistoryLines.of(it.data) }
                    ?: return false.also { readFailed(r.problem(machine, HistoryLines.COMMAND)?.text ?: unreadableReply(machine, HistoryLines.COMMAND, r.toString()).text) }
            records += page.records
            more = page.next > nextSeq && !page.eof
            nextSeq = maxOf(nextSeq, page.next)
        }
        nextSeq = maxOf(nextSeq, until)
        return true
    }

    /**
     * 这条会话的活卡：只收它主运行的（子运行的卡不进主线，同桌面主 tab）。换了一条流 ⇒ 这条连接上的号全作废、全撤，
     * 那台在新的那条上给名单里的会话补整卡。
     */
    private fun followLive(s: String) {
        scope.launch {
            link.live.collect { f ->
                if (f.sid == s && board.apply(f)) _state.value = _state.value.copy(live = board.cardsOf(s))
            }
        }
        scope.launch {
            link.epoch.drop(1).collect { if (board.clear()) _state.value = _state.value.copy(live = emptyList()) }
        }
    }

    private fun watchRunning(s: String) {
        scope.launch {
            link.table.collect { t ->
                // 只有那台判得出「在跑」才亮停止键；判不出（说不清 · 读记录中）不亮。
                val running = t.sessions[s]?.running == true
                if (running != _state.value.running) _state.value = _state.value.copy(running = running)
            }
        }
    }

    /** 那条会话的记录文件：会话表里有就用它，没有（不在跑了）问 `history-list`。 */
    private suspend fun pathOf(s: String): String? {
        link.table.value.sessions[s]
            ?.path
            ?.let { return it }
        return when (val r = link.call(HistoryList.COMMAND, null, READ_MS)) {
            is Reply.Ok -> {
                val list = HistoryList.of(r.data) ?: return null.also { failSession(unreadableReply(machine, HistoryList.COMMAND, r.data.toString()).text) }
                list.rows.firstOrNull { it.sessionId == s }?.jsonlPath ?: null.also { failSession(copyText("mobChat.record.missing", "machine" to machine)) }
            }
            else -> null.also { failSession(r.problem(machine, HistoryList.COMMAND)?.text) }
        }
    }

    private suspend fun readTail(
        p: String,
        n: Int,
    ): TailPlan? =
        when (val r = link.call(HistoryTail.COMMAND, HistoryTail.args(p, n), READ_MS)) {
            is Reply.Ok -> HistoryTail.of(r.data) ?: null.also { readFailed(unreadableReply(machine, HistoryTail.COMMAND, r.data.toString()).text) }
            else -> null.also { readFailed(r.problem(machine, HistoryTail.COMMAND)?.text) }
        }

    /** 读 `[from, until)` 里的记录，一页不够就接着读。 */
    private suspend fun readRange(
        p: String,
        from: Long,
        until: Long,
    ): List<LineRecord>? {
        val out = mutableListOf<LineRecord>()
        var at = from
        while (at < until) {
            val r = link.call(HistoryRead.COMMAND, HistoryRead.args(p, at, until), READ_MS)
            val page =
                (r as? Reply.Ok)?.let { HistoryRead.of(it.data) }
                    ?: return null.also { readFailed(r.problem(machine, HistoryRead.COMMAND)?.text ?: unreadableReply(machine, HistoryRead.COMMAND, r.toString()).text) }
            out += page.records
            if (page.eof || page.next <= at) break
            at = page.next
        }
        return out
    }

    private fun readFailed(text: String?) {
        if (records.isEmpty()) failSession(text) else _state.value = _state.value.copy(historyError = text)
    }

    private fun publish() {
        scope.launch { lock.withLock { publishLocked() } }
    }

    /** 记录里出现了同一句人话 ⇒ 那一句的本地标消失；其余的接在记录后面。 */
    private fun publishLocked() {
        val said = records.filter { it.t == RecordKind.SAID || it.t == RecordKind.QUEUED }.mapNotNull { it.who?.text }.toMutableList()
        outgoing.removeAll { o -> o.delivery != DeliveryState.FAILED && o.delivery != DeliveryState.FAILED_PERMANENT && said.remove(o.text) }
        val local = outgoing.map { RenderUnit.UserText(it.localId, it.text, null, it.delivery, it.deliveryText) }
        _state.value = _state.value.copy(units = RecordUnits.of(records) + local)
    }

    companion object {
        /** 打开一条先读最后这么多行；往上翻一次再多读这么多。 */
        const val PAGE_LINES = 60

        /** 等新起的会话报到多久（同桌面 `ARRIVAL_BUDGET_MS`）。 */
        const val ARRIVAL_MS = 45_000L

        /** 这边等起会话多久（`session-new` 是阻塞档：不交 `within_ms`、到点不撤单；第一句作为启动参数交，起好就回）。 */
        private const val NEW_MS = 45_000L
        private const val INPUT_MS = 20_000L

        /** 换模型 · 切档等多久读回（那台送了命令要等那一家重画 / 写记录）；到点没读回 ⇒ 回原字、说结果未知。 */
        private const val SWITCH_MS = 20_000L
        private const val READ_MS = 15_000L

        /** 改得了的拒因（画面变了 · 一时认不出）：给［再发一次］；会话已结束 · 不归这里管的 ⇒ 重试没用。 */
        private val RETRYABLE = setOf("screen_changed", "ambiguous", "not_known")
    }
}

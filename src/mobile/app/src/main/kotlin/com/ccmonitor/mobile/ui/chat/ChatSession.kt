package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.cache.Anchor
import com.ccmonitor.mobile.cache.ChatCache
import com.ccmonitor.mobile.cache.Pending
import com.ccmonitor.mobile.cache.RawPage
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
import com.ccmonitor.mobile.core.claude.link.SessionNew
import com.ccmonitor.mobile.core.claude.link.SessionSay
import com.ccmonitor.mobile.core.claude.link.SessionWatch
import com.ccmonitor.mobile.core.claude.link.TerminalInput
import com.ccmonitor.mobile.core.claude.link.wire.Held
import com.ccmonitor.mobile.core.claude.link.wire.LineRecord
import com.ccmonitor.mobile.core.claude.link.wire.NewSessionPick
import com.ccmonitor.mobile.core.claude.link.wire.RecordKind
import com.ccmonitor.mobile.core.claude.link.wire.SessionCommands
import com.ccmonitor.mobile.core.claude.link.wire.TailHow
import com.ccmonitor.mobile.core.claude.model.DeliveryState
import com.ccmonitor.mobile.core.claude.model.RecordUnits
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.Draft
import com.ccmonitor.mobile.core.data.db.PendingOutcome
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import com.ccmonitor.mobile.ui.chat.newsession.NewSessionPicker
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.onSubscription
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import java.util.UUID

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
    /** 新建会话还没发第一条（或起不来、回到草稿）：选项那两排 chip 在。 */
    val drafting: Boolean = false,
    /** 这条会话的工作目录（在它里面点「新建会话」⇒ 新的那张用它）；还不知道 ⇒ `null`。 */
    val cwd: String? = null,
    /** 上次没发的草稿（本地存的）：输入框还空着时填进去；没有 ⇒ `null`。 */
    val draft: String? = null,
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
    /** 此刻不能换：那台写好的那一句（会话帧的 `switch`；那台离线 ⇒ 离线那一句）；能换 ⇒ `null`。 */
    val blocked: String? = null,
)

/** 两个 chip 抽屉里列的：那台 `session-new-facts` 给这条会话那一家的模型与权限档；问不到 ⇒ [error] 是那一句。 */
data class Picks(
    val models: List<NewSessionPick> = emptyList(),
    val modes: List<NewSessionPick> = emptyList(),
    /** 模型抽屉第一项「默认」（那台给的值与字）；那一家没有 ⇒ `null`。 */
    val modelDefault: NewSessionPick? = null,
    /** 模型抽屉底那一行灰字（那台写的）；没有 ⇒ `null`。 */
    val modelNote: String? = null,
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
        /** 从哪进来的那个目录（在一条会话里点「新建会话」⇒ 那条的目录）；没有 ⇒ `null`（那台最近的第一个 · `~`，见 [NewSessionPicker]）。 */
        val cwd: String?,
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
    /** 这台的本地缓存（记录页 · 续点 · 待发 · 草稿）：先上它、再跟那台对齐。 */
    private val cache: ChatCache = ChatCache.None,
) {
    private val _state = MutableStateFlow(ChatUiState(sid = (target as? ChatTarget.Existing)?.sid, drafting = target is ChatTarget.Draft))
    val state: StateFlow<ChatUiState> = _state.asStateFlow()

    private val draft = target as? ChatTarget.Draft
    private val chips = ChatChips(link, machine, scope, _state) { sid }

    /** 新建会话那一张的选项（机器以外的那几样：agent · 账号 · 轮换 · 目录）；已有的那条 ⇒ `null`。 */
    val picker: NewSessionPicker? = draft?.let { NewSessionPicker(link, machine, it.cwd, it.agent, scope) }
    private val records = mutableListOf<LineRecord>()
    private val outgoing = mutableListOf<Outgoing>()
    private val lock = Mutex()

    @Volatile private var sid: String? = (target as? ChatTarget.Existing)?.sid
    private var path: String? = null

    /** 已读到的最早一行的字节起点；已有的最新一行之后那一行的行号；尾段要了几行。 */
    private var head = 0L
    private var nextSeq = 0L
    private var tailLines = PAGE_LINES

    /** 已有最新一行之后的字节（续读从这里接）；不知道 ⇒ `null`（下次从头读）。手上的记录是哪份文件的（[anchorPath]）· 那台给的记号（[anchorMark]，原样交回）。 */
    private var anchorOffset: Long? = null
    private var anchorPath: String? = null
    private var anchorMark: String = ""

    /** 存草稿那一下（停手一会儿才存）：发出去之前先撤掉，免得删了的草稿又被写回来。 */
    private var draftSave: Job? = null

    /** 在屏上 ⇒ 这一条在名单里（[holding]）。sid 还没有（新建会话起好之前）⇒ 起好再进。 */
    @Volatile private var shown = false
    private var holding: AutoCloseable? = null

    private var follow: Job? = null
    private val board = LiveBoard()
    private var starting: Job? = null

    private class Outgoing(
        val localId: String,
        val text: String,
        var delivery: DeliveryState?,
        var deliveryText: String?,
        var attempts: Int = 1,
    ) {
        /** 这一句已随 `session-new` 交出去了（结局已标上）。 */
        var handed = false
    }

    /** 开屏：先填上次没发的草稿；已有的那条先上缓存、再读最后一屏、跟上；新建会话什么都不问那台。 */
    fun open() {
        if (!draftAsked) {
            draftAsked = true
            scope.launch { cache.draft(draftKey())?.let { _state.value = _state.value.copy(draft = it) } }
        }
        if (!succeeding) {
            succeeding = true
            scope.launch { link.table.collect { t -> sid?.let { t.successors[it] }?.let { by -> succeed(by) } } }
        }
        val s = sid ?: return
        if (follow == null) follow = scope.launch { attach(s) }
    }

    private var draftAsked = false
    private var succeeding = false

    /**
     * 这条会话被顶替了（清空之后同一个进程另起一份记录，那台在 `session_removed.by` 里说是哪一条）⇒ 跟过去：换 sid、从新记录读起；
     * 旧的那条照旧在会话列表里（已结束）。
     */
    private suspend fun succeed(by: String) {
        lock.withLock {
            if (sid == by) return
            follow?.cancel()
            attached = false
            syncHold()
            sid = by
            path = null
            head = 0L
            nextSeq = 0L
            tailLines = PAGE_LINES
            anchorOffset = null
            anchorPath = null
            records.clear()
            board.clear()
            _state.value = _state.value.copy(sid = by, live = emptyList(), canLoadOlder = false, historyError = null, failedWhy = null)
            publishLocked()
            follow = scope.launch { attach(by, fresh = true) }
        }
    }

    /** 草稿存在哪一格：这条会话；新建会话还没起成 ⇒ 新建会话那一张。 */
    private fun draftKey(): String = sid ?: Draft.NEW

    /** 输入框里的字变了：停手一会儿存下（空了 ⇒ 删）。 */
    fun editDraft(text: String) {
        draftSave?.cancel()
        val key = draftKey()
        draftSave =
            scope.launch {
                delay(DRAFT_SAVE_MS)
                val c = picker?.takeIf { sid == null }?.chosen()
                cache.saveDraft(key, text, c?.cwd, c?.account)
            }
    }

    fun send(text: String) {
        if (text.isBlank()) return
        draftSave?.cancel()
        val o = Outgoing("local-${UUID.randomUUID()}", text, DeliveryState.SENDING, copyText("mobChat.delivery.sending"))
        outgoing += o
        publish()
        val key = draftKey()
        scope.launch {
            // 草稿变待发：删草稿与落待发同一个事务。
            cache.promote(key, Pending(o.localId, sid, o.text, PendingOutcome.PENDING, o.attempts))
            deliver(o)
        }
    }

    /** 再发一次没送到的那一句（只有能改的原因才有这颗）。 */
    fun retry(localId: String) {
        val o = outgoing.firstOrNull { it.localId == localId && it.delivery == DeliveryState.FAILED } ?: return
        o.delivery = DeliveryState.SENDING
        o.deliveryText = copyText("mobChat.delivery.sending")
        o.attempts++
        publish()
        scope.launch {
            cache.settle(o.localId, PendingOutcome.PENDING, o.attempts)
            deliver(o)
        }
    }

    /** 换模型（见 [ChatChips.switchModel]）。 */
    fun switchModel(pick: NewSessionPick) = chips.switchModel(pick)

    fun switchPermission(pick: NewSessionPick) = chips.switchPermission(pick)

    fun loadPicks() =
        chips.loadPicks(
            sid?.let {
                link.table.value.sessions[it]
                    ?.agentKind
            } ?: draft?.agent,
        )

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
        val s = sid ?: return
        if (_state.value.loadingOlder || head <= 0L) return
        _state.value = _state.value.copy(loadingOlder = true, historyError = null)
        scope.launch {
            val want = tailLines + PAGE_LINES
            val got =
                ask(HistoryTail.COMMAND, HistoryTail.args(p, want))?.let { data ->
                    when (val t = HistoryTail.of(data)) {
                        null -> null.also { readFailed(unreadableReply(machine, HistoryTail.COMMAND, data.toString()).text) }
                        else -> readRange(p, t.splitAt, head, data)?.let { t to it }
                    }
                }
            lock.withLock {
                if (got == null) {
                    _state.value = _state.value.copy(loadingOlder = false)
                    return@withLock
                }
                val (t, read) = got
                tailLines = want
                head = t.splitAt
                records.addAll(0, read.records)
                cache.prepend(s, read.pages)
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
        scope.launch { cache.settle(o.localId, outcomeOf(delivery), o.attempts) }
    }

    /** 气泡下的标 ⇒ 落盘的那四种结局之一（照 `session-say` 的回包）。 */
    private fun outcomeOf(d: DeliveryState?): String =
        when (d) {
            null -> PendingOutcome.DELIVERED
            DeliveryState.SENDING -> PendingOutcome.PENDING
            DeliveryState.UNSURE -> PendingOutcome.UNSURE
            DeliveryState.FAILED, DeliveryState.FAILED_PERMANENT -> PendingOutcome.REFUSED
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
        _state.value = _state.value.copy(failedWhy = null, drafting = false)
        val before = link.table.value.sessions.keys
        // 选项打开时问过那台（起哪一家 · 默认号 · 最近的目录）；问不到 ⇒ 照存的那一家、从哪进来的目录交，认不出由那台那一句说。
        val p = picker ?: return
        p.settled()
        val c = p.chosen()
        val args = SessionNew.args(c.cwd, d.ticket, c.agent, first.text, c.account, c.rotation)
        val answer =
            when (val r = link.call(SessionNew.COMMAND, args, NEW_MS)) {
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
        cache.bind(outgoing.map { it.localId }, arrived)
        _state.value = _state.value.copy(sid = arrived, cwd = answer.cwd)
        syncHold()
        follow = scope.launch { attach(arrived, fresh = true) }
    }

    /** 会话那一层没成：那一句上屏；新建会话还没起成 ⇒ 回到草稿（选项回来，改了再发）。 */
    private fun failSession(text: String?) {
        _state.value = _state.value.copy(failedWhy = text, drafting = draft != null && sid == null)
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

        /** 那份记录被那台从头重读了（会话表里那一条换了一代）：读过的那一代作废。 */
        data object Reread : Event
    }

    /** 先订上这条会话的 `line` 帧与 `from`（攒着），再读最后一屏；读完按行号接上攒着的，之后来一样接一样。[fresh]：刚起的那条（见 [pathOf]）。 */
    private suspend fun attach(
        s: String,
        fresh: Boolean = false,
    ) = coroutineScope {
        val events = Channel<Event>(Channel.UNLIMITED)
        val subscribed = CompletableDeferred<Unit>()
        val fromSubscribed = CompletableDeferred<Unit>()
        launch {
            link.lines.onSubscription { subscribed.complete(Unit) }.collect { if (it.sid == s) events.send(Event.Line(it)) }
        }
        launch {
            watch.from.onSubscription { fromSubscribed.complete(Unit) }.collect { if (it.sid == s) events.send(Event.From(it.seq)) }
        }
        subscribed.await()
        fromSubscribed.await()
        // 先上缓存（离线也看得到上次那几条），再跟那台对齐；刚起的那条没有可上的。
        if (!fresh) lock.withLock { restoreLocked(s) }
        cache.viewed(s)
        launch { link.epoch.drop(1).collect { events.send(Event.Reconnected) } }
        launch {
            link.table
                .map { it.generation(s) }
                .distinctUntilChanged()
                .drop(1)
                .collect { events.send(Event.Reread) }
        }
        followLive(s)
        attached = true
        syncHold()
        watchRunning(s)
        with(chips) { watch(s) }
        val p = recordPath(s, fresh) ?: return@coroutineScope
        path = p
        launch { chips.readFacts(p) }
        // 读没成（那一句已上屏）⇒ 等换了一条流再整份重读；成了之后断开再接上 ⇒ 从断处补。
        var loaded = lock.withLock { reloadLocked(p) }

        suspend fun take(e: Event) =
            lock.withLock {
                if (!loaded) {
                    if (e == Event.Reconnected || e == Event.Reread) loaded = reloadLocked(p)
                } else if (stepLocked(p, e)) {
                    publishLocked()
                }
            }
        for (e in events) take(e)
    }

    /**
     * 这条会话的记录文件。上次读过的那份（续点里记着）⇒ 照它接，不先问清单：离线时也挂得上，网回来（换了一条流）就整份对齐；
     * 那样接上的，目录另问一次清单，问不到不说（正文那一侧自己会说断开了）。
     */
    private suspend fun recordPath(
        s: String,
        fresh: Boolean,
    ): String? {
        if (fresh) return freshPath(s)
        val p =
            link.table.value.sessions[s]
                ?.path ?: anchorPath ?: pathOf(s) ?: return null
        if (_state.value.cwd == null) {
            link.table.value.sessions[s]
                ?.cwd
                ?.let { _state.value = _state.value.copy(cwd = it) }
        }
        if (_state.value.cwd == null && p == anchorPath) scope.launch { cwdOf(s) }
        return p
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
            // 换了一条流：交手上那一份问一次，断开那一段怎么补那台判（续得上就只读那一截）。
            Event.Reconnected -> reloadLocked(p)
            // 从头重读了：手上那一份不交（续点作废），读最后一屏、整份重来（同桌面那一 tab 整份重来）。
            Event.Reread -> {
                anchorOffset = null
                reloadLocked(p)
            }
        }

    /**
     * 上次读到的（缓存里的页原样，交给与线上同一个解码器）先上屏 ＋ 续点；上次没送完的那几句照它们的结局挂回去。
     * 有一页解不出 ⇒ 当没有缓存（照那台重读）。
     */
    private suspend fun restoreLocked(s: String) {
        restorePendingLocked(s)
        val cached = cache.load(s) ?: return publishLocked()
        val got = cached.pages.map { HistoryRead.decodeRows(it.rows) ?: return publishLocked() }.flatten()
        records.clear()
        records.addAll(got)
        head = cached.pages.first().start
        cached.anchor?.let {
            anchorPath = it.path
            anchorOffset = it.offset
            anchorMark = it.mark
            nextSeq = it.seq
        }
        _state.value = _state.value.copy(canLoadOlder = head > 0L)
        publishLocked()
    }

    /** 待发里这条会话的那几句：送没送到照落盘那一格；上次还在等回包就没了 ⇒ 不知道送没送到（别重发）。 */
    private suspend fun restorePendingLocked(s: String) {
        val known = outgoing.map { it.localId }.toSet()
        for (p in cache.pending(s)) {
            if (p.id in known) continue
            val (d, text) =
                when (p.outcome) {
                    PendingOutcome.DELIVERED -> null to null
                    PendingOutcome.REFUSED -> failed(null)
                    else -> unsure()
                }
            outgoing += Outgoing(p.id, p.text, d, text, p.attempts)
        }
    }

    /** 一行：比已有的旧 ⇒ 重叠、不要；比已有的新、中间缺着 ⇒ 先补上。末页与续点跟着落盘。 */
    private suspend fun lineLocked(
        p: String,
        line: RecordLine,
    ): Boolean {
        if (line.seq > nextSeq && !fillLocked(p, line.seq)) return false
        if (line.seq < nextSeq) return false
        line.record?.let { records += it }
        nextSeq = line.seq + 1
        anchorOffset = line.byteOffset
        sid?.let { s -> cache.append(s, listOf(HistoryRead.row(line.byteOffset, line.raw)), line.byteOffset?.let { Anchor(p, it, nextSeq, anchorMark) }) }
        return true
    }

    /**
     * 对齐那台：交手上那一份（续点）问 `history-tail`，怎么接那台判、第一页随回包来。续得上 ⇒ 从续点往下读到 `end`、接在后面；
     * 没交 · 变短 · 被改写 ⇒ 读最后一屏（[tailLines] 行）、整份重来；缺得比一屏多 ⇒ 那台不读那一截，这里读最后一屏、整份重来。
     * 没读成 ⇒ `false`（那一句已上屏），已有的不动。
     */
    private suspend fun reloadLocked(p: String): Boolean {
        val s = sid ?: return false
        val held = anchorOffset?.takeIf { records.isNotEmpty() }?.let { o -> anchorPath?.let { Held(it, nextSeq, o, anchorMark) } }
        val data = ask(HistoryTail.COMMAND, HistoryTail.args(p, tailLines, held)) ?: return false
        val t = HistoryTail.of(data) ?: return false.also { readFailed(unreadableReply(machine, HistoryTail.COMMAND, data.toString()).text) }
        val anchor = Anchor(t.held.path, t.held.nextByte, t.held.next, t.held.mark)
        val ok =
            when {
                t.how == TailHow.RESUME && held != null ->
                    readRange(p, held.nextByte, t.end, data)?.also { read ->
                        records.addAll(read.records)
                        cache.append(s, read.pages.flatMap { it.rows }, anchor)
                    } != null
                // 缺得多那一形那台没读那一截：最后一屏这里自己读。
                else ->
                    readRange(p, t.splitAt, t.end, data.takeIf { t.how != TailHow.HANDOFF })?.also { read ->
                        head = t.splitAt
                        records.clear()
                        records.addAll(read.records)
                        // 一页都没有（尾段是空的）也记一页空的：页锚留着，下次先上缓存时「还能往上翻」照样对。
                        cache.replace(s, read.pages.ifEmpty { listOf(RawPage(t.splitAt, t.end, emptyList())) }, anchor)
                    } != null
            }
        if (ok) {
            nextSeq = anchor.seq
            anchorOffset = anchor.offset
            anchorPath = anchor.path
            anchorMark = anchor.mark
            _state.value = _state.value.copy(canLoadOlder = head > 0L, failedWhy = null, historyError = null)
            publishLocked()
        }
        return ok
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
            // 续点跟到这一段最后一个可计行之后；每格带它那一行之后的字节落末页。
            anchorOffset = page.end
            val anchor = anchorPath?.takeIf { it == p }?.let { Anchor(it, page.end, nextSeq, anchorMark) }
            sid?.let { s -> cache.append(s, page.raw.mapIndexed { i, r -> HistoryRead.row(page.ends.getOrNull(i), r) }, anchor) }
        }
        nextSeq = maxOf(nextSeq, until)
        return true
    }

    /**
     * 这条会话的活卡：只收它主运行的（子运行的卡不进主线，同桌面主 tab）。换了一条流 ⇒ 这条连接上的号全作废、全撤，
     * 那台在新的那条上给名单里的会话补整卡。
     */
    private fun CoroutineScope.followLive(s: String) {
        launch {
            link.live.collect { f ->
                if (f.sid == s && board.apply(f)) _state.value = _state.value.copy(live = board.cardsOf(s))
            }
        }
        launch {
            link.epoch.drop(1).collect { if (board.clear()) _state.value = _state.value.copy(live = emptyList()) }
        }
    }

    private fun CoroutineScope.watchRunning(s: String) {
        launch {
            link.table.collect { t ->
                // 只有那台判得出「在跑」才亮停止键；判不出（说不清 · 读记录中）不亮。
                val running = t.sessions[s]?.running == true
                if (running != _state.value.running) _state.value = _state.value.copy(running = running)
            }
        }
    }

    /** 这条会话的目录（清单那一行的 `projectPath`）；问不到就算了。 */
    private suspend fun cwdOf(s: String) {
        val r = link.call(HistoryList.COMMAND, null, READ_MS) as? Reply.Ok ?: return
        HistoryList.of(r.data)?.rows?.firstOrNull { it.sessionId == s }?.projectPath?.let { cwd ->
            if (_state.value.cwd == null) _state.value = _state.value.copy(cwd = cwd)
        }
    }

    /** 那条会话的记录文件：会话表里有就用它，没有（不在跑了）问 `history-list`。 */
    private suspend fun pathOf(s: String): String? {
        link.table.value.sessions[s]?.let { live ->
            live.cwd?.let { _state.value = _state.value.copy(cwd = it) }
            live.path?.let { return it }
        }
        return when (val r = link.call(HistoryList.COMMAND, null, READ_MS)) {
            is Reply.Ok -> {
                val list = HistoryList.of(r.data) ?: return null.also { failSession(unreadableReply(machine, HistoryList.COMMAND, r.data.toString()).text) }
                val row = list.rows.firstOrNull { it.sessionId == s }
                row?.projectPath?.let { if (_state.value.cwd == null) _state.value = _state.value.copy(cwd = it) }
                row?.jsonlPath ?: null.also { failSession(copyText("mobChat.record.missing", "machine" to machine)) }
            }
            else -> null.also { failSession(r.problem(machine, HistoryList.COMMAND)?.text) }
        }
    }

    /** 读到的一段：记录 ＋ 每一问的 `rows[]` 原样（一问一页，缓存存它）。 */
    private class Read(
        val records: List<LineRecord>,
        val pages: List<RawPage>,
    )

    /** 问一次；没成 ⇒ 那一句上屏、`null`。 */
    private suspend fun ask(
        cmd: String,
        args: Map<String, Any?>,
    ): Any? =
        when (val r = link.call(cmd, args, READ_MS)) {
            is Reply.Ok -> r.data
            else -> null.also { readFailed(r.problem(machine, cmd)?.text) }
        }

    /** 一页回包里 [until] 之前的那几行：记录 ＋ `rows[]` 原样（缓存存它）；形状不对 ⇒ `null`。 */
    private class Cut(
        val records: List<LineRecord>,
        val rows: List<Any?>,
        val next: Long,
        val eof: Boolean,
    )

    private fun cut(
        data: Any?,
        until: Long,
    ): Cut? {
        val page = HistoryRead.of(data) ?: return null
        val rows = HistoryRead.rawRows(data)?.filter { ((it as? Map<*, *>)?.get("end") as? Number)?.toLong()?.let { e -> e <= until } ?: true } ?: return null
        return HistoryRead.decodeRows(rows)?.let { Cut(it, rows, page.next, page.eof) }
    }

    /**
     * 读 `[from, until)` 里的记录，一页不够就接着读。[first] 是已经拿到的第一页（`history-tail` 随回包带来的，从 [from] 起）；
     * 那一页越过 [until] 的几行不要（往上翻时第一页是整个尾段的）。
     */
    private suspend fun readRange(
        p: String,
        from: Long,
        until: Long,
        first: Any? = null,
    ): Read? {
        val out = mutableListOf<LineRecord>()
        val pages = mutableListOf<RawPage>()
        var at = from
        var given = first
        while (at < until) {
            val data = given ?: ask(HistoryRead.COMMAND, HistoryRead.args(p, at, until)) ?: return null
            given = null
            val page = cut(data, until) ?: return null.also { readFailed(unreadableReply(machine, HistoryRead.COMMAND, data.toString()).text) }
            out += page.records
            pages += RawPage(at, minOf(page.next, until), page.rows)
            if (page.eof || page.next <= at) break
            at = page.next
        }
        return Read(out, pages)
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
        val seen = outgoing.filter { o -> o.delivery != DeliveryState.FAILED && o.delivery != DeliveryState.FAILED_PERMANENT && said.remove(o.text) }
        if (seen.isNotEmpty()) {
            outgoing.removeAll(seen)
            scope.launch { cache.forget(seen.map { it.localId }) }
        }
        val local = outgoing.map { RenderUnit.UserText(it.localId, it.text, null, it.delivery, it.deliveryText) }
        _state.value = _state.value.copy(units = RecordUnits.of(records) + local)
    }

    companion object {
        /** 打开一条先读最后这么多行；往上翻一次再多读这么多。 */
        const val PAGE_LINES = 60

        /** 输入框停手这么久才存草稿。 */
        private const val DRAFT_SAVE_MS = 500L

        /** 等新起的会话报到多久（同桌面 `ARRIVAL_BUDGET_MS`）。 */
        const val ARRIVAL_MS = 45_000L

        /** 这边等起会话多久（`session-new` 是阻塞档：不交 `within_ms`、到点不撤单；第一句作为启动参数交，起好就回）。 */
        private const val NEW_MS = 45_000L
        private const val INPUT_MS = 20_000L

        private const val READ_MS = 15_000L

        /** 改得了的拒因（画面变了 · 一时认不出）：给［再发一次］；会话已结束 · 不归这里管的 ⇒ 重试没用。 */
        private val RETRYABLE = setOf("screen_changed", "ambiguous", "not_known")
    }
}

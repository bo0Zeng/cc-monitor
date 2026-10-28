package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.cache.ChatCache
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionArrive
import com.ccmonitor.mobile.core.claude.link.SessionInterrupt
import com.ccmonitor.mobile.core.claude.link.SessionOpen
import com.ccmonitor.mobile.core.claude.link.SessionWatch
import com.ccmonitor.mobile.core.claude.link.TerminalInput
import com.ccmonitor.mobile.core.claude.link.canStop
import com.ccmonitor.mobile.core.claude.link.generation
import com.ccmonitor.mobile.core.claude.model.DeliveryState
import com.ccmonitor.mobile.core.claude.model.RecordUnits
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.claude.model.StreamItem
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.Draft
import com.ccmonitor.mobile.core.data.db.PendingOutcome
import com.ccmonitor.mobile.core.wire.gen.AgentPick
import com.ccmonitor.mobile.core.wire.gen.ArriveEnd
import com.ccmonitor.mobile.core.wire.gen.Changed
import com.ccmonitor.mobile.core.wire.gen.LineRecord
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.LiveCardNow
import com.ccmonitor.mobile.core.wire.gen.SentAct
import com.ccmonitor.mobile.core.wire.gen.SessionCommands
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import com.ccmonitor.mobile.ui.chat.attach.AttachTray
import com.ccmonitor.mobile.ui.chat.attach.handOver
import com.ccmonitor.mobile.ui.chat.newsession.NewSessionPicker
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onSubscription
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.util.UUID

/** 聊天面的一帧 UI 状态。 */
data class ChatUiState(
    /** 记录 ＋ 会话那台已回执、还没在记录里出现的那几句。 */
    val units: List<RenderUnit> = emptyList(),
    /** 输入框上方那一排：交出去了、会话那台还没回执的那几句（「发送中」· 撤）。 */
    val outbox: List<OutboxLine> = emptyList(),
    /** 消息流末尾那一排：会话那台那份输入队名单（哪一端投的都在；自己的回执到了就挪进这里）· 每条可撤。 */
    val queue: List<QueueLine> = emptyList(),
    /** 这条会话此刻在跑（那台给的脸上在干什么是 `working`）：右端是 ■。 */
    val running: Boolean = false,
    /** 会话那一层的失败（起不来 · 读不出记录）那一句；`null` ＝ 没有。 */
    val failedWhy: String? = null,
    val canLoadOlder: Boolean = false,
    val loadingOlder: Boolean = false,
    /** 往上翻没读成的那一句（不崩页面、再上滑一次重试）。 */
    val historyError: String? = null,
    /** 已经起成、认出来的那条会话；新建会话发第一条之前是 `null`。 */
    val sid: String? = null,
    /** 这条会话主运行此刻的活卡（核心折好的整份照画；记录到了那台撤卡）。 */
    val live: List<LiveCardNow> = emptyList(),
    /** 模型 chip。 */
    val model: SwitchChip = SwitchChip(),
    /** 权限档 chip。 */
    val permission: SwitchChip = SwitchChip(),
    /** chip 上方那一行（换的结局没成）：那台写好的那一句；`null` ＝ 不画。 */
    val note: String? = null,
    /** 两个 chip 抽屉里列的（那台那一家给的）；还没问 ⇒ `null`。 */
    val picks: Picks? = null,
    /** ［+］面板里的命令与 Skill（那台给的这条会话那一份）。 */
    val commands: CommandsView = CommandsView.Idle,
    /** 新建会话还没发第一条（或起不来、回到草稿）：选项那两排 chip 在。 */
    val drafting: Boolean = false,
    /** 这条会话的工作目录（在它里面点「新建会话」⇒ 新的那张用它）；还不知道 ⇒ `null`。 */
    val cwd: String? = null,
    /** 叫这条会话什么（那台定的：会话簿那一格，状态帧来了就换）；簿里还没有它 ⇒ `null`。 */
    val label: String? = null,
    /** 上次没发的草稿（本地存的）：输入框还空着时填进去；没有 ⇒ `null`。 */
    val draft: String? = null,
    /** 编辑已发的那几句改到一半的字（记录里那一句的 uuid ⇒ 字，本地存的）：再长按编辑那一句时填进去。 */
    val editDrafts: Map<String, String> = emptyMap(),
    /** 从一处命中打开的落点（那张卡的 key：滚到它、折着就展开）；不是从命中打开的 · 那张卡还没读到 ⇒ `null`。 */
    val landing: String? = null,
    /** 流里画的那几项：[units] 照那台 `history-turns` 的分段折好（[ChatTurns]）；分段还没问到 · 问不到 ⇒ 逐条摊开。 */
    val stream: List<StreamItem> = emptyList(),
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
    val pending: AgentPick? = null,
    /** 此刻不能换：那台写好的那一句（会话帧的 `switch`；那台离线 ⇒ 离线那一句）；能换 ⇒ `null`。 */
    val blocked: String? = null,
)

/** 两个 chip 抽屉里列的：那台 `session-open-facts` 给这条会话那一家的模型与权限档；问不到 ⇒ [error] 是那一句。 */
data class Picks(
    val models: List<AgentPick> = emptyList(),
    val modes: List<AgentPick> = emptyList(),
    /** 模型抽屉第一项「默认」（那台给的值与字）；那一家没有 ⇒ `null`。 */
    val modelDefault: AgentPick? = null,
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
    ) : ChatTarget
}

/**
 * 一条会话的聊天状态。起会话、送字、正文都走那台的核心（[CoreLink]）：
 * - 新建会话：第一条按 ➤ ⇒ `session-open`（在那台 tmux 里起，桌面那一侧看得到、接得上），那一句随起交（`prompt`：那台作为启动参数交给 agent，
 *   作为启动参数交，送没送到看回包）；那台没交（回包没有 `prompt`）⇒ 等它报到（会话表里那个目录新出现的一条）再经 `terminal-input` 送。
 *   新起的会话记录要等第一句进去才落盘 ⇒ 读正文之前先等那台推来它的记录路径（[freshPath]）。
 * - 送字：投进那条会话的输入队（`session-queue-put`，编号去重，回执才算出去，见 [ChatOutbox]）；停：`session-interrupt`（不进队、不靠持有输入，没成照那台那一句）。
 * - 正文：`history-tail` ＋ `history-read` 读最后一屏，之后跟流上这条会话的 `line` 帧；往上翻再读上一段（读到哪 · 怎么读归 [ChatRecord]）。记录 ⇒ 渲染单元见 [RecordUnits]。
 *   从搜内容的一处命中打开（[jump]）⇒ 先读命中那一页，再往下读到尾、尾段就从命中那一行起，之后同上。
 * - 在屏上（[visible]）才进这台的 `stream-watch` 名单；进名单 · 断线重接之后，`[已有, seq)` 那一段按行号用 `history-lines` 补上
 *   （`from[]` 给的 seq，或来了一行比已有的新、中间缺着），缺得多就整份重读最后一屏；重叠处按行号去重。
 * 手机这边只记「送出去、记录里还没出现」的那几句；记录里出现了同一句人话 ⇒ 那一句的标消失。
 * - 附件：输入框上方那一排（[attach]，`attachOf` 造，给它这条会话的作用域与 sid；不给 ⇒ 没有那一排）只给已有的那条
 *   （落附件要 sid 认工作目录，起会话那一问还不收附件）。➤ 照 [handOver]：全落好了才发，编号随那一句交、重发照带。
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
    attachOf: ((CoroutineScope, () -> String?) -> AttachTray)? = null,
    /** 那台推来的 `changed` 帧（这一条会话的输入队名单从这里来）。 */
    private val queueMoved: Flow<Changed> = emptyFlow(),
) {
    private val _state = MutableStateFlow(ChatUiState(sid = (target as? ChatTarget.Existing)?.sid, drafting = target is ChatTarget.Draft))
    val state: StateFlow<ChatUiState> = _state.asStateFlow()

    private val draft = target as? ChatTarget.Draft
    private val chips = ChatChips(link, machine, scope, _state) { sid }
    val attach: AttachTray? = attachOf?.takeIf { draft == null }?.invoke(scope) { sid }

    /** 新建会话那一张的选项（机器以外的那几样：agent · 账号 · 轮换 · 目录）；已有的那条 ⇒ `null`。 */
    val picker: NewSessionPicker? = draft?.let { NewSessionPicker(link, machine, it.cwd, scope) }
    private val body = RecordUnits()

    /** 读正文的那几问；没成的那一句照 [readFailed] 上屏。 */
    private val reads = ChatReads(link, machine) { readFailed(it) }

    /** 上次上屏以后新排进来的那几条里人说的话（只拿它们消本地那几句的标）。 */
    private val heard = mutableListOf<String>()
    private val box = ChatOutbox(link, machine, scope, cache, attach) { publish() }

    /** 这条会话的输入队（会话那台那一份）。 */
    private val queue = ChatQueue(link, scope) { publish() }
    private val outgoing get() = box.lines
    private val lock = Mutex()

    @Volatile private var sid: String? = (target as? ChatTarget.Existing)?.sid

    /** 存草稿那一下（停手一会儿才存）：发出去之前先撤掉，免得删了的草稿又被写回来。 */
    private var draftSave: Job? = null

    /** 在屏上 ⇒ 这一条在名单里（[holding]）。sid 还没有（新建会话起好之前）⇒ 起好再进。 */
    @Volatile private var shown = false
    private var holding: AutoCloseable? = null

    private var follow: Job? = null
    private var starting: Job? = null

    /** 过程怎么分段（那台 `history-turns`）· 点开着的那几段。 */
    private val folds = ChatTurns(link)

    /** 正文跟读（读到哪 · 读最后一屏 · 补缺 · 往上翻 · 从命中读起）；排进来的记录里人说的话记进 [heard]。 */
    private val record =
        ChatRecord(
            link,
            machine,
            scope,
            lock,
            cache,
            reads,
            body,
            _state,
            { sid },
            took = { rs ->
                rs.filterIsInstance<LineRecord.Said>().mapTo(heard) { it.who.text }
                folds.want()
            },
            publishLocked = ::publishLocked,
            failSession = ::failSession,
            readFailed = ::readFailed,
            landed = { landingKey = it },
        )

    /** 开屏：先填上次没发的草稿；已有的那条先上缓存、再读最后一屏、跟上；新建会话什么都不问那台。 */
    fun open() {
        if (!draftAsked) {
            draftAsked = true
            scope.launch { cache.draft(draftKey())?.let { _state.value = _state.value.copy(draft = it) } }
            sid?.let { s -> scope.launch { edits.load(s) } }
        }
        attach?.open()
        if (!succeeding) {
            succeeding = true
            scope.launch { link.table.collect { t -> sid?.let { t.successors[it] }?.let { by -> succeed(by) } } }
            // 那台推来的输入队名单（这一条的）照画。
            queue.watch(queueMoved) { sid }
            // 每次接上那台：待发里还没回执的那几句照同一个编号再交一遍（那台按编号去重，只进一条）；名单问一次（断着时漏了的推送补上）。
            scope.launch {
                link.state
                    .map { it is LinkLine.Up }
                    .distinctUntilChanged()
                    .collect { up ->
                        if (up) {
                            resend()
                            sid?.let { queue.read(it) }
                        }
                    }
            }
        }
        val s = sid ?: return
        if (follow == null) follow = scope.launch { attach(s) }
    }

    private var draftAsked = false
    private var succeeding = false

    /** 下一次挂上从哪一处读起（[jump] 交的命中那一行的字节起点）；`null` ⇒ 读最后一屏。 */
    @Volatile private var jumpAt: Long? = null

    /** 落点：命中那一页的第一张卡（[landingKey]）；落在一次工具调用的卡里 ⇒ 那张工具卡（[landingTool]，往上翻读到它那一行才有）。 */
    private var landingKey: String? = null
    private var landingTool: String? = null

    /**
     * 从搜内容的一处命中打开：[offset] 是命中那条记录那一行的字节起点，[tool] 是它落在哪次工具调用的卡里（缺 ⇒ 它自己那张卡）。
     * 还没挂上 ⇒ 挂上时从那里读；已经挂着 ⇒ 撤掉重挂一次，从那里读（见 [jumpLocked]）。
     */
    fun jump(
        offset: Long,
        tool: String?,
    ) {
        val s = sid ?: return
        jumpAt = offset
        landingTool = tool
        landingKey = null
        val old = follow ?: return
        old.cancel()
        attached = false
        syncHold()
        follow =
            scope.launch {
                old.join()
                attach(s)
            }
    }

    /**
     * 这条会话被顶替了（清空之后同一个进程另起一份记录，那台在离开那一帧的 `face.life.by` 里说是哪一条）⇒ 跟过去：换 sid、从新记录读起；
     * 旧的那条照旧在会话列表里（已结束）。
     */
    private suspend fun succeed(by: String) {
        lock.withLock {
            if (sid == by) return
            follow?.cancel()
            attached = false
            syncHold()
            sid = by
            record.reset()
            landingKey = null
            landingTool = null
            folds.forget(all = true)
            body.reset(emptyList())
            queue.reset()
            _state.value = _state.value.copy(sid = by, live = emptyList(), canLoadOlder = false, historyError = null, failedWhy = null)
            publishLocked()
            follow = scope.launch { attach(by, fresh = true) }
        }
    }

    /** 点一行过程：开 ⇄ 合。 */
    fun toggle(after: String) = underLock { folds.toggle(after) }

    /** 「跳到第一处失败」：那一段展开（记成手动开），落点是那台指的那一步那张卡。 */
    fun jumpFail(after: String) =
        underLock {
            folds.failOf(body.units(), after)?.let {
                landingKey = it
                landingTool = null
            }
        }

    private fun underLock(f: () -> Unit) {
        scope.launch {
            lock.withLock {
                f()
                publishLocked()
            }
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

    /**
     * ➤：那一句先进待发（编号 ＝ 投进队的 id，落库之后才交出去）；回执到之前不进消息流，在输入框上方「发送中」、可撤。
     * 没回执（断线 · 期限到了 · App 被杀）⇒ 留在待发，接上那台照同一个编号再交（[resend]）。
     */
    fun send(text: String) {
        val (localId, ids) = attach.handOver(text, "local-${UUID.randomUUID()}") ?: return
        draftSave?.cancel()
        val o = Outgoing(localId, text, DeliveryState.SENDING, copyText("mobChat.delivery.sending"), attachments = ids)
        outgoing += o
        publish()
        val key = draftKey()
        scope.launch {
            // 草稿变待发：删草稿与落待发同一个事务。
            box.keep(key, o, sid)
            deliver(o)
        }
    }

    /** 再发一次那台拒了的那一句（只有能改的原因才有这颗）：同一个编号。 */
    fun retry(localId: String) {
        val o = box.retrying(localId) ?: return
        publish()
        scope.launch {
            cache.settle(o.localId, PendingOutcome.PENDING, o.attempts)
            deliver(o)
        }
    }

    /**
     * 编辑已发的那几句（[SaidEdits]：草稿 · 问选项 · 确认那一下；聊天屏直接经它）。那几句进待发（确认那一下 · 开屏读回的）同普通那一句：
     * 上屏、交出去；没回执 ⇒ 接上那台照同一个编号再交（[resend]）。
     */
    internal val edits =
        SaidEdits(link, machine, cache, scope, { sid }, { os ->
            box.adopt(os)
            publish()
            resend()
        }) { _state.value = _state.value.copy(editDrafts = it) }

    /** 撤待发里一句（见 [ChatOutbox.cancel]）；是编辑的那一句 ⇒ 它的编辑草稿留着字、不再带待发编号。 */
    fun cancel(localId: String) = box.cancel(localId, sid).also { edits.unsent(localId) }

    /** 撤输入队名单里一条（哪一端投的都撤得了；照编号请会话那台撤，见 [ChatQueue.cancel]）；编辑的那一句同上。 */
    fun cancelQueued(id: String) = sid?.let { queue.cancel(id, it) }.also { edits.unsent(id) }

    /** 待发里该再交的那几句照同一个编号再交一遍。 */
    private fun resend() {
        if (sid == null) return
        box.due().forEach { o -> scope.launch { deliver(o) } }
    }

    /** 换模型（见 [ChatChips.switchModel]）。 */
    fun switchModel(pick: AgentPick) = chips.switchModel(pick)

    fun switchPermission(pick: AgentPick) = chips.switchPermission(pick)

    fun loadPicks() = chips.loadPicks(sid?.let(link.table.value.live::get)?.agentKind)

    /** ［+］面板打开（见 [ChatChips.loadCommands]）。 */
    fun loadCommands() = chips.loadCommands()

    /** ［+］面板里的一次性动作（压缩上下文 · 清空会话 · Skill）：同 ➤ 那一句一样进待发、投进那条会话的输入队。 */
    fun say(text: String) {
        if (sid == null) return
        val o = Outgoing("local-${UUID.randomUUID()}", text, DeliveryState.SENDING, copyText("mobChat.delivery.sending"))
        outgoing += o
        publish()
        scope.launch {
            box.keep(PANEL_KEY, o, sid)
            deliver(o)
        }
    }

    fun rename(title: String) = chips.rename(title)

    /** ■：中断那条会话这一轮（`session-interrupt`：不进队、不靠持有输入）；中断那一条看记录。成了 · 不知道成没成不说（看记录），被拒 ⇒ chip 上方那一行照那台那一句。 */
    fun stop() {
        val s = sid ?: return
        scope.launch {
            val r = link.call(SessionInterrupt.COMMAND, SessionInterrupt.args(s), INPUT_MS)
            val said =
                if (r !is Reply.Ok) {
                    r.problem(machine, SessionInterrupt.COMMAND, link.offline.value)?.text
                } else {
                    (TerminalInput.of(r.data) as? TerminalInput.Result.Refused)?.said
                }
            if (said != null) _state.value = _state.value.copy(note = said)
        }
    }

    /** 往上翻（见 [ChatRecord.loadOlder]）。 */
    fun loadOlder() = record.loadOlder()

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

    /** 交那一句（见 [ChatOutbox.put]）；新建会话还没起好 ⇒ 先起（头一句随起交）。 */
    private suspend fun deliver(o: Outgoing) {
        // 正在交 · 两次再交排到了同一句、前一次已拿到回执（又不是要刷新排第几）⇒ 不再交。
        if (o.inFlight || o.receipt && !o.refresh) return
        val s = sid ?: started(o)?.also { if (o.handed) return } ?: return fail(o, permanent = false, text = null)
        box.put(o, s)
        edits.landed(o)
    }

    private fun mark(
        o: Outgoing,
        delivery: DeliveryState?,
        text: String?,
    ) = box.mark(o, delivery, text)

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
        // 选项打开时问过那台（起哪一家 · 默认号 · 最近的目录）；问不到 ⇒ 照存的那一家、从哪进来的目录交，认不出由那台那一句说。
        val p = picker ?: return
        p.settled()
        val c = p.chosen()
        val args = SessionOpen.newIn(c.cwd, d.ticket, c.agent, first.text, c.account, c.rotation)
        val (answer, cwd) =
            when (val r = link.call(SessionOpen.COMMAND, args, NEW_MS)) {
                is Reply.Ok ->
                    SessionOpen.of(r.data)?.let { a -> a.cwd?.let { a to it } }
                        ?: return failSession(unreadableReply(machine, SessionOpen.COMMAND, r.data.toString()).text)
                else -> return failSession(r.problem(machine, SessionOpen.COMMAND, link.offline.value)?.text)
            }
        val arrived =
            when (val a = arrivalOf(link, machine, answer, arrivalMs)) {
                is Arrival.Sid -> a.sid
                is Arrival.Missed -> return failSession(a.said)
            }
        answer.prompt?.let { said ->
            first.handed = true
            val (delivery, text) = outcomeOf(said, machine)
            // 随起交进去了（那台作为启动参数收下）⇒ 进流；没交到 ⇒ 留在待发、能改的给［重试］（那时有 sid，走投进队）。
            first.receipt = delivery != DeliveryState.FAILED && delivery != DeliveryState.FAILED_PERMANENT
            mark(first, delivery, text)
        }
        sid = arrived
        cache.bind(outgoing.map { it.localId }, arrived)
        _state.value = _state.value.copy(sid = arrived, cwd = cwd)
        syncHold()
        follow = scope.launch { attach(arrived, fresh = true) }
    }

    /** 会话那一层没成：那一句上屏；新建会话还没起成 ⇒ 回到草稿（选项回来，改了再发）。 */
    private fun failSession(text: String?) {
        _state.value = _state.value.copy(failedWhy = text, drafting = draft != null && sid == null)
    }

    /** 先订上这条会话的 `line` 帧与 `from`（攒着），再读最后一屏；读完按行号接上攒着的，之后来一样接一样。[fresh]：刚起的那条（见 [freshPath]）。 */
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
        val at = jumpAt.also { jumpAt = null }
        // 先上缓存（离线也看得到上次那几条），再跟那台对齐；刚起的那条没有可上的。从一处命中打开 ⇒ 缓存里那一屏不上（只挂回待发）。
        if (!fresh) {
            lock.withLock {
                restorePendingLocked(s)
                record.restoreLocked(s, at)
            }
        }
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
        val (p, facts) = record.recordOf(s, fresh) ?: return@coroutineScope
        record.path = p
        followTurns(s, p)
        if (facts != null) chips.take(facts) else launch { chips.readFacts(p) }
        // 读没成（那一句已上屏）⇒ 等换了一条流再整份重读；成了之后断开再接上 ⇒ 从断处补。
        var loaded = lock.withLock { record.loadLocked(p, at) }

        suspend fun take(e: Event) =
            lock.withLock {
                if (!loaded) {
                    if (e == Event.Reconnected || e == Event.Reread) loaded = record.reloadLocked(p)
                } else if (record.stepLocked(p, e) { folds.forget() }) {
                    publishLocked()
                }
            }
        for (e in events) take(e)
    }

    /** 过程分段跟着记录问（见 [ChatTurns.follow]）：问之前取已拉到的最上面那一行，问到了在锁里接上、上屏。 */
    private fun CoroutineScope.followTurns(
        s: String,
        p: String,
    ) = folds.follow(this, p, link.table.map { it.live[s]?.face }, { record.head }) { block ->
        lock.withLock {
            if (record.path == p && block()) publishLocked()
        }
    }

    /** 待发里这条会话的那几句挂回来（见 [ChatOutbox.restore]）；那台此刻连着 ⇒ 当场照同一个编号再交。 */
    private suspend fun restorePendingLocked(s: String) {
        box.restore(s)
        if (link.state.value is LinkLine.Up) resend()
    }

    /**
     * 这条会话的活卡：只收它主运行那一处的整份（子运行的卡不进主线，同桌面主 tab）。换了一条流 ⇒ 核心交那一处空的，
     * 那台在新的那条上给名单里的会话补整卡。
     */
    private fun CoroutineScope.followLive(s: String) {
        launch {
            link.live.collect { c ->
                if (c.sid == s && c.run == null) _state.value = _state.value.copy(live = c.cards)
            }
        }
    }

    private fun CoroutineScope.watchRunning(s: String) {
        launch {
            link.table.collect { t ->
                // 只有那台判得出「在跑」才亮停止键；判不出（说不清 · 读记录中）不亮。
                val running = t.canStop(s)
                // 叫它什么照会话簿那一格（那台定的）；簿里没有 ⇒ 留着上一个。
                val label = t.live[s]?.label ?: _state.value.label
                if (running != _state.value.running || label != _state.value.label) _state.value = _state.value.copy(running = running, label = label)
            }
        }
    }

    private fun readFailed(text: String?) {
        if (body.isEmpty()) failSession(text) else _state.value = _state.value.copy(historyError = text)
    }

    private fun publish() {
        scope.launch { lock.withLock { publishLocked() } }
    }

    /** 新排进来的记录里出现了同一句人话 ⇒ 那一句的本地标消失（一行只消一句）；其余的接在记录后面。 */
    private fun publishLocked() {
        val seen = outgoing.filter { o -> o.delivery != DeliveryState.FAILED && o.delivery != DeliveryState.FAILED_PERMANENT && heard.remove(o.text) }
        heard.clear()
        if (seen.isNotEmpty()) {
            outgoing.removeAll(seen)
            scope.launch { cache.forget(seen.map { it.localId }) }
        }
        // 回执过的那一句在名单里见过、此刻不在名单也不在已送进 ⇒ 哪一端把它撤了：拿掉。
        val cancelled = outgoing.filter { it.receipt && queue.gone(it.localId) }
        if (cancelled.isNotEmpty()) {
            outgoing.removeAll(cancelled)
            scope.launch { cache.forget(cancelled.map { it.localId }) }
        }
        // 回执到了（会话那台已落盘）的才进流；还在名单里的那几句只在名单那一排（[ChatUiState.queue]）；还没回执的在输入框上方（[ChatUiState.outbox]）。
        val (local, outbox) = box.view()
        val units = body.units() + local.filterNot { queue.holds(it.key) }
        val landing = landingTool?.let { t -> units.firstOrNull { it is RenderUnit.ToolCall && it.toolUseId == t }?.key } ?: landingKey
        _state.value = _state.value.copy(units = units, outbox = outbox, queue = queue.lines(), landing = landing, stream = folds.fold(units, landing))
    }

    companion object {
        /** 打开一条先读最后这么多行；往上翻一次再多读这么多。 */
        const val PAGE_LINES = 60

        /** 输入框停手这么久才存草稿。 */
        private const val DRAFT_SAVE_MS = 500L

        /** 愿意等新起的会话报到多久（`session-arrive` 的期限，那台照它减余量等）。 */
        const val ARRIVAL_MS = 45_000L

        /** 这边等起会话多久（`session-open` 是阻塞档：不交 `within_ms`、到点不撤单；第一句作为启动参数交，起好就回）。 */
        private const val NEW_MS = 45_000L
        private const val INPUT_MS = 20_000L

        /** ［+］面板动作进待发时交的那张「草稿」键（没有这张草稿：落待发不删任何一张）。 */
        private const val PANEL_KEY = "\u0000panel"
    }
}

/** 那台判的送没送到（`terminal-input` 的回话 · `session-open` 回包里第一句的结局）⇒ 标与那一句。能不能再发一次照那台给的下一步（[SentAct.RETRY]）。 */
private fun outcomeOf(
    res: TerminalInput.Result,
    machine: String,
): Pair<DeliveryState?, String?> =
    when (res) {
        TerminalInput.Result.Delivered -> null to null
        TerminalInput.Result.Unsure -> unsure(machine)
        is TerminalInput.Result.Refused ->
            (if (SentAct.RETRY in res.acts) DeliveryState.FAILED else DeliveryState.FAILED_PERMANENT) to
                copyText("terminal.input.failed", "why" to res.said)
    }

private fun unsure(machine: String) = DeliveryState.UNSURE to copyText("terminal.input.unsure", "machine" to machine)

/** 新起的那一条报到没有：报到了 ⇒ 它的 sid；没报到 ⇒ 上屏那一句。 */
private sealed interface Arrival {
    data class Sid(
        val sid: String,
    ) : Arrival

    data class Missed(
        val said: String?,
    ) : Arrival
}

/**
 * 报到没有由那台等、那台说（`session-arrive`，回包里那一问原样交回）；那台说不用等 ⇒ 照回包里的 sid。
 * [withinMs] 是愿意等多久（`session-arrive` 的期限，那台照它减余量等）。
 */
private suspend fun arrivalOf(
    link: CoreLink,
    machine: String,
    answer: SessionOpen.Answer,
    withinMs: Long,
): Arrival {
    val ask =
        answer.arrival
            ?: return answer.sid?.let { Arrival.Sid(it) } ?: Arrival.Missed(unreadableReply(machine, SessionOpen.COMMAND, answer.toString()).text)
    val r = link.call(SessionArrive.COMMAND, SessionArrive.args(ask), withinMs)
    val end = (r as? Reply.Ok)?.let { SessionArrive.of(it.data) }
    return when {
        end is ArriveEnd.Arrived -> Arrival.Sid(end.sid)
        end is ArriveEnd.Missed -> Arrival.Missed(end.said)
        r !is Reply.Ok -> Arrival.Missed(r.problem(machine, SessionArrive.COMMAND, link.offline.value)?.text)
        else -> Arrival.Missed(unreadableReply(machine, SessionArrive.COMMAND, r.data.toString()).text)
    }
}

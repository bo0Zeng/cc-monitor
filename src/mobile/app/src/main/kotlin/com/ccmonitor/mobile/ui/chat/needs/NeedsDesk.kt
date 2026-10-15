package com.ccmonitor.mobile.ui.chat.needs

import com.ccmonitor.mobile.core.claude.link.Changed
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.NeedsAnswer
import com.ccmonitor.mobile.core.claude.link.NeedsPick
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionsNeeds
import com.ccmonitor.mobile.core.claude.link.wire.Needs
import com.ccmonitor.mobile.core.claude.link.wire.NeedsAsk
import com.ccmonitor.mobile.core.claude.link.wire.NeedsChoice
import com.ccmonitor.mobile.core.claude.link.wire.NeedsNote
import com.ccmonitor.mobile.core.claude.link.wire.NeedsQuestion
import com.ccmonitor.mobile.core.copy.DurationFormat
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch

/** 聊天屏上需手动那一块此刻的样子：钉条 ＋ 答题抽屉（都可能没有）。 */
data class NeedsUi(
    val bar: NeedsBar? = null,
    val sheet: AskSheet? = null,
)

/** 钉条（输入框上方）：第一行那台写好的 [head] ＋ 已等，第二行等宽的 [line2]（答不了的那种是那台那一句，[code] 为假）。 */
data class NeedsBar(
    val head: String,
    val line2: String?,
    /** 第二行是命令 / 主参数（等宽）；`false` ＝ 那台写的一句话（界面字体）。 */
    val code: Boolean,
    /** 挂着请求、这里还没答过 ⇒ 右侧［答］。答不了的（`ask: null`）不画按钮：终端只当逃生口、只在 ⋯ 里。 */
    val canAnswer: Boolean,
    val waitedMs: Long?,
    val receivedAt: Long,
) {
    /** 第一行：「等批准 · Bash · 12s」，已等从收到那份起接着走（同桌面 `stateLine`）。 */
    fun firstLine(now: Long): String = waited(now)?.let { copyText("sessionFace.state.waiting", "kind" to head, "waited" to it) } ?: head

    fun waited(now: Long): String? = DurationFormat.waited(waitedMs, receivedAt, now)
}

/** 答题抽屉：画的是哪一份请求（拿到手那一刻的快照，那台撤了也照画到关上）＋ 这边答到哪了。 */
data class AskSheet(
    val ask: NeedsAsk,
    val head: String,
    val waitedMs: Long?,
    val receivedAt: Long,
    /** 送出中那一颗的选项 id。 */
    val sending: String? = null,
    /** 要附话的那一项点开了输入框。 */
    val noteFor: String? = null,
    /** 提问那一种：第几题（一屏一题）。 */
    val page: Int = 0,
    val picks: Map<String, NeedsPick> = emptyMap(),
    /** 别处先答了 / 那一头撤了：那台写好的那一句（点了才知道的）或同桌面那一句（重问里没了的）。 */
    val gone: String? = null,
    /** 没答成（别的原因）：那一句 ＋ 详情，按钮照旧可点。 */
    val failed: Problem? = null,
) {
    /** 按钮都不能点：送出中 · 别处先答了。 */
    val locked: Boolean get() = sending != null || gone != null

    val question: NeedsQuestion? get() = ask.questions.getOrNull(page)

    val last: Boolean get() = page >= ask.questions.lastIndex

    /** 这一题答了（选了一项或写了字）。 */
    val canNext: Boolean get() = question?.let { picks[it.id]?.answered == true } == true

    /** 每题都答了。 */
    val allAnswered: Boolean get() = ask.questions.all { picks[it.id]?.answered == true }

    fun waited(now: Long): String? = DurationFormat.waited(waitedMs, receivedAt, now)

    /** 每题的答（没碰过的那题交空的）。 */
    fun everyPick(): Map<String, NeedsPick> = ask.questions.associate { it.id to (picks[it.id] ?: NeedsPick(emptySet(), "")) }

    /** 重问里挂着的是 [current]：画着的这一份不在了、也不是这里正在答 / 已知别处答了的 ⇒ 别处先答了。 */
    fun lostTo(current: NeedsAsk?): Boolean = gone == null && sending == null && ask.id != current?.id
}

/** 一颗选项长什么样。 */
enum class ChoiceLook {
    /** 主按钮（强调色实心）：每一族的第一颗只在允许那一族。 */
    PRIMARY,

    /** 描边。 */
    OUTLINE,

    /** 「拒绝并停下」：红字描边。 */
    DANGER,

    /** 文字按钮（「改到终端里答」「不回答」）。 */
    QUIET,
}

data class ChoiceRow(
    val choice: NeedsChoice,
    val look: ChoiceLook,
    /** 要附话的字后加「…」（同桌面 `askBlock.choice.more`）。 */
    val label: String,
)

/**
 * 选项分段（照那台排好的次序，不重排）：允许那一族 · 拒绝那一族另起一段 · 改到终端里答排末尾。哪一项进哪一段按选项 id
 * （协议里的闭集，`control::asks::verdict_of`；同桌面 `answer-block.ts` 的 `rowOf`），不按字。提问那一种：「提交答案」一段、其余文字按钮。
 */
fun choiceGroups(ask: NeedsAsk): List<List<ChoiceRow>> {
    val groups = linkedMapOf<Int, MutableList<ChoiceRow>>()
    for (c in ask.choices) {
        val seg =
            when {
                c.id == RELEASE -> SEG_RELEASE
                ask.questions.isNotEmpty() && c.id != ANSWER -> SEG_RELEASE
                c.id in SECOND -> SEG_SECOND
                else -> SEG_MAIN
            }
        val list = groups.getOrPut(seg) { mutableListOf() }
        val look =
            when {
                seg == SEG_RELEASE -> ChoiceLook.QUIET
                c.id == DENY_STOP -> ChoiceLook.DANGER
                seg == SEG_MAIN && list.isEmpty() -> ChoiceLook.PRIMARY
                else -> ChoiceLook.OUTLINE
            }
        list += ChoiceRow(c, look, if (c.note == NeedsNote.NONE) c.text else copyText("askBlock.choice.more", "label" to c.text))
    }
    return groups.toSortedMap().values.toList()
}

private const val SEG_MAIN = 0
private const val SEG_SECOND = 1
private const val SEG_RELEASE = 2
private const val RELEASE = "release"
private const val ANSWER = "answer"
private const val DENY_STOP = "deny-stop"

/** 另起一段的那几项：拒绝一族 ＋ 批计划的「继续规划」。 */
private val SECOND = setOf("deny", "deny-note", DENY_STOP, "plan-keep")

/**
 * 一条会话的需手动（聊天屏那一块）：sid 有了就问 `sessions-needs` 取这一条；那台推 `changed {needs, key=sid}`、这条会话的会话帧变了、
 * 换了一条流 ⇒ 去抖后重问。答走 `needs-answer`（同桌面那一条，不送键）：
 * - 答上了 ⇒ 抽屉收起，那一份不再给［答］，钉条等重问里没了才消失；
 * - `already_answered` · `closed_elsewhere` ⇒ 抽屉里按钮全灰、那台那一句、只留［关闭］，随后重问；
 * - 抽屉开着、重问里那条请求没了（终端 / 桌面先答了）⇒ 同一态，那一句同桌面；
 * - 别的失败 ⇒ 那一句 ＋ 详情，照旧可点。
 * 字 · 选项 · 次序 · 附话要求全照那台（[NeedsAsk]）；这里只记这边答到哪了。
 */
@OptIn(FlowPreview::class)
class NeedsDesk(
    private val link: CoreLink,
    /** 那台流上的 `changed` 帧（`ResidentFeed.pushes`）。 */
    pushes: Flow<Changed>,
    sid: Flow<String?>,
    private val machine: String,
    private val scope: CoroutineScope,
    /** 本机单调钟（「已等」接着走表用）。 */
    private val now: () -> Long,
    private val answerCall: suspend (String, Map<String, Any?>) -> Reply = { cmd, args -> link.call(cmd, args, ANSWER_MS) },
) {
    private val _state = MutableStateFlow(NeedsUi())
    val state: StateFlow<NeedsUi> = _state.asStateFlow()

    /** 此刻对着的那条会话（新建会话起好之前没有）。 */
    @Volatile private var current: String? = null
    private var needs: Needs? = null
    private var receivedAt = 0L

    /** 这边答过 / 别处先答了的那一份：重问里还在也不再给［答］。 */
    private var settled: String? = null
    private val wake = Channel<Unit>(Channel.CONFLATED)

    init {
        scope.launch {
            sid.filterNotNull().distinctUntilChanged().collectLatest { s ->
                current = s
                wake.trySend(Unit)
                merge(
                    pushes.map { if (it.topic == NeedsAnswer.TOPIC && it.key == s) Unit else null }.filterNotNull(),
                    link.table
                        .map { it.sessions[s]?.activity }
                        .distinctUntilChanged()
                        .drop(1)
                        .map { },
                    link.epoch.drop(1).map { },
                ).collect { wake.trySend(Unit) }
            }
        }
        scope.launch { wake.receiveAsFlow().debounce(DEBOUNCE_MS).collect { fetch() } }
    }

    /** ［答］：升起抽屉（答不了的不升）。 */
    fun open() {
        val n = needs ?: return
        val ask = n.ask ?: return
        if (ask.id == settled) return
        update { it.copy(sheet = AskSheet(ask, n.head, n.waitedMs, receivedAt)) }
    }

    /** 抽屉外点一下 · 下滑 · ［关闭］：收起、不算答。 */
    fun dismiss() = update { it.copy(sheet = null) }

    /** 点一项：要附话的先换成输入框；提问的「提交答案」每题都答了才交。 */
    fun pick(choice: String) {
        val sheet = _state.value.sheet?.takeIf { !it.locked } ?: return
        val c = sheet.ask.choices.firstOrNull { it.id == choice } ?: return
        val quiz = c.id == ANSWER && sheet.ask.questions.isNotEmpty()
        when {
            c.note != NeedsNote.NONE -> update { it.copy(sheet = sheet.copy(noteFor = c.id, failed = null)) }
            quiz && !sheet.allAnswered -> Unit
            quiz -> send(sheet, c.id, picks = sheet.everyPick())
            else -> send(sheet, c.id)
        }
    }

    /** 附话那一格交出去：必附的空着不交。 */
    fun sendNote(text: String) {
        val sheet = _state.value.sheet ?: return
        if (sheet.locked) return
        val c = sheet.ask.choices.firstOrNull { it.id == sheet.noteFor } ?: return
        val note = text.trim()
        if (c.note == NeedsNote.REQUIRED && note.isEmpty()) return
        send(sheet, c.id, note = note.ifEmpty { null })
    }

    fun cancelNote() = editSheet { it.copy(noteFor = null) }

    /** 提问：点这一题的一项（单选 ⇒ 换成它、清掉其他那格；多选 ⇒ 加 / 去）。 */
    fun toggle(option: String) =
        editSheet { s ->
            val q = s.question ?: return@editSheet s
            if (q.options.none { it.id == option }) return@editSheet s
            val p = s.picks[q.id] ?: NeedsPick(emptySet(), "")
            val next =
                when {
                    !q.multi -> NeedsPick(setOf(option), "")
                    option in p.options -> p.copy(options = p.options - option)
                    else -> p.copy(options = p.options + option)
                }
            s.copy(picks = s.picks + (q.id to next))
        }

    /** 提问：这一题的「其他…」（单选写了字 ⇒ 选项清掉）。 */
    fun other(text: String) =
        editSheet { s ->
            val q = s.question ?: return@editSheet s
            val p = s.picks[q.id] ?: NeedsPick(emptySet(), "")
            val options = if (!q.multi && text.isNotBlank()) emptySet() else p.options
            s.copy(picks = s.picks + (q.id to NeedsPick(options, text)))
        }

    /** 提问：下一题（这一题答了才走）。 */
    fun next() = editSheet { s -> if (s.canNext && !s.last) s.copy(page = s.page + 1) else s }

    private fun editSheet(f: (AskSheet) -> AskSheet) =
        update { ui ->
            val s = ui.sheet
            if (s == null || s.locked) ui else ui.copy(sheet = f(s))
        }

    private fun send(
        sheet: AskSheet,
        choice: String,
        note: String? = null,
        picks: Map<String, NeedsPick>? = null,
    ) {
        val s = current ?: return
        update { it.copy(sheet = sheet.copy(sending = choice, failed = null)) }
        scope.launch {
            val r = answerCall(NeedsAnswer.COMMAND, NeedsAnswer.args(s, sheet.ask.id, choice, note, picks))
            when {
                r is Reply.Ok -> {
                    settled = sheet.ask.id
                    update { it.copy(sheet = null) }
                    publish()
                    wake.trySend(Unit)
                }
                r is Reply.Failed && NeedsAnswer.gone(r.failure.code) -> {
                    settled = sheet.ask.id
                    update { ui -> ui.copy(sheet = ui.sheet?.copy(sending = null, noteFor = null, gone = r.failure.message)) }
                    publish()
                    wake.trySend(Unit)
                }
                else -> {
                    val p = r.problem(machine, NeedsAnswer.COMMAND) ?: unreadableReply(machine, NeedsAnswer.COMMAND, r.toString())
                    update { ui -> ui.copy(sheet = ui.sheet?.copy(sending = null, failed = p)) }
                }
            }
        }
    }

    private suspend fun fetch() {
        val s = current ?: return
        val r = link.call(SessionsNeeds.COMMAND, null, READ_MS)
        // 读不出 ⇒ 照常画已读到的（不清钉条，下一次变化再问）。
        val rows = (r as? Reply.Ok)?.let { SessionsNeeds.of(it.data) } ?: return
        needs = rows.firstOrNull { it.sid == s }?.needs
        receivedAt = now()
        val ask = needs?.ask
        // 答过的那一份不在了 ⇒ 忘掉它（下一份照常给［答］）。
        if (ask?.id != settled) settled = null
        update { ui ->
            val sheet = ui.sheet
            // 抽屉开着、那一份不在了、不是这里答掉的 ⇒ 别处先答了。
            sheet?.takeIf { it.lostTo(ask) }?.let { ui.copy(sheet = it.copy(gone = copyText("askBlock.gone.elsewhere"), noteFor = null, failed = null)) } ?: ui
        }
        publish()
    }

    private fun publish() =
        update { ui ->
            val n = needs
            ui.copy(
                bar =
                    n?.let {
                        NeedsBar(
                            head = it.head,
                            line2 = if (it.ask != null) it.headCode else it.manualText,
                            code = it.ask != null,
                            canAnswer = it.ask != null && it.ask?.id != settled,
                            waitedMs = it.waitedMs,
                            receivedAt = receivedAt,
                        )
                    },
            )
        }

    @Synchronized
    private fun update(f: (NeedsUi) -> NeedsUi) {
        _state.value = f(_state.value)
    }

    companion object {
        /** 推送 · 会话帧连着来时合成一次重问。 */
        const val DEBOUNCE_MS = 300L
        private const val READ_MS = 15_000L

        /** 答是就地登记、立刻回（同桌面 `ANSWER_WITHIN_MS`）。 */
        private const val ANSWER_MS = 15_000L
    }
}

package com.ccmonitor.mobile.ui.chat.needs

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.NeedsAnswer
import com.ccmonitor.mobile.core.claude.link.NeedsPick
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.copy.DurationFormat
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.AskEnd
import com.ccmonitor.mobile.core.wire.gen.Needs
import com.ccmonitor.mobile.core.wire.gen.NeedsAsk
import com.ccmonitor.mobile.core.wire.gen.NeedsChoice
import com.ccmonitor.mobile.core.wire.gen.NeedsChoiceRow
import com.ccmonitor.mobile.core.wire.gen.NeedsItem
import com.ccmonitor.mobile.core.wire.gen.NeedsList
import com.ccmonitor.mobile.core.wire.gen.NeedsNote
import com.ccmonitor.mobile.core.wire.gen.NeedsQuestion
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import com.ccmonitor.mobile.ui.needs.MachineNeeds
import com.ccmonitor.mobile.ui.needs.waitingNow
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/** 聊天屏上需手动那一块此刻的样子：钉条 ＋ 答题抽屉（都可能没有）。 */
data class NeedsUi(
    val bar: NeedsBar? = null,
    val sheet: AskSheet? = null,
    /** 没在等、上一条提问没人答已替人答过 ⇒ 那台写好的那一句（钉条那一格画成一行灰字）。 */
    val answered: String? = null,
    /** 流里在等的那一步（工具卡上标它）；那台判不出是哪一步 · 不在等 ⇒ `null`。 */
    val awaiting: AwaitingStep? = null,
)

/** 在等的那一步：记录里那次工具调用的 id（`needs.call`）＋ 卡上那枚标的字（`needs.text`，那台写好的「等批准」…）。 */
data class AwaitingStep(
    val call: String,
    val text: String,
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
    /** 那台写好的第一行（`clock.head`，已等那一截是 `{dur}` 空位）；不知道等了多久 ⇒ `null`。 */
    val clock: String? = null,
) {
    /** 第一行：那台写好的「等批准 · Bash · 12s」，已等从收到那份起接着走（同需手动屏那一行）；没有 ⇒ [head]。 */
    fun firstLine(now: Long): String = clock?.let { waitingNow(it, waitedMs, receivedAt, now) } ?: head
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

data class ChoiceRow(
    val choice: NeedsChoice,
    /** 要附话的字后加「…」（同桌面 `askBlock.choice.more`）。 */
    val label: String,
)

/**
 * 选项分段（照那台排好的次序，不重排）：哪一项进哪一段照那台给的 `row`，每颗长什么样照那台给的 `look`，
 * 不按 id 认、不按次序挑、不按字。
 */
fun choiceGroups(ask: NeedsAsk): List<List<ChoiceRow>> {
    val groups = sortedMapOf<NeedsChoiceRow, MutableList<ChoiceRow>>()
    for (c in ask.choices) {
        groups.getOrPut(c.row) { mutableListOf() } += ChoiceRow(c, if (c.note == NeedsNote.NONE) c.text else copyText("askBlock.choice.more", "label" to c.text))
    }
    return groups.values.toList()
}

/**
 * 一条会话的需手动（聊天屏那一块）：读跨会话需手动板上那台那一份（[NeedsBoard]，这里不另问 `sessions-needs`）里这一条；
 * 什么时候重问归板管，这里答完叫它重问一次。答走 `needs-answer`（同桌面那一条，不送键）：
 * - 答上了 ⇒ 抽屉收起，那一份不再给［答］，钉条等重问里没了才消失；
 * - 回包是「别处已答」那一种结局（`result: gone`）⇒ 抽屉里按钮全灰、那台那一句、只留［关闭］，随后重问；
 * - 抽屉开着、重问里那条请求没了（终端 / 桌面先答了 · 没人答、那台替人答了）⇒ 同一态，那一句照那台 `sessions-needs.ended`（同桌面，不在这里兜底）；
 * - 没在等、那台说上一条请求收场那一句留着（替人答过）⇒ 那一句（[NeedsUi.answered]），下一次挂上请求那台才换掉；
 * - 别的失败 ⇒ 那一句 ＋ 详情，照旧可点。
 * 字 · 选项 · 次序 · 附话要求全照那台（[NeedsAsk]）；这里只记这边答到哪了。
 */
class NeedsDesk(
    private val link: CoreLink,
    /** 板上那台此刻那一份（[NeedsBoard.state] 里那一台）。 */
    board: Flow<MachineNeeds?>,
    /** 叫板重问那一台。 */
    private val reask: () -> Unit,
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

    /** 那一项上那台写好的钉条第一行（[NeedsBar.clock]）。 */
    private var clock: String? = null

    /** 那台说这条会话的上一条请求怎么收的场（`sessions-needs.ended`：那一句 · 留不留着）。 */
    private var ended: AskEnd? = null
    private var receivedAt = 0L

    /** 这边答过 / 别处先答了的那一份：重问里还在也不再给［答］。 */
    private var settled: String? = null

    init {
        scope.launch {
            combine(sid.filterNotNull().distinctUntilChanged(), board.map { it?.list }.filterNotNull().distinctUntilChanged()) { s, list -> s to list }
                .collect { (s, list) ->
                    current = s
                    take(s, list)
                }
        }
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
        val quiz = c.picks
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
            val gone = (r as? Reply.Ok)?.let { NeedsAnswer.gone(it.data) }
            when {
                gone != null -> {
                    settled = sheet.ask.id
                    update { ui -> ui.copy(sheet = ui.sheet?.copy(sending = null, noteFor = null, gone = gone)) }
                    publish()
                    reask()
                }
                r is Reply.Ok -> {
                    settled = sheet.ask.id
                    update { it.copy(sheet = null) }
                    publish()
                    reask()
                }
                else -> {
                    val p = r.problem(machine, NeedsAnswer.COMMAND, link.offline.value) ?: unreadableReply(machine, NeedsAnswer.COMMAND, r.toString())
                    update { ui -> ui.copy(sheet = ui.sheet?.copy(sending = null, failed = p)) }
                }
            }
        }
    }

    private fun take(
        s: String,
        list: NeedsList,
    ) {
        val item = list.items.firstNotNullOfOrNull { (it as? NeedsItem.Session)?.takeIf { i -> i.session.sid == s } }
        needs = item?.session?.needs
        clock = item?.clock?.head
        ended = list.ended.firstOrNull { it.sid == s }?.end
        receivedAt = now()
        val ask = needs?.ask
        // 答过的那一份不在了 ⇒ 忘掉它（下一份照常给［答］）。
        if (ask?.id != settled) settled = null
        update { ui ->
            val sheet = ui.sheet
            // 抽屉开着、那一份不在了、不是这里答掉的 ⇒ 别处先答了：那台写好的那一句（那台没说 ⇒ 抽屉照旧，再点由那台答 gone）。
            val said = ended?.said
            sheet?.takeIf { said != null && it.lostTo(ask) }?.let { ui.copy(sheet = it.copy(gone = said, noteFor = null, failed = null)) } ?: ui
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
                            clock = clock,
                        )
                    },
                awaiting = n?.call?.let { AwaitingStep(it, n.text) },
                answered = if (n == null) ended?.takeIf { it.stays }?.said else null,
            )
        }

    @Synchronized
    private fun update(f: (NeedsUi) -> NeedsUi) {
        _state.value = f(_state.value)
    }

    companion object {
        /** 答是就地登记、立刻回（同桌面 `ANSWER_WITHIN_MS`）。 */
        private const val ANSWER_MS = 15_000L
    }
}

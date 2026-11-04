package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.HistoryAnnotate
import com.ccmonitor.mobile.core.claude.link.HistoryFacts
import com.ccmonitor.mobile.core.claude.link.InSessionFind
import com.ccmonitor.mobile.core.claude.link.LineStart
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SaidList
import com.ccmonitor.mobile.core.claude.link.SessionsStop
import com.ccmonitor.mobile.core.claude.link.blocked
import com.ccmonitor.mobile.core.claude.link.face
import com.ccmonitor.mobile.core.wire.gen.AutoChoice
import com.ccmonitor.mobile.core.wire.gen.FindHit
import com.ccmonitor.mobile.core.wire.gen.SessionState
import com.ccmonitor.mobile.core.wire.gen.UserInputRow
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/** ⋯「会话里找」那一张：查询串 · 在问 · 命中（那台的先后）· 全量命中数 · 没问成那一句。 */
data class FindUi(
    val query: String = "",
    val busy: Boolean = false,
    val hits: List<FindHit> = emptyList(),
    val total: Long = 0L,
    val failed: String? = null,
)

/** ⋯「大纲」那一张：人说过的话（对话序）· 在问 · 没问成那一句。 */
data class OutlineUi(
    val busy: Boolean = true,
    val rows: List<UserInputRow> = emptyList(),
    val failed: String? = null,
)

/** ⋯「无人答时自动答」那一组：几档（几分钟 ＋ 那台写好的字，`0` 是关）· 此刻那一档 · 在问。 */
data class AutoUi(
    val busy: Boolean = true,
    val choices: List<AutoChoice> = emptyList(),
    val now: Int = 0,
)

/** 结束会话那个确认框：要结束的那几条的名字。 */
data class EndAsk(
    val titles: List<String>,
)

/** ⋯ 里那几面此刻的样子；[note] 是没做成时那台写好的那一句（顶栏下那一行）。 */
data class MoreUi(
    val find: FindUi? = null,
    val outline: OutlineUi? = null,
    val auto: AutoUi? = null,
    val ending: EndAsk? = null,
    val note: String? = null,
)

/** ⋯「结束会话…」那一项：那台说能结束 ⇒ [Ready]；不能 ⇒ [Blocked]（灰着、写那台那一句）。 */
sealed interface EndEntry {
    data object Ready : EndEntry

    data class Blocked(
        val why: String,
    ) : EndEntry
}

/**
 * 一条会话 ⋯ 里的会话里找 · 大纲 · 结束会话 · 无人答时自动答。判定都在那台：命中与清单是那台给的，能不能结束看脸上的 `can.end`，
 * 自动答的几档与此刻那一档看会话事实（`history-facts` 的 `autoAnswerChoices` · `autoAnswer`）；这里只拼入参、照摆、交。
 * 跳（命中 · 大纲的一句）交 [jump]：那一行的字节起点 ＋ 落在哪次工具调用的卡里（[ChatSession.jump]）。命中只带行号 ⇒ 先问那一行的起点（[LineStart]）。
 */
class ChatMore(
    private val link: CoreLink,
    private val machine: String,
    private val scope: CoroutineScope,
    private val sid: () -> String?,
    private val jump: (Long, String?) -> Unit,
) {
    private val _state = MutableStateFlow(MoreUi())
    val state: StateFlow<MoreUi> = _state.asStateFlow()

    private var finding: Job? = null

    private fun put(f: (MoreUi) -> MoreUi) {
        _state.value = f(_state.value)
    }

    /** 那条记录的路径：会话表里有 ⇒ 用它；没有 ⇒ 问那台 `history-facts {sid}`。问不到 ⇒ 那一句、`null`。 */
    private suspend fun pathOf(s: String): Pair<String?, String?> {
        link.table.value.live[s]
            ?.path
            ?.let { return it to null }
        val r = link.call(HistoryFacts.COMMAND, HistoryFacts.bySid(s), READ_MS)
        if (r !is Reply.Ok) return null to r.problem(machine, HistoryFacts.COMMAND, link.offline.value)?.text
        return HistoryFacts.of(r.data)?.path?.let { it to null } ?: (null to unreadableReply(machine, HistoryFacts.COMMAND, r.data.toString()).text)
    }

    fun clearNote() = put { it.copy(note = null) }

    // ── 会话里找 ──

    fun openFind() = put { it.copy(find = it.find ?: FindUi(), note = null) }

    fun closeFind() {
        finding?.cancel()
        put { it.copy(find = null) }
    }

    /** 找 [query]（空 ⇒ 清掉命中，不问）；上一问还没回 ⇒ 撤掉，只认这一问。 */
    fun find(query: String) {
        val s = sid() ?: return
        finding?.cancel()
        if (query.isBlank()) return put { it.copy(find = FindUi(query)) }
        put { it.copy(find = FindUi(query, busy = true)) }
        finding =
            scope.launch {
                val (p, why) = pathOf(s)
                val ui =
                    if (p == null) {
                        FindUi(query, failed = why)
                    } else {
                        val r = link.call(InSessionFind.COMMAND, InSessionFind.args(p, query), READ_MS)
                        when (r) {
                            is Reply.Ok ->
                                InSessionFind.of(r.data)?.let { FindUi(query, hits = it.hits, total = it.total) }
                                    ?: FindUi(query, failed = unreadableReply(machine, InSessionFind.COMMAND, r.data.toString()).text)
                            else -> FindUi(query, failed = r.problem(machine, InSessionFind.COMMAND, link.offline.value)?.text)
                        }
                    }
                put { it.copy(find = ui) }
            }
    }

    /** 点一条命中：问那一行的起点、跳过去、收起。起点问不到 ⇒ 那一句写在那一张上，不跳。 */
    fun pickHit(hit: FindHit) {
        val s = sid() ?: return
        scope.launch {
            val (p, why) = pathOf(s)
            if (p == null) return@launch put { it.copy(find = it.find?.copy(failed = why)) }
            val r = link.call(LineStart.COMMAND, LineStart.args(p, hit.line), READ_MS)
            val at = (r as? Reply.Ok)?.let { LineStart.of(it.data) }
            if (at == null) {
                val said = if (r is Reply.Ok) unreadableReply(machine, LineStart.COMMAND, r.data.toString()).text else r.problem(machine, LineStart.COMMAND, link.offline.value)?.text
                return@launch put { it.copy(find = it.find?.copy(failed = said)) }
            }
            put { it.copy(find = null) }
            jump(at, hit.tool)
        }
    }

    // ── 大纲 ──

    fun openOutline() {
        val s = sid() ?: return
        put { it.copy(outline = OutlineUi(), note = null) }
        scope.launch {
            val (p, why) = pathOf(s)
            val ui =
                if (p == null) {
                    OutlineUi(busy = false, failed = why)
                } else {
                    val r = link.call(SaidList.COMMAND, SaidList.args(p), READ_MS)
                    when (r) {
                        is Reply.Ok ->
                            SaidList.of(r.data)?.let { OutlineUi(busy = false, rows = it.entries) }
                                ?: OutlineUi(busy = false, failed = unreadableReply(machine, SaidList.COMMAND, r.data.toString()).text)
                        else -> OutlineUi(busy = false, failed = r.problem(machine, SaidList.COMMAND, link.offline.value)?.text)
                    }
                }
            put { if (it.outline == null) it else it.copy(outline = ui) }
        }
    }

    fun closeOutline() = put { it.copy(outline = null) }

    /** 点大纲里的一句：照它那一行的起点跳、收起。 */
    fun pickSaid(row: UserInputRow) {
        put { it.copy(outline = null) }
        jump(row.at, null)
    }

    // ── 结束会话 ──

    /** ⋯ 里那一项的样子（照脸上的 `can.end`）；簿里还没这一条 ⇒ `null`、不出这一项。 */
    fun endEntry(): EndEntry? {
        val face = sid()?.let { link.table.value.face(it) } ?: return null
        val why = face.can.end.blocked() ?: return EndEntry.Ready
        return EndEntry.Blocked(why)
    }

    /** 点「结束会话…」：那台说能结束才开确认框（框里列 [title]）。 */
    fun askEnd(title: String) {
        if (endEntry() != EndEntry.Ready) return
        put { it.copy(ending = EndAsk(listOf(title)), note = null) }
    }

    fun cancelEnd() = put { it.copy(ending = null) }

    /** 确认框答了「结束」：交 `sessions-stop`；变已结束由会话流兜（不主动改状态）；没成 ⇒ 那台写好的那一句。 */
    fun confirmEnd() {
        _state.value.ending ?: return
        put { it.copy(ending = null) }
        val s = sid() ?: return
        if (endEntry() != EndEntry.Ready) return
        scope.launch {
            val r = link.call(SessionsStop.COMMAND, SessionsStop.args(listOf(s)), STOP_MS)
            val note =
                when (r) {
                    is Reply.Ok -> {
                        val b = SessionsStop.of(r.data)
                        if (b == null) {
                            unreadableReply(machine, SessionsStop.COMMAND, r.data.toString()).text
                        } else {
                            val one = b.results.firstOrNull { it.sid == s }
                            if (one == null || one.outcome == SessionState.DONE) null else one.said
                        }
                    }
                    else -> r.problem(machine, SessionsStop.COMMAND, link.offline.value)?.text
                }
            put { it.copy(note = note) }
        }
    }

    // ── 无人答时自动答 ──

    /** ⋯ 里出不出这一项：簿里有这一条就出（那一格是会话注解，交不交得成由那台答）。 */
    fun autoShown(): Boolean = sid()?.let { link.table.value.face(it) } != null

    /** 开那一组：问一次会话事实，照它列几档、勾在此刻那一档（没设 ⇒ 关）。 */
    fun openAuto() {
        val s = sid() ?: return
        put { it.copy(auto = AutoUi(), note = null) }
        scope.launch {
            val r = link.call(HistoryFacts.COMMAND, HistoryFacts.bySid(s), READ_MS)
            val f = (r as? Reply.Ok)?.let { HistoryFacts.of(it.data) }
            if (f == null) {
                val why = if (r is Reply.Ok) unreadableReply(machine, HistoryFacts.COMMAND, r.data.toString()).text else r.problem(machine, HistoryFacts.COMMAND, link.offline.value)?.text
                return@launch put { it.copy(auto = null, note = why) }
            }
            put { if (it.auto == null) it else it.copy(auto = AutoUi(busy = false, choices = f.autoAnswerChoices, now = f.autoAnswer ?: 0)) }
        }
    }

    fun closeAuto() = put { it.copy(auto = null) }

    /** 选一档：交 `history-annotate {autoAnswer}`；交成了收起（那台推帧、会话事实跟着换），没交成 ⇒ 那一句、几档照旧摆着。 */
    fun setAuto(min: Int) {
        val s = sid() ?: return
        scope.launch {
            val r = link.call(HistoryAnnotate.COMMAND, HistoryAnnotate.autoAnswer(s, min), READ_MS)
            if (r is Reply.Ok) {
                put { it.copy(auto = null) }
            } else {
                put { it.copy(note = r.problem(machine, HistoryAnnotate.COMMAND, link.offline.value)?.text) }
            }
        }
    }

    private companion object {
        const val READ_MS = 15_000L

        /** `sessions-stop` 总期限上限 392 秒；一条会话等不了那么久，给到 60 秒（那台在期限前收紧）。 */
        const val STOP_MS = 60_000L
    }
}

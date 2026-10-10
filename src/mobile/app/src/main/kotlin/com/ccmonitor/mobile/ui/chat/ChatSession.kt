package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.HistoryList
import com.ccmonitor.mobile.core.claude.link.HistoryRead
import com.ccmonitor.mobile.core.claude.link.HistoryTail
import com.ccmonitor.mobile.core.claude.link.RecordLine
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionNew
import com.ccmonitor.mobile.core.claude.link.TerminalInput
import com.ccmonitor.mobile.core.claude.link.Tone
import com.ccmonitor.mobile.core.claude.link.str
import com.ccmonitor.mobile.core.claude.model.DeliveryState
import com.ccmonitor.mobile.core.claude.model.RecordUnits
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.ui.copy.copyText
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
)

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
    ) : ChatTarget
}

/**
 * 一条会话的聊天状态。起会话、送字、正文都走那台的核心（[CoreLink]）：
 * - 新建会话：第一条按 ➤ ⇒ `session-new`（在那台 tmux 里起，桌面那一侧看得到、接得上）⇒ 等它报到（会话表里那个目录新出现的一条）⇒ 再送那句。
 * - 送字 / 停：`terminal-input`（字 · `esc`）；送没送到是那台判的（`delivered` · `unsure` · `refused`）。
 * - 正文：`history-tail` ＋ `history-read` 读最后一屏，之后跟流上这条会话的 `line` 帧；往上翻再读上一段。记录 ⇒ 渲染单元见 [RecordUnits]。
 * 手机这边只记「送出去、记录里还没出现」的那几句；记录里出现了同一句人话 ⇒ 那一句的标消失。
 *
 * 它不是 `ViewModel`：持有者是应用级的 [ChatController]，离开聊天屏照样跟着。
 */
class ChatSession(
    private val link: CoreLink,
    target: ChatTarget,
    /** 这台在手机上的名字（「{machine} 无应答」那几句）。 */
    private val machine: String,
    private val scope: CoroutineScope,
    private val arrivalMs: Long = ARRIVAL_MS,
) {
    private val _state = MutableStateFlow(ChatUiState(sid = (target as? ChatTarget.Existing)?.sid))
    val state: StateFlow<ChatUiState> = _state.asStateFlow()

    private val draft = target as? ChatTarget.Draft
    private val records = mutableListOf<Map<String, Any?>>()
    private val outgoing = mutableListOf<Outgoing>()
    private val lock = Mutex()

    @Volatile private var sid: String? = (target as? ChatTarget.Existing)?.sid
    private var path: String? = null

    /** 已读到的最早一行的字节起点 · 最新一行之后的字节；尾段要了几行。 */
    private var head = 0L
    private var tail = 0L
    private var tailLines = PAGE_LINES

    private var follow: Job? = null
    private var starting: Job? = null
    private var seq = 0

    private class Outgoing(
        val localId: String,
        val text: String,
        var delivery: DeliveryState?,
        var deliveryText: String?,
    )

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

    fun close() = scope.cancel()

    private suspend fun deliver(o: Outgoing) {
        val s = sid ?: started() ?: return fail(o, permanent = false, text = null)
        val (delivery, text) = outcomeOf(link.call(TerminalInput.COMMAND, TerminalInput.text(s, o.text), INPUT_MS))
        mark(o, delivery, text)
    }

    /** `terminal-input` 的回话 ⇒ 气泡下的标与那一句（送到了 ⇒ 没有标）。 */
    private fun outcomeOf(r: Reply): Pair<DeliveryState?, String?> {
        val unsure = DeliveryState.UNSURE to copyText("terminal.input.unsure", "machine" to machine)

        fun failed(why: String?) = DeliveryState.FAILED to copyText("terminal.input.failed", "why" to why.orEmpty())
        return when (r) {
            is Reply.Ok ->
                when (val res = TerminalInput.of(r.data)) {
                    TerminalInput.Result.Delivered -> null to null
                    TerminalInput.Result.Unsure -> unsure
                    is TerminalInput.Result.Refused ->
                        (if (res.why in RETRYABLE) DeliveryState.FAILED else DeliveryState.FAILED_PERMANENT) to
                            copyText("terminal.input.failed", "why" to res.said)
                    null -> failed(unreadableReply(machine, TerminalInput.COMMAND, r.data.toString()).text)
                }
            // 期限到了：发出去了、不知道送没送到 ⇒ 不重发。
            Reply.TimedOut -> unsure
            else -> failed(r.problem(machine, TerminalInput.COMMAND)?.text)
        }
    }

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

    /** 新建会话起好、认出来了 ⇒ 它的 sid；起不来 ⇒ `null`（那一句已在 [ChatUiState.failedWhy]）。几句同时等同一次起。 */
    private suspend fun started(): String? {
        val d = draft ?: return null
        val job = lock.withLock { starting?.takeIf { it.isActive } ?: scope.launch { start(d) }.also { starting = it } }
        job.join()
        return sid
    }

    private suspend fun start(d: ChatTarget.Draft) {
        _state.value = _state.value.copy(failedWhy = null)
        val before = link.table.value.sessions.keys
        val answer =
            when (val r = link.call(SessionNew.COMMAND, SessionNew.args(d.cwd, d.ticket), NEW_MS)) {
                is Reply.Ok -> SessionNew.of(r.data) ?: return failSession(unreadableReply(machine, SessionNew.COMMAND, r.data.toString()).text)
                else -> return failSession(r.problem(machine, SessionNew.COMMAND)?.text)
            }
        val arrived =
            answer.sid ?: withTimeoutOrNull(arrivalMs) {
                link.table.mapNotNull { SessionNew.arrived(it, answer.cwd, before)?.sid }.first()
            } ?: return failSession(copyText("launchArrival.missed.title"))
        sid = arrived
        _state.value = _state.value.copy(sid = arrived)
        follow = scope.launch { attach(arrived) }
    }

    private fun failSession(text: String?) {
        _state.value = _state.value.copy(failedWhy = text)
    }

    /** 先订上这条会话的 `line` 帧（攒着），再读最后一屏；读完把攒着的、比读到的更新的接上，之后来一条接一条。 */
    private suspend fun attach(s: String) {
        val buffered = Channel<RecordLine>(Channel.UNLIMITED)
        val subscribed = CompletableDeferred<Unit>()
        scope.launch {
            link.lines.onSubscription { subscribed.complete(Unit) }.collect { if (it.sid == s) buffered.send(it) }
        }
        subscribed.await()
        watchRunning(s)
        val p = pathOf(s) ?: return
        path = p
        val t = readTail(p, tailLines) ?: return
        val page = readRange(p, t.splitAt, t.end) ?: return
        lock.withLock {
            head = t.splitAt
            tail = t.end
            records.addAll(page)
            _state.value = _state.value.copy(canLoadOlder = head > 0L, failedWhy = null)
            publishLocked()
        }
        for (line in buffered) {
            lock.withLock {
                if (line.byteOffset <= tail) return@withLock
                tail = line.byteOffset
                line.record?.let { records += it }
                publishLocked()
            }
        }
    }

    private fun watchRunning(s: String) {
        scope.launch {
            link.table.collect { t ->
                val running = t.sessions[s]?.activity?.tone == Tone.NOW
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
    ): HistoryTail.Answer? =
        when (val r = link.call(HistoryTail.COMMAND, HistoryTail.args(p, n), READ_MS)) {
            is Reply.Ok -> HistoryTail.of(r.data) ?: null.also { readFailed(unreadableReply(machine, HistoryTail.COMMAND, r.data.toString()).text) }
            else -> null.also { readFailed(r.problem(machine, HistoryTail.COMMAND)?.text) }
        }

    /** 读 `[from, until)` 里的记录，一页不够就接着读。 */
    private suspend fun readRange(
        p: String,
        from: Long,
        until: Long,
    ): List<Map<String, Any?>>? {
        val out = mutableListOf<Map<String, Any?>>()
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
        val said = records.filter { it.str("t") == "said" || it.str("t") == "queued" }.mapNotNull { (it["who"] as? Map<*, *>)?.get("text") as? String }.toMutableList()
        outgoing.removeAll { o -> o.delivery != DeliveryState.FAILED && o.delivery != DeliveryState.FAILED_PERMANENT && said.remove(o.text) }
        val local = outgoing.map { RenderUnit.UserText(it.localId, it.text, null, it.delivery, it.deliveryText) }
        _state.value = _state.value.copy(units = RecordUnits.of(records) + local)
    }

    companion object {
        /** 打开一条先读最后这么多行；往上翻一次再多读这么多。 */
        const val PAGE_LINES = 60

        /** 等新起的会话报到多久（同桌面 `ARRIVAL_BUDGET_MS`）。 */
        const val ARRIVAL_MS = 45_000L
        private const val NEW_MS = 25_000L
        private const val INPUT_MS = 20_000L
        private const val READ_MS = 15_000L

        /** 改得了的拒因（画面变了 · 一时认不出）：给［再发一次］；会话已结束 · 不归这里管的 ⇒ 重试没用。 */
        private val RETRYABLE = setOf("screen_changed", "ambiguous", "not_known")
    }
}

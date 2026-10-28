package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.cache.ChatCache
import com.ccmonitor.mobile.cache.Pending
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionQueueCancel
import com.ccmonitor.mobile.core.claude.link.SessionQueuePut
import com.ccmonitor.mobile.core.claude.model.DeliveryState
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.PendingOutcome
import com.ccmonitor.mobile.core.wire.gen.QueuePut
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import com.ccmonitor.mobile.ui.chat.attach.AttachTray
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

/**
 * 一条会话的待发：这一端交出去、会话那台还没回执的那几句（输入框上方「发送中」· 撤），与回执到了、记录里还没出现的那几句（流里，标那台写的那一句）。
 * 编号就是投进那条会话输入队的 id：没回执（断线 · 期限到了 · App 被杀）⇒ 接上照同一个编号再交，那台去重只进一条。
 */
internal class ChatOutbox(
    private val link: CoreLink,
    private val machine: String,
    private val scope: CoroutineScope,
    private val cache: ChatCache,
    private val attach: AttachTray?,
    private val publish: () -> Unit,
) {
    val lines = mutableListOf<Outgoing>()

    /**
     * 交那一句：投进 [sid] 那条会话的输入队（`session-queue-put`）。回执到了（那台已落盘）⇒ 进流；那台拒了（码）⇒ 留在待发、标没送到；
     * 没回执（断线 · 期限到了 · 那台离线）⇒ 照旧「发送中」，接上再交同一个编号。
     */
    suspend fun put(
        o: Outgoing,
        sid: String,
    ) {
        o.inFlight = true
        val r = link.call(SessionQueuePut.COMMAND, SessionQueuePut.args(sid, o.localId, o.text, System.currentTimeMillis(), o.attachments), INPUT_MS)
        o.inFlight = false
        if (lines.none { it === o }) return
        when {
            r is Reply.Ok ->
                SessionQueuePut.of(r.data)?.let { got(o, it) }
                    ?: mark(o, DeliveryState.FAILED, failed(unreadableReply(machine, SessionQueuePut.COMMAND, r.data.toString()).text).second)
            r is Reply.Failed && r.failure.code != OFFLINE -> mark(o, DeliveryState.FAILED, failed(r.problem(machine, SessionQueuePut.COMMAND, link.offline.value)?.text).second)
            else -> Unit
        }
    }

    /** 回执到了：那台已落盘 ⇒ 进流，那一句照那台写的（排队 · 第几 · 已送进 · 已取消）。 */
    private fun got(
        o: Outgoing,
        put: QueuePut,
    ) {
        o.receipt = true
        o.refresh = false
        mark(o, DeliveryState.QUEUED, put.said)
        attach?.settled(o.localId)
    }

    fun mark(
        o: Outgoing,
        delivery: DeliveryState?,
        text: String?,
    ) {
        o.delivery = delivery
        o.deliveryText = text
        publish()
        scope.launch { cache.settle(o.localId, outcomeOf(delivery), o.attempts) }
    }

    /** 撤待发里一句：本地删掉；万一已经交到那台了，照编号请它撤掉（送过的撤不了，那台说了算）。 */
    fun cancel(
        localId: String,
        sid: String?,
    ) {
        val o = lines.firstOrNull { it.localId == localId && !it.receipt } ?: return
        lines -= o
        publish()
        scope.launch {
            cache.forget(listOf(o.localId))
            attach?.settled(o.localId)
            sid?.let { link.call(SessionQueueCancel.COMMAND, SessionQueueCancel.args(it, o.localId), INPUT_MS) }
        }
    }

    /** 该再交一遍的那几句：还没回执也没在交的 · 回执是上次开着时到的（拿此刻排第几）。 */
    fun due(): List<Outgoing> = lines.filter { !it.inFlight && (it.refresh || !it.receipt && it.delivery == DeliveryState.SENDING) }

    /**
     * 待发里这条会话的那几句挂回来：那台拒过的 ⇒ 没送到；回执到过的 ⇒ 在流里；别的（上次没等到回执 App 就没了）⇒ 回到「发送中」。
     * 拒过的以外都等接上那台照同一个编号再交一遍（[due]）。
     */
    suspend fun restore(s: String) {
        val known = lines.map { it.localId }.toSet()
        for (p in cache.pending(s)) {
            if (p.id in known) continue
            val (d, text) =
                when (p.outcome) {
                    PendingOutcome.DELIVERED -> DeliveryState.QUEUED to null
                    PendingOutcome.REFUSED -> failed(null)
                    else -> DeliveryState.SENDING to copyText("mobChat.delivery.sending")
                }
            lines +=
                Outgoing(p.id, p.text, d, text, p.attempts, attach?.handedOf(p.id).orEmpty()).also {
                    it.receipt = p.outcome == PendingOutcome.DELIVERED
                    it.refresh = it.receipt
                }
        }
    }

    /** 落库一句新交出去的（[draftKey] 那张草稿同一个事务删掉）。 */
    suspend fun keep(
        draftKey: String,
        o: Outgoing,
        sid: String?,
    ) = cache.promote(draftKey, Pending(o.localId, sid, o.text, PendingOutcome.PENDING, o.attempts))

    /** 流里那几句（回执到了的）· 输入框上方那一排（还没回执的）。 */
    fun view(): Pair<List<RenderUnit.UserText>, List<OutboxLine>> {
        val (inFlow, waiting) = lines.partition { it.receipt }
        val local = inFlow.map { RenderUnit.UserText(it.localId, it.text, null, it.delivery, it.deliveryText) }
        val outbox =
            waiting.map {
                val failed = it.delivery == DeliveryState.FAILED || it.delivery == DeliveryState.FAILED_PERMANENT
                OutboxLine(it.localId, it.text, it.deliveryText.orEmpty(), failed, it.delivery == DeliveryState.FAILED)
            }
        return local to outbox
    }

    private companion object {
        const val INPUT_MS = 20_000L

        /** 跨台那一跳说那台够不到（同断线：没回执，接上再交）。 */
        const val OFFLINE = "offline"
    }
}

/** 输入框上方那一排里的一句：交出去了、会话那台还没回执（「发送中」· 撤）；那台拒了 ⇒ [failed]、[said] 是那台写的那一句，原因改得了 ⇒ [retry]。 */
data class OutboxLine(
    val id: String,
    val text: String,
    val said: String,
    val failed: Boolean,
    val retry: Boolean,
)

/** 待发里的那一句：回执到之前在输入框上方，回执到了进流，记录里出现了就消失。 */
internal class Outgoing(
    val localId: String,
    val text: String,
    var delivery: DeliveryState?,
    var deliveryText: String?,
    var attempts: Int = 1,
    /** 随这一句发的附件编号（重发照带）。 */
    val attachments: List<String> = emptyList(),
) {
    /** 这一句已随 `session-open` 交出去了（结局已标上）。 */
    var handed = false

    /** 会话那台的回执到了（已落盘）：进消息流。 */
    var receipt = false

    /** 正在交（同一句不同时交两遍）。 */
    var inFlight = false

    /** 回执是上次开着时到的：接上再交一遍，拿此刻排第几。 */
    var refresh = false
}

/** 没送到的那一句（那台写的原因）。 */
internal fun failed(why: String?) = DeliveryState.FAILED to copyText("terminal.input.failed", "why" to why.orEmpty())

/** 气泡下的标 ⇒ 落盘的那几种结局之一：回执到了（排着）记 `delivered`，还没回执记 `pending`。 */
private fun outcomeOf(d: DeliveryState?): String =
    when (d) {
        null, DeliveryState.QUEUED -> PendingOutcome.DELIVERED
        DeliveryState.SENDING -> PendingOutcome.PENDING
        DeliveryState.UNSURE -> PendingOutcome.UNSURE
        DeliveryState.FAILED, DeliveryState.FAILED_PERMANENT -> PendingOutcome.REFUSED
    }

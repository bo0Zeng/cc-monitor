package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.cache.ChatCache
import com.ccmonitor.mobile.cache.Pending
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionQueueCancel
import com.ccmonitor.mobile.core.claude.link.SessionQueuePut
import com.ccmonitor.mobile.core.claude.model.DeliveryState
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.PendingOutcome
import com.ccmonitor.mobile.core.wire.gen.Rewind
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import com.ccmonitor.mobile.ui.chat.attach.AttachTray
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

/**
 * 一条会话的待发：这一端交出去、会话那台还没回执的那几句（输入框上方「发送中」· 撤），同桌面 `outbox.ts`：回执到了（那台已落盘）⇒ 离开待发，
 * 之后只照那台的名单（排队 · 第 N）与记录画，这一端不另留一行。
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
     * 交那一句：投进 [sid] 那条会话的输入队（`session-queue-put`）。回执到了（那台已落盘）⇒ 离开待发（[received]）；那台拒了（码）⇒ 留在待发、标没送到；
     * 没回执（断线 · 期限到了 · 那台离线）⇒ 照旧「发送中」，接上再交同一个编号。
     */
    suspend fun put(
        o: Outgoing,
        sid: String,
    ) {
        o.inFlight = true
        val r = link.call(SessionQueuePut.COMMAND, SessionQueuePut.args(sid, o.localId, o.text, System.currentTimeMillis(), o.attachments, o.rewind), INPUT_MS)
        o.inFlight = false
        if (lines.none { it === o }) return
        when {
            r is Reply.Ok ->
                SessionQueuePut.of(r.data)?.let { received(o) }
                    ?: mark(o, DeliveryState.FAILED, failed(unreadableReply(machine, SessionQueuePut.COMMAND, r.data.toString()).text).second)
            r is Reply.Failed && r.failure.code != OFFLINE -> mark(o, DeliveryState.FAILED, failed(r.problem(machine, SessionQueuePut.COMMAND, link.offline.value)?.text).second)
            else -> Unit
        }
    }

    /** 回执到了（投进队的 · 随 `session-open` 交进去的）：那台已落盘 ⇒ 离开待发、本地那一份删掉；那一句此刻在哪照那台的名单与记录。 */
    fun received(o: Outgoing) {
        o.receipt = true
        lines.removeAll { it === o }
        publish()
        attach?.settled(o.localId)
        scope.launch { cache.forget(listOf(o.localId)) }
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
        val o = lines.firstOrNull { it.localId == localId } ?: return
        lines -= o
        publish()
        scope.launch {
            cache.forget(listOf(o.localId))
            attach?.settled(o.localId)
            sid?.let { link.call(SessionQueueCancel.COMMAND, SessionQueueCancel.args(it, o.localId), INPUT_MS) }
        }
    }

    /** 再发一次那台拒了的那一句（能改的原因才有这颗）：同一个编号，标回「发送中」、次数加一；不是没送到的那一句 ⇒ `null`。 */
    fun retrying(localId: String): Outgoing? {
        val o = lines.firstOrNull { it.localId == localId && it.delivery == DeliveryState.FAILED } ?: return null
        o.delivery = DeliveryState.SENDING
        o.deliveryText = copyText("mobChat.delivery.sending")
        o.attempts++
        return o
    }

    /** 收进待发（编辑已发的那几句：确认那一下 · 开屏照编辑草稿读回的）；同一个编号已在 ⇒ 不再收。 */
    fun adopt(os: List<Outgoing>) {
        lines += os.filter { n -> lines.none { it.localId == n.localId } }
    }

    /** 该再交一遍的那几句：还没回执也没在交的。 */
    fun due(): List<Outgoing> = lines.filter { !it.inFlight && it.delivery == DeliveryState.SENDING }

    /**
     * 待发里这条会话的那几句挂回来：那台拒过的 ⇒ 没送到；别的（上次没等到回执 App 就没了）⇒ 回到「发送中」，等接上那台照同一个编号再交一遍（[due]）。
     * 回执到过的（后台重交落的）不挂、删掉：那一句已在那台的名单或记录里。
     */
    suspend fun restore(s: String) {
        val known = lines.map { it.localId }.toSet()
        val (landed, waiting) = cache.pending(s).filterNot { it.id in known }.partition { it.outcome == PendingOutcome.DELIVERED }
        if (landed.isNotEmpty()) cache.forget(landed.map { it.id })
        for (p in waiting) {
            val (d, text) = if (p.outcome == PendingOutcome.REFUSED) failed(null) else DeliveryState.SENDING to copyText("mobChat.delivery.sending")
            lines += Outgoing(p.id, p.text, d, text, p.attempts, attach?.handedOf(p.id).orEmpty())
        }
    }

    /** 落库一句新交出去的（[draftKey] 那张草稿同一个事务删掉）。 */
    suspend fun keep(
        draftKey: String,
        o: Outgoing,
        sid: String?,
    ) = cache.promote(draftKey, Pending(o.localId, sid, o.text, PendingOutcome.PENDING, o.attempts))

    /** 输入框上方那一排（还没回执的那几句）。 */
    fun view(): List<OutboxLine> =
        lines.map {
            val failed = it.delivery == DeliveryState.FAILED || it.delivery == DeliveryState.FAILED_PERMANENT
            OutboxLine(it.localId, it.text, it.deliveryText.orEmpty(), failed, it.delivery == DeliveryState.FAILED)
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

/** 待发里的那一句：回执到之前在输入框上方，回执到了离开待发。 */
internal class Outgoing(
    val localId: String,
    val text: String,
    var delivery: DeliveryState?,
    var deliveryText: String?,
    var attempts: Int = 1,
    /** 随这一句发的附件编号（重发照带）。 */
    val attachments: List<String> = emptyList(),
    /** 编辑已发的那一句（`SaidEdit.kt`）：会话那台投递时先退到 `rewind.to` 之前再说它（重发照带）；普通的一句 ⇒ `null`。 */
    val rewind: Rewind? = null,
) {
    /** 这一句已随 `session-open` 交出去了（结局已标上）。 */
    var handed = false

    /** 会话那台的回执到了（已落盘）：已离开待发。 */
    var receipt = false

    /** 正在交（同一句不同时交两遍）。 */
    var inFlight = false
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

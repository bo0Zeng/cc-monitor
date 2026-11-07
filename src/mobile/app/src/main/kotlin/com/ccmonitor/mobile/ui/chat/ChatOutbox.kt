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
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.Rewind
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.ui.chat.attach.AttachTray
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

/**
 * 一条会话的待发：这一端交出去、会话那台还没回执的那几句（输入框上方那一排 · 撤），同桌面 `outbox.ts`：回执到了（那台已落盘）⇒ 离开待发，
 * 之后只照那台的名单与记录画，这一端不另留一行。
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
     * 没回执（断线 · 期限到了 · 回包读不出 · 那台离线）⇒ 那一行换成那台的 `heldSaid`，接上再交同一个编号。
     */
    suspend fun put(
        o: Outgoing,
        sid: String,
    ) {
        o.inFlight = true
        o.ended = false
        publish()
        val r = link.call(SessionQueuePut.COMMAND, SessionQueuePut.args(sid, o.localId, o.text, System.currentTimeMillis(), o.attachments, o.rewind), INPUT_MS)
        o.inFlight = false
        if (lines.none { it === o }) return
        val landed = (r as? Reply.Ok)?.let { SessionQueuePut.of(it.data) }
        when {
            landed != null -> received(o)
            r is Reply.Failed && r.failure.code != OFFLINE -> mark(o, DeliveryState.FAILED, r.problem(machine, SessionQueuePut.COMMAND, link.offline.value)?.text)
            else -> {
                o.ended = true
                publish()
            }
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

    /** 撤待发里一句：本地删掉、回那一句的字（回到输入框）；万一已经交到那台了，照编号请它撤掉（送过的撤不了，那台说了算）。不在 ⇒ `null`。 */
    fun cancel(
        localId: String,
        sid: String?,
    ): String? {
        val o = lines.firstOrNull { it.localId == localId } ?: return null
        lines -= o
        publish()
        scope.launch {
            cache.forget(listOf(o.localId))
            attach?.settled(o.localId)
            sid?.let { link.call(SessionQueueCancel.COMMAND, SessionQueueCancel.args(it, o.localId), INPUT_MS) }
        }
        return o.text
    }

    /** 收进待发（编辑已发的那几句：确认那一下 · 开屏照编辑草稿读回的）；同一个编号已在 ⇒ 不再收。 */
    fun adopt(os: List<Outgoing>) {
        lines += os.filter { n -> lines.none { it.localId == n.localId } }
    }

    /** 该再交一遍的那几句：还没回执也没在交的。 */
    fun due(): List<Outgoing> = lines.filter { !it.inFlight && it.delivery == DeliveryState.SENDING }

    /**
     * 待发里这条会话的那几句挂回来：那台拒过的 ⇒ 未送进会话；别的（上次没等到回执 App 就没了）⇒ 交了没回执（那台的 `heldSaid`），
     * 等接上那台照同一个编号再交一遍（[due]）。回执到过的（后台重交落的）不挂、删掉：那一句已在那台的名单或记录里。
     */
    suspend fun restore(s: String) {
        val known = lines.map { it.localId }.toSet()
        val (landed, waiting) = cache.pending(s).filterNot { it.id in known }.partition { it.outcome == PendingOutcome.DELIVERED }
        if (landed.isNotEmpty()) cache.forget(landed.map { it.id })
        for (p in waiting) {
            val refused = p.outcome == PendingOutcome.REFUSED
            val d = if (refused) DeliveryState.FAILED else DeliveryState.SENDING
            val text = if (refused) copyText("beQueue.said.undelivered") else null
            lines += Outgoing(p.id, p.text, d, text, p.attempts, attach?.handedOf(p.id).orEmpty()).also { it.ended = !refused }
        }
    }

    /** 落库一句新交出去的（[draftKey] 那张草稿同一个事务删掉）。 */
    suspend fun keep(
        draftKey: String,
        o: Outgoing,
        sid: String?,
    ) = cache.promote(draftKey, Pending(o.localId, sid, o.text, PendingOutcome.PENDING, o.attempts))

    /**
     * 输入框上方那一排（还没回执的那几句），同桌面：那台拒了 ⇒ 那台写的那一句（错误色）；交出去、回执还没到 ⇒「发送中 · 等确认」；
     * 交了没回执、这一问已结束（淡）：那台此刻断着 ⇒ 那台机器成品的 `heldSaid`；连着（超时 · 回包读不出）⇒「排队 · 无应答」
     * （不借 heldSaid：连着时说离线是假话）。两种都是接上照原编号重交。
     */
    fun view(): List<OutboxLine> {
        val st = link.state.value
        val held = if (st is LinkLine.Down) st.machine.heldSaid else copyText("beQueue.said.noAnswer")
        return lines.map {
            val failed = it.delivery == DeliveryState.FAILED || it.delivery == DeliveryState.FAILED_PERMANENT
            val waiting = it.delivery == DeliveryState.SENDING
            val said =
                when {
                    failed -> it.deliveryText ?: copyText("beQueue.said.undelivered")
                    waiting && it.ended -> held
                    waiting -> copyText("beQueue.said.confirming")
                    else -> it.deliveryText.orEmpty()
                }
            OutboxLine(it.localId, it.text, said, failed, held = waiting && it.ended)
        }
    }

    private companion object {
        const val INPUT_MS = 20_000L

        /** 跨台那一跳说那台够不到（同断线：没回执，接上再交）。 */
        const val OFFLINE = "offline"
    }
}

/** 输入框上方那一排里的一句：交出去了、会话那台还没回执（那一句 ·［撤回］）；那台拒了 ⇒ [failed]；交了没回执 ⇒ [held]（淡着）。 */
data class OutboxLine(
    val id: String,
    val text: String,
    val said: String,
    val failed: Boolean,
    val held: Boolean,
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

    /** 交过、这一问结束了还没回执（断线 · 期限到了 · 回包读不出 · 上次 App 没等到）：那一行照那台的 `heldSaid`。再交那一下清掉。 */
    var ended = false
}

/** 气泡下的标 ⇒ 落盘的那几种结局之一：回执到了（排着）记 `delivered`，还没回执记 `pending`。 */
private fun outcomeOf(d: DeliveryState?): String =
    when (d) {
        null, DeliveryState.QUEUED -> PendingOutcome.DELIVERED
        DeliveryState.SENDING -> PendingOutcome.PENDING
        DeliveryState.UNSURE -> PendingOutcome.UNSURE
        DeliveryState.FAILED, DeliveryState.FAILED_PERMANENT -> PendingOutcome.REFUSED
    }

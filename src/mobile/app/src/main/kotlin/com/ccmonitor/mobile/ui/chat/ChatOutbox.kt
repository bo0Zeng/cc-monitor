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
import com.ccmonitor.mobile.core.wire.gen.QueueList
import com.ccmonitor.mobile.core.wire.gen.Rewind
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import com.ccmonitor.mobile.ui.chat.attach.AttachTray
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * 一条会话的待发：这一端交出去、会话那台还没回执的那几句（输入框上方那一排 · 撤），同桌面 `outbox.ts`：回执到了（那台已落盘）、
 * 或那台的名单说已送（[wentIn]）⇒ 离开待发，之后只照那台的名单与记录画，这一端不另留一行。
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
    /** 同一个调度、不挂在屏的作用域下：撤那一问在它上面跑，关屏不取消（[withdraw]）。 */
    private val outlives = CoroutineScope(scope.coroutineContext.minusKey(Job) + SupervisorJob())

    val lines = mutableListOf<Outgoing>()

    /**
     * 交那一句：投进 [sid] 那条会话的输入队（`session-queue-put`）。回执到了（那台已落盘）⇒ 离开待发（[received]）；那台拒了（码）⇒ 留在待发、标没送到；
     * 没回执（断线 · 期限到了 · 回包读不出 · 那台离线）⇒ 那一行换成那台的 `heldSaid`，接上再交同一个编号。
     * 那一问回来时那一句已离开待发（名单说已送 · 撤了）⇒ 不挂回、不再叫结局；撤那一问没答复本地拿掉的，回执到了照记上（编辑草稿照它删）。
     * 回 true ⇐ 回执是这一问带回来的、那一句还在待发里。
     */
    suspend fun put(
        o: Outgoing,
        sid: String,
    ): Boolean {
        o.inFlight = true
        o.asked = true
        o.ended = false
        publish()
        val r = link.call(SessionQueuePut.COMMAND, SessionQueuePut.args(sid, o.localId, o.text, System.currentTimeMillis(), o.attachments, o.rewind), INPUT_MS)
        o.inFlight = false
        val landed = (r as? Reply.Ok)?.let { SessionQueuePut.of(it.data) }
        if (lines.none { it === o }) {
            if (landed != null && !o.recalled) o.receipt = true
            return false
        }
        when {
            landed != null -> {
                received(o)
                return true
            }
            r is Reply.Failed && r.failure.code != OFFLINE -> mark(o, DeliveryState.FAILED, r.problem(machine, SessionQueuePut.COMMAND, link.offline.value)?.text)
            else -> {
                o.ended = true
                publish()
            }
        }
        return false
    }

    /**
     * 那台的名单（推来的 · 重问的 · 撤回包里的）`sent` 里有待发里某一句的编号 ⇒ 同回执到了（[received]）：离开待发、本地那一份删掉；
     * 在飞的那一问回来不再挂回，交了没回执那一行也由它收掉、不等接上。回收下的那几句。
     */
    fun wentIn(ids: Collection<String>): List<Outgoing> {
        if (ids.isEmpty()) return emptyList()
        val gone = lines.filter { it.localId in ids }
        gone.forEach(::received)
        return gone
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
        val outcome = outcomeOf(delivery)
        // 那台拒了 ⇒ 它那一句跟结局同一下落库，重开照它画。
        scope.launch { cache.settle(o.localId, outcome, o.attempts, text.takeIf { outcome == PendingOutcome.REFUSED }) }
    }

    /**
     * 撤待发里一句（同桌面 `Outbox.withdraw`）。那台拒过的（没进它的队）· 还没交到哪条会话（[sid] 缺）· 一次都还没交过（落库那一下还没完）
     * ⇒ 本地拿掉，当场交 [Withdrawn.Back]，不问那台。
     * 别的先不动那一行、不回字，记作撤着（同一句撤着时再点不另问、接上不重交；先落盘，关屏 · App 被杀了下次开屏也不重交，接着问，见 [restore]），
     * 照编号请那台撤，答了才交结局：
     * 撤成了 ⇒ 拿掉、[Withdrawn.Back]；没撤成（在送 · 送过）⇒ 当送进去了（[received]）、[Withdrawn.In]；
     * 那一问没答复 ⇒ 本地拿掉（不再重交）、[Withdrawn.Lost] 带那一问回来的那一句：送没送进去说不清。不在 · 撤不回（[Outgoing.opening]）· 正在问 ⇒ 什么都不交。
     * 撤这一问不随屏的作用域取消：关屏时照样交到那台；答复回来时屏已收掉 ⇒ 不交结局，落盘那一行留着撤着，下次开屏照它接着问（[unasked]；那台已记撤过 ⇒ 答撤成了）。
     * 接着问的那一句（上一屏撤过，那台可能已撤成）没答复 ⇒ 不拿掉，仍撤着，下一回接上再问；字只在答撤成了时回框。
     * [asking]：记成撤着、要去问那台的那一下叫一声（问之前）；接着问的那一句照样叫；不在待发里（回执与撤正好撞上）· 撤不回的 · 当场拿掉的 · 正在问的都不叫。
     */
    fun withdraw(
        localId: String,
        sid: String?,
        asking: () -> Unit,
        ended: (Withdrawn) -> Unit,
    ) {
        val o = lines.firstOrNull { it.localId == localId } ?: return
        if (o.asking || o.opening) return
        val refused = o.delivery == DeliveryState.FAILED || o.delivery == DeliveryState.FAILED_PERMANENT
        if (refused || sid == null || !o.asked) {
            o.recalled = true
            drop(o)
            ended(Withdrawn.Back(o, null))
            return
        }
        val resumed = o.withdrawing
        o.withdrawing = true
        o.asking = true
        asking()
        outlives.launch {
            if (!resumed) cache.settle(localId, PendingOutcome.WITHDRAWING, o.attempts, null)
            val r = link.call(SessionQueueCancel.COMMAND, SessionQueueCancel.args(sid, localId), INPUT_MS)
            if (scope.isActive) answered(o, r, resumed, ended)
        }
    }

    /** 撤那一问的答复（屏还在）：照 [withdraw] 那几种结局走；[resumed] ＝ 接着问的那一句（没答复不拿掉、仍撤着）。 */
    private fun answered(
        o: Outgoing,
        r: Reply,
        resumed: Boolean,
        ended: (Withdrawn) -> Unit,
    ) {
        o.asking = false
        val answer = (r as? Reply.Ok)?.let { SessionQueueCancel.of(it.data) }
        if (answer != null || !resumed) o.withdrawing = false
        when {
            answer == null -> {
                if (!resumed) drop(o)
                val said = if (r is Reply.Ok) unreadableReply(machine, SessionQueueCancel.COMMAND, r.data.toString()).text else r.problem(machine, SessionQueueCancel.COMMAND, link.offline.value)?.text
                ended(Withdrawn.Lost(o, said))
            }
            answer.cancelled -> {
                o.recalled = true
                drop(o)
                ended(Withdrawn.Back(o, answer.queue))
            }
            else -> {
                if (lines.any { it === o }) received(o)
                ended(Withdrawn.In(o, answer.said, answer.queue))
            }
        }
    }

    /** 本地拿掉那一句（不问那台、不再交）：待发里删掉、本地那一份删掉。不在 ⇒ `null`。 */
    fun drop(localId: String): Outgoing? = lines.firstOrNull { it.localId == localId }?.also { drop(it) }

    private fun drop(o: Outgoing) {
        if (!lines.removeAll { it === o }) return
        publish()
        scope.launch {
            cache.forget(listOf(o.localId))
            attach?.settled(o.localId)
        }
    }

    /** 收进待发（编辑已发的那几句：确认那一下 · 开屏照编辑草稿读回的，后者上次可能交过 ⇒ 都当交过，撤要问那台）；同一个编号已在 ⇒ 不再收。 */
    fun adopt(os: List<Outgoing>) {
        lines += os.filter { n -> lines.none { it.localId == n.localId } }.onEach { it.asked = true }
    }

    /** 该再交一遍的那几句：还没回执、没在交、没在撤的。 */
    fun due(): List<Outgoing> = lines.filter { !it.inFlight && !it.withdrawing && it.delivery == DeliveryState.SENDING }

    /** 撤着、此刻没在问的那几句（上一屏撤着时关了 · 接着问没答复的）的编号：接上那台就照同一个编号接着请它撤。 */
    fun unasked(): List<String> = lines.filter { it.withdrawing && !it.asking }.map { it.localId }

    /**
     * 待发里这条会话的那几句挂回来：那台拒过的 ⇒ 标失败、照落库的那台那一句（迁上来的老行没有 ⇒ [view] 照实说原因未留存）；别的（上次没等到回执 App 就没了）⇒ 交了没回执（那台的 `heldSaid`），
     * 等接上那台照同一个编号再交一遍（[due]）。回执到过的（后台重交落的）不挂、删掉：那一句已在那台的名单或记录里。
     * 上次撤着的（撤那一问没等到答复屏就收了 · App 没了）照样挂回、仍撤着，不重交：接上那台照同一个编号接着请它撤（[unasked] · [withdraw]）。
     */
    suspend fun restore(s: String) {
        val known = lines.map { it.localId }.toSet()
        val (landed, waiting) = cache.pending(s).filterNot { it.id in known }.partition { it.outcome == PendingOutcome.DELIVERED }
        if (landed.isNotEmpty()) cache.forget(landed.map { it.id })
        for (p in waiting) {
            val refused = p.outcome == PendingOutcome.REFUSED
            val d = if (refused) DeliveryState.FAILED else DeliveryState.SENDING
            lines +=
                Outgoing(p.id, p.text, d, p.refusedSaid, p.attempts, attach?.handedOf(p.id).orEmpty()).also {
                    it.ended = !refused
                    it.asked = !refused
                    it.withdrawing = p.outcome == PendingOutcome.WITHDRAWING
                }
        }
    }

    /** 落库一句新交出去的（[draftKey] 那张草稿同一个事务删掉）；落好时它已离开待发（落库那一下里撤了）⇒ 落下的那一份删掉。 */
    suspend fun keep(
        draftKey: String,
        o: Outgoing,
        sid: String?,
    ) {
        cache.promote(draftKey, Pending(o.localId, sid, o.text, PendingOutcome.PENDING, o.attempts))
        if (lines.none { it === o }) cache.forget(listOf(o.localId))
    }

    /**
     * 输入框上方那一排（还没回执的那几句），同桌面：那台拒了 ⇒ 那台写的那一句（错误色）；交出去、回执还没到 ⇒「发送中 · 等确认」；
     * 交了没回执、这一问已结束（淡）：那台此刻断着 ⇒ 那台机器成品的 `heldSaid`；连着（超时 · 回包读不出）⇒「排队 · 无应答」
     * （不借 heldSaid：连着时说离线是假话）。两种都是接上照原编号重交。
     * 撤着、此刻没在问的那一句（上一屏撤着 · 接着问没答复），那台此刻断着 ⇒ 淡着、照那台成品的 `withdrawHeld`：它不再交，借 `heldSaid` 说补交是假话。
     */
    fun view(): List<OutboxLine> {
        val down = (link.state.value as? LinkLine.Down)?.machine
        val held = down?.heldSaid ?: copyText("beQueue.said.noAnswer")
        return lines.map {
            val failed = it.delivery == DeliveryState.FAILED || it.delivery == DeliveryState.FAILED_PERMANENT
            val waiting = it.delivery == DeliveryState.SENDING
            val parked = if (waiting && parked(it)) down?.withdrawHeld else null
            val said =
                when {
                    // 拒了 ⇒ 那台写的那一句（交的那一下回来的 · 重开时读回落库的那一句），照摆；
                    // 只有状态库 28 版迁上来的被拒行没存那一句：照实说原因未留存（同后端未送达那一处一把键）。
                    failed -> it.deliveryText ?: copyText("beQueue.said.noWhy")
                    parked != null -> parked
                    waiting && it.ended -> held
                    waiting -> copyText("beQueue.said.confirming")
                    else -> it.deliveryText.orEmpty()
                }
            OutboxLine(it.localId, it.text, said, failed, held = parked != null || (waiting && it.ended), withdrawable = !it.opening)
        }
    }

    /** 撤着、此刻没在问（等接上那台接着问撤没撤成）。 */
    private fun parked(o: Outgoing): Boolean = o.withdrawing && !o.asking

    private companion object {
        const val INPUT_MS = 20_000L

        /** 跨台那一跳说那台够不到（同断线：没回执，接上再交）。 */
        const val OFFLINE = "offline"
    }
}

/**
 * 输入框上方那一排里的一句：交出去了、会话那台还没回执（那一句 ·［撤回］）；那台拒了 ⇒ [failed]；交了没回执 ⇒ [held]（淡着）；
 * 撤不回（新建会话随起交着的头一句）⇒ [withdrawable] 假，不摆［撤回］。
 */
data class OutboxLine(
    val id: String,
    val text: String,
    val said: String,
    val failed: Boolean,
    val held: Boolean,
    val withdrawable: Boolean = true,
)

/** 撤待发里一句的结局（[ChatOutbox.withdraw]）：[o] 是撤的那一句。 */
internal sealed interface Withdrawn {
    val o: Outgoing

    /** 撤成了（那台答撤成了 · 拒过的 · 没交到那台的，本地拿掉）：字回框；[queue] 撤回包里的名单（没问那台 ⇒ `null`）。 */
    class Back(
        override val o: Outgoing,
        val queue: QueueList?,
    ) : Withdrawn

    /** 没撤成（在送 · 送过）：当送进去了，字不回框；[said] 撤回包里那台写好的那一句（框下照摆）· [queue] 撤回包里的名单。 */
    class In(
        override val o: Outgoing,
        val said: String,
        val queue: QueueList,
    ) : Withdrawn

    /** 撤那一问没答复（断着 · 期限到了 · 回包读不出）：本地拿掉，字不回框；[said] 那一问回来的那一句（框下照摆）。 */
    class Lost(
        override val o: Outgoing,
        val said: String?,
    ) : Withdrawn
}

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

    /** 撤着（点了撤、那台还没答成；已落盘）：那一行照旧摆，接上不重交。 */
    var withdrawing = false

    /** 撤那一问此刻在路上：再点不另问。 */
    var asking = false

    /** 新建会话的头一句、正随 `session-open` 交着（作为启动参数，撤不回）：不摆［撤回］、点撤不理；起完（成没成）清掉。 */
    var opening = false

    /** 交过（那台可能收到过）：撤要问那台；一次都还没交过 ⇒ 本地拿掉。 */
    var asked = false

    /** 撤成了（字已回框）：在飞的那一问回执晚到也不再记回执。 */
    var recalled = false

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

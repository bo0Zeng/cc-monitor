package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionQueueCancel
import com.ccmonitor.mobile.core.claude.link.SessionQueueRead
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.Changed
import com.ccmonitor.mobile.core.wire.gen.QueueList
import com.ccmonitor.mobile.core.wire.gen.QueueState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.launch

/**
 * 一条会话的输入队，照会话那台那一份画：那台推 `changed {session_queue, key: sid}` 带整份名单（没带 ⇒ 重问 `session-queue-read`），
 * 撤（`session-queue-cancel`）回撤完的整份。这一端不各自记队；每条那一句（`said`）是那台写好的，照摆。
 */
internal class ChatQueue(
    private val link: CoreLink,
    private val scope: CoroutineScope,
    private val publish: () -> Unit,
) {
    /** 那台此刻那一份（还没问到 ⇒ `null`）。 */
    @Volatile private var list: QueueList? = null

    /** 跟那台推来的那几帧（[sid] 是此刻看着的那一条）。 */
    fun watch(
        changed: Flow<Changed>,
        sid: () -> String?,
    ): Job =
        scope.launch {
            changed.collect { c ->
                val s = sid()
                if (c is Changed.SessionQueue && c.key == s) c.body?.let(::got) ?: read(s)
            }
        }

    /** 问一次那条会话此刻的名单；问不到 ⇒ 留着手上那一份（推送或下一次重问补上）。 */
    suspend fun read(sid: String) {
        val r = link.call(SessionQueueRead.COMMAND, SessionQueueRead.args(sid), CALL_MS)
        if (r is Reply.Ok) SessionQueueRead.of(r.data)?.let(::got)
    }

    /** 撤名单里一条（照编号请那台撤），照回的整份名单画；撤成了 ⇒ 那一句的字交 [recalled]（回到输入框）。 */
    fun cancel(
        id: String,
        sid: String,
        recalled: (String) -> Unit = {},
    ) {
        val text = textOf(id)
        scope.launch {
            val r = link.call(SessionQueueCancel.COMMAND, SessionQueueCancel.args(sid, id), CALL_MS)
            if (r is Reply.Ok) {
                SessionQueueCancel.of(r.data)?.let {
                    if (it.cancelled && text != null) recalled(text)
                    got(it.queue)
                }
            }
        }
    }

    private fun got(q: QueueList) {
        list = q
        publish()
    }

    /** 输入框上方那一排：名单里每一条（先后照那台）。 */
    fun lines(): List<QueueLine> = list?.items.orEmpty().map { QueueLine(it.id, it.said, it.text, it.state == QueueState.UNDELIVERED) }

    /** 名单里那一条的字（撤成了回到输入框）；不在 ⇒ `null`。 */
    fun textOf(id: String): String? = list?.items?.firstOrNull { it.id == id }?.text

    /** 换了一条会话：手上那一份作废。 */
    fun reset() {
        list = null
    }

    private companion object {
        const val CALL_MS = 10_000L
    }
}

/** 名单里的一条：会话那台写好的那一句（发送中 · 等确认 · 需手动 · 答完再送 · 未送进会话 …）· 那一句 · 没送进会话。 */
data class QueueLine(
    val id: String,
    val said: String,
    val text: String,
    val undelivered: Boolean,
)

/**
 * 输入框上方那一排的一行（名单与待发同一种，一条一行，同桌面 `queue-row.ts`）：那一句此刻的样子 · 开头（只取第一行）·［撤回］。
 * 不写第几、不写哪一端。错误色 ⇐ 没送进会话 · 那台拒了；[held] ⇒ 交了没回执（淡着）；[listed] ⇒［撤回］撤名单里那一条，否则撤待发。
 */
data class QueueRowFace(
    val id: String,
    val state: String,
    val head: String,
    val error: Boolean,
    val held: Boolean,
    val listed: Boolean,
)

/** 输入框上方那一排：每条一行 · [stale] ＝ 那台离线时排头那一行（「名单为上次所知」，整排淡着；连着 · 没有一行 ⇒ `null`）。 */
data class QueueRows(
    val rows: List<QueueRowFace>,
    val stale: String?,
)

/** 名单在前（先后照那台），待发接在后面；[offline] ＝ 那台此刻离线（排头说名单是上次所知；是哪台、为什么看机器条），同桌面 `queue-row.ts`。 */
fun queueRows(
    queue: List<QueueLine>,
    outbox: List<OutboxLine>,
    offline: Boolean = false,
): QueueRows {
    val rows =
        queue.map { QueueRowFace(it.id, it.said, headOf(it.text), error = it.undelivered, held = false, listed = true) } +
            outbox.map { QueueRowFace(it.id, it.said, headOf(it.text), error = it.failed, held = it.held, listed = false) }
    val stale = copyText("terminal.queue.stale").takeIf { offline && rows.isNotEmpty() }
    return QueueRows(rows, stale)
}

private fun headOf(text: String): String {
    val first = text.lineSequence().firstOrNull { it.isNotBlank() } ?: return ""
    return first.trim()
}

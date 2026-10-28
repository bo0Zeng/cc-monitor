package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionQueueCancel
import com.ccmonitor.mobile.core.claude.link.SessionQueueRead
import com.ccmonitor.mobile.core.wire.gen.Changed
import com.ccmonitor.mobile.core.wire.gen.QueueList
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.launch

/**
 * 一条会话的输入队，照会话那台那一份画：那台推 `changed {session_queue, key: sid}` 带整份名单（没带 ⇒ 重问 `session-queue-read`），
 * 撤（`session-queue-cancel`）回撤完的整份。这一端不各自记队；每条那一句（`said`）与发自哪一端（`from`）是那台写好的，照摆。
 */
internal class ChatQueue(
    private val link: CoreLink,
    private val scope: CoroutineScope,
    private val publish: () -> Unit,
) {
    /** 那台此刻那一份（还没问到 ⇒ `null`）。 */
    @Volatile private var list: QueueList? = null

    /** 在名单里见过的编号（见过、又不在名单也不在已送进 ⇒ 撤掉了）。 */
    private val seen = mutableSetOf<String>()

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

    /** 撤名单里一条（照编号请那台撤），照回的整份名单画。 */
    fun cancel(
        id: String,
        sid: String,
    ) {
        scope.launch {
            val r = link.call(SessionQueueCancel.COMMAND, SessionQueueCancel.args(sid, id), CALL_MS)
            if (r is Reply.Ok) SessionQueueCancel.of(r.data)?.let { got(it.queue) }
        }
    }

    private fun got(q: QueueList) {
        list = q
        synchronized(seen) { q.items.forEach { seen += it.id } }
        publish()
    }

    /** 输入框上方那一排：名单里每一条（先后照那台）。 */
    fun lines(): List<QueueLine> = list?.items.orEmpty().map { QueueLine(it.id, it.said, it.from, it.text) }

    /** 那一句此刻在名单里（流里不再另画它）。 */
    fun holds(id: String): Boolean = list?.items.orEmpty().any { it.id == id }

    /** 那一句在名单里见过、此刻不在名单也不在已送进 ⇒ 被撤掉了（哪一端撤的都算）。 */
    fun gone(id: String): Boolean {
        val l = list ?: return false
        return synchronized(seen) { id in seen } && l.items.none { it.id == id } && id !in l.sent
    }

    /** 换了一条会话：手上那一份作废。 */
    fun reset() {
        list = null
        synchronized(seen) { seen.clear() }
    }

    private companion object {
        const val CALL_MS = 10_000L
    }
}

/** 输入框上方那一排里的一条：会话那台写好的那一句（排队 · 第 N …）· 发自哪一端 · 那一句 ·［撤回］。 */
data class QueueLine(
    val id: String,
    val said: String,
    val from: String,
    val text: String,
)

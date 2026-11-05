package com.ccmonitor.mobile.link

import com.ccmonitor.mobile.cache.LocalCache
import com.ccmonitor.mobile.cache.Pending
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionQueuePut
import com.ccmonitor.mobile.core.data.db.PendingOutcome
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.ui.chat.attach.AttachStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import java.util.concurrent.ConcurrentHashMap

/** 一台手上落盘的待发：该再交的那几句（有会话 · 还没回执 · 没被拒）· 交的结局记回去 · 那一句随带的附件编号。 */
interface HostOutbox {
    suspend fun due(): List<Pending>

    suspend fun settle(
        id: String,
        outcome: String,
        attempts: Int,
    )

    fun attachments(id: String): List<String>
}

/** 生产上那一份：待发住本地库（[LocalCache.due]），附件编号住那台的附件目录（[AttachStore.sentOf]）。 */
fun hostOutbox(
    cache: LocalCache,
    hostId: String,
    attach: AttachStore,
): HostOutbox =
    object : HostOutbox {
        override suspend fun due() = cache.due(hostId)

        override suspend fun settle(
            id: String,
            outcome: String,
            attempts: Int,
        ) = cache.settle(id, outcome, attempts)

        override fun attachments(id: String) = attach.sentOf(id)
    }

/**
 * 待发重交挂在「那台接上」那一下（进程级一份）：起着的每条常驻流一接上，就把那台名下各会话还没回执的待发照原编号再交一遍，
 * 不等打开那条会话（被杀重开后没打开的会话里的待发也照样在送）。开着的那几条（[open]）归各自的会话交、改屏上那一排，这里跳过：同一个编号只一处交。
 * 回执到了 ⇒ 记成已投（打开那条会话时它在流里）；那台拒了 ⇒ 记成没送到（不再交，打开时标出来）；没回执 ⇒ 留着等下一回接上。
 * 编辑已发的那几句（带 `rewind`）住编辑草稿、不在这里：打开那条会话时照旧重交。
 */
class PendingResend(
    feeds: Flow<Map<String, CoreLink>>,
    private val outboxOf: (hostId: String) -> HostOutbox,
    private val scope: CoroutineScope,
    /** 此刻开着的那几条会话（`ChatController` 持着的）：它们的待发归各自的 `ChatSession` 交，这里不交。 */
    private val open: () -> Set<String> = { emptySet() },
) {
    private val jobs = ConcurrentHashMap<String, Job>()

    init {
        scope.launch {
            feeds.collect { now ->
                for ((id, link) in now) if (!jobs.containsKey(id)) jobs[id] = watch(id, link)
            }
        }
    }

    private fun watch(
        id: String,
        link: CoreLink,
    ) = scope.launch {
        link.state
            .map { it is LinkLine.Up }
            .distinctUntilChanged()
            .filter { it }
            .collect { resend(link, outboxOf(id)) }
    }

    private suspend fun resend(
        link: CoreLink,
        box: HostOutbox,
    ) {
        val held = open()
        for (p in box.due()) {
            val sid = p.sid?.takeIf { it !in held } ?: continue
            val r = link.call(SessionQueuePut.COMMAND, SessionQueuePut.args(sid, p.id, p.text, System.currentTimeMillis(), box.attachments(p.id)), INPUT_MS)
            when {
                r is Reply.Ok && SessionQueuePut.of(r.data) != null -> box.settle(p.id, PendingOutcome.DELIVERED, p.attempts)
                r is Reply.Failed && r.failure.code != OFFLINE -> box.settle(p.id, PendingOutcome.REFUSED, p.attempts)
                else -> Unit
            }
        }
    }

    private companion object {
        const val INPUT_MS = 20_000L

        /** 跨台那一跳说那台够不到（同断线：没回执，下一回接上再交）。 */
        const val OFFLINE = "offline"
    }
}

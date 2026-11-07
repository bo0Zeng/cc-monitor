package com.ccmonitor.mobile.cache

import com.ccmonitor.mobile.core.claude.link.KeptBatches
import com.ccmonitor.mobile.core.claude.link.RawJson
import com.ccmonitor.mobile.core.claude.link.wire.RawPatch
import com.ccmonitor.mobile.core.claude.link.wire.rawPatch
import com.ccmonitor.mobile.core.data.db.CacheDao
import com.ccmonitor.mobile.core.data.db.Draft
import com.ccmonitor.mobile.core.data.db.HandDao
import com.ccmonitor.mobile.core.data.db.PendingBatch
import com.ccmonitor.mobile.core.data.db.PendingSend
import com.ccmonitor.mobile.core.data.db.RecordPage
import com.ccmonitor.mobile.core.data.db.SyncAnchor
import com.ccmonitor.mobile.core.rs.Core

/**
 * 手机落盘的那几样：缓存（记录页 · 续读点 · 看过哪几条，住缓存库，`CacheDao`；会话列表副本住嵌的核心）与这一端手上的（待发 · 草稿 · 在做的一组，住状态库，`HandDao`）。
 * 一台一份，成品原样写成 JSON 文本存、读回来原样交还。
 */
class LocalCache(
    private val dao: CacheDao,
    private val hand: HandDao,
    private val now: () -> Long = System::currentTimeMillis,
) {
    fun chat(hostId: String): ChatCache = RoomChatCache(dao, hand, hostId, now)

    fun list(hostId: String): ListCache = CoreListCache(hostId)

    fun batches(hostId: String): KeptBatches = RoomKeptBatches(hand, hostId)

    /** 这台名下该再交的待发（有会话 · 还没回执 · 没被拒），照落库的先后；那台一接上就照原编号重交（`PendingResend`）。 */
    suspend fun due(hostId: String): List<Pending> = hand.duePending(hostId).map { Pending(it.id, it.sid, it.text, it.outcome, it.attempts) }

    /** 记回一句交的结局（同 [ChatCache.settle]）。 */
    suspend fun settle(
        id: String,
        outcome: String,
        attempts: Int,
    ) = hand.settle(id, outcome, attempts)
}

private class RoomKeptBatches(
    private val dao: HandDao,
    private val hostId: String,
) : KeptBatches {
    override suspend fun all(): List<Pair<String, String>> = dao.batches(hostId).map { it.ticket to it.plan }

    override suspend fun put(
        ticket: String,
        plan: String,
    ) = dao.putBatch(PendingBatch(hostId, ticket, plan))

    override suspend fun drop(ticket: String) = dao.dropBatch(hostId, ticket)
}

/** 会话列表副本住嵌的核心（换 · 存盘都在那边）：这里只把回包原样交过去、把核心交回的那一行读成整份。 */
private class CoreListCache(
    private val hostId: String,
) : ListCache {
    override suspend fun held(): RawPatch? = fromCore(Core.listingHeld(hostId))

    override suspend fun keep(reply: Any?): RawPatch? = fromCore(Core.listingKeep(hostId, RawJson.write(reply)))

    /** 核心交回的那一行；空串（没有 · 读不动）⇒ `null`。 */
    private fun fromCore(line: String): RawPatch? = line.takeIf { it.isNotEmpty() }?.let(RawJson::read)?.let(::rawPatch)
}

private class RoomChatCache(
    private val dao: CacheDao,
    private val hand: HandDao,
    private val hostId: String,
    private val now: () -> Long,
) : ChatCache {
    private fun RecordPage.raw(): RawPage? = (RawJson.read(rows) as? List<*>)?.let { RawPage(start, end, it.toList()) }

    private fun RawPage.row(sid: String) = RecordPage(hostId, sid, start, end, RawJson.write(rows))

    private fun Anchor.row(sid: String) = SyncAnchor(hostId, sid, path, offset, seq, mark)

    override suspend fun load(sid: String): CachedRecords? {
        val pages = dao.pages(hostId, sid).map { it.raw() ?: return null }
        if (pages.isEmpty()) return null
        return CachedRecords(pages, dao.anchor(hostId, sid)?.let { Anchor(it.path, it.offset, it.seq, it.mark) })
    }

    override suspend fun viewed(sid: String) = dao.viewed(hostId, sid, now())

    override suspend fun replace(
        sid: String,
        pages: List<RawPage>,
        anchor: Anchor?,
    ) = dao.replacePages(hostId, sid, pages.map { it.row(sid) }, anchor?.row(sid))

    override suspend fun prepend(
        sid: String,
        pages: List<RawPage>,
    ) = dao.putPages(hostId, sid, pages.map { it.row(sid) }, null, keepAnchor = true)

    override suspend fun append(
        sid: String,
        rows: List<Any?>,
        anchor: Anchor?,
    ) {
        val last = dao.pages(hostId, sid).lastOrNull()?.raw()
        dao.putPages(hostId, sid, appendRows(last, rows, anchor?.offset).map { it.row(sid) }, anchor?.row(sid))
    }

    override suspend fun pending(sid: String): List<Pending> = hand.pending(hostId, sid).map { Pending(it.id, it.sid, it.text, it.outcome, it.attempts) }

    override suspend fun promote(
        draftSid: String,
        p: Pending,
    ) = hand.promote(hostId, draftSid, PendingSend(p.id, hostId, p.sid, p.text, now(), p.outcome, p.attempts))

    override suspend fun settle(
        id: String,
        outcome: String,
        attempts: Int,
    ) = hand.settle(id, outcome, attempts)

    override suspend fun bind(
        ids: List<String>,
        sid: String,
    ) = hand.bind(ids, sid)

    override suspend fun forget(ids: List<String>) = hand.forget(ids)

    override suspend fun draft(sid: String): String? = hand.draft(hostId, sid)?.text

    override suspend fun saveDraft(
        sid: String,
        text: String,
    ) = if (text.isEmpty()) hand.dropDraft(hostId, sid) else hand.putDraft(Draft(hostId, sid, text, now()))
}

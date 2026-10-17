package com.ccmonitor.mobile.cache

import com.ccmonitor.mobile.core.claude.link.RawJson
import com.ccmonitor.mobile.core.data.db.CacheDao
import com.ccmonitor.mobile.core.data.db.Draft
import com.ccmonitor.mobile.core.data.db.PendingSend
import com.ccmonitor.mobile.core.data.db.RecordPage
import com.ccmonitor.mobile.core.data.db.SyncAnchor

/** 缓存落在 Room 那五张表里（`CacheDao`）：一台一份，成品原样写成 JSON 文本存、读回来原样交还。 */
class LocalCache(
    private val dao: CacheDao,
    private val now: () -> Long = System::currentTimeMillis,
) {
    fun chat(hostId: String): ChatCache = RoomChatCache(dao, hostId, now)

    fun list(hostId: String): ListCache = RoomListCache(dao, hostId)
}

private class RoomListCache(
    private val dao: CacheDao,
    private val hostId: String,
) : ListCache {
    override suspend fun rows(): List<Any?> = dao.listed(hostId).mapNotNull { it.listRow?.let(RawJson::read) }

    override suspend fun save(rows: List<Pair<String, Any?>>) = dao.replaceList(hostId, rows.map { (sid, row) -> sid to RawJson.write(row) })
}

private class RoomChatCache(
    private val dao: CacheDao,
    private val hostId: String,
    private val now: () -> Long,
) : ChatCache {
    private fun RecordPage.raw(): RawPage? = (RawJson.read(rows) as? List<*>)?.let { RawPage(start, end, it.toList()) }

    private fun RawPage.row(sid: String) = RecordPage(hostId, sid, start, end, RawJson.write(rows), now())

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

    override suspend fun pending(sid: String): List<Pending> = dao.pending(hostId, sid).map { Pending(it.id, it.sid, it.text, it.outcome, it.attempts) }

    override suspend fun promote(
        draftSid: String,
        p: Pending,
    ) = dao.promote(hostId, draftSid, PendingSend(p.id, hostId, p.sid, p.text, null, now(), p.outcome, p.attempts))

    override suspend fun settle(
        id: String,
        outcome: String,
        attempts: Int,
    ) = dao.settle(id, outcome, attempts)

    override suspend fun bind(
        ids: List<String>,
        sid: String,
    ) = dao.bind(ids, sid)

    override suspend fun forget(ids: List<String>) = dao.forget(ids)

    override suspend fun draft(sid: String): String? = dao.draft(hostId, sid)?.text

    override suspend fun saveDraft(
        sid: String,
        text: String,
        cwd: String?,
        account: String?,
    ) = if (text.isEmpty()) dao.dropDraft(hostId, sid) else dao.putDraft(Draft(hostId, sid, text, cwd, account, null, null, now()))
}

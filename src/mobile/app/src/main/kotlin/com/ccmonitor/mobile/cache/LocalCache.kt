package com.ccmonitor.mobile.cache

import com.ccmonitor.mobile.core.claude.link.KeptBatches
import com.ccmonitor.mobile.core.claude.link.RawJson
import com.ccmonitor.mobile.core.claude.link.wire.SyncHand
import com.ccmonitor.mobile.core.claude.link.wire.SyncHead
import com.ccmonitor.mobile.core.claude.link.wire.Wire
import com.ccmonitor.mobile.core.data.db.CacheDao
import com.ccmonitor.mobile.core.data.db.CachedSession
import com.ccmonitor.mobile.core.data.db.Draft
import com.ccmonitor.mobile.core.data.db.ListHold
import com.ccmonitor.mobile.core.data.db.PendingBatch
import com.ccmonitor.mobile.core.data.db.PendingSend
import com.ccmonitor.mobile.core.data.db.RecordPage
import com.ccmonitor.mobile.core.data.db.SyncAnchor

/** 缓存落在 Room 那七张表里（`CacheDao`）：一台一份，成品原样写成 JSON 文本存、读回来原样交还。 */
class LocalCache(
    private val dao: CacheDao,
    private val now: () -> Long = System::currentTimeMillis,
) {
    fun chat(hostId: String): ChatCache = RoomChatCache(dao, hostId, now)

    fun list(hostId: String): ListCache = RoomListCache(dao, hostId)

    fun batches(hostId: String): KeptBatches = RoomKeptBatches(dao, hostId)
}

private class RoomKeptBatches(
    private val dao: CacheDao,
    private val hostId: String,
) : KeptBatches {
    override suspend fun all(): List<Pair<String, String>> = dao.batches(hostId).map { it.ticket to it.plan }

    override suspend fun put(
        ticket: String,
        plan: String,
    ) = dao.putBatch(PendingBatch(hostId, ticket, plan))

    override suspend fun drop(ticket: String) = dao.dropBatch(hostId, ticket)
}

private class RoomListCache(
    private val dao: CacheDao,
    private val hostId: String,
) : ListCache {
    override suspend fun hand(): SyncHand = dao.heldList(hostId).let { (hold, rows) -> handOf(hold, rows) }

    override suspend fun keep(hand: SyncHand) {
        val (rows, hold) = storedOf(hostId, hand) ?: return
        dao.keepList(hostId, rows, hold)
    }
}

/** 库里那一份 ⇒ 手上那一份：行照 `ord` 的先后、原样读回；记号与头在 [ListHold]（没答过 ⇒ 没有记号，下次那台答整份）。 */
internal fun handOf(
    hold: ListHold?,
    rows: List<CachedSession>,
): SyncHand {
    val items = rows.mapNotNull { r -> r.listRow?.let(RawJson::read)?.let { r.sid to it } }
    return SyncHand(
        held = hold?.held,
        order = items.map { it.first },
        items = items.toMap(),
        head = hold?.head?.let(RawJson::read)?.let { Wire.bind<SyncHead>(it) },
    )
}

/** 手上那一份 ⇒ 库里那几行（sid ＋ 原样，照先后）与记号 · 头；还没有记号（没答过）⇒ 不存。 */
internal fun storedOf(
    hostId: String,
    hand: SyncHand,
): Pair<List<Pair<String, String>>, ListHold>? {
    val held = hand.held ?: return null
    val rows = hand.order.mapNotNull { k -> hand.items[k]?.let { k to RawJson.write(it) } }
    val head = hand.head?.let { RawJson.write(mapOf("total" to it.total, "info" to it.info)) }
    return rows to ListHold(hostId, held, head)
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

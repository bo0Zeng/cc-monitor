package com.ccmonitor.mobile.core.data.db

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import androidx.room.Upsert

/** 缓存六张表的存取（[CachedSession] · [RecordPage] · [SyncAnchor] · [PendingSend] · [Draft] · [PendingBatch]）。只搬原样，不认 JSON 里的格。 */
@Dao
abstract class CacheDao {
    // ---- 会话列表 ----------------------------------------------------------

    @Query("SELECT * FROM sessions WHERE hostId = :hostId AND ord IS NOT NULL ORDER BY ord")
    abstract suspend fun listed(hostId: String): List<CachedSession>

    @Query("SELECT * FROM sessions WHERE hostId = :hostId")
    abstract suspend fun allSessions(hostId: String): List<CachedSession>

    @Upsert
    abstract suspend fun upsertSessions(rows: List<CachedSession>)

    @Query("DELETE FROM sessions WHERE hostId = :hostId AND ord IS NULL AND viewedAt IS NULL")
    protected abstract suspend fun dropUnlistedUnviewed(hostId: String)

    /**
     * 那台给了一份新的清单（[rows]：sid ＋ 那一行原样，照它的先后）：整份换上。点开过的那几条（[CachedSession.viewedAt]）留着、只是不在清单里了，
     * 它们的记录缓存照最近看过的规矩淘汰；没点开过、也不在新清单里的删掉。
     */
    @Transaction
    open suspend fun replaceList(
        hostId: String,
        rows: List<Pair<String, String>>,
    ) {
        val viewed = allSessions(hostId).associate { it.sid to it.viewedAt }
        val listed = rows.map { it.first }.toSet()
        val gone = viewed.keys.filter { it !in listed }.map { CachedSession(hostId, it, null, null, viewed[it]) }
        upsertSessions(gone + rows.mapIndexed { i, (sid, row) -> CachedSession(hostId, sid, row, i, viewed[sid]) })
        dropUnlistedUnviewed(hostId)
    }

    @Query("INSERT OR IGNORE INTO sessions (hostId, sid, listRow, ord, viewedAt) VALUES (:hostId, :sid, NULL, NULL, :at)")
    protected abstract suspend fun insertViewed(
        hostId: String,
        sid: String,
        at: Long,
    )

    @Query("UPDATE sessions SET viewedAt = :at WHERE hostId = :hostId AND sid = :sid")
    protected abstract suspend fun setViewed(
        hostId: String,
        sid: String,
        at: Long,
    )

    /**
     * 不在最近看过的 [keep] 条里的会话，记录页与续点都删掉（淘汰不上屏：那一条下次点开从那台重读）。
     * 跨机器算：手机上一共留 [keep] 条。
     */
    @Query(
        "DELETE FROM record_pages WHERE NOT EXISTS (SELECT 1 FROM (SELECT hostId, sid FROM sessions WHERE viewedAt IS NOT NULL " +
            "ORDER BY viewedAt DESC LIMIT :keep) k WHERE k.hostId = record_pages.hostId AND k.sid = record_pages.sid)",
    )
    protected abstract suspend fun evictPages(keep: Int)

    @Query(
        "DELETE FROM sync_anchors WHERE NOT EXISTS (SELECT 1 FROM (SELECT hostId, sid FROM sessions WHERE viewedAt IS NOT NULL " +
            "ORDER BY viewedAt DESC LIMIT :keep) k WHERE k.hostId = sync_anchors.hostId AND k.sid = sync_anchors.scope)",
    )
    protected abstract suspend fun evictAnchors(keep: Int)

    /** 点开了一条：记下时刻，顺手把超出最近 [keep] 条的那几条的记录缓存淘汰掉。 */
    @Transaction
    open suspend fun viewed(
        hostId: String,
        sid: String,
        at: Long,
        keep: Int = RECORD_CACHE_SESSIONS,
    ) {
        insertViewed(hostId, sid, at)
        setViewed(hostId, sid, at)
        evictPages(keep)
        evictAnchors(keep)
    }

    // ---- 记录页 · 续点 -----------------------------------------------------

    @Query("SELECT * FROM record_pages WHERE hostId = :hostId AND sid = :sid ORDER BY start")
    abstract suspend fun pages(
        hostId: String,
        sid: String,
    ): List<RecordPage>

    @Upsert
    abstract suspend fun upsertPages(pages: List<RecordPage>)

    @Query("DELETE FROM record_pages WHERE hostId = :hostId AND sid = :sid")
    protected abstract suspend fun dropPages(
        hostId: String,
        sid: String,
    )

    @Query("SELECT * FROM sync_anchors WHERE hostId = :hostId AND scope = :scope")
    abstract suspend fun anchor(
        hostId: String,
        scope: String,
    ): SyncAnchor?

    @Upsert
    abstract suspend fun upsertAnchor(anchor: SyncAnchor)

    @Query("DELETE FROM sync_anchors WHERE hostId = :hostId AND scope = :scope")
    abstract suspend fun dropAnchor(
        hostId: String,
        scope: String,
    )

    /** 整份重来：这一条的页全换成 [pages]、续点换成 [anchor]（`null` ＝ 不知道接在哪，下次从头读）。 */
    @Transaction
    open suspend fun replacePages(
        hostId: String,
        sid: String,
        pages: List<RecordPage>,
        anchor: SyncAnchor?,
    ) {
        dropPages(hostId, sid)
        upsertPages(pages)
        if (anchor != null) upsertAnchor(anchor) else dropAnchor(hostId, sid)
    }

    /** 加几页（往上翻的那一页 · 末页长了）＋ 续点跟着走（`null` ＝ 续点作废）；[keepAnchor] ⇒ 续点不动。 */
    @Transaction
    open suspend fun putPages(
        hostId: String,
        sid: String,
        pages: List<RecordPage>,
        anchor: SyncAnchor?,
        keepAnchor: Boolean = false,
    ) {
        upsertPages(pages)
        when {
            keepAnchor -> Unit
            anchor != null -> upsertAnchor(anchor)
            else -> dropAnchor(hostId, sid)
        }
    }

    // ---- 待发 · 草稿 -------------------------------------------------------

    @Query("SELECT * FROM pending WHERE hostId = :hostId AND sid = :sid ORDER BY mintedAt")
    abstract suspend fun pending(
        hostId: String,
        sid: String,
    ): List<PendingSend>

    @Insert(onConflict = OnConflictStrategy.ABORT)
    abstract suspend fun insertPending(p: PendingSend)

    @Query("UPDATE pending SET outcome = :outcome, attempts = :attempts WHERE id = :id")
    abstract suspend fun settle(
        id: String,
        outcome: String,
        attempts: Int,
    )

    /** 新建会话起成了：那几句归到这条会话。 */
    @Query("UPDATE pending SET sid = :sid WHERE id IN (:ids)")
    abstract suspend fun bind(
        ids: List<String>,
        sid: String,
    )

    @Query("DELETE FROM pending WHERE id IN (:ids)")
    abstract suspend fun forget(ids: List<String>)

    @Query("SELECT * FROM drafts WHERE hostId = :hostId AND sid = :sid")
    abstract suspend fun draft(
        hostId: String,
        sid: String,
    ): Draft?

    @Upsert
    abstract suspend fun putDraft(d: Draft)

    @Query("DELETE FROM drafts WHERE hostId = :hostId AND sid = :sid")
    abstract suspend fun dropDraft(
        hostId: String,
        sid: String,
    )

    /** 草稿变待发：删草稿与落待发同一个事务（落不下 ⇒ 草稿还在，字不会两头都没）。 */
    @Transaction
    open suspend fun promote(
        hostId: String,
        draftSid: String,
        p: PendingSend,
    ) {
        dropDraft(hostId, draftSid)
        insertPending(p)
    }

    // ---- 在做的一组 ---------------------------------------------------------

    @Query("SELECT * FROM pending_batches WHERE hostId = :hostId")
    abstract suspend fun batches(hostId: String): List<PendingBatch>

    @Upsert
    abstract suspend fun putBatch(b: PendingBatch)

    @Query("DELETE FROM pending_batches WHERE hostId = :hostId AND ticket = :ticket")
    abstract suspend fun dropBatch(
        hostId: String,
        ticket: String,
    )

    companion object {
        /** 记录缓存留最近看过的这么多条会话。 */
        const val RECORD_CACHE_SESSIONS = 50
    }
}

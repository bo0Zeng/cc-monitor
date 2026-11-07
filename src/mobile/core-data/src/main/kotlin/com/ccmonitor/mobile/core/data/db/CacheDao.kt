package com.ccmonitor.mobile.core.data.db

import androidx.room.Dao
import androidx.room.Query
import androidx.room.Transaction
import androidx.room.Upsert

/** 缓存库那三张表的存取（[CachedSession] · [RecordPage] · [SyncAnchor]）。只搬原样，不认 JSON 里的格。 */
@Dao
abstract class CacheDao {
    // ---- 看过哪几条 --------------------------------------------------------

    @Query("SELECT * FROM sessions WHERE hostId = :hostId")
    abstract suspend fun viewedSessions(hostId: String): List<CachedSession>

    @Upsert
    protected abstract suspend fun putViewed(s: CachedSession)

    /**
     * 不在最近看过的 [keep] 条里的会话，记录页与续点都删掉（淘汰不上屏：那一条下次点开从那台重读）。
     * 跨机器算：手机上一共留 [keep] 条。
     */
    @Query(
        "DELETE FROM record_pages WHERE NOT EXISTS (SELECT 1 FROM (SELECT hostId, sid FROM sessions " +
            "ORDER BY viewedAt DESC LIMIT :keep) k WHERE k.hostId = record_pages.hostId AND k.sid = record_pages.sid)",
    )
    protected abstract suspend fun evictPages(keep: Int)

    @Query(
        "DELETE FROM sync_anchors WHERE NOT EXISTS (SELECT 1 FROM (SELECT hostId, sid FROM sessions " +
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
        putViewed(CachedSession(hostId, sid, at))
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

    // ---- 删一台 -------------------------------------------------------------

    @Query("DELETE FROM sessions WHERE hostId = :hostId")
    protected abstract suspend fun dropSessions(hostId: String)

    @Query("DELETE FROM record_pages WHERE hostId = :hostId")
    protected abstract suspend fun dropHostPages(hostId: String)

    @Query("DELETE FROM sync_anchors WHERE hostId = :hostId")
    protected abstract suspend fun dropHostAnchors(hostId: String)

    /** 删了一台：它的缓存一并清掉（撤回了，下次连上照那台重取）。 */
    @Transaction
    open suspend fun forgetHost(hostId: String) {
        dropSessions(hostId)
        dropHostPages(hostId)
        dropHostAnchors(hostId)
    }

    companion object {
        /** 记录缓存留最近看过的这么多条会话。 */
        const val RECORD_CACHE_SESSIONS = 50
    }
}

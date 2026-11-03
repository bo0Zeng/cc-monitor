package com.ccmonitor.mobile.core.data.db

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Transaction
import androidx.room.Upsert

/** 这一端手上的（④，住状态库）：待发 [PendingSend] · 草稿 [Draft] · 在做的一组 [PendingBatch]。只搬原样，不认 JSON 里的格。 */
@Dao
abstract class HandDao {
    // ---- 待发 · 草稿 -------------------------------------------------------

    @Query("SELECT * FROM pending WHERE hostId = :hostId AND sid = :sid ORDER BY mintedAt")
    abstract suspend fun pending(
        hostId: String,
        sid: String,
    ): List<PendingSend>

    /** 一台名下该再交的那几句：有会话、还没回执（`pending` · `unsure`），照落库的先后。 */
    @Query("SELECT * FROM pending WHERE hostId = :hostId AND sid IS NOT NULL AND outcome IN ('pending', 'unsure') ORDER BY mintedAt")
    abstract suspend fun duePending(hostId: String): List<PendingSend>

    /** 落一句待发只走 [promote]（草稿变待发同一个事务）。 */
    @Insert(onConflict = OnConflictStrategy.ABORT)
    protected abstract suspend fun insertPending(p: PendingSend)

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
}

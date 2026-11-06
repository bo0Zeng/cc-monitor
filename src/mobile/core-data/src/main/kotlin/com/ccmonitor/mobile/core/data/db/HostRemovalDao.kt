package com.ccmonitor.mobile.core.data.db

import androidx.room.Dao
import androidx.room.Query
import androidx.room.Transaction
import androidx.room.Upsert

/**
 * 删了一台、撤销时放得回去的那几样：机器那一行 ＋ 这一端为它留的（草稿 · 待发 · 在做的一组）。
 * 那台的缓存（会话列表 · 记号 · 记录页 · 续点）住缓存库、不在里面：删一台时另清一次（[CacheDao.forgetHost]），撤回了，下次连上照那台重取。
 */
data class HostRows(
    val host: Host,
    val drafts: List<Draft>,
    val pending: List<PendingSend>,
    val batches: List<PendingBatch>,
)

/** 删一台只走这里：一个事务把它在状态库每张表里的行删光（[remove]），撤销照 [HostRows] 放回（[restore]）。 */
@Dao
abstract class HostRemovalDao {
    @Query("SELECT * FROM hosts WHERE id = :id")
    protected abstract suspend fun host(id: String): Host?

    @Query("SELECT * FROM drafts WHERE hostId = :id")
    protected abstract suspend fun drafts(id: String): List<Draft>

    @Query("SELECT * FROM pending WHERE hostId = :id")
    protected abstract suspend fun pending(id: String): List<PendingSend>

    @Query("SELECT * FROM pending_batches WHERE hostId = :id")
    protected abstract suspend fun batches(id: String): List<PendingBatch>

    @Query("DELETE FROM hosts WHERE id = :id")
    protected abstract suspend fun dropHost(id: String)

    @Query("DELETE FROM pending WHERE hostId = :id")
    protected abstract suspend fun dropPending(id: String)

    @Query("DELETE FROM drafts WHERE hostId = :id")
    protected abstract suspend fun dropDrafts(id: String)

    @Query("DELETE FROM pending_batches WHERE hostId = :id")
    protected abstract suspend fun dropBatches(id: String)

    @Upsert
    protected abstract suspend fun putHost(host: Host)

    @Upsert
    protected abstract suspend fun putDrafts(rows: List<Draft>)

    @Upsert
    protected abstract suspend fun putPending(rows: List<PendingSend>)

    @Upsert
    protected abstract suspend fun putBatches(rows: List<PendingBatch>)

    /** 删掉 [id] 那一台在这一端的每一行，回撤销要的那几样；没有这一台 ⇒ `null`，什么都不动。 */
    @Transaction
    open suspend fun remove(id: String): HostRows? {
        val host = host(id) ?: return null
        val kept = HostRows(host, drafts(id), pending(id), batches(id))
        dropHost(id)
        dropPending(id)
        dropDrafts(id)
        dropBatches(id)
        return kept
    }

    /** 撤销：[remove] 回的那几样照原样放回。 */
    @Transaction
    open suspend fun restore(rows: HostRows) {
        putHost(rows.host)
        putDrafts(rows.drafts)
        putPending(rows.pending)
        putBatches(rows.batches)
    }
}

package com.ccmonitor.mobile.core.data.db

import androidx.room.Dao
import androidx.room.Delete
import androidx.room.Query
import androidx.room.Transaction
import androidx.room.Upsert
import kotlinx.coroutines.flow.Flow

/**
 * 键条的组与按键。先后一律 `sortOrder, id`（相同时也有确定的顺序）。
 *
 * 删一组连组里的按键一起删（[deleteGroup]），撤销照原样放回（[putBack]）；「恢复默认」整批替换（[replaceAll]）。
 */
@Dao
abstract class KeyDao {
    @Query("SELECT * FROM key_groups ORDER BY sortOrder, id")
    abstract fun observeGroups(): Flow<List<KeyGroup>>

    @Query("SELECT * FROM key_groups ORDER BY sortOrder, id")
    abstract suspend fun groups(): List<KeyGroup>

    @Query("SELECT * FROM term_keys WHERE groupId = :groupId ORDER BY sortOrder, id")
    abstract fun observeKeys(groupId: String): Flow<List<TermKey>>

    @Query("SELECT * FROM term_keys ORDER BY groupId, sortOrder, id")
    abstract fun observeAllKeys(): Flow<List<TermKey>>

    @Query("SELECT * FROM term_keys ORDER BY groupId, sortOrder, id")
    abstract suspend fun allKeys(): List<TermKey>

    @Query("SELECT * FROM term_keys WHERE groupId = :groupId ORDER BY sortOrder, id")
    abstract suspend fun keys(groupId: String): List<TermKey>

    @Query("SELECT * FROM term_keys WHERE id = :id")
    abstract suspend fun key(id: String): TermKey?

    /** 升级那一次没转过来的那几颗（编辑页顶上那条警告条）；都改过或删掉之后就是空的。 */
    @Query("SELECT * FROM term_keys WHERE unconverted IS NOT NULL ORDER BY groupId, sortOrder, id")
    abstract fun observeUnconverted(): Flow<List<TermKey>>

    @Upsert
    abstract suspend fun upsertGroup(group: KeyGroup)

    /** 改先后（拖动 · 左移右移）一次写一批。 */
    @Upsert
    abstract suspend fun upsertGroups(groups: List<KeyGroup>)

    @Upsert
    abstract suspend fun upsertKey(key: TermKey)

    @Upsert
    abstract suspend fun upsertKeys(keys: List<TermKey>)

    @Delete
    abstract suspend fun deleteKey(key: TermKey)

    @Query("DELETE FROM term_keys WHERE groupId = :groupId")
    protected abstract suspend fun deleteKeysOf(groupId: String)

    @Query("DELETE FROM key_groups WHERE id = :groupId")
    protected abstract suspend fun deleteGroupRow(groupId: String)

    @Query("DELETE FROM term_keys")
    protected abstract suspend fun deleteAllKeys()

    @Query("DELETE FROM key_groups")
    protected abstract suspend fun deleteAllGroups()

    /** 删一组和组里的按键；回的是删掉的那几颗（撤销时连组一起放回）。 */
    @Transaction
    open suspend fun deleteGroup(group: KeyGroup): List<TermKey> {
        val gone = keys(group.id)
        deleteKeysOf(group.id)
        deleteGroupRow(group.id)
        return gone
    }

    /** 撤销删组：组和它的按键照原样放回原位。 */
    @Transaction
    open suspend fun putBack(
        group: KeyGroup,
        keys: List<TermKey>,
    ) {
        upsertGroup(group)
        upsertKeys(keys)
    }

    /** 整批替换（恢复默认）：先清空两张表，再写进 [groups] 与 [keys]。 */
    @Transaction
    open suspend fun replaceAll(
        groups: List<KeyGroup>,
        keys: List<TermKey>,
    ) {
        deleteAllKeys()
        deleteAllGroups()
        upsertGroups(groups)
        upsertKeys(keys)
    }
}

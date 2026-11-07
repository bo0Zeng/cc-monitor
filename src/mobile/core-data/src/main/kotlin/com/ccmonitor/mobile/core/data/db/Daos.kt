package com.ccmonitor.mobile.core.data.db

import androidx.room.Dao
import androidx.room.Delete
import androidx.room.Query
import androidx.room.Upsert
import kotlinx.coroutines.flow.Flow

@Dao
interface HostDao {
    @Query("SELECT * FROM hosts ORDER BY sortOrder, label")
    fun observeAll(): Flow<List<Host>>

    @Query("SELECT * FROM hosts WHERE id = :id")
    suspend fun get(id: String): Host?

    @Upsert
    suspend fun upsert(host: Host)

    /** 批量导入（ssh config）。 */
    @Upsert
    suspend fun upsertAll(hosts: List<Host>)

    // 删一台不在这里：它在这一端每张表里的行同一个事务删（HostRemovalDao）。
}

@Dao
interface IdentityDao {
    @Query("SELECT * FROM identities ORDER BY label")
    fun observeAll(): Flow<List<Identity>>

    @Query("SELECT * FROM identities WHERE id = :id")
    suspend fun get(id: String): Identity?

    @Upsert
    suspend fun upsert(identity: Identity)

    @Delete
    suspend fun delete(identity: Identity)
}

@Dao
interface SettingsDao {
    @Query("SELECT `value` FROM settings WHERE `key` = :key")
    fun observe(key: String): Flow<String?>

    @Query("SELECT `value` FROM settings WHERE `key` = :key")
    suspend fun get(key: String): String?

    @Upsert
    suspend fun upsert(setting: Settings)
}

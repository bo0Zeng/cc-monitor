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

    @Delete
    suspend fun delete(host: Host)

    @Query("DELETE FROM hosts WHERE id = :id")
    suspend fun deleteById(id: String)
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
interface KnownHostDao {
    @Query("SELECT * FROM known_hosts WHERE host = :host AND port = :port")
    suspend fun forHost(
        host: String,
        port: Int,
    ): List<KnownHost>

    @Upsert
    suspend fun upsert(knownHost: KnownHost)

    /** 忘记某 host:port 的全部已钉扎密钥（TOFU 恢复路径：服务器真·重装/轮换后重新走首见接受）。 */
    @Query("DELETE FROM known_hosts WHERE host = :host AND port = :port")
    suspend fun forget(
        host: String,
        port: Int,
    )
}

@Dao
interface BookmarkDao {
    @Query("SELECT * FROM bookmarks WHERE hostId = :hostId ORDER BY sortOrder, label")
    fun observeForHost(hostId: String): Flow<List<Bookmark>>

    @Upsert
    suspend fun upsert(bookmark: Bookmark)

    @Delete
    suspend fun delete(bookmark: Bookmark)

    @Query("DELETE FROM bookmarks WHERE id = :id")
    suspend fun deleteById(id: String)
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

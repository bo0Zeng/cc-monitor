package com.ccmonitor.mobile.core.data.db

import androidx.room.Dao
import androidx.room.Delete
import androidx.room.Insert
import androidx.room.OnConflictStrategy
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
interface CustomButtonDao {
    // 按分组渲染：CustomButtonBar 取 [ButtonGroups.MAIN]（用户按钮+工具条内建）；ExtraKeysRow 取活动组。
    // 编辑器用 observeGlobal/observeHostOnly，全组可见。
    // 注意：刻意不提供不按组过滤的渲染查询，否则 fn/vim 单键会混进命令面板。
    // `, id` 二级键：sortOrder 相同时给确定的顺序。
    @Query(
        "SELECT * FROM custom_buttons WHERE (hostId IS NULL OR hostId = :hostId) AND btnGroup = :group ORDER BY sortOrder, id",
    )
    fun observeGroup(
        hostId: String?,
        group: String,
    ): Flow<List<CustomButton>>

    // 编辑器分区：全局按钮（hostId IS NULL）/ 某主机专属按钮。
    // `, id` 二级键与 observeGroup 一致，编辑器里的顺序与渲染顺序对齐。
    @Query("SELECT * FROM custom_buttons WHERE hostId IS NULL ORDER BY sortOrder, id")
    fun observeGlobal(): Flow<List<CustomButton>>

    @Query("SELECT * FROM custom_buttons WHERE hostId = :hostId ORDER BY sortOrder, id")
    fun observeHostOnly(hostId: String): Flow<List<CustomButton>>

    @Upsert
    suspend fun upsert(button: CustomButton)

    // 恢复默认按键：`IGNORE` 冲突策略即 `INSERT OR IGNORE`，固定 id 命中即跳过，不覆盖用户改过的同 id 行。
    // 公开的 suspend 入口是 restoreDefaultButtons()（DefaultButtons.kt），复用 DEFAULT_SEED_BUTTONS。
    @Insert(onConflict = OnConflictStrategy.IGNORE)
    suspend fun insertIgnore(buttons: List<CustomButton>)

    @Delete
    suspend fun delete(button: CustomButton)
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
interface LauncherDao {
    @Query("SELECT * FROM launchers WHERE hostId = :hostId ORDER BY sortOrder, label")
    fun observeForHost(hostId: String): Flow<List<Launcher>>

    @Query("SELECT * FROM launchers ORDER BY sortOrder, label")
    fun observeAll(): Flow<List<Launcher>> // 主机列表按 hostId 分组显示 chips

    @Query("SELECT * FROM launchers WHERE id = :id")
    suspend fun get(id: String): Launcher? // 一键启动按 launcherId 取配置

    @Upsert
    suspend fun upsert(launcher: Launcher)

    @Delete
    suspend fun delete(launcher: Launcher)

    @Query("DELETE FROM launchers WHERE id = :id")
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

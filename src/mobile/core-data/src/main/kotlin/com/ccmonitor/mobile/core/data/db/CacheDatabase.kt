package com.ccmonitor.mobile.core.data.db

import android.content.Context
import androidx.room.Database
import androidx.room.Room
import androidx.room.RoomDatabase

/**
 * 缓存库（⑤ 别台东西的副本：会话列表 · 列表记号 · 记录页 · 续读点）。跟状态库（[AppDatabase]）分开住：
 * 不写迁移，版本一换整份丢了重建（缓存丢了照那台重取）；状态库里的 ④ 从不跟着它动。
 */
@Database(
    entities = [
        CachedSession::class,
        ListHold::class,
        RecordPage::class,
        SyncAnchor::class,
    ],
    version = 1,
    exportSchema = false,
)
abstract class CacheDatabase : RoomDatabase() {
    abstract fun cacheDao(): CacheDao
}

/** 缓存库那一份文件（跟状态库 `ccm.db` 并排）。 */
const val CACHE_DB_FILE = "ccm-cache.db"

/** 开缓存库：表的形一换（升 · 降）整份丢了重建，不迁移。全仓只有这一处许破坏性回退。 */
fun cacheDatabase(context: Context): CacheDatabase =
    Room
        .databaseBuilder(context, CacheDatabase::class.java, "ccm-cache.db")
        .fallbackToDestructiveMigration(dropAllTables = true)
        .fallbackToDestructiveMigrationOnDowngrade(dropAllTables = true)
        .build()

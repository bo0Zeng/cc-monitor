package com.ccmonitor.mobile.core.data.db

import androidx.room.Database
import androidx.room.RoomDatabase
import androidx.room.TypeConverters

/** Room 数据库。扩列走迁移，不重建。 */
@Database(
    entities = [
        Host::class,
        Identity::class,
        KnownHost::class,
        KeyGroup::class,
        TermKey::class,
        Bookmark::class,
        Settings::class,
        CachedSession::class,
        RecordPage::class,
        SyncAnchor::class,
        PendingSend::class,
        Draft::class,
    ],
    version = 19,
    exportSchema = true,
)
@TypeConverters(KeyConverters::class)
abstract class AppDatabase : RoomDatabase() {
    abstract fun hostDao(): HostDao

    abstract fun identityDao(): IdentityDao

    abstract fun knownHostDao(): KnownHostDao

    abstract fun keyDao(): KeyDao

    abstract fun bookmarkDao(): BookmarkDao

    abstract fun settingsDao(): SettingsDao

    abstract fun cacheDao(): CacheDao
}

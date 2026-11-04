package com.ccmonitor.mobile.core.data.db

import androidx.room.Database
import androidx.room.RoomDatabase
import androidx.room.TypeConverters

/** 状态库（④ 这一端自己的：机器 · 钥匙 · 指纹 · 按键 · 设置 · 待发 · 草稿 · 在做的一组）。扩列走迁移，不重建；缓存住 [CacheDatabase]。 */
@Database(
    entities = [
        Host::class,
        Identity::class,
        KnownHost::class,
        KeyGroup::class,
        TermKey::class,
        Bookmark::class,
        Settings::class,
        PendingSend::class,
        Draft::class,
        PendingBatch::class,
    ],
    version = 26,
    exportSchema = true,
)
@TypeConverters(KeyConverters::class, HostConverters::class)
abstract class AppDatabase : RoomDatabase() {
    abstract fun hostDao(): HostDao

    abstract fun identityDao(): IdentityDao

    abstract fun knownHostDao(): KnownHostDao

    abstract fun keyDao(): KeyDao

    abstract fun bookmarkDao(): BookmarkDao

    abstract fun settingsDao(): SettingsDao

    abstract fun handDao(): HandDao

    abstract fun hostRemovalDao(): HostRemovalDao
}

package com.ccmonitor.mobile.core.data.db

import androidx.room.Database
import androidx.room.RoomDatabase

/** Room 数据库。扩列走迁移，不重建。 */
@Database(
    entities = [
        Host::class,
        Identity::class,
        KnownHost::class,
        CustomButton::class,
        Bookmark::class,
        Launcher::class,
        Settings::class,
    ],
    version = 14,
    exportSchema = true,
)
abstract class AppDatabase : RoomDatabase() {
    abstract fun hostDao(): HostDao

    abstract fun identityDao(): IdentityDao

    abstract fun knownHostDao(): KnownHostDao

    abstract fun customButtonDao(): CustomButtonDao

    abstract fun bookmarkDao(): BookmarkDao

    abstract fun launcherDao(): LauncherDao

    abstract fun settingsDao(): SettingsDao
}

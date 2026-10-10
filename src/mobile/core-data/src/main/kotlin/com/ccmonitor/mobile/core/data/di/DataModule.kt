package com.ccmonitor.mobile.core.data.di

import androidx.room.Room
import androidx.room.RoomDatabase
import androidx.sqlite.db.SupportSQLiteDatabase
import com.ccmonitor.mobile.core.data.crypto.CryptoBox
import com.ccmonitor.mobile.core.data.crypto.KeystoreCryptoBox
import com.ccmonitor.mobile.core.data.db.ALL_MIGRATIONS
import com.ccmonitor.mobile.core.data.db.AppDatabase
import com.ccmonitor.mobile.core.data.db.seedDefaultButtons
import com.ccmonitor.mobile.core.data.repo.BookmarkRepository
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.data.repo.LauncherRepository
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import org.koin.android.ext.koin.androidContext
import org.koin.dsl.module

/**
 * core-data 的 Koin 模块。加密边界走 [CryptoBox]（绑到 KeystoreCryptoBox）。
 */
val dataModule =
    module {
        // 迁移链（升级不丢数据），见 ALL_MIGRATIONS。
        single {
            Room
                .databaseBuilder(androidContext(), AppDatabase::class.java, "aterm.db")
                .addMigrations(*ALL_MIGRATIONS)
                // 新装也要种默认按键：迁移只在升级时跑。
                .addCallback(
                    object : RoomDatabase.Callback() {
                        override fun onCreate(db: SupportSQLiteDatabase) = seedDefaultButtons(db)
                    },
                ).build()
        }
        single { get<AppDatabase>().hostDao() }
        single { get<AppDatabase>().identityDao() }
        single { get<AppDatabase>().knownHostDao() }
        single { get<AppDatabase>().customButtonDao() }
        single { get<AppDatabase>().bookmarkDao() }
        single { get<AppDatabase>().launcherDao() }
        single { get<AppDatabase>().settingsDao() }

        single<CryptoBox> { KeystoreCryptoBox(androidContext()) }

        single { HostRepository(get(), get()) }
        single { IdentityRepository(get(), get()) }
        single { BookmarkRepository(get()) }
        single { LauncherRepository(get()) }
        single { SettingsRepository(get()) }
    }

package com.ccmonitor.mobile.core.data.di

import androidx.room.Room
import androidx.room.RoomDatabase
import androidx.sqlite.db.SupportSQLiteDatabase
import com.ccmonitor.mobile.core.data.crypto.CryptoBox
import com.ccmonitor.mobile.core.data.crypto.KeystoreCryptoBox
import com.ccmonitor.mobile.core.data.db.AppDatabase
import com.ccmonitor.mobile.core.data.db.CacheDatabase
import com.ccmonitor.mobile.core.data.db.STATE_DB_FILE
import com.ccmonitor.mobile.core.data.db.allMigrations
import com.ccmonitor.mobile.core.data.db.cacheDatabase
import com.ccmonitor.mobile.core.data.db.seedDefaultKeys
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import org.koin.android.ext.koin.androidContext
import org.koin.dsl.module

/**
 * core-data 的 Koin 模块。加密边界走 [CryptoBox]（绑到 KeystoreCryptoBox）。
 */
val dataModule =
    module {
        // 迁移链（升级不丢数据，从 OLDEST_STATE_DB 起），见 allMigrations；更老的库 App 起来时就拒开了（StateDbAge），走不到这里。
        single {
            Room
                .databaseBuilder(androidContext(), AppDatabase::class.java, STATE_DB_FILE)
                .addMigrations(*allMigrations())
                // 新装也要种默认三组按键：迁移只在升级时跑。
                .addCallback(
                    object : RoomDatabase.Callback() {
                        override fun onCreate(db: SupportSQLiteDatabase) = seedDefaultKeys(db)
                    },
                ).build()
        }
        single { get<AppDatabase>().hostDao() }
        single { get<AppDatabase>().identityDao() }
        single { get<AppDatabase>().keyDao() }
        single { get<AppDatabase>().settingsDao() }
        single { get<AppDatabase>().handDao() }
        single { get<AppDatabase>().hostRemovalDao() }
        // 缓存另一个库：不迁移，版本一换整份重建（见 cacheDatabase）。
        single { cacheDatabase(androidContext()) }
        single { get<CacheDatabase>().cacheDao() }

        single<CryptoBox> { KeystoreCryptoBox(androidContext()) }

        single { HostRepository(get(), get()) }
        single { IdentityRepository(get(), get()) }
        single { SettingsRepository(get()) }
    }

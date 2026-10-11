package com.ccmonitor.mobile.core.data.repo

import com.ccmonitor.mobile.core.data.db.Settings
import com.ccmonitor.mobile.core.data.db.SettingsDao
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** SettingsRepository 类型化访问（fake dao，纯 JVM）。 */
class SettingsRepositoryTest {
    private class FakeSettingsDao : SettingsDao {
        val store = mutableMapOf<String, String?>()

        override fun observe(key: String): Flow<String?> = flowOf(store[key])

        override suspend fun get(key: String): String? = store[key]

        override suspend fun upsert(setting: Settings) {
            store[setting.key] = setting.value
        }
    }

    @Test
    fun setThenGetLastHost() =
        runBlocking {
            val dao = FakeSettingsDao()
            val repo = SettingsRepository(dao)
            assertNull("初始未设应为 null", repo.getLastHost())
            repo.setLastHost("h1")
            assertEquals("h1", repo.getLastHost())
        }

    @Test
    fun blankStoredAsNullAndValueTrimmed() =
        runBlocking {
            val repo = SettingsRepository(FakeSettingsDao())
            repo.setLastHost("   ")
            assertNull("纯空白应存为 null", repo.getLastHost())
            repo.setLastHost("  h2  ")
            assertEquals("应去首尾空白", "h2", repo.getLastHost())
        }
}

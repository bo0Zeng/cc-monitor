package com.ccmonitor.mobile.core.data.repo

import com.ccmonitor.mobile.core.data.db.Settings
import com.ccmonitor.mobile.core.data.db.SettingsDao
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 手机原来存的启动配置在迁移那一次转成一张说明（设置表里一格），每台只出一次：
 * 原样读回（字一个不改）· 只给那一台的 · 「知道了」之后那一台的就没了、别的台的还在 · 全没了那一格清掉。
 */
class MovedLaunchRepositoryTest {
    private class FakeSettingsDao : SettingsDao {
        val store = mutableMapOf<String, String?>()

        override fun observe(key: String): Flow<String?> = flowOf(store[key])

        override suspend fun get(key: String): String? = store[key]

        override suspend fun upsert(setting: Settings) {
            store[setting.key] = setting.value
        }
    }

    /** 迁移那一句 SQL 写出来的那一形：一条一段（[MovedLaunchRepository.RECORD]），一段五格（[MovedLaunchRepository.FIELD]），空格 ＝ 没写。 */
    private fun stored(vararg rows: List<String>) = rows.joinToString(MovedLaunchRepository.RECORD) { it.joinToString(MovedLaunchRepository.FIELD) }

    @Test
    fun theStoredRowsComeBackVerbatimPerHost() =
        runBlocking {
            val dao = FakeSettingsDao()
            dao.store[MovedLaunchRepository.KEY] =
                stored(
                    listOf("h1", "cc", "", "", "claude"),
                    listOf("h1", "cct", "~/w x", "cc", "claude --resume  'a b'"),
                    listOf("h2", "top", "", "", "htop"),
                )
            val repo = MovedLaunchRepository(dao)
            assertEquals(
                listOf(
                    MovedLaunch("h1", "cc", null, null, "claude"),
                    MovedLaunch("h1", "cct", "~/w x", "cc", "claude --resume  'a b'"),
                ),
                repo.forHost("h1"),
            )
            assertEquals(listOf(MovedLaunch("h2", "top", null, null, "htop")), repo.forHost("h2"))
            assertTrue(repo.forHost("h3").isEmpty())
        }

    @Test
    fun acknowledgingOneHostShowsItNeverAgainAndKeepsTheOthers() =
        runBlocking {
            val dao = FakeSettingsDao()
            dao.store[MovedLaunchRepository.KEY] = stored(listOf("h1", "cc", "", "", "claude"), listOf("h2", "top", "", "", "htop"))
            val repo = MovedLaunchRepository(dao)
            repo.ack("h1")
            assertTrue("那一台「知道了」之后不再出", repo.forHost("h1").isEmpty())
            assertEquals(listOf(MovedLaunch("h2", "top", null, null, "htop")), repo.forHost("h2"))
            repo.ack("h2")
            assertNull("全没了 ⇒ 那一格清掉", dao.store[MovedLaunchRepository.KEY])
        }

    @Test
    fun nothingStoredMeansNothingToShow() =
        runBlocking {
            val dao = FakeSettingsDao()
            val repo = MovedLaunchRepository(dao)
            assertTrue(repo.forHost("h1").isEmpty())
            repo.ack("h1")
            assertNull(dao.store[MovedLaunchRepository.KEY])
        }
}

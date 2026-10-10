package com.ccmonitor.mobile.ui.settings

import com.ccmonitor.mobile.core.data.db.Settings
import com.ccmonitor.mobile.core.data.db.SettingsDao
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

/** 设置 VM 读写应用级默认 claudeDir（SettingsRepository + fake DAO）。 */
@OptIn(ExperimentalCoroutinesApi::class)
class SettingsViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setMain() = Dispatchers.setMain(dispatcher)

    @After fun reset() = Dispatchers.resetMain()

    @Test
    fun saveWritesDefaultClaudeDir() =
        runTest(dispatcher) {
            val dao = FakeSettingsDao()
            val vm = SettingsViewModel(SettingsRepository(dao), io = dispatcher)
            vm.save("/custom/.claude")
            advanceUntilIdle()
            assertEquals("/custom/.claude", dao.flow.value)
        }

    @Test
    fun emptySaveNormalizesToNull() =
        runTest(dispatcher) {
            val dao = FakeSettingsDao("/old")
            val vm = SettingsViewModel(SettingsRepository(dao), io = dispatcher)
            vm.save("   ") // 空白 → repo 归一化为 null（回退内置默认）
            advanceUntilIdle()
            assertEquals(null, dao.flow.value)
        }

    @Test
    fun observeReflectsStoredValue() =
        runTest(dispatcher) {
            val vm = SettingsViewModel(SettingsRepository(FakeSettingsDao("/x/.claude")), io = dispatcher)
            val job = launch { vm.defaultClaudeDir.collect {} } // 触发 stateIn 上游
            advanceUntilIdle()
            assertEquals("/x/.claude", vm.defaultClaudeDir.value)
            job.cancel()
        }
}

/** 单键（默认 claudeDir）reactive fake：upsert 更新 flow → observe 立即反映。 */
private class FakeSettingsDao(
    initial: String? = null,
) : SettingsDao {
    val flow = MutableStateFlow(initial)

    override fun observe(key: String): Flow<String?> = flow

    override suspend fun get(key: String): String? = flow.value

    override suspend fun upsert(setting: Settings) {
        flow.value = setting.value
    }
}

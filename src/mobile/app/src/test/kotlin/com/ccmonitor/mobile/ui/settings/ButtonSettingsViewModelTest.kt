package com.ccmonitor.mobile.ui.settings

import com.ccmonitor.mobile.core.data.db.CustomButton
import com.ccmonitor.mobile.core.data.db.CustomButtonDao
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

/** 按钮编辑器 [moveButton] 重排纯函数 + [ButtonSettingsViewModel] 的 flowFor 分区 / reorder 持久化。 */
@OptIn(ExperimentalCoroutinesApi::class)
class ButtonSettingsViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before fun setMain() = Dispatchers.setMain(dispatcher)

    @After fun reset() = Dispatchers.resetMain()

    private fun b(
        id: String,
        order: Int = 0,
        hostId: String? = null,
    ) = CustomButton(id = id, hostId = hostId, label = id, command = id, sortOrder = order)

    @Test
    fun moveDownSwapsAndReindexes() {
        val r = moveButton(listOf(b("a"), b("b"), b("c")), 0, up = false)
        assertEquals("下移：a↔b 交换", listOf("b", "a", "c"), r.map { it.id })
        assertEquals("sortOrder 重排为下标", listOf(0, 1, 2), r.map { it.sortOrder })
    }

    @Test
    fun moveUpSwaps() {
        val r = moveButton(listOf(b("a"), b("b")), 1, up = true)
        assertEquals(listOf("b", "a"), r.map { it.id })
    }

    @Test
    fun moveOutOfBoundsNoOp() {
        val list = listOf(b("a"), b("b"))
        assertEquals("顶部上移无操作", list, moveButton(list, 0, up = true))
        assertEquals("底部下移无操作", list, moveButton(list, 1, up = false))
        assertEquals("非法下标无操作", list, moveButton(list, 5, up = true))
    }

    @Test
    fun flowForRoutesByScope() =
        runBlocking {
            val vm = ButtonSettingsViewModel(FakeDao(listOf(b("G")), listOf(b("H", hostId = "x"))), dispatcher)
            assertEquals("null → 全局 observeGlobal", listOf("G"), vm.flowFor(null).first().map { it.id })
            assertEquals("hostId → observeHostOnly", listOf("H"), vm.flowFor("x").first().map { it.id })
        }

    // reorder 必须真把新 sortOrder 落库：`if (b.sortOrder != i)` 这种守卫在这里恒 false、一次都不写。
    @Test
    fun reorderPersistsAllSortOrders() =
        runTest(dispatcher) {
            val dao = FakeDao(emptyList(), emptyList())
            val vm = ButtonSettingsViewModel(dao, dispatcher)
            // moveButton 已把 sortOrder 归一化为下标：[b(0), a(1), c(2)]。
            vm.reorder(moveButton(listOf(b("a"), b("b"), b("c")), 0, up = false))
            advanceUntilIdle()
            assertEquals(
                "三行都被 upsert、sortOrder=新下标",
                listOf("b" to 0, "a" to 1, "c" to 2),
                dao.upserts.map { it.id to it.sortOrder },
            )
        }

    private class FakeDao(
        val global: List<CustomButton>,
        val host: List<CustomButton>,
    ) : CustomButtonDao {
        val upserts = mutableListOf<CustomButton>()

        // 按 btnGroup 渲染（键排/主栏/命令面板各过各的组）。编辑器 VM 不用它 → 空流即可。
        override fun observeGroup(
            hostId: String?,
            group: String,
        ): Flow<List<CustomButton>> = flowOf(emptyList())

        override fun observeGlobal(): Flow<List<CustomButton>> = flowOf(global)

        override fun observeHostOnly(hostId: String): Flow<List<CustomButton>> = flowOf(host)

        override suspend fun upsert(button: CustomButton) {
            upserts += button
        }

        // 恢复默认走它（此 VM 测不覆盖恢复路径 → 记录即可，reorder/upsert 测不受影响）。
        val inserted = mutableListOf<CustomButton>()

        override suspend fun insertIgnore(buttons: List<CustomButton>) {
            inserted += buttons
        }

        override suspend fun delete(button: CustomButton) = Unit
    }
}

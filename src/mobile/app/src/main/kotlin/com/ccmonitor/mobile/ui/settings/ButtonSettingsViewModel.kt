package com.ccmonitor.mobile.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.data.db.CustomButton
import com.ccmonitor.mobile.core.data.db.CustomButtonDao
import com.ccmonitor.mobile.core.data.db.restoreDefaultButtons
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.launch

/**
 * 编辑器重排：交换 index 与相邻项（up=上移），并把 sortOrder 归一为下标。越界返回原列表。
 */
fun moveButton(
    list: List<CustomButton>,
    index: Int,
    up: Boolean,
): List<CustomButton> {
    val target = if (up) index - 1 else index + 1
    if (index !in list.indices || target !in list.indices) {
        return list
    }
    val reordered = list.toMutableList()
    val tmp = reordered[index]
    reordered[index] = reordered[target]
    reordered[target] = tmp
    return reordered.mapIndexed { i, b -> b.copy(sortOrder = i) }
}

/**
 * 按钮编辑器 VM，直接用 CustomButtonDao（同 CustomButtonBar，没有独立 repo）。
 * flowFor 按作用域分区（null=全局 / hostId=专属）；upsert 承担新建、改字段、改作用域（同 id 改 hostId）；reorder 落 sortOrder。
 */
class ButtonSettingsViewModel(
    private val dao: CustomButtonDao,
    private val io: CoroutineDispatcher = Dispatchers.IO,
) : ViewModel() {
    fun flowFor(hostId: String?): Flow<List<CustomButton>> =
        if (hostId == null) dao.observeGlobal() else dao.observeHostOnly(hostId)

    fun upsert(button: CustomButton) {
        viewModelScope.launch(io) { dao.upsert(button) }
    }

    fun delete(button: CustomButton) {
        viewModelScope.launch(io) { dao.delete(button) }
    }

    /** 整组删除：删该组全部按钮（UI 传入已按当前作用域过滤的该组按钮）。 */
    fun deleteAll(buttons: List<CustomButton>) {
        viewModelScope.launch(io) { buttons.forEach { dao.delete(it) } }
    }

    /**
     * 撤销 [deleteAll]：把整组按原样写回。按钮是本地纯数据，`id`/`sortOrder` 都在被删的对象里，
     * 原样 upsert 即完全复原，所以删除用撤销而不用确认框。
     */
    fun restoreAll(buttons: List<CustomButton>) {
        viewModelScope.launch(io) { buttons.forEach { dao.upsert(it) } }
    }

    /**
     * 落 [list] 的 sortOrder（已由 [moveButton] 归一化为下标），逐行按 id upsert。
     * 注意：不能用 `b.sortOrder != i` 守卫，归一化后它恒 false，一行都不会写。小列表全量 upsert 可接受。
     */
    fun reorder(list: List<CustomButton>) {
        viewModelScope.launch(io) {
            list.forEachIndexed { i, b -> dao.upsert(b.copy(sortOrder = i)) }
        }
    }

    /**
     * 恢复默认按键：补回被删的默认键（main/fn/alt/vim 四组，含 ⇌ 组切换与 Ctrl/Alt/粘贴）。
     * `INSERT OR IGNORE` 幂等：不覆盖用户已改的同 id 行、不碰自建按钮。走 core-data 的 [restoreDefaultButtons]。
     */
    fun restoreDefaults() {
        viewModelScope.launch(io) { dao.restoreDefaultButtons() }
    }
}

package com.ccmonitor.mobile.ui.session.keys

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.data.db.KeyDao
import com.ccmonitor.mobile.core.data.db.KeyGroup
import com.ccmonitor.mobile.core.data.db.KeyMod
import com.ccmonitor.mobile.core.data.db.TermKey
import com.ccmonitor.mobile.core.data.db.restoreDefaults
import com.ccmonitor.mobile.core.data.db.revertToDefault
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import java.util.UUID

/** 编辑页要画的：组（照先后）· 全部按键 · 两个总时长 · 终端当前那一组 · 升级那一次没转过来的那几颗。 */
data class KeysState(
    val groups: List<KeyGroup> = emptyList(),
    val keys: List<TermKey> = emptyList(),
    val doubleTapMs: Int? = null,
    val longPressMs: Int? = null,
    val currentGroup: String? = null,
    val unconverted: List<TermKey> = emptyList(),
    val loaded: Boolean = false,
) {
    fun keysOf(groupId: String): List<TermKey> = keys.filter { it.groupId == groupId }.sortedWith(compareBy({ it.sortOrder }, { it.id }))

    fun groupName(id: String): String? = groups.firstOrNull { it.id == id }?.name
}

/**
 * 编辑按键那一页的数据口：读写都经 [KeyOps]（可在 JVM 上直接测），这里只把它接到 viewModelScope 与两条流上。
 */
class KeysEditorViewModel(
    dao: KeyDao,
    settings: SettingsRepository,
) : ViewModel() {
    val ops = KeyOps(dao, settings)

    val state: StateFlow<KeysState> =
        combine(
            dao.observeGroups(),
            dao.observeAllKeys(),
            settings.observeKeyDoubleTapMs(),
            settings.observeKeyLongPressMs(),
            settings.observeCurrentKeyGroup(),
        ) { g, k, d, l, c -> KeysState(g, k, d, l, c, loaded = true) }
            .combine(dao.observeUnconverted()) { s, u -> s.copy(unconverted = u) }
            .stateIn(viewModelScope, SharingStarted.WhileSubscribed(STOP_MS), KeysState())

    private companion object {
        const val STOP_MS = 5_000L
    }
}

/** 编辑按键的读写（删的都回原样数据，撤销照它放回）。 */
class KeyOps(
    private val dao: KeyDao,
    private val settings: SettingsRepository,
    private val newId: () -> String = { UUID.randomUUID().toString() },
) {
    /** 新建组：排在最后、建好后键条切到它（空组也存下来）。回新组的 id。 */
    suspend fun createGroup(name: String): String {
        val groups = dao.groups()
        val g = KeyGroup("kg-${newId()}", name, (groups.maxOfOrNull { it.sortOrder } ?: -1) + 1)
        dao.upsertGroup(g)
        settings.setCurrentKeyGroup(g.id)
        return g.id
    }

    suspend fun renameGroup(
        g: KeyGroup,
        name: String,
    ) = dao.upsertGroup(g.copy(name = name))

    /** 组挪一格（左移 -1 · 右移 +1）或拖到 [to]；先后整批重编。 */
    suspend fun moveGroup(
        id: String,
        to: Int,
    ) {
        val groups = dao.groups()
        val from = groups.indexOfFirst { it.id == id }
        dao.upsertGroups(groups.moved(from, to).renumbered())
    }

    /** 删一组连组里的按键；回的是放回要用的那几颗。 */
    suspend fun deleteGroup(g: KeyGroup): List<TermKey> = dao.deleteGroup(g)

    suspend fun undoDeleteGroup(
        g: KeyGroup,
        keys: List<TermKey>,
    ) = dao.putBack(g, keys)

    /** 组内按键拖到 [to]。 */
    suspend fun moveKey(
        groupId: String,
        from: Int,
        to: Int,
    ) = dao.upsertKeys(dao.keys(groupId).moved(from, to).renumberedKeys())

    suspend fun deleteKey(k: TermKey) = dao.deleteKey(k)

    /** 撤销删一颗：照原样（连先后）放回。 */
    suspend fun undoDeleteKey(k: TermKey) = dao.upsertKey(k)

    /** 存一颗（新建或改过的）；回写进去的那一颗。 */
    suspend fun save(d: KeyDraft): TermKey {
        val existing = d.id?.let { dao.key(it) }
        val end = endOf(d.groupId)
        val k = d.toKey(existing, endOf = { end }, newId = { "tk-${newId()}" })
        dao.upsertKey(k)
        return k
    }

    /** 加一颗修饰键到 [groupId] 最后。 */
    suspend fun addModifier(
        groupId: String,
        m: KeyMod,
    ): TermKey = save(KeyDraft(id = null, groupId = groupId, modifier = m))

    suspend fun revert(k: TermKey): TermKey = dao.revertToDefault(k)

    suspend fun restoreDefaults() = dao.restoreDefaults()

    suspend fun setDoubleTapMs(ms: Int?) = settings.setKeyDoubleTapMs(ms)

    suspend fun setLongPressMs(ms: Int?) = settings.setKeyLongPressMs(ms)

    private suspend fun endOf(groupId: String): Int = (dao.keys(groupId).maxOfOrNull { it.sortOrder } ?: -1) + 1
}

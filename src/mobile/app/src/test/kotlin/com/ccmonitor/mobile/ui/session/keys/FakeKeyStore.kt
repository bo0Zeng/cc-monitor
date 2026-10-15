package com.ccmonitor.mobile.ui.session.keys

import com.ccmonitor.mobile.core.data.db.KeyDao
import com.ccmonitor.mobile.core.data.db.KeyGroup
import com.ccmonitor.mobile.core.data.db.Settings
import com.ccmonitor.mobile.core.data.db.SettingsDao
import com.ccmonitor.mobile.core.data.db.TermKey
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.map

/** 内存里的两张表（照 Room 那份的先后：sortOrder, id）。 */
class FakeKeyDao(
    groups: List<KeyGroup> = emptyList(),
    keys: List<TermKey> = emptyList(),
) : KeyDao() {
    val g = MutableStateFlow(groups.associateBy { it.id })
    val k = MutableStateFlow(keys.associateBy { it.id })

    private fun gs() = g.value.values.sortedWith(compareBy({ it.sortOrder }, { it.id }))

    private fun ks() = k.value.values.sortedWith(compareBy({ it.groupId }, { it.sortOrder }, { it.id }))

    override fun observeGroups(): Flow<List<KeyGroup>> = g.map { gs() }

    override suspend fun groups(): List<KeyGroup> = gs()

    override fun observeKeys(groupId: String): Flow<List<TermKey>> = k.map { ks().filter { it.groupId == groupId }.sortedWith(compareBy({ it.sortOrder }, { it.id })) }

    override fun observeAllKeys(): Flow<List<TermKey>> = k.map { ks() }

    override suspend fun allKeys(): List<TermKey> = ks()

    override suspend fun keys(groupId: String): List<TermKey> = ks().filter { it.groupId == groupId }.sortedWith(compareBy({ it.sortOrder }, { it.id }))

    override suspend fun key(id: String): TermKey? = k.value[id]

    override fun observeUnconverted(): Flow<List<TermKey>> = k.map { ks().filter { it.unconverted != null } }

    override suspend fun upsertGroup(group: KeyGroup) {
        g.value = g.value + (group.id to group)
    }

    override suspend fun upsertGroups(groups: List<KeyGroup>) = groups.forEach { upsertGroup(it) }

    override suspend fun upsertKey(key: TermKey) {
        k.value = k.value + (key.id to key)
    }

    override suspend fun upsertKeys(keys: List<TermKey>) = keys.forEach { upsertKey(it) }

    override suspend fun deleteKey(key: TermKey) {
        k.value = k.value - key.id
    }

    override suspend fun deleteKeysOf(groupId: String) {
        k.value = k.value.filterValues { it.groupId != groupId }
    }

    override suspend fun deleteGroupRow(groupId: String) {
        g.value = g.value - groupId
    }

    override suspend fun deleteAllKeys() {
        k.value = emptyMap()
    }

    override suspend fun deleteAllGroups() {
        g.value = emptyMap()
    }
}

class FakeSettingsDao : SettingsDao {
    val m = MutableStateFlow<Map<String, String?>>(emptyMap())

    override fun observe(key: String): Flow<String?> = m.map { it[key] }

    override suspend fun get(key: String): String? = m.value[key]

    override suspend fun upsert(setting: Settings) {
        m.value = m.value + (setting.key to setting.value)
    }
}

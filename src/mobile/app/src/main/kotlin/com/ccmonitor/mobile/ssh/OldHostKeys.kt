package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.data.db.KnownHostDao
import com.ccmonitor.mobile.core.rs.Core
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Room 旧表 `known_hosts` 迁进核心那份库（只活这一版）：全部行交核心，核心写进库、按「主机 ＋ 口」读回比。
 * 对上 ⇒ 清空旧表；对不上 ⇒ 旧表不动、核心那边拨号一律拒，[unmigrated] 是核心说的那一句（下次启动再迁）。
 * 那一句的出路是 [forgetAll]（需手动、确认过才调）：核心清空库、开闸，说清了才清旧表，之后每台照首见重新记。
 * App 起来、核心装好之后、第一次拨号之前跑 [migrate]（[com.ccmonitor.mobile.CcMonitorApp]）。
 */
class OldHostKeys(
    private val dao: KnownHostDao,
    private val inCore: (Array<String>) -> String? = Core::migrateHostKeys,
    private val forgetInCore: () -> Boolean = Core::forgetAllHostKeys,
) {
    private val said = MutableStateFlow<String?>(null)
    val unmigrated: StateFlow<String?> = said.asStateFlow()

    suspend fun migrate() {
        val rows = dao.all().ifEmpty { return }
        val flat = rows.flatMap { listOf(it.host, it.port.toString(), it.keyType, it.publicKeyBase64) }.toTypedArray()
        said.value = inCore(flat)
        if (said.value == null) dao.clear()
    }

    /** 清掉旧表与迁进库的那些；核心没清成 ⇒ `false`，旧表不动、那一句还在。 */
    suspend fun forgetAll(): Boolean {
        if (!forgetInCore()) return false
        dao.clear()
        said.value = null
        return true
    }
}

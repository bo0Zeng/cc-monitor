package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.data.db.KnownHostDao
import com.ccmonitor.mobile.core.rs.Core

/**
 * Room 旧表 `known_hosts` 迁进核心那份库（只活这一版）：全部行交核心，核心写进库、按「主机 ＋ 口」读回比。
 * 对上 ⇒ 清空旧表；对不上 ⇒ 旧表不动，核心那边拨号一律拒（那一句说清旧表几条、读回几条），下次启动再迁。
 * App 起来、核心装好之后、第一次拨号之前跑（[com.ccmonitor.mobile.CcMonitorApp]）。
 */
suspend fun migrateOldHostKeys(
    dao: KnownHostDao,
    migrate: (Array<String>) -> String? = Core::migrateHostKeys,
) {
    val rows = dao.all().ifEmpty { return }
    val flat = rows.flatMap { listOf(it.host, it.port.toString(), it.keyType, it.publicKeyBase64) }.toTypedArray()
    if (migrate(flat) == null) dao.clear()
}

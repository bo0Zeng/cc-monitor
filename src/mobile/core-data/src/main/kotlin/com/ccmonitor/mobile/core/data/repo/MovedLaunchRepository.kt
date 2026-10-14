package com.ccmonitor.mobile.core.data.repo

import com.ccmonitor.mobile.core.data.db.Settings
import com.ccmonitor.mobile.core.data.db.SettingsDao

/**
 * 手机原来存的一条启动配置，原样（字一个不改）：起会话改用那台的别名之后，只在迁移说明里列一次。空格 ⇒ `null`。
 */
data class MovedLaunch(
    val hostId: String,
    val label: String,
    val workingDir: String?,
    val tmuxSession: String?,
    val command: String?,
)

/**
 * 迁移说明那一格（设置表里 [KEY]）：删启动配置表的那一版迁移把旧行原样写进来（[RECORD] 分条、[FIELD] 分格）；
 * 每台连上终端时读那一台的（[forHost]），「知道了」⇒ 那一台的删掉（[ack]），全删完那一格清空。只出一次。
 */
class MovedLaunchRepository(
    private val dao: SettingsDao,
) {
    suspend fun forHost(hostId: String): List<MovedLaunch> = all().filter { it.hostId == hostId }

    suspend fun ack(hostId: String) {
        val all = all()
        if (all.none { it.hostId == hostId }) return
        val left = all.filterNot { it.hostId == hostId }
        dao.upsert(Settings(KEY, left.takeIf { it.isNotEmpty() }?.joinToString(RECORD, transform = ::encode)))
    }

    private suspend fun all(): List<MovedLaunch> =
        dao
            .get(KEY)
            ?.takeIf { it.isNotEmpty() }
            ?.split(RECORD)
            ?.mapNotNull(::decode)
            .orEmpty()

    companion object {
        /** 设置表里那一格的键。 */
        const val KEY = "launch_moved"

        /** 条与条之间（ASCII RS）。 */
        const val RECORD = "\u001e"

        /** 一条里格与格之间（ASCII US）：台 · 名 · 目录 · 会话名 · 命令。 */
        const val FIELD = "\u001f"

        private fun decode(rec: String): MovedLaunch? {
            val f = rec.split(FIELD)
            if (f.size != 5) return null

            fun opt(i: Int) = f[i].ifEmpty { null }
            return MovedLaunch(f[0], f[1], opt(2), opt(3), opt(4))
        }

        private fun encode(m: MovedLaunch): String =
            listOf(m.hostId, m.label, m.workingDir.orEmpty(), m.tmuxSession.orEmpty(), m.command.orEmpty()).joinToString(FIELD)
    }
}

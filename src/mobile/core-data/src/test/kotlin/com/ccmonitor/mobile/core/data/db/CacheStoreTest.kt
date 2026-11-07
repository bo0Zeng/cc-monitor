package com.ccmonitor.mobile.core.data.db

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 缓存（别台东西的副本：记录页 · 续读点 · 看过哪几条；会话列表副本住嵌的核心）住它自己那个库，永不迁移：纯 JVM，读源码。
 *
 * 缓存读不懂 · 版本不对就当没有、照那台重取；跟 ④（机器 · 钥匙 · 待发 · 草稿 …）住一个库，缓存换一次形就得写一跳迁移、
 * 迁移写错连 ④ 一起丢。所以：缓存那几张表只在 `CacheDatabase`、状态库一张都没有；缓存库不接迁移、版本一换整份丢了重建。
 * 真开库的那几条在设备上：`MigrationTest`（24→25 状态库里的缓存表删掉、④ 一行不丢）· `CacheDaoTest`（缓存库那几张表的规矩）。
 */
class CacheStoreTest {
    @Test
    fun theCacheTablesLiveOnlyInTheCacheDatabase() {
        assertEquals("缓存库的表就是缓存那几张", CACHE_ENTITIES, entitiesOf(CACHE_DATABASE))
        val state = entitiesOf(APP_DATABASE)
        assertTrue("状态库里还有缓存表：${state intersect CACHE_ENTITIES}", (state intersect CACHE_ENTITIES).isEmpty())
    }

    @Test
    fun theCacheDatabaseNeverMigratesAndIsRebuiltWhenItsVersionChanges() {
        val src = read(CACHE_DATABASE)
        assertFalse(
            "缓存库接了迁移：缓存不迁移，版本不对就整份重建",
            "addMigrations" in src || Regex("(?<![A-Za-z])Migration\\(").containsMatchIn(src),
        )
        assertTrue(
            "缓存库的 builder 没写「版本一换整份重建」（`fallbackToDestructiveMigration(dropAllTables = true)`）",
            "fallbackToDestructiveMigration(dropAllTables = true)" in src,
        )
        assertTrue("缓存库不是另一个文件", "\"$CACHE_FILE\"" in src)
        assertTrue("DataModule 没从缓存库那一口造它", "cacheDatabase(" in read(DATA_MODULE))
    }

    /** `@Database(entities = [A::class, …])` 里那几个名字。 */
    private fun entitiesOf(rel: String): Set<String> =
        Regex("(\\w+)::class")
            .findAll(read(rel).substringAfter("entities = [").substringBefore("]"))
            .map { it.groupValues[1] }
            .toSet()

    private fun repoRoot(): File =
        generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
            .take(MAX_WALK_UP)
            .firstOrNull { File(it, "settings.gradle.kts").isFile }
            ?: error("找不到仓根（往上 $MAX_WALK_UP 层都没有 settings.gradle.kts）")

    private fun read(rel: String): String = File(repoRoot(), rel).readText()

    private companion object {
        const val DB = "core-data/src/main/kotlin/com/ccmonitor/mobile/core/data/db"
        const val APP_DATABASE = "$DB/AppDatabase.kt"
        const val CACHE_DATABASE = "$DB/CacheDatabase.kt"
        const val DATA_MODULE = "core-data/src/main/kotlin/com/ccmonitor/mobile/core/data/di/DataModule.kt"
        const val CACHE_FILE = "ccm-cache.db"
        const val MAX_WALK_UP = 12
        val CACHE_ENTITIES = setOf("CachedSession", "RecordPage", "SyncAnchor")
    }
}

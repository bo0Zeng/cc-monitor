package com.ccmonitor.mobile.core.data.db

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Room schema 的处置必须是有意的：纯 JVM，零设备。
 *
 * 迁移真跑一次的是设备上的 `MigrationTest`；没有模拟器时那条不会被跑到，
 * 这里只保证「schema 被动过」不会悄悄发生。不替代 `MigrationTest`：迁移 SQL 写得对不对这里看不见。
 *
 * 处置二选一，并写明在 [PINNED_DISPOSITION]：
 * - `migrate`：`DataModule.kt` 里 `.addMigrations(*ALL_MIGRATIONS)`；迁移链完整到 `version`，
 *   `MigrationTest` 的全链条也指到 `version`。
 * - `destructive`：`.fallbackToDestructiveMigration…`；不要求迁移链，`version` 与导出 json 仍要对齐。
 * 两者都不写或同时写都红。换处置必须改常量，这个决定才会出现在 diff 里。
 *
 * 注意：bump 版本后构建时 KSP 会自动导出新版本的 json，所以「bump 了没导出」抓不到；
 * 导出那条守的是中间缺版本、或 `exportSchema` 被关掉。`@Test` 按行首计数，不剥注释。
 */
class SchemaDispositionTest {
    // ---- 判据 ---------------------------------------------------------------

    /** 声明的 schema 版本还是钉着的那个：任何一次 bump 都要在这里手动改，并选定处置。 */
    @Test
    fun theDeclaredSchemaVersionIsStillTheOnePinnedHere() {
        assertEquals(
            "`AppDatabase` 的 schema 版本变了（钉着 $PINNED_VERSION，实读 ${declaredVersion()}）。\n" +
                "   先定这一版的处置是 `migrate` 还是 `destructive`。不做向后兼容时默认是 `destructive`，\n" +
                "   要同时改 [PINNED_DISPOSITION] 与 `DataModule.kt` 的 builder；仍要 `migrate` 则迁移链与 `MigrationTest` 都跟到新版本。",
            PINNED_VERSION,
            declaredVersion(),
        )
    }

    /**
     * 生产 builder 里恰好一种处置，且是钉着的那一种。
     * 一种都没有时 Room 在版本不匹配时抛 `IllegalStateException`，升级即崩；两种都有则看不出哪个生效。
     */
    @Test
    fun exactlyOneSchemaDispositionIsWiredInTheProductionBuilder() {
        val src = read(DATA_MODULE)
        val migrate = MIGRATE_CALL.containsMatchIn(src)
        val destructive = DESTRUCTIVE_CALL.containsMatchIn(src)

        assertTrue(
            "`$DATA_MODULE` 里一种 schema 处置都没接，版本一变，Room 当场抛异常（用户升级即崩）。\n" +
                "   要么 `.addMigrations(*ALL_MIGRATIONS)`，要么 `.fallbackToDestructiveMigration…`，二者必居其一。",
            migrate || destructive,
        )
        assertTrue(
            "`$DATA_MODULE` 里同时接了迁移与破坏性重建，看不出哪个生效。只留一个。",
            !(migrate && destructive),
        )

        val actual = if (migrate) MIGRATE else DESTRUCTIVE
        assertEquals(
            "schema 处置换了（钉着 `$PINNED_DISPOSITION`，实读 `$actual`），本文件的常量没跟着改。\n" +
                "   换处置是发版级决定（迁移发出去就固定，重建会清库），需手动改常量，让它出现在 diff 里。",
            PINNED_DISPOSITION,
            actual,
        )
    }

    /** `1..version` 每一版都有导出的 json，不多不少；两种处置下都成立。 */
    @Test
    fun everySchemaVersionUpToTheDeclaredOneHasAnExportedJson() {
        val v = declaredVersion()
        assertEquals(
            "导出的 schema json 与声明的版本不对齐。\n" +
                "   缺一版时 `MigrationTestHelper.runMigrationsAndValidate` 拿不到那一版的基线。\n" +
                "   导出目录：`$SCHEMA_DIR`（ksp `room.schemaLocation`，见 core-data/build.gradle.kts）。",
            (1..v).toList(),
            exportedSchemaVersions(),
        )
    }

    /** 迁移链完整：`MIGRATION_i_(i+1)` 逐版声明齐，`ALL_MIGRATIONS` 不漏不多。只在 `migrate` 下要求。 */
    @Test
    fun theMigrationChainIsCompleteAndMatchesTheDeclaredVersion() {
        if (PINNED_DISPOSITION != MIGRATE) {
            assertEquals(
                "处置是 `$PINNED_DISPOSITION`，本条不要求迁移链；但 `ALL_MIGRATIONS` 若还在被生产侧引用就说明没改干净。",
                DESTRUCTIVE,
                PINNED_DISPOSITION,
            )
            return
        }
        val v = declaredVersion()
        val expected = (1 until v).toList()
        assertEquals(
            "`MIGRATION_i_(i+1)` 的声明没覆盖到 $v。少一条，升级到那一版就崩（或被清库）。",
            expected,
            declaredMigrationSteps(),
        )
        assertEquals(
            "`ALL_MIGRATIONS` 数组与声明不一致，声明了却没进数组等于没写：\n" +
                "   Room 只认传进 builder 的那几条。",
            expected,
            migrationsInAllArray(),
        )
    }

    /**
     * 设备侧的全链测也指到了声明的版本。迁移写了、数组进了，但全链还验到旧版本时，
     * 新那一跳没有任何测，本机又跑不了 androidTest，没人会发现。
     */
    @Test
    fun theInstrumentedFullChainTestTargetsTheDeclaredVersion() {
        if (PINNED_DISPOSITION != MIGRATE) return
        val v = declaredVersion()
        val src = read(MIGRATION_TEST)

        val fullChain = FULL_CHAIN_TARGET.findAll(src).map { it.groupValues[1].toInt() }.toList()
        assertEquals(
            "`$MIGRATION_TEST` 的全链条（`runMigrationsAndValidate(dbName, N, true, *ALL_MIGRATIONS)`）\n" +
                "   应当恰好一条且指到 $v，实读 $fullChain。\n" +
                "   它要模拟器才跑得起来；本条只保证它指对了版本，不保证它跑过。",
            listOf(v),
            fullChain,
        )

        val lastStep = "migrate${v - 1}To$v"
        assertTrue(
            "`$MIGRATION_TEST` 里找不到最后那一跳的逐步测（期望一个名字以 `$lastStep` 开头的 `fun`）。\n" +
                "   全链 validate 只比 schema，不验数据保留；逐步那条才验「旧行还在、新列取到默认值」。",
            Regex("fun\\s+" + Regex.escape(lastStep)).containsMatchIn(src),
        )
    }

    /** 自检：读的四份源文件都真读到了。路径打错时别的几条可能拿到空串而恒绿。 */
    @Test
    fun theSourcesThisJudgeReadsAreActuallyBeingRead() {
        for (rel in listOf(APP_DATABASE, DATA_MODULE, MIGRATIONS, MIGRATION_TEST)) {
            val f = File(repoRoot(), rel)
            assertTrue("读不到 `$rel`。仓根实读：${repoRoot().absolutePath}", f.isFile)
            assertTrue("`$rel` 读出来是空的", f.readText().length > MIN_SOURCE_CHARS)
        }
        assertTrue(
            "`AppDatabase` 没开 `exportSchema`，导出目录会停更",
            EXPORT_SCHEMA_ON.containsMatchIn(read(APP_DATABASE)),
        )
    }

    // ---- 读取 ---------------------------------------------------------------

    /** 仓根 ＝ 有 `settings.gradle.kts` 的那一层（测试工作目录可能是仓根，也可能是模块目录）。 */
    private fun repoRoot(): File =
        generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
            .take(MAX_WALK_UP)
            .firstOrNull { File(it, "settings.gradle.kts").isFile }
            ?: error("找不到仓根（往上 $MAX_WALK_UP 层都没有 settings.gradle.kts）：${File(".").absolutePath}")

    private fun read(rel: String): String = File(repoRoot(), rel).readText()

    /**
     * `@Database(... version = N, ...)` 里那个 N。
     * 按行首锚定（`^\s*version = N,`）：KDoc 行以 `*` 开头、行注释以 `//` 开头，都不会命中，不必剥注释。
     */
    private fun declaredVersion(): Int {
        val hits = VERSION_DECL.findAll(read(APP_DATABASE)).map { it.groupValues[1].toInt() }.toList()
        assertEquals("`$APP_DATABASE` 里 `version = N,` 应恰好一处，实得 $hits，锚点失效", 1, hits.size)
        return hits.single()
    }

    private fun exportedSchemaVersions(): List<Int> =
        (File(repoRoot(), SCHEMA_DIR).listFiles() ?: emptyArray())
            .filter { it.isFile && it.extension == "json" }
            .mapNotNull { it.nameWithoutExtension.toIntOrNull() }
            .sorted()

    private fun declaredMigrationSteps(): List<Int> =
        MIGRATION_DECL
            .findAll(read(MIGRATIONS))
            .map { it.groupValues[1].toInt() }
            .toList()
            .sorted()

    private fun migrationsInAllArray(): List<Int> {
        val src = read(MIGRATIONS)
        val arrayBody =
            src
                .substringAfter(ALL_MIGRATIONS_ANCHOR, "")
                .substringAfter("arrayOf(", "")
                .substringBefore(")")
        assertTrue("`$MIGRATIONS` 里找不到 `$ALL_MIGRATIONS_ANCHOR` 的 `arrayOf(` 块，锚点失效", arrayBody.isNotBlank())
        return MIGRATION_REF
            .findAll(arrayBody)
            .map { it.groupValues[1].toInt() }
            .toList()
            .sorted()
    }

    private companion object {
        const val MIGRATE = "migrate"
        const val DESTRUCTIVE = "destructive"

        /** 改 schema 版本时同时改这里。 */
        const val PINNED_VERSION = 14

        /** schema 处置：[MIGRATE]（写迁移、不丢数据）或 [DESTRUCTIVE]（重建、清库）。换处置必须改这一行。 */
        const val PINNED_DISPOSITION = MIGRATE

        const val APP_DATABASE = "core-data/src/main/kotlin/com/ccmonitor/mobile/core/data/db/AppDatabase.kt"
        const val MIGRATIONS = "core-data/src/main/kotlin/com/ccmonitor/mobile/core/data/db/Migrations.kt"
        const val DATA_MODULE = "core-data/src/main/kotlin/com/ccmonitor/mobile/core/data/di/DataModule.kt"
        const val MIGRATION_TEST = "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/db/MigrationTest.kt"
        const val SCHEMA_DIR = "core-data/schemas/com.ccmonitor.mobile.core.data.db.AppDatabase"

        const val ALL_MIGRATIONS_ANCHOR = "val ALL_MIGRATIONS"
        const val MAX_WALK_UP = 12
        const val MIN_SOURCE_CHARS = 200

        val VERSION_DECL = Regex("^\\s*version = (\\d+),", RegexOption.MULTILINE)
        val EXPORT_SCHEMA_ON = Regex("^\\s*exportSchema = true,", RegexOption.MULTILINE)
        val MIGRATION_DECL = Regex("^val MIGRATION_(\\d+)_\\d+ =", RegexOption.MULTILINE)
        val MIGRATION_REF = Regex("^\\s*MIGRATION_(\\d+)_\\d+,", RegexOption.MULTILINE)
        val MIGRATE_CALL = Regex("^\\s*\\.addMigrations\\(", RegexOption.MULTILINE)
        val DESTRUCTIVE_CALL = Regex("^\\s*\\.fallbackToDestructiveMigration", RegexOption.MULTILINE)
        val FULL_CHAIN_TARGET = Regex("runMigrationsAndValidate\\(dbName, (\\d+), true, \\*ALL_MIGRATIONS\\)")
    }
}

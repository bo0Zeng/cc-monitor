package com.ccmonitor.mobile.core.data.db

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * Room schema 只许走迁移：纯 JVM，零设备。
 *
 * 库里有 ssh 身份（私钥）与机器表：破坏性重建（`fallbackToDestructiveMigration…`）等于每次升级把用户的钥匙和机器清空，
 * 所以生产 builder 只接 `.addMigrations(*ALL_MIGRATIONS)`、一处破坏性回退都不许有；迁移链完整到 `version`，
 * 设备上的 `MigrationTest` 全链条也指到 `version`、最后那一跳有逐步测（旧行还在）。
 *
 * 迁移真跑一次的是设备上的 `MigrationTest`；没有模拟器时那条不会被跑到，这里只保证「schema 被动过」不会悄悄发生，
 * 迁移 SQL 写得对不对这里看不见。
 *
 * 注意：bump 版本后构建时 KSP 会自动导出新版本的 json，所以「bump 了没导出」抓不到；
 * 导出那条守的是中间缺版本、或 `exportSchema` 被关掉。`@Test` 按行首计数，不剥注释。
 */
class SchemaDispositionTest {
    // ---- 判据 ---------------------------------------------------------------

    /** 声明的 schema 版本还是钉着的那个：任何一次 bump 都要在这里手动改，并写上那一跳的迁移。 */
    @Test
    fun theDeclaredSchemaVersionIsStillTheOnePinnedHere() {
        assertEquals(
            "`AppDatabase` 的 schema 版本变了（钉着 $PINNED_VERSION，实读 ${declaredVersion()}）。\n" +
                "   写上那一跳的 `MIGRATION_i_(i+1)` 并进 `ALL_MIGRATIONS`，`MigrationTest` 的全链条与逐步测跟到新版本，再改这里。",
            PINNED_VERSION,
            declaredVersion(),
        )
    }

    /**
     * 生产 builder 接了迁移链，且手机生产源里一处破坏性回退都没有。
     * 没接迁移 ⇒ 版本一变 Room 当场抛异常（升级即崩）；有破坏性回退 ⇒ 迁移缺一跳时悄悄清库（钥匙与机器全没）。
     */
    @Test
    fun onlyMigrationsAreWiredAndThereIsNoDestructiveFallback() {
        assertTrue(
            "`$DATA_MODULE` 里没接 `.addMigrations(*ALL_MIGRATIONS)`：版本一变，Room 当场抛异常（用户升级即崩）。",
            MIGRATE_CALL.containsMatchIn(read(DATA_MODULE)),
        )
        val destructive =
            repoRoot()
                .walkTopDown()
                .filter { it.isFile && it.extension == "kt" && "/src/main/" in it.invariantSeparatorsPath }
                .filter { DESTRUCTIVE_CALL.containsMatchIn(it.readText()) }
                .map { it.relativeTo(repoRoot()).invariantSeparatorsPath }
                .toList()
        assertEquals(
            "手机生产源里出现了破坏性回退（升级时会清掉库里的钥匙与机器）：只许写迁移。",
            emptyList<String>(),
            destructive,
        )
    }

    /** `1..version` 每一版都有导出的 json，不多不少。 */
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

    /** 迁移链完整：`MIGRATION_i_(i+1)` 逐版声明齐，`ALL_MIGRATIONS` 不漏不多。 */
    @Test
    fun theMigrationChainIsCompleteAndMatchesTheDeclaredVersion() {
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
        /** 改 schema 版本时同时改这里。 */
        const val PINNED_VERSION = 18

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
        val DESTRUCTIVE_CALL = Regex("fallbackToDestructiveMigration")
        val FULL_CHAIN_TARGET = Regex("runMigrationsAndValidate\\(dbName, (\\d+), true, \\*ALL_MIGRATIONS\\)")
    }
}

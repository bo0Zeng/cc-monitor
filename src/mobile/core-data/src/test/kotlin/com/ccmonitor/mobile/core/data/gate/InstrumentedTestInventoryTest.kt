package com.ccmonitor.mobile.core.data.gate

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 把 androidTest 的接线面钉住，纯 JVM、零设备。
 *
 * instrumented 测要模拟器或真机：全量门禁的 `android-device` 那一格起一台模拟器跑它们（10-11 起），CI 不跑；
 * 这份 JVM 单测（门禁 `mobile` 格与 CI 都跑）守的是「它们还在、还接着那个入口」：
 * - 文件集与逐文件 `@Test` 数（行首 `@Test` 计数，不剥注释，写在注释里的也算）；
 * - 设备测的入口 `scripts/android-test.sh` 仍为每个有 instrumented 测的模块跑 `connectedDebugAndroidTest`
 *   （门禁那一格调的就是它，它是设备测唯一的入口）；
 * - 入口脚本不用 instrumentation 过滤挑文件（每一份都跑得到；要台架的那一份不带参数时自己跳过）。
 *
 * 它不知道那些测跑起来是红是绿，那要设备。
 *
 * 注意：`app/`、`core-ssh/` 的 androidTest 源和入口脚本不是任何 unit test 任务的输入，
 * 必须在 `core-data/build.gradle.kts` 里登记成 `inputs`，否则改了它们这里也不重跑，恒绿。
 * 看回绿时要看到任务真执行（或加 `--rerun`），`UP-TO-DATE` / `FROM-CACHE` 不算跑过。
 */
class InstrumentedTestInventoryTest {
    // ---- 判据 ---------------------------------------------------------------

    /** 文件集逐字钉死：加、删、挪位置都红。 */
    @Test
    fun theSetOfInstrumentedTestFilesIsExactlyWhatIsPinnedHere() {
        assertEquals(
            "androidTest 的文件集变了。\n" +
                "   它们要模拟器或真机才跑得起来，本机门禁发现不了改动，需手动把新文件集写进这里。\n" +
                "   `MigrationTest` 不许删：手机库只许走迁移（`SchemaDispositionTest`）。",
            PINNED_FILES,
            instrumentedTestFiles().map { it.first },
        )
    }

    /** 逐文件 `@Test` 数与总数都钉：只钉总数时，一处删一条、另一处加一条会悄悄过去。 */
    @Test
    fun theNumberOfInstrumentedJudgesPerFileIsExactlyWhatIsPinnedHere() {
        val actual = instrumentedTestFiles().toMap()
        assertEquals(
            "逐文件 `@Test` 数变了（口径：行首 `@Test` 计数）。",
            PINNED_TEST_COUNTS,
            actual,
        )
        assertEquals(
            "androidTest 的 `@Test` 总数变了，口径是方法数（$PINNED_TOTAL_TESTS），不是文件数（${PINNED_FILES.size}）。",
            PINNED_TOTAL_TESTS,
            actual.values.sum(),
        )
    }

    /** 设备测的入口还在，三个有 instrumented 测的模块都还接着线；删掉入口本机门禁不会报。 */
    @Test
    fun theDeviceTestEntryStillWiresEveryModuleThatHasInstrumentedTests() {
        val ci = read(CI_WORKFLOW)
        assertTrue(
            "`$CI_WORKFLOW` 里找不到 `$ANDROID_TEST_JOB`，那 $PINNED_TOTAL_TESTS 条 instrumented 测一条都不会跑。",
            ci.contains(ANDROID_TEST_JOB),
        )
        for (module in MODULES_WIRED_IN_CI) {
            assertTrue(
                "`$CI_WORKFLOW` 不再为 `$module` 跑 `connectedDebugAndroidTest`，\n" +
                    "   那个模块的 instrumented 测没有入口可跑。",
                Regex(Regex.escape(module) + ":connectedDebugAndroidTest").containsMatchIn(ci),
            )
        }
    }

    /**
     * 每一份 instrumented 测都从入口脚本跑到：入口里不再用 instrumentation 过滤挑文件
     * （从前那一份要真 SSH 服务端的测已经删了；RustTransportTest 不带台架参数时自己整份跳过，不靠过滤）。
     */
    @Test
    fun noInstrumentedFileIsFilteredOutOfTheEntry() {
        val ci = read(CI_WORKFLOW)
        assertTrue(
            "`$CI_WORKFLOW` 又用 instrumentation 过滤挑文件了：被挑掉的那几份没有入口可跑。",
            !ci.contains(CLASS_FILTER),
        )
    }

    /** 自检：扫描面与入口脚本真读到了。路径打错时文件集为空、计数为 0，别的几条不一定能发现。 */
    @Test
    fun theScanSurfaceAndTheCiFileAreActuallyBeingRead() {
        val found = instrumentedTestFiles()
        assertEquals(
            "androidTest 扫描面扫到 ${found.size} 个文件（期望 ${PINNED_FILES.size}），路径或仓根解析写错了。\n" +
                "   仓根实读：${repoRoot().absolutePath}",
            PINNED_FILES.size,
            found.size,
        )
        assertTrue("扫到的文件里一条 `@Test` 都没有，计数器失效", found.sumOf { it.second } > 0)
        assertTrue(
            "读不到 `$CI_WORKFLOW`",
            File(repoRoot(), CI_WORKFLOW).isFile,
        )
        assertTrue("`$CI_WORKFLOW` 读出来是空的", read(CI_WORKFLOW).length > MIN_CI_CHARS)
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
     * 全仓 androidTest 的 `.kt`，按仓根相对路径排序，配上各自的行首 `@Test` 计数。
     * 按模块目录枚举而不全仓 walk：`build/` 里的产物随上次构建漂移，会让结果不稳定。
     */
    private fun instrumentedTestFiles(): List<Pair<String, Int>> {
        val root = repoRoot()
        return (root.listFiles() ?: emptyArray())
            .filter { it.isDirectory && it.name !in SKIPPED_DIRS }
            .map { File(it, ANDROID_TEST_SRC) }
            .filter { it.isDirectory }
            .flatMap { dir -> dir.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList() }
            .map { f -> f.relativeTo(root).path.replace(File.separatorChar, '/') to TEST_ANNOTATION.findAll(f.readText()).count() }
            .sortedBy { it.first }
    }

    private companion object {
        /** androidTest 文件集。 */
        val PINNED_FILES =
            listOf(
                "app/src/androidTest/kotlin/com/ccmonitor/mobile/SmokeTest.kt",
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/HostDaoTest.kt",
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/KeystoreCryptoTest.kt",
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/db/CacheDaoTest.kt",
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/db/MigrationTest.kt",
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/db/RestoreDefaultButtonsTest.kt",
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/db/SeedOnCreateTest.kt",
                "core-rs/src/androidTest/kotlin/com/ccmonitor/mobile/core/rs/CoreLoadTest.kt",
                "core-ssh/src/androidTest/kotlin/com/ccmonitor/mobile/core/ssh/PublicKeyDeriverTest.kt",
                "core-ssh/src/androidTest/kotlin/com/ccmonitor/mobile/core/ssh/RustTransportTest.kt",
            )

        /** 逐文件 `@Test` 方法数。 */
        val PINNED_TEST_COUNTS =
            mapOf(
                "app/src/androidTest/kotlin/com/ccmonitor/mobile/SmokeTest.kt" to 1,
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/HostDaoTest.kt" to 1,
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/KeystoreCryptoTest.kt" to 3,
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/db/CacheDaoTest.kt" to 3,
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/db/MigrationTest.kt" to 18,
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/db/RestoreDefaultButtonsTest.kt" to 1,
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/db/SeedOnCreateTest.kt" to 1,
                "core-rs/src/androidTest/kotlin/com/ccmonitor/mobile/core/rs/CoreLoadTest.kt" to 1,
                "core-ssh/src/androidTest/kotlin/com/ccmonitor/mobile/core/ssh/PublicKeyDeriverTest.kt" to 2,
                "core-ssh/src/androidTest/kotlin/com/ccmonitor/mobile/core/ssh/RustTransportTest.kt" to 5,
            )

        const val PINNED_TOTAL_TESTS = 36
        const val CI_WORKFLOW = "scripts/android-test.sh"
        const val ANDROID_TEST_JOB = "connectedDebugAndroidTest"
        const val CLASS_FILTER = "testInstrumentationRunnerArguments.class="

        val MODULES_WIRED_IN_CI = listOf(":core-data", ":app", ":core-ssh", ":core-rs")

        const val ANDROID_TEST_SRC = "src/androidTest"
        val SKIPPED_DIRS = setOf("build", ".git", ".gradle", "libs", "bridge", "doc", "scratchpad", "testing", "config", "gradle")

        const val MAX_WALK_UP = 12
        const val MIN_CI_CHARS = 500

        val TEST_ANNOTATION = Regex("^\\s*@Test\\b", RegexOption.MULTILINE)
    }
}

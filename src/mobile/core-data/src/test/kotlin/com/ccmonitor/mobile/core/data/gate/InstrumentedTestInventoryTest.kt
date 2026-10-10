package com.ccmonitor.mobile.core.data.gate

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 把 androidTest 的接线面钉住，纯 JVM、零设备。
 *
 * instrumented 测要模拟器或真机，本机门禁跑不到它们，所以这里守的是「它们还在、还接着 CI」：
 * - 文件集与逐文件 `@Test` 数（行首 `@Test` 计数，不剥注释，写在注释里的也算）；
 * - 设备测的入口 `scripts/android-test.sh` 仍为每个有 instrumented 测的模块跑 `connectedDebugAndroidTest`
 *   （并进 cc-monitor 后 androidTest 暂不进门禁与 CI，这份脚本是它们唯一的入口）；
 * - `SshConnectionTest` 要一台真 SSH 服务端，入口脚本刻意用 instrumentation 过滤跳过它，这个缺口写成断言。
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
                "   删 `MigrationTest` 时同时改 `SchemaDispositionTest` 的 `PINNED_DISPOSITION`。",
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
     * `SshConnectionTest` 刻意不进入口脚本：它要真 SSH 服务端，留真机手测。
     * 入口脚本用 `-Pandroid.testInstrumentationRunnerArguments.class=…KeystoreSignerInstrumentedTest`
     * 把 `:core-ssh` 过滤成只跑那一个文件；过滤没了或放宽了，都会去连真服务端而必挂。
     */
    @Test
    fun theOneInstrumentedFileThatCiDeliberatelySkipsIsStillSkippedOnPurpose() {
        val ci = read(CI_WORKFLOW)
        assertTrue(
            "`$CI_WORKFLOW` 里 `:core-ssh` 的 instrumentation 过滤不见了。\n" +
                "   没有它，会去跑 `$CI_SKIPPED_FILE`（要连一台真 SSH 服务端），在模拟器上必挂。",
            ci.contains(CORE_SSH_FILTER),
        )
        val counts = instrumentedTestFiles().toMap()
        val skipped = counts[CI_SKIPPED_FILE]
        assertEquals(
            "`$CI_SKIPPED_FILE` 的条数变了（它是唯一不进入口脚本的文件）。\n" +
                "   它的每一条都只能真机手测，增减要有意识地做。",
            PINNED_TESTS_NOT_IN_CI,
            skipped,
        )
        assertEquals(
            "「入口脚本接线到的 `@Test` 数」对不上了。\n" +
                "   分解：总 $PINNED_TOTAL_TESTS 条 − 刻意不跑的 $PINNED_TESTS_NOT_IN_CI 条（`$CI_SKIPPED_FILE`）＝ $PINNED_TESTS_IN_CI 条。",
            PINNED_TESTS_IN_CI,
            counts.values.sum() - PINNED_TESTS_NOT_IN_CI,
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
                "app/src/androidTest/kotlin/com/ccmonitor/mobile/ui/chat/ChatReplayUiTest.kt",
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/HostDaoTest.kt",
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/KeystoreCryptoTest.kt",
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/db/MigrationTest.kt",
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/db/RestoreDefaultButtonsTest.kt",
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/db/SeedOnCreateTest.kt",
                "core-ssh/src/androidTest/kotlin/com/ccmonitor/mobile/core/ssh/KeystoreSignerInstrumentedTest.kt",
                "core-ssh/src/androidTest/kotlin/com/ccmonitor/mobile/core/ssh/SshConnectionTest.kt",
            )

        /** 逐文件 `@Test` 方法数。 */
        val PINNED_TEST_COUNTS =
            mapOf(
                "app/src/androidTest/kotlin/com/ccmonitor/mobile/SmokeTest.kt" to 1,
                "app/src/androidTest/kotlin/com/ccmonitor/mobile/ui/chat/ChatReplayUiTest.kt" to 14,
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/HostDaoTest.kt" to 1,
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/KeystoreCryptoTest.kt" to 3,
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/db/MigrationTest.kt" to 14,
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/db/RestoreDefaultButtonsTest.kt" to 1,
                "core-data/src/androidTest/kotlin/com/ccmonitor/mobile/core/data/db/SeedOnCreateTest.kt" to 1,
                "core-ssh/src/androidTest/kotlin/com/ccmonitor/mobile/core/ssh/KeystoreSignerInstrumentedTest.kt" to 2,
                "core-ssh/src/androidTest/kotlin/com/ccmonitor/mobile/core/ssh/SshConnectionTest.kt" to 9,
            )

        const val PINNED_TOTAL_TESTS = 46
        const val PINNED_TESTS_IN_CI = 37
        const val PINNED_TESTS_NOT_IN_CI = 9

        const val CI_SKIPPED_FILE = "core-ssh/src/androidTest/kotlin/com/ccmonitor/mobile/core/ssh/SshConnectionTest.kt"
        const val CI_WORKFLOW = "scripts/android-test.sh"
        const val ANDROID_TEST_JOB = "connectedDebugAndroidTest"
        const val CORE_SSH_FILTER =
            "android.testInstrumentationRunnerArguments.class=com.ccmonitor.mobile.core.ssh.KeystoreSignerInstrumentedTest"

        val MODULES_WIRED_IN_CI = listOf(":core-data", ":app", ":core-ssh")

        const val ANDROID_TEST_SRC = "src/androidTest"
        val SKIPPED_DIRS = setOf("build", ".git", ".gradle", "libs", "bridge", "doc", "scratchpad", "testing", "config", "gradle")

        const val MAX_WALK_UP = 12
        const val MIN_CI_CHARS = 500

        val TEST_ANNOTATION = Regex("^\\s*@Test\\b", RegexOption.MULTILINE)
    }
}

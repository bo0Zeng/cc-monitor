package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 两个名字空间前缀各只定义一处；后端那个只许出现在「读」的位置。
 *
 * 远端分两层：通用层是 cc-monitor 后端的（`~/.cc-monitor/` 下的一切、tmux `cc-*`），我们只读；
 * 专属层是我们的（`~/.aterm/` 下的一切、tmux `atermpipe-*`），我们才写。本文件是那条界线的源码扫描那一道：
 *
 * | # | 问什么 |
 * |---|---|
 * | ① | `const val ATERM_HOME` / `const val PIPE_SESSION_PREFIX` 各恰好定义一处 |
 * | ② | `.aterm` 与 `atermpipe-` 两个字面在生产代码里的每文件命中数逐字钉死 |
 * | ③ | `.cc-monitor` 只出现在 `DaemonLocator` 的只读候选表 |
 *
 * 行为那一道（会写远端的命令串，其真实输出里的路径落在哪）住 `core-claude` 的 `NamespacePrefixTest`。
 * 扫描防「新长出来一处写路径」，真实输出防「这几支偷偷改了落点」。
 *
 * 注意：一条读别的模块源文件的判据，必须住在那个 task 的输入涵盖得到的地方。放在 `core-claude` 里的话，
 * 改 `app/` 下的文件不会让 `:core-claude:test` 重跑（它照样 `UP-TO-DATE`），判据就成了恒绿。
 * `:app:testDebugUnitTest` 的运行时 classpath 含全部 `core-…` 模块的产物，任何模块的代码改动都会让它重跑。
 * 只改注释的改动可能不改字节码、不重跑；对本判据无影响（[codeOnly] 本来就把注释剥掉了）。
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 这三条 |
 * |---|---|
 * | 复制一份 `".aterm"` / `"atermpipe-"` 字面到第二处 | 红（点名文件 + 计数） |
 * | 在生产代码里写死 `"~/.cc-monitor/x"` | 红（点名文件） |
 * | `"." + "cc-monitor"` · `String(charArrayOf(…))` · 反射拼串 | 抓不到：防手滑不防拼接 |
 * | `androidTest` / `src/debug` 源集 | 不在扫描面内 |
 * | 注释 / KDoc 里提到这些字面 | 刻意不算（[codeOnly] 先剥注释），注释里写不出路径 |
 */
class NamespaceLiteralScanTest {
    /**
     * 两个前缀常量各恰好定义一处，住址定值钉死。
     *
     * 多一处红（有人又造了一份），挪窝也红（要重新过一遍眼再改这张表）。
     */
    @Test
    fun theTwoPrefixConstantsAreDefinedExactlyOnce() {
        val root = repoRoot()
        val sources = productionSources(root)
        assertTrue("前提：得真扫到源文件，否则这条是空跑（实得 ${sources.size} 个）", sources.size > 150)

        for ((decl, expectedFile) in PINNED_CONST_DECLS) {
            val found =
                sources
                    .filter { codeOnly(it.readText()).contains(decl) }
                    .map { it.relativeTo(root).invariantSeparatorsPath }
                    .sorted()
            assertEquals(
                "`$decl` 必须只有一个定义处（两个前缀各一个唯一定义处，别处只许引用常量）。",
                listOf(expectedFile),
                found,
            )
        }
    }

    /**
     * `.aterm` 与 `atermpipe-` 两个字面在生产代码里的每文件命中数逐字钉死。
     *
     * 钉的是 file -> count（不只是文件集）：同一个文件里再长出第二处照样红。
     *
     * `.aterm` 的匹配前面不许挨标识符字符：否则 `com.ccmonitor.mobile` 这个包名
     * 会让每个文件都命中，判据当场沦为噪音。
     */
    @Test
    fun theTwoNamespaceLiteralsAppearNowhereElse() {
        val root = repoRoot()
        val sources = productionSources(root)
        assertTrue("前提：得真扫到源文件，否则这条是空跑（实得 ${sources.size} 个）", sources.size > 150)

        for ((mark, pinned) in PINNED_LITERAL_COUNTS) {
            val counts = sortedMapOf<String, Int>()
            for (f in sources) {
                val n = countLiteral(codeOnly(f.readText()), mark)
                if (n > 0) counts[f.relativeTo(root).invariantSeparatorsPath] = n
            }
            assertTrue("前提：`$mark` 整份语料一次都没命中 ⇒ 扫描器瞎了、这条判据恒绿", counts.values.sum() > 0)
            assertEquals(
                "`$mark` 这个字面只许住在常量的定义处（别处引用常量即可）。" +
                    "多出来的请改成引用常量；确属散文/引文的，连同理由加进 PINNED_LITERAL_COUNTS。",
                pinned,
                counts.toMap(),
            )
        }
    }

    /**
     * `.cc-monitor` 这个字面在生产代码里只许出现在 `DaemonLocator`。
     *
     * 那里它是 `SHARED_BIN_DIR = ".cc-monitor/bin/"`，唯一用途是 `DaemonLocator.candidates`
     * 拼出 `[ -x "$HOME"/.cc-monitor/bin/… ]` 这种存在性探测，纯读
     * （「它真的没写东西」由 `NamespacePrefixTest.theDaemonLocatorCandidatesOnlyProbeAndNeverWrite` 拿真实输出证）。
     *
     * 定值钉：多一个文件红（有人往后端名字空间伸手了），少一个也红（候选表挪窝，要重新过眼）。
     */
    @Test
    fun theCcMonitorLiteralOnlyLivesInTheReadOnlyLocator() {
        val root = repoRoot()
        val sources = productionSources(root)
        assertTrue("前提：得真扫到源文件，否则这条是空跑（实得 ${sources.size} 个）", sources.size > 150)

        val found = sortedSetOf<String>()
        var hits = 0
        for (f in sources) {
            val n = countLiteral(codeOnly(f.readText()), CC_MONITOR)
            hits += n
            if (n > 0) found += f.relativeTo(root).invariantSeparatorsPath
        }
        assertTrue("前提：`$CC_MONITOR` 整份语料一次都没命中 ⇒ 扫描器瞎了、这条判据恒绿", hits > 0)
        assertEquals(
            "`$CC_MONITOR` 是后端的名字空间：我们只读、绝不写。" +
                "合法住处是 `BackendBin`（部署落点）与 `DaemonLocator` 的只读候选表。",
            PINNED_CC_MONITOR_FILES,
            found.toSet(),
        )
    }

    /**
     * 扫描器自检（同 `AgentProfileSingleAddressTest.theScannerCountsRealDispatchAndNothingElse`）。
     *
     * 上面三条全靠 [codeOnly] + [countLiteral] 认得准。认错了（把注释算进来、
     * 或者把包名 `com.ccmonitor.mobile` 也算成命中），判据要么恒绿要么开始误红，两种都比没有更坏。
     */
    @Test
    fun theScannerCountsRealLiteralsAndNothingElse() {
        val sample =
            "package com.ccmonitor.mobile.ssh\n" + // 包名：前面挨着标识符字符 ⇒ 不算
                "import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation\n" + // 同上
                "// 注释里的 \".aterm\" 与 atermpipe- 不算\n" +
                "/* 块注释里的 .cc-monitor 也不算 */\n" +
                "const val H = \".aterm\"\n" + // 真字面
                "const val P = \"atermpipe-\"\n" // 真字面
        val code = codeOnly(sample)
        assertEquals("`.aterm` 只该认出常量那一处：\n$code", 1, countLiteral(code, DOT_ATERM))
        assertEquals("`atermpipe-` 只该认出常量那一处：\n$code", 1, countLiteral(code, PIPE_PREFIX))
        assertEquals("`.cc-monitor` 全在注释里 ⇒ 零命中：\n$code", 0, countLiteral(code, CC_MONITOR))
        // 反过来也要证：真字面一定认得出，否则「剥得干净」可以靠「什么都不认」作弊。
        assertEquals("裸一行真字面必须认出", 1, countLiteral(codeOnly("val x = \"$CC_MONITOR/bin/\""), CC_MONITOR))
        assertEquals("包名一个都不许算进来", 0, countLiteral(codeOnly("package com.ccmonitor.mobile.ui.host"), DOT_ATERM))
        assertEquals("更长的名字（`.atermfoo`）不许算成命中", 0, countLiteral(codeOnly("val x = \".atermfoo\""), DOT_ATERM))
    }

    // ---- 扫描器 ---------------------------------------------------------------

    /**
     * 剥注释、留字面量，走全仓唯一一份词法扫描器 [KotlinSourceScanner.codeOnlyKeepingLiterals]。
     *
     * 必须留字面：本条的载荷就是 `.aterm` / `atermpipe-` / `.cc-monitor` 这几个路径字面。
     * 换成「删字面」那支，三条判据一起变成零命中；[theScannerCountsRealLiteralsAndNothingElse]
     * 里那句「裸一行真字面必须认出」正是为这个方向钉的。它看不见什么写在 [KotlinSourceScanner] 的头注里。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)

    /**
     * 数 [mark] 的真实出现次数。两侧都护：
     * - 前面紧挨标识符字符 ⇒ 是别的东西（`com.ccmonitor.mobile` 的包名尾巴），不算；
     * - 后面紧挨标识符字符 ⇒ 是更长的名字（`.atermfoo`），不算。
     */
    private fun countLiteral(
        code: String,
        mark: String,
    ): Int {
        var n = 0
        var i = 0
        while (true) {
            val at = code.indexOf(mark, i)
            if (at < 0) break
            i = at + mark.length
            if (at > 0 && isIdentChar(code[at - 1])) continue
            if (i < code.length && isIdentChar(code[i])) continue
            n++
        }
        return n
    }

    private fun isIdentChar(c: Char): Boolean = c.isLetterOrDigit() || c == '_'

    /** 仓根 = 往上走到含 `settings.gradle.kts` 的那一层（Gradle 跑测试时 cwd = 模块目录）。 */
    private fun repoRoot(): File =
        generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
            .take(MAX_WALK_UP)
            .firstOrNull { File(it, "settings.gradle.kts").isFile }
            ?: error("找不到仓根（往上 $MAX_WALK_UP 层都没有 settings.gradle.kts）：${File(".").absolutePath}")

    /**
     * 扫描面：`app/src/main/kotlin` + 每个 `core-…` 模块的 `src/main/kotlin` 下的全部 `.kt`。
     *
     * 这就是本仓全部 Kotlin 生产代码（仓根那个 `bridge/` 不含 `.kt`），但只含 `main` 源集。
     */
    private fun productionSources(root: File): List<File> {
        val roots =
            (listOf(File(root, "app")) + (root.listFiles()?.filter { it.isDirectory && it.name.startsWith("core-") } ?: emptyList()))
                .map { File(it, "src/main/kotlin") }
                .filter { it.isDirectory }
                .sortedBy { it.path }
        assertTrue("前提：得找到扫描面（app + core-*），实得 ${roots.size} 个源根", roots.size >= 2)
        return roots.flatMap { r -> r.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList() }
    }

    companion object {
        private const val MAX_WALK_UP = 6

        private const val DOT_ATERM = ".aterm"
        private const val PIPE_PREFIX = "atermpipe-"
        private const val CC_MONITOR = ".cc-monitor"

        /** 定值钉：两个前缀常量的声明各住哪个文件。 */
        private val PINNED_CONST_DECLS =
            linkedMapOf(
                "const val ATERM_HOME" to "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/command/ClaudeInvocation.kt",
                "const val PIPE_SESSION_PREFIX" to "app/src/main/kotlin/com/ccmonitor/mobile/ssh/PipeLauncher.kt",
            )

        /**
         * 定值钉：两个名字空间字面在生产代码里的 file -> 次数。
         *
         * | 文件 | 次数 | 为什么它可以有 |
         * |---|---|---|
         * | `ClaudeInvocation.kt` | 1 | `const val ATERM_HOME = ".aterm"`，唯一定义处 |
         * | `PipeLauncher.kt` | 1 | `const val PIPE_SESSION_PREFIX = "atermpipe-"`，唯一定义处 |
         *
         * 钉的是次数不只是文件集：同一个文件里再长出一处也红。
         */
        private val PINNED_LITERAL_COUNTS =
            linkedMapOf(
                DOT_ATERM to
                    mapOf(
                        "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/command/ClaudeInvocation.kt" to 1,
                    ),
                PIPE_PREFIX to mapOf("app/src/main/kotlin/com/ccmonitor/mobile/ssh/PipeLauncher.kt" to 1),
            )

        /**
         * 定值钉：`.cc-monitor` 的合法住处——`BackendBin`（部署落点 `~/.cc-monitor/bin/ccm`，拼后端命令只在那里），
         * 以及还没删的 `DaemonLocator` 只读候选表（α 一轮完成那条流还用，随它一起删）。
         */
        private val PINNED_CC_MONITOR_FILES =
            setOf(
                "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/link/OneShot.kt",
                "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/transport/DaemonLocator.kt",
            )
    }
}

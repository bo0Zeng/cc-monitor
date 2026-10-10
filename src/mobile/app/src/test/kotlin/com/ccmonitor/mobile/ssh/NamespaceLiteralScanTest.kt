package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 手机在远端不再有自己的名字空间；后端那个只许出现在「读」的位置。
 *
 * 从前手机在远端有一层自己的（`~/.aterm/` 下的管道文件、tmux `atermpipe-*`）；起会话 · 送字改走那台核心之后那一层没了。
 * 本文件是那条界线的源码扫描那一道：
 *
 * | # | 问什么 |
 * |---|---|
 * | ① | `.aterm` 与 `atermpipe-` 两个字面在生产代码里一次都不出现 |
 * | ② | `.cc-monitor` 只出现在 `BackendBin`（部署落点）那一处 |
 *
 * 注意：一条读别的模块源文件的判据，必须住在那个 task 的输入涵盖得到的地方。放在 `core-claude` 里的话，
 * 改 `app/` 下的文件不会让 `:core-claude:test` 重跑（它照样 `UP-TO-DATE`），判据就成了恒绿。
 * `:app:testDebugUnitTest` 的运行时 classpath 含全部 `core-…` 模块的产物，任何模块的代码改动都会让它重跑。
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 这几条 |
 * |---|---|
 * | 又写回一处 `".aterm"` / `"atermpipe-"` 字面 | 红（点名文件 + 计数） |
 * | 在生产代码里写死 `"~/.cc-monitor/x"` | 红（点名文件） |
 * | `"." + "cc-monitor"` · `String(charArrayOf(…))` · 反射拼串 | 抓不到：防手滑不防拼接 |
 * | `androidTest` / `src/debug` 源集 | 不在扫描面内 |
 * | 注释 / KDoc 里提到这些字面 | 刻意不算（[codeOnly] 先剥注释） |
 *
 * 「一次都不出现」那一条的判别力押在扫描器自检（[theScannerCountsRealLiteralsAndNothingElse]）上：它证真字面一定认得出。
 */
class NamespaceLiteralScanTest {
    /** 手机自己那一层名字空间的两个字面，生产代码里一次都不出现。 */
    @Test
    fun theOldPhoneNamespaceLiteralsAppearNowhere() {
        val root = repoRoot()
        val sources = productionSources(root)
        assertTrue("前提：得真扫到源文件，否则这条是空跑（实得 ${sources.size} 个）", sources.size > 150)
        for (mark in listOf(DOT_ATERM, PIPE_PREFIX)) {
            val counts = sortedMapOf<String, Int>()
            for (f in sources) {
                val n = countLiteral(codeOnly(f.readText()), mark)
                if (n > 0) counts[f.relativeTo(root).invariantSeparatorsPath] = n
            }
            assertEquals("`$mark` 是手机从前在远端自己那一层的名字：那一层已经没有了，别再往那里写。", emptyMap<String, Int>(), counts.toMap())
        }
    }

    /**
     * `.cc-monitor` 这个字面在生产代码里只许出现在 `BackendBin`：那里它是 `"$HOME"/.cc-monitor/bin/ccm`，
     * 拼后端命令只在那一处，手机不往后端的名字空间写别的东西。
     *
     * 定值钉：多一个文件红（有人往后端名字空间伸手了），少一个也红（落点挪窝，要重新过眼）。
     */
    @Test
    fun theCcMonitorLiteralOnlyLivesInBackendBin() {
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
                "合法住处只有 `BackendBin`（部署落点）。",
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

        /**
         * 定值钉：`.cc-monitor` 的合法住处——`BackendBin`（部署落点 `~/.cc-monitor/bin/ccm`，拼后端命令只在那里）。
         */
        private val PINNED_CC_MONITOR_FILES =
            setOf(
                "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/link/OneShot.kt",
            )
    }
}

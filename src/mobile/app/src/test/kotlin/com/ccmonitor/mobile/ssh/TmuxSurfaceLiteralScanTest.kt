package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 手机一条 tmux 命令都不拼：进哪儿 · 建会话 · 找会话 · 附着 · 结束 · 抓屏 · 送字都是那台核心的命令，手机只交意图、原样敲回来的那一行。
 * 所以八个锚点在生产代码的字符串字面量里零命中，不开例外。
 *
 * 扫的是字符串字面量里面，不是整份源码：拼给远端的东西一定在字面量里，标识符与注释不在（按钮显示名 · 菜单文案里的「tmux」在文案表里）。
 * 全仓源码扫描类判据一律住 `app`，理由见 `NamespaceLiteralScanTest` 头注。
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 本文件 |
 * |---|---|
 * | 在任何生产文件里写一条 `"tmux …"` / `"… send-keys …"` 之类 | 红（点名文件 + 计数） |
 * | 用变量装动词再拼（`"${'$'}TMUX send-keys …"`） | `tmux ` 那个锚点抓不到，子命令锚点仍会红 |
 * | `"tm" + "ux "` · `String(charArrayOf(…))` · 反射拼串 | 抓不到：防手滑不防拼接 |
 * | 别的 tmux 子命令（`list-panes` / `respawn-pane` / `run-shell`） | 不在锚点表里 |
 * | `androidTest` / `src/debug` 源集 | 不在扫描面内 |
 */
class TmuxSurfaceLiteralScanTest {
    /** 八个锚点在生产代码字符串字面量里一次都没有；正控：同一条尺子往语料里种一处 `"tmux attach -t x"`，要数得出来。 */
    @Test
    fun noTmuxShellFragmentLivesInProductionCode() {
        val root = repoRoot()
        val sources = productionSources(root)
        assertTrue("前提：得真扫到源文件，否则这条是空跑（实得 ${sources.size} 个）", sources.size > 100)
        val corpus = sources.associate { it.relativeTo(root).invariantSeparatorsPath to it.readText() }
        assertEquals("手机生产代码里拼了 tmux 命令：改成问那台核心（`session-open` · `terminals-list` · `kill` …）", ANCHORS.associateWith { emptyMap<String, Int>() }, census(corpus))
        val planted = census(corpus + (PLANTED to "val c = \"tmux attach -t x\""))
        assertEquals("正控：种的那一处要数得出来，否则这条恒绿", mapOf(PLANTED to 1), planted.getValue(TMUX_VERB))
        assertEquals("正控：种的那一处要数得出来，否则这条恒绿", mapOf(PLANTED to 1), planted.getValue(ATTACH_T))
    }

    /** 每个锚点 ⇒ 命中它的文件 → 次数（不命中的文件不列）。 */
    private fun census(files: Map<String, String>): Map<String, Map<String, Int>> {
        val literalsOf = files.mapValues { (_, text) -> stringLiteralsOf(text) }
        return ANCHORS.associateWith { mark ->
            literalsOf.mapValues { (_, lits) -> countOccurrences(lits, mark) }.filterValues { it > 0 }.toSortedMap().toMap()
        }
    }

    /**
     * 扫描器自检：上面那条的判别力全押在 [stringLiteralsOf] 认得准。
     *
     * 认多了判据会开始误红；认少了（把字面量当代码跳过）判据恒绿。两种都比没有判据更坏，两边都要证。
     */
    @Test
    fun theScannerCountsRealTmuxFragmentsAndNothingElse() {
        val sample =
            "val tmuxName = tmuxSession + \"x\"\n" + // 标识符里的 tmux：不在字面量里 ⇒ 不算
                "// 注释里的 tmux send-keys 与 kill-session 不算\n" +
                "/* 块注释里的 tmux capture-pane 也不算 */\n" +
                "val c = '\"'\n" + // 字符字面量里的引号：不许把它当字符串开头（否则后面全乱套）
                "val b = \"tmux attach -t \" + q\n" // 真字面
        val lits = stringLiteralsOf(sample)
        assertEquals("注释里的 `tmux ` 不算，代码里那一处要算：\n$lits", 1, countOccurrences(lits, TMUX_VERB))
        assertEquals("注释里的 `send-keys ` 不算：\n$lits", 0, countOccurrences(lits, SEND_KEYS))
        assertEquals("注释里的 `kill-session ` 不算：\n$lits", 0, countOccurrences(lits, KILL_SESSION))
        assertEquals("注释里的 `capture-pane` 不算：\n$lits", 0, countOccurrences(lits, CAPTURE_PANE))
        assertEquals("代码里那条 `attach -t` 要算：\n$lits", 1, countOccurrences(lits, ATTACH_T))

        // 反过来也要证：真片段一定认得出，否则「剥得干净」可以靠「什么都不认」作弊。
        assertEquals("裸一行真字面必须认出", 1, countOccurrences(stringLiteralsOf("val x = \"tmux ls\""), TMUX_VERB))
        assertEquals(
            "子命令锚点必须认出（它是「有人用变量装动词」时唯一还看得见的那一格）",
            1,
            countOccurrences(stringLiteralsOf("val x = \"\$T send-keys -t x\""), SEND_KEYS),
        )
        // 插值里嵌套的字面量不许让引号配对错位（本仓的命令串全是这个形状）。
        val nested = "val x = \"tmux send-keys -t \${q(\"\$n\")} \${q(\"\$c\")} Enter\""
        assertEquals("嵌套插值下 `tmux ` 仍恰好一次：\n${stringLiteralsOf(nested)}", 1, countOccurrences(stringLiteralsOf(nested), TMUX_VERB))
        assertEquals("嵌套插值下 `send-keys ` 仍恰好一次", 1, countOccurrences(stringLiteralsOf(nested), SEND_KEYS))
        // 相邻两个字面量之间不许「粘」出一个假命中（用 NUL 隔开）。
        assertEquals("跨字面量拼不出锚点", 0, countOccurrences(stringLiteralsOf("val x = \"tm\" + \"ux \""), TMUX_VERB))
        // 原始字符串（三引号）里的片段也要算：golden 判据自己就用它装期望值。
        assertEquals("三引号里的片段要算", 1, countOccurrences(stringLiteralsOf("val x = \"\"\"tmux ls\"\"\""), TMUX_VERB))
    }

    // ---- 扫描器 ---------------------------------------------------------------

    /**
     * 剥注释（含 Kotlin 的嵌套块注释）并把字符串字面的内容抠出来，用 `NUL` 隔开拼成一份语料，
     * 走全仓唯一一份词法扫描器 [KotlinSourceScanner.literalCorpusOf]。
     *
     * 载荷就是字面内容：本文件数的是「生产代码的字面里出现了几次这个 shell 片段」。
     * 接到 `codeOnlyDroppingLiterals` 上的话语料是空壳、锚点全零命中然后全绿。
     * [theScannerCountsRealTmuxFragmentsAndNothingElse] 正反两向钉着这件事；
     * 它看不见什么写在 [KotlinSourceScanner] 的头注里。
     */
    private fun stringLiteralsOf(text: String): String = KotlinSourceScanner.literalCorpusOf(text)

    /** 数 [mark] 在语料里出现多少次（不重叠）。锚点都是带空格/连字符的 shell 片段，不需要标识符护栏。 */
    private fun countOccurrences(
        text: String,
        mark: String,
    ): Int {
        var n = 0
        var i = 0
        while (true) {
            val at = text.indexOf(mark, i)
            if (at < 0) return n
            n++
            i = at + mark.length
        }
    }

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

        /**
         * 字面量之间的隔断符：必须是不可能出现在锚点里的字符。
         *
         * 注意：不能是 `' '`（空格）。`sessionManager == "tmux"` 这种以「锚点去掉尾空格」结尾的字面量
         * 会与空格粘出一个假命中 `tmux `。写成 `'\u0000'` 转义形，好让人一眼看见。
         */

        private const val TMUX_VERB = "tmux "
        private const val SEND_KEYS = "send-keys "
        private const val CAPTURE_PANE = "capture-pane"
        private const val DISPLAY_MESSAGE = "display-message "
        private const val KILL_SESSION = "kill-session "

        // 找会话 · 附着那几形（手机不拼：列会话问 `terminals-list`，进终端敲 `session-open` 渲出的那一行）。
        private const val ATTACH_T = "attach -t"
        private const val SWITCH_CLIENT = "switch-client"
        private const val LIST_SESSIONS = "ls -F"

        private const val PLANTED = "planted/Seed.kt"

        private val ANCHORS = listOf(TMUX_VERB, SEND_KEYS, CAPTURE_PANE, DISPLAY_MESSAGE, KILL_SESSION, ATTACH_T, SWITCH_CLIENT, LIST_SESSIONS)
    }
}

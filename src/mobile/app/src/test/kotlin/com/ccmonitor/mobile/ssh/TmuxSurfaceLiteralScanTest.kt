package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * tmux 那半的每一个 shell 片段，只许住在网关里。
 *
 * 每个协议面一个网关类：别处只拿结构化结果，不拼字符串；网关之外的文件里不许出现 `"tmux "` 这类拼串。
 * 本文件是 tmux 这一面的机检落点（管道那一面是 `PipeSurfaceLiteralScanTest`）。
 *
 * 白名单里有两类文件：网关 `ssh/TmuxCommands.kt`；以及用户内容（`ui/session/ButtonBuiltins.kt` ·
 * `ui/session/SessionScreen.kt` · `ui/session/TmuxManager.kt` · `core-data/.../DefaultButtons.kt` · `dev/DevSeeder.kt`）。
 * 后者里面的 `tmux` 字样是按钮的显示名 / 菜单项文案 / 种子按钮的默认命令，用户可编辑；把它们搬进网关
 * 等于把用户的字锁进协议层。一个按钮的标题不和远端说话，它经用户编辑后才变成一条命令，那时它是用户的命令。
 * 白名单给它们逐文件钉了次数：哪天某个 UI 文件里冒出第二处 `tmux …`，这条照样红一次
 * （重新过一遍眼：那到底是新文案，还是有人在 UI 层拼了条命令）。
 *
 * 扫的是字符串字面量里面，不是整份源码（同 `PipeSurfaceLiteralScanTest`）：拼给远端的东西一定在字面量里，
 * 标识符与注释不在。全仓源码扫描类判据一律住 `app`，理由见 `NamespaceLiteralScanTest` 头注。
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 本文件 |
 * |---|---|
 * | 在网关之外新写一条 `"tmux send-keys …"` | 红（点名文件 + 计数） |
 * | 把网关里某一支的片段删掉/挪走 | 红（次数少了也不等） |
 * | 用变量装动词再拼（`"${'$'}TMUX send-keys …"`） | `tmux ` 那个锚点抓不到，但 [SEND_KEYS] / [CAPTURE_PANE] / [DISPLAY_MESSAGE] / [KILL_SESSION] 四个子命令锚点仍会红：这正是它们与 `tmux ` 并列的理由 |
 * | `"tm" + "ux "` · `String(charArrayOf(…))` · 反射拼串 | 抓不到：防手滑不防拼接 |
 * | 别的 tmux 子命令（`list-panes` / `respawn-pane` / `run-shell`） | 不在锚点表里：锚点是真在用的那几个 |
 * | 命令串内容对不对 | 本文件不答，那是 `TmuxGatewayGoldenTest` 的逐字节对拍 |
 * | 谁在调网关、有没有人绕开 | 本文件不答，`TmuxGatewayCallSiteTest` 答 |
 * | `androidTest` / `src/debug` 源集 | 不在扫描面内 |
 */
class TmuxSurfaceLiteralScanTest {
    /**
     * 五个锚点各自的 file → 次数逐字钉死。
     *
     * 钉的是次数不只是文件集 ⇒ 同一个文件里再长出一处也红。
     */
    @Test
    fun everyTmuxShellFragmentLivesOnlyInTheGatewayOrUserContent() {
        val root = repoRoot()
        val sources = productionSources(root)
        assertTrue("前提：得真扫到源文件，否则这条是空跑（实得 ${sources.size} 个）", sources.size > 150)

        val literalsOf = sources.associateWith { stringLiteralsOf(it.readText()) }
        val census = linkedMapOf<String, Map<String, Int>>()
        for (mark in PINNED_FRAGMENT_COUNTS.keys) {
            val counts = sortedMapOf<String, Int>()
            for ((f, lits) in literalsOf) {
                val n = countOccurrences(lits, mark)
                if (n > 0) counts[f.relativeTo(root).invariantSeparatorsPath] = n
            }
            assertTrue("前提：锚点 `$mark` 整份语料一次都没命中 ⇒ 扫描器瞎了、这条判据恒绿", counts.values.sum() > 0)
            census[mark] = counts.toMap()
        }
        assertEquals(
            "tmux 面的 shell 片段只许由网关 `TmuxCommands` 产出。" +
                "多出来的请改成调网关；确属用户内容（按钮显示名 / 默认命令 / 菜单文案）的，" +
                "连同理由加进 PINNED_FRAGMENT_COUNTS。",
            PINNED_FRAGMENT_COUNTS.toMap(),
            census.toMap(),
        )
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

        private const val GATEWAY = "app/src/main/kotlin/com/ccmonitor/mobile/ssh/TmuxCommands.kt"

        // ---- 用户内容（不是协议面，逐条写着这是谁的字）----
        private const val SINK_COPY = "app/src/main/kotlin/com/ccmonitor/mobile/ssh/TmuxSendKeysSink.kt"
        private const val SEEDER = "app/src/main/kotlin/com/ccmonitor/mobile/dev/DevSeeder.kt"
        private const val BUILTIN_LABELS = "app/src/main/kotlin/com/ccmonitor/mobile/ui/session/ButtonBuiltins.kt"
        private const val SESSION_SCREEN = "app/src/main/kotlin/com/ccmonitor/mobile/ui/session/SessionScreen.kt"
        private const val TMUX_DIALOG = "app/src/main/kotlin/com/ccmonitor/mobile/ui/session/TmuxManager.kt"
        private const val SEED_BUTTONS = "core-data/src/main/kotlin/com/ccmonitor/mobile/core/data/db/DefaultButtons.kt"

        /**
         * 定值钉：五个锚点在生产代码字符串字面量里的 file → 次数。
         *
         * | 文件 | 次 | 那是什么 |
         * |---|---|---|
         * | `TmuxCommands.kt` | 27/4/2/4/1 | 网关本身 |
         * | `TmuxSendKeysSink.kt` | 3 | 给用户看的三句话：「没有可投递的 tmux 会话」·「这个 tmux 会话不归我们管」·「tmux 没给任何回应…」。不是命令，是 [SendOutcome] 的文案 |
         * | `DevSeeder.kt` | 1 | 种子按钮的默认命令 `CustomButton("btn-tmux", null, "tmux", "tmux new -A -s main")`：用户进按钮编辑就能改掉，是用户的字 |
         * | `ButtonBuiltins.kt` | 1 | 内建动作的显示名 `"tmux_sessions" to "tmux 会话"` |
         * | `SessionScreen.kt` | 2 | Toast「无 tmux 会话可抓屏」+ 命令面板项「tmux 会话管理」 |
         * | `TmuxManager.kt` | 2 | 弹窗标题「tmux 会话」+ 空态「无 tmux 会话。下方新建一个。」 |
         * | `DefaultButtons.kt` | 1 | 种子按钮的显示名 `SeedButton("ak-tb-tmux", "tmux 会话", …)` |
         *
         * 四个子命令锚点在网关之外一次都没有（这一格才是真正的「零拼串」读数）：
         * 用户内容那七处全是 `tmux` + 空格 + 中文，一个 tmux 子命令都不带，
         * 唯一的例外是 `DevSeeder` 那条种子命令（`new -A`，不在这四个锚点里）。
         */
        private val PINNED_FRAGMENT_COUNTS =
            linkedMapOf(
                TMUX_VERB to
                    mapOf(
                        SEEDER to 1,
                        GATEWAY to 27,
                        SINK_COPY to 3,
                        BUILTIN_LABELS to 1,
                        SESSION_SCREEN to 2,
                        TMUX_DIALOG to 2,
                        SEED_BUTTONS to 1,
                    ),
                SEND_KEYS to mapOf(GATEWAY to 4),
                CAPTURE_PANE to mapOf(GATEWAY to 2),
                DISPLAY_MESSAGE to mapOf(GATEWAY to 4),
                KILL_SESSION to mapOf(GATEWAY to 1),
            )
    }
}

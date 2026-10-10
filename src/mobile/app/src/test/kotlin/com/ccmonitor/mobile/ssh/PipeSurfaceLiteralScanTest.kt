package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 管道那半的每一个 shell 片段，只许住在网关里。
 *
 * 每个协议面一个网关类：别处只拿结构化结果，不拼字符串。本文件是文件管道这一面的机检落点：
 * 把管道那几个动词按字面扫全仓生产代码，file → 次数逐字钉死。有人在网关之外新拼一条
 * `mkdir -p …` / `printf '%s\n' … >> …`，这条当场红并点名文件。
 *
 * 扫的是字符串字面量里面，不是整份源码：`>>` 在 Kotlin 里满地都是（`Flow<List<Host>>` 的泛型收尾），
 * 按整份源码数的话这张表会长成二十多行纯噪声。拼给远端的东西一定在字面量里，泛型不在。
 *
 * 全仓源码扫描类判据一律住 `app`：`:app:testDebugUnitTest` 的运行时 classpath 含全部 `core-…` 模块的产物，
 * 任何模块的代码改动都会让它重跑（同 `NamespaceLiteralScanTest` 头注那一节）。
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 本文件 |
 * |---|---|
 * | 在网关之外新写一条 `"mkdir -p …"` / `"… >> …"` | 红（点名文件 + 计数） |
 * | 把网关里某一支的片段删掉/挪走 | 红（次数少了也不等） |
 * | `"mkdir" + " -p "` · `String(charArrayOf(…))` · 反射拼串 | 抓不到：防手滑不防拼接 |
 * | 用别的动词写远端（`cat > x`、`tee`、`dd`） | 不在锚点表里：锚点是管道那半真在用的那几个 |
 * | 命令串内容对不对 | 本文件不答，那是 `PipeGatewayGoldenTest` 的逐字节对拍 |
 * | `androidTest` / `src/debug` 源集 | 不在扫描面内 |
 *
 * 白名单里不是管道网关的文件属于别的协议面：`TmuxCommands.kt` 是 tmux 面的网关（`TmuxSurfaceLiteralScanTest` 管它），
 * `RemoteCommands.kt` 那两处是推公钥那条一次性安装命令。它们哪天变了，这张表也会红一次，那正是要的（重新过一遍眼）。
 */
class PipeSurfaceLiteralScanTest {
    /**
     * 五个锚点各自的 file → 次数逐字钉死。
     *
     * 钉的是次数不只是文件集 ⇒ 同一个文件里再长出一处也红。
     */
    @Test
    fun everyPipeShellFragmentLivesOnlyInTheGateway() {
        val root = repoRoot()
        val sources = productionSources(root)
        assertTrue("前提：得真扫到源文件，否则这条是空跑（实得 ${sources.size} 个）", sources.size > 150)

        val literalsOf = sources.associateWith { stringLiteralsOf(it.readText()) }
        for ((mark, pinned) in PINNED_FRAGMENT_COUNTS) {
            val counts = sortedMapOf<String, Int>()
            for ((f, lits) in literalsOf) {
                val n = countOccurrences(lits, mark)
                if (n > 0) counts[f.relativeTo(root).invariantSeparatorsPath] = n
            }
            assertTrue("前提：锚点 `$mark` 整份语料一次都没命中 ⇒ 扫描器瞎了、这条判据恒绿", counts.values.sum() > 0)
            assertEquals(
                "管道面的 shell 片段 `$mark` 只许由网关 `PipeCommands` 产出。" +
                    "多出来的请改成调网关；确属别的协议面的，连同理由加进 PINNED_FRAGMENT_COUNTS。",
                pinned,
                counts.toMap(),
            )
        }
    }

    /**
     * 扫描器自检：上面那条的判别力全押在 [stringLiteralsOf] 认得准。
     *
     * 认多了（把泛型 `>>` 算进来）判据会开始误红；认少了（把字面量当代码跳过）判据恒绿。
     * 两种都比没有判据更坏，所以两边都要证。
     */
    @Test
    fun theScannerCountsRealShellFragmentsAndNothingElse() {
        val sample =
            "val a: Flow<List<Pair<String, Int>>> = x\n" + // 泛型收尾：`>> ` 但不在字面量里 ⇒ 不算
                "// 注释里的 mkdir -p 与 printf '%s 不算\n" +
                "/* 块注释里的 tail -n 0 -f 也不算 */\n" +
                "val c = '\"'\n" + // 字符字面量里的引号：不许把它当字符串开头（否则后面全乱套）
                "val b = \"mkdir -p \" + q\n" // 真字面
        val lits = stringLiteralsOf(sample)
        assertEquals("泛型的 `>> ` 一个都不许算进来：\n$lits", 0, countOccurrences(lits, REDIRECT_APPEND))
        assertEquals("注释里的 `mkdir -p ` 不算，代码里那一处要算：\n$lits", 1, countOccurrences(lits, MKDIR))
        assertEquals("注释里的 `printf '%s` 不算：\n$lits", 0, countOccurrences(lits, PRINTF_S))
        assertEquals("注释里的 `tail -n 0 -f` 不算：\n$lits", 0, countOccurrences(lits, TAIL_FOLLOW))

        // 反过来也要证：真片段一定认得出，否则「剥得干净」可以靠「什么都不认」作弊。
        assertEquals("裸一行真字面必须认出", 1, countOccurrences(stringLiteralsOf("val x = \"a >> b\""), REDIRECT_APPEND))
        // 插值里嵌套的字面量不许让引号配对错位（本仓的命令串全是这个形状）。
        val nested = "val x = \"mkdir -p \${q(\"\$d/\$f\")} && touch \${q(\"\$d/\$f\")} && \""
        assertEquals("嵌套插值下 `mkdir -p ` 仍恰好一次：\n${stringLiteralsOf(nested)}", 1, countOccurrences(stringLiteralsOf(nested), MKDIR))
        assertEquals("嵌套插值下 `touch ` 仍恰好一次", 1, countOccurrences(stringLiteralsOf(nested), TOUCH))
        // 相邻两个字面量之间不许「粘」出一个假命中（用 NUL 隔开）。
        assertEquals("跨字面量拼不出锚点", 0, countOccurrences(stringLiteralsOf("val x = \"mkdir\" + \" -p \""), MKDIR))
        // 原始字符串（三引号）里的片段也要算：判据文件自己就用它装 golden。
        assertEquals("三引号里的片段要算", 1, countOccurrences(stringLiteralsOf("val x = \"\"\"a >> b\"\"\""), REDIRECT_APPEND))
    }

    // ---- 扫描器 ---------------------------------------------------------------

    /**
     * 剥注释（含 Kotlin 的嵌套块注释）并把字符串字面的内容抠出来，用 `NUL` 隔开拼成一份语料，
     * 走全仓唯一一份词法扫描器 [KotlinSourceScanner.literalCorpusOf]。
     *
     * 载荷就是字面内容：本文件数的是「生产代码的字面里出现了几次这个 shell 片段」。
     * 接到 `codeOnlyDroppingLiterals` 上的话语料是空壳、锚点全零命中然后全绿。
     * [theScannerCountsRealShellFragmentsAndNothingElse] 正反两向钉着这件事；
     * 它看不见什么写在 [KotlinSourceScanner] 的头注里。
     */
    private fun stringLiteralsOf(text: String): String = KotlinSourceScanner.literalCorpusOf(text)

    /** 数 [mark] 在语料里出现多少次（不重叠）。锚点都是带空格的 shell 片段，不需要标识符护栏。 */
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
        private const val SEPARATOR = ' '

        private const val MKDIR = "mkdir -p "
        private const val TOUCH = "touch "
        private const val TAIL_FOLLOW = "tail -n 0 -f"
        private const val PRINTF_S = "printf '%s"
        private const val REDIRECT_APPEND = ">> "

        private const val GATEWAY = "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/command/PipeCommands.kt"
        private const val SSH_KEYS = "app/src/main/kotlin/com/ccmonitor/mobile/ssh/RemoteCommands.kt"

        /**
         * tmux 面的网关：身份门那段 shell 里的 `printf '%s' "$info" | cut -f2` 与 `NO_TMUX` 分支，
         * 串由 `TmuxGatewayGoldenTest` 逐字节钉着。
         */
        private const val TMUX_GATEWAY = "app/src/main/kotlin/com/ccmonitor/mobile/ssh/TmuxCommands.kt"

        /**
         * 定值钉：五个锚点在生产代码字符串字面量里的 file → 次数。
         *
         * | 文件 | 为什么它可以有 |
         * |---|---|
         * | `PipeCommands.kt` | 管道面的网关本身 |
         * | `RemoteCommands.kt` | 推公钥那条一次性安装命令（`mkdir -p ~/.ssh` … `printf '%s\n' … >> authorized_keys`）：另一个协议面 |
         * | `TmuxCommands.kt` | tmux 面的网关（`printf '%s' "$info" \| cut`、`NO_TMUX` 分支） |
         */
        private val PINNED_FRAGMENT_COUNTS =
            linkedMapOf(
                MKDIR to mapOf(GATEWAY to 2, SSH_KEYS to 1),
                TOUCH to mapOf(GATEWAY to 1, SSH_KEYS to 1),
                TAIL_FOLLOW to mapOf(GATEWAY to 1),
                PRINTF_S to mapOf(GATEWAY to 3, SSH_KEYS to 1, TMUX_GATEWAY to 2),
                REDIRECT_APPEND to mapOf(GATEWAY to 4, SSH_KEYS to 1),
            )
    }
}

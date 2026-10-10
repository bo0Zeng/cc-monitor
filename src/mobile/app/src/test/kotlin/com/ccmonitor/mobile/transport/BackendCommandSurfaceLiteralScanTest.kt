package com.ccmonitor.mobile.transport

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 拼后端命令这件事，只许在点名的那几个文件里发生。
 *
 * 后端命令只有一个产地：`link/OneShot.kt` 的 `BackendBin.command`（路径 `"$HOME"/.cc-monitor/bin/ccm` ＋ `--` 门 ＋
 * 常量词）。子命令名作为常量住在用它的那一处（门槛 `--backend-probe` · 起常驻 `--resident-ensure` · 接流
 * `--resident-attach`）；接上之后一切都走常驻流上的一问一答，不再拼命令行。
 *
 * 写错叫法不会报错。后端 `route()` 的充要条件是 `argv[1] == "--"` 且 `argv[2]` 是后端词，没有 fallback 分支：
 * 没有 `--` ⇒ 整行交给 claude；零参数 ⇒ 默默起一个 claude 进程。一条绕过网关的命令会在用户机器上起一个 claude。
 *
 * ### 三个网关是并列的，别把另外两个的串当违规
 *
 * | 网关 | 面 | 走 `--` 门？ |
 * |---|---|---|
 * | `core-claude/.../link/OneShot.kt`（`BackendBin`） | cc-monitor 后端 | 走，本文件守的就是它 |
 * | `app/.../ssh/TmuxCommands` | tmux | 合法地不走（`TmuxSurfaceLiteralScanTest` 守） |
 * | `core-claude/.../command/PipeCommands` | 我们自己的管道 | 合法地不走（`PipeSurfaceLiteralScanTest` 守） |
 *
 * 所以锚点表里一个 tmux / 管道 / claude CLI 的词都没有，全是只属于后端的子命令字面。
 *
 * 剥注释 + 抠字面走 [KotlinSourceScanner.literalCorpusOf]。必须是这一支：本文件的锚点就是字面内容
 * （后端子命令只可能写在字面里）；接到 `codeOnlyDroppingLiterals` 上会让锚点全零命中然后全绿。
 *
 * 全仓源码扫描类判据一律住 `app`：`:app:testDebugUnitTest` 的运行时 classpath 含全部 core 模块产物，
 * 任何模块的代码改动都会让它重跑。
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 本文件 |
 * |---|---|
 * | 在网关外写一条 `"'${'$'}p' -- --list-projects"` | 红（锚点命中 + 文件不在白名单） |
 * | 在网关外写一条 `"'${'$'}p' --list-projects"`（忘了门） | 红（字面在那儿，门在不在都红） |
 * | 在网关外写一条 `"'${'$'}p' -- --list-subagents"`（发明一个新子命令） | 红，但红的是 [theArgvGateNextToABackendWordNeverAppearsInAnInlinedLiteral] 那条（`-- --` 这个形） |
 * | 发明一个新子命令且忘了门（`"'${'$'}p' --list-subagents"`） | 抓不到：词表里没有它、`-- --` 那个形也不在。这是本文件最大的盲区 |
 * | 把锚点拆开拼（`"--list" + "-projects"` · `String(charArrayOf(…))` · 反射 · 按名拼接） | 抓不到：防手滑不防拼接 |
 * | 锚点藏在 `${'$'}{…}` 插值里（跨字面量边界） | 抓不到：扫描器在插值里的内层引号处断开 |
 * | 白名单文件里命令串内容对不对 | 本文件不答，那是 `OneShotAndGateTest` 的事 |
 * | 调用方存在但走的是死分支 | 源码扫描只答「有没有这个字面」 |
 * | `src/test` / `androidTest` 源集 | 不在扫描面内：测试里拿子命令当 fixture 是正常的 |
 * | 非 Kotlin（`scripts/` · `bridge/`） | 不扫：`bridge/vectors` 下那几份 `.ndjson` 是录制的 fixture，不是命令构造 |
 */
class BackendCommandSurfaceLiteralScanTest {
    /**
     * 每个后端子命令锚点的 file → 次数逐字钉死。
     *
     * 钉的是次数不只是文件集：同一个白名单文件里再长出一处也红（重新过一遍眼：
     * 那到底是网关新接的一条，还是有人在那儿内联拼了一条）。
     *
     * 三道前提自检：
     * 1. 扫描面非空且够大：路径写错 ⇒ 零文件 ⇒ 恒绿；
     * 2. 被守的网关源文件真的在扫描面里；
     * 3. 一个活着的锚点必须真扫到，否则「零命中」只说明扫描器瞎了。
     */
    @Test
    fun everyBackendSubcommandLiteralLivesOnlyInTheNamedFiles() {
        val root = repoRoot()
        val sources = scannedSources(root)
        assertTrue("前提①：得真扫到源文件，否则本条恒绿（实得 ${sources.size} 个）", sources.size > 150)

        val literalsOf = sources.associate { it.relativeTo(root).invariantSeparatorsPath to backendLiteralsOf(it.readText()) }
        assertTrue(
            "前提②：被守的网关 `$GATEWAY` 不在扫描面里：这条判据正在扫一片无关的代码",
            GATEWAY in literalsOf.keys,
        )
        for (f in PINNED_BACKEND_LITERALS.values.flatMap { it.keys }.toSet()) {
            assertTrue("白名单点名的文件不存在了：$f（它被删了/改名了？那白名单要跟着改）", f in literalsOf.keys)
        }

        val census = linkedMapOf<String, Map<String, Int>>()
        for (anchor in PINNED_BACKEND_LITERALS.keys) {
            val counts = sortedMapOf<String, Int>()
            for ((f, lits) in literalsOf) {
                val n = countOccurrences(lits, anchor)
                if (n > 0) counts[f] = n
            }
            census[anchor] = counts.toMap()
        }
        assertTrue(
            "前提③：活着的锚点 `$LIVE_ANCHOR` 整份语料一次都没命中 ⇒ 扫描器瞎了 ⇒ 下面那条恒绿",
            (census[LIVE_ANCHOR]?.values?.sum() ?: 0) > 0,
        )

        assertEquals(
            "拼后端命令这件事只许在点名的那几个文件里发生（任何拼命令串的地方都必须经网关）。" +
                "多出来的一处：要么改成经 `BackendBin.command`（或常驻流上的一问一答），" +
                "要么连理由一起加进 PINNED_BACKEND_LITERALS。tmux / 管道 / claude CLI 的串不归这里管，" +
                "它们各有自己的网关与判据。",
            PINNED_BACKEND_LITERALS.toMap(),
            census.toMap(),
        )
    }

    /**
     * 「那道门紧挨着一个后端词」这个形，在生产字面量里一次都不许出现。
     *
     * 锚点表是枚举：有人发明一个表里没有的子命令（`--list-subagents`），锚点表看不见。
     * 但只要他还记得带那道门，串里就会出现 `-- --` 这个形，本条认的就是它。
     *
     * 它在网关里也是零命中：`BackendBin.command` 的门是拼出来的（`"$PATH_WORD -- ${words…}"`），
     * 不是写成一个 `"-- --resident-attach"` 字面。
     * 零命中 + 零阳性对照 = 恒绿，所以本条自带阳性对照：先喂一份合成样本证明扫描器认得出这个形，
     * 再断言生产段为空。
     */
    @Test
    fun theArgvGateNextToABackendWordNeverAppearsInAnInlinedLiteral() {
        // 阳性对照在先：扫描器认不出这个形 ⇒ 下面那条恒绿。
        //   喂的是「有人内联拼一条、而且记得带门」最可能写出来的样子。
        val planted = "val cmd = shellQuote(p) + \" -- --list-subagents\""
        assertEquals(
            "阳性对照：扫描器认不出 `$GATE_NEXT_TO_WORD` 这个形 ⇒ 本条判据是空的",
            1,
            countOccurrences(backendLiteralsOf(planted), GATE_NEXT_TO_WORD),
        )
        // 阴性对照：POSIX 的「选项到此为止」惯用法（`grep … -- "$f"` / `stat -c … -- …`）
        //   在生产代码里有好几处，它们与后端那道门毫无关系，一处都不许被误判。
        assertEquals(
            "阴性对照：POSIX 的 `-- ` 惯用法被误判成后端门 ⇒ 本条会开始误红、然后被人删掉",
            0,
            countOccurrences(backendLiteralsOf("val x = \"grep -a -m1 -o 'x' -- \\\"\$f\\\" 2>/dev/null\""), GATE_NEXT_TO_WORD),
        )

        val root = repoRoot()
        val sources = scannedSources(root)
        assertTrue("前提：得真扫到源文件（实得 ${sources.size} 个）", sources.size > 150)
        val hits =
            sources
                .filter { countOccurrences(backendLiteralsOf(it.readText()), GATE_NEXT_TO_WORD) > 0 }
                .map { it.relativeTo(root).invariantSeparatorsPath }
                .sorted()
        assertEquals(
            "有人把「门 + 后端词」内联写进了一条字面：后端命令只许由 `BackendBin.command` 产出。" +
                "它是拼出来的（append 门、append 后端词），所以生产段这个形恒为零；" +
                "冒出来一处就说明有第二个产地。",
            emptyList<String>(),
            hits,
        )
    }

    /**
     * 扫描器自检：上面两条的判别力全押在 [backendLiteralsOf] 认得准。
     *
     * 认多了判据会开始误红；认少了（把字面量当注释跳过）判据恒绿。两种都比没有判据更坏，两边都要证。
     * 特意钉住正则版 `codeOnly` 栽过的那一格：字符串字面里的「星号-斜杠」不许把后面的代码当注释吃掉。
     */
    @Test
    fun theScannerReadsLiteralsAndSkipsCommentsBothWays() {
        // ① 正向：真字面必须认出来（单引号串 / 三引号串都要）。
        assertEquals("普通字面里的锚点必须认出", 1, countOccurrences(backendLiteralsOf("val x = \"--resident-attach\""), LIVE_ANCHOR))
        assertEquals(
            "三引号字面里的锚点也必须认出（golden / 桩数据常用这个形）",
            1,
            countOccurrences(backendLiteralsOf("val x = \"\"\"--resident-attach\"\"\""), LIVE_ANCHOR),
        )

        // ② 反向：注释里的引文是资料不是调用，一个都不许算。
        val commented =
            "// 行注释里的 --resident-attach 不算\n" +
                "/* 块注释里的 --resident-attach 也不算 */\n" +
                "/* 嵌套 /* 再来一层 --resident-attach */ 仍在注释里 --resident-attach */\n" +
                "val a = 1\n"
        assertEquals("注释（含嵌套块注释）里的锚点一个都不许算：${backendLiteralsOf(commented)}", 0, countOccurrences(backendLiteralsOf(commented), LIVE_ANCHOR))

        // ③ 正则版 codeOnly 栽的那一格：字面里的「星号-斜杠」不许吃掉后面的代码。
        val starSlashTrap = "val a = \"*/\"\nval b = \"--resident-attach\"\n"
        assertEquals(
            "字面里的「星号-斜杠」把后面的代码当注释吃掉了：这正是正则版 codeOnly 的那个坑",
            1,
            countOccurrences(backendLiteralsOf(starSlashTrap), LIVE_ANCHOR),
        )

        // ④ 字符字面量里的引号不许把后面全带偏（否则整份语料错位、判据恒绿）。
        val charLiteral = "val q = '\"'\nval b = \"--resident-attach\"\n"
        assertEquals("字符字面量里的引号不许让后面错位", 1, countOccurrences(backendLiteralsOf(charLiteral), LIVE_ANCHOR))

        // ⑤ 相邻两个字面量之间不许「粘」出一个假命中（隔断符必须是锚点里不可能有的字符）。
        assertEquals(
            "跨字面量拼不出锚点（隔断符失效 ⇒ 一片假阳）",
            0,
            countOccurrences(backendLiteralsOf("val x = \"--list\" + \"-projects\""), LIVE_ANCHOR),
        )
    }

    // ---- 扫描器 ---------------------------------------------------------------

    /**
     * 剥注释（含 Kotlin 的嵌套块注释）并把字符串字面的内容抠出来，用 `NUL` 隔开拼成一份语料，
     * 走全仓唯一一份词法扫描器 [KotlinSourceScanner.literalCorpusOf]。
     *
     * 必须是「留字面」那一族：本文件的锚点就是字面内容。
     * [theScannerReadsLiteralsAndSkipsCommentsBothWays] 正反两向钉着它认得准；
     * 它看不见什么写在 [KotlinSourceScanner] 的头注里。
     */
    private fun backendLiteralsOf(text: String): String = KotlinSourceScanner.literalCorpusOf(text)

    /** 数 [mark] 在语料里出现多少次（不重叠）。锚点都带 `--` 前缀，不需要标识符护栏。 */
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
     * 扫描面：`app` ＋ 每个 `core-…` 模块的 `src/main/kotlin` 与 `src/debug/kotlin` 下的全部 `.kt`。
     *
     * 比 `TmuxSurfaceLiteralScanTest` 多收了 `src/debug`，是有意的：「临时在 debug 里内联拼一条真命令」
     * 恰恰是这类代码最可能长出来的地方。
     *
     * `src/test` / `androidTest` 不在扫描面内：测试里拿子命令当 fixture 是正常的。
     */
    private fun scannedSources(root: File): List<File> {
        val modules = listOf(File(root, "app")) + (root.listFiles()?.filter { it.isDirectory && it.name.startsWith("core-") } ?: emptyList())
        val roots =
            modules
                .flatMap { m -> SOURCE_SETS.map { File(m, it) } }
                .filter { it.isDirectory }
                .sortedBy { it.path }
        assertTrue("前提：得找到扫描面（app + core-* 的 main/debug 源集），实得 ${roots.size} 个源根", roots.size >= 2)
        return roots.flatMap { r -> r.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList() }
    }

    private companion object {
        private const val MAX_WALK_UP = 6

        private val SOURCE_SETS = listOf("src/main/kotlin", "src/debug/kotlin")

        private const val GATEWAY = "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/link/OneShot.kt"
        private const val GATE = "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/link/BackendGate.kt"
        private const val RESIDENT = "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/link/ResidentLink.kt"

        /**
         * 扫描器自检的靶子：今天还活着、而且真在生产代码里的那个子命令。
         *
         * 换锚点表时这一条必须跟着换成一个仍有正向命中的 —— 它是「扫描器没瞎」的唯一证据。
         */
        private const val LIVE_ANCHOR = "--resident-attach"

        /**
         * 「那道门紧挨着一个后端词」这个形。
         *
         * 它不是词表里的一项（词表是枚举、补不全），而是结构：
         * 对面 `route()` 要的就是 `argv[1] == "--"` 且 `argv[2]` 是个后端词，
         * 而后端词全部以 `--` 开头 ⇒ 任何内联拼出来的合规后端命令串里必然有这五个字符。
         */
        private const val GATE_NEXT_TO_WORD = "-- --"

        /**
         * 后端子命令锚点的 file → 次数定值钉。
         *
         * | 锚点 | 出处 |
         * |---|---|
         * | `--backend-probe` · `--resident-ensure` · `--resident-attach` | 接常驻流之前那几步（`BackendGate` · `ResidentLink`） |
         * | `--stream` · `--list-projects` · `--list-sessions` · `--read-session`（前缀）· `--resolve` · `--daemon-probe` · `--fork-session` · `--search` · `--list-accounts` | 后端还认、手机不发的那几条：必须是零 |
         *
         * 零命中的锚点也留在表里：将来有人要接的时候，这张表就是那一刻的拦截点。
         * （它们不构成恒绿：活着的 [LIVE_ANCHOR] 的正向命中是前提③。）
         */
        private val PINNED_BACKEND_LITERALS: LinkedHashMap<String, Map<String, Int>> =
            linkedMapOf(
                // ── 接常驻流（R）之前那几步：门槛 · 起常驻 · 接流（命令经 `BackendBin.command` 拼） ──
                "--backend-probe" to mapOf(GATE to 1),
                "--resident-ensure" to mapOf(RESIDENT to 1),
                "--resident-attach" to mapOf(RESIDENT to 1),
                // ── 后端还认、我们不发的那几条：必须保持零 ──
                "--stream" to emptyMap(),
                "--list-projects" to emptyMap(),
                "--list-sessions" to emptyMap(),
                "--read-session" to emptyMap(),
                "--resolve" to emptyMap(),
                "--daemon-probe" to emptyMap(),
                "--fork-session" to emptyMap(),
                "--search" to emptyMap(),
                "--list-accounts" to emptyMap(),
            )
    }
}

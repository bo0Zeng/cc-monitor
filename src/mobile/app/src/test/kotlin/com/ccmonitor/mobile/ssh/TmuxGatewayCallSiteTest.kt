package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * tmux 面的每条远端命令串都由那一个网关产出，而且它每一支都真的接上了。
 *
 * | # | 问什么 | 不问会怎样 |
 * |---|---|---|
 * | ① | 网关公开的每一支对外函数，在生产代码里有至少一个调用方，且住址逐字钉死 | 「已 build 零引用」：网关写好了、没人用、别的路照跑 |
 * | ② | 网关里的函数一支不多一支不少（新增一支必须在这张表里表态） | 新长出来的函数会悄悄绕开①，表看着还全绿 |
 * | ③ | 焊在命令串里的哨兵常量，读它的人住哪儿逐字钉死 | 「命令改了、解析没跟着改」：静默失败 |
 *
 * ①是「谁在用」，`TmuxSurfaceLiteralScanTest` 是「还有谁在自己拼」，`TmuxGatewayGoldenTest` 是「串本身没变」。
 * 三条互不替代：只有①时别处可以照拼；只有②时网关可以是死代码；只有③时可以有第二个产地各自正确。
 *
 * 全仓源码扫描类判据一律住 `app`：`:app:testDebugUnitTest` 的运行时 classpath 含全部 core 模块的产物，
 * 任何模块的代码改动都会让它重跑（被扫的消费方遍布 `app` 与 `core-*`）。
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 本文件 |
 * |---|---|
 * | 网关新增一支函数 | 红（②那条集合不等） |
 * | 某支函数失去全部生产调用方 | 红（①那条文件集不等，且要求非空） |
 * | 有人在网关外重新手抄一份哨兵字面（如 `"ATERM_SENT"`） | 红（③那条文件集不等） |
 * | 调用方存在但走的是死分支 | 抓不到：源码扫描只答「有没有引用」 |
 * | 反射 / 别名 / 通过局部 `val f = TmuxCommands::tmuxKillCommand` 转一手 | 抓不到（防手滑不防拼接） |
 * | 消费方成员导入（`import ….TmuxCommands.tmuxKillCommand`）后裸调 | 仍算命中：import 那一行里就有 `TmuxCommands.<名>`。①问的是「这个文件有没有经网关」，不是「每个调用点都写全限定名」 |
 */
class TmuxGatewayCallSiteTest {
    /** ①：每支对外函数的生产调用方住址定值钉，且非空。 */
    @Test
    fun everyOutwardGatewayFunctionHasPinnedProductionCallers() {
        val root = repoRoot()
        val sources = productionSources(root).filter { it.relativeTo(root).invariantSeparatorsPath != GATEWAY }
        assertTrue("前提：得真扫到源文件（且已排除网关自己），实得 ${sources.size} 个", sources.size > 150)
        val codeOf = sources.associate { it.relativeTo(root).invariantSeparatorsPath to codeOnly(it.readText()) }

        for ((fn, pinned) in PINNED_OUTWARD_CALLERS) {
            val found = codeOf.filterValues { it.contains("$OBJECT.$fn") }.keys.toSortedSet()
            assertTrue(
                "`$OBJECT.$fn` 零生产调用方：网关函数要么接上、要么删掉。",
                found.isNotEmpty(),
            )
            assertEquals(
                "`$OBJECT.$fn` 的生产调用方住址变了。多出来的：确认它该经网关（那就把住址加进表）；" +
                    "少掉的：那一路是不是又自己拼串去了（`TmuxSurfaceLiteralScanTest` 会从另一头红）。",
                pinned,
                found.toSet(),
            )
        }
    }

    /**
     * ②：网关里的函数一支不多一支不少。
     *
     * 新增一支就必须在 [PINNED_OUTWARD_CALLERS]（对外）或 [GATEWAY_INTERNAL_ONLY]（只网关自己用）
     * 里表态一次：那一次表态就是「它有没有人用」这个问题被真正问过的证据。
     */
    @Test
    fun theGatewayDeclaresExactlyThePinnedFunctions() {
        val src = File(repoRoot(), GATEWAY)
        assertTrue("被守的网关源文件不在：${src.absolutePath}", src.isFile)
        val declared =
            FUN_DECL_RE
                .findAll(codeOnly(src.readText()))
                .map { it.groupValues[1] }
                .toSortedSet()
        assertTrue("前提：一支函数都没抠出来 ⇒ 抠取器瞎了、这条恒绿", declared.isNotEmpty())
        assertEquals(
            "网关的函数集变了。新增的那一支要么写进 PINNED_OUTWARD_CALLERS（并点名谁在用），" +
                "要么写进 GATEWAY_INTERNAL_ONLY（并说清为什么只有网关自己用）。",
            (PINNED_OUTWARD_CALLERS.keys + GATEWAY_INTERNAL_ONLY).toSortedSet(),
            declared,
        )
    }

    /**
     * ③：哨兵常量与读它的人同源：命令那头一改，解析这头当场跟着变。
     *
     * 这几个常量是焊在命令串里的（`printf '__aterm_started__\n'` / `&& echo __aterm_kill_ok__` /
     * `printf 'ATERM_SENT\n'`）。读方若在别的文件里比对手抄的同一个串，命令一改就会把失败报成成功
     * （kill 失败报成成功、发送失败判成送达）。本条钉住「读方只许问网关要这个串」。
     */
    @Test
    fun everyCommandSentinelIsReadOnlyFromTheGateway() {
        val root = repoRoot()
        val sources = productionSources(root).filter { it.relativeTo(root).invariantSeparatorsPath != GATEWAY }
        val codeOf = sources.associate { it.relativeTo(root).invariantSeparatorsPath to codeOnly(it.readText()) }

        for ((constName, pinned) in PINNED_SENTINEL_READERS) {
            val found = codeOf.filterValues { it.contains("$OBJECT.$constName") }.keys.toSortedSet()
            assertTrue("哨兵 `$OBJECT.$constName` 在生产代码里没有任何读方：那它是干什么用的？", found.isNotEmpty())
            assertEquals("哨兵 `$constName` 的读方住址变了。", pinned, found.toSet())
        }
        // 反面：哨兵的字面不许在网关之外再出现一次（手抄一份 = 命令改了它不跟着改）。
        for (literal in SENTINEL_LITERALS) {
            val leaked = codeOf.filterValues { it.contains("\"$literal") }.keys.toSortedSet()
            assertEquals("哨兵字面 `$literal` 在网关之外被手抄了一份：请改成读 `$OBJECT` 的常量。", emptySet<String>(), leaked.toSet())
        }
    }

    /** 抠取器自检：②那条的判别力全押在 [FUN_DECL_RE] 与 [codeOnly] 上。 */
    @Test
    fun theFunctionExtractorFindsDeclarationsAndOnlyDeclarations() {
        val sample =
            "object X {\n" +
                "    fun realOne(a: String): String = a\n" +
                "    private fun hiddenOne(): Int = 1\n" + // private 也算：它同样可能是新长出来的一支
                "    // fun commentedOut(): Unit = Unit\n" + // 注释里的不算
                "    val notAFun = listOf(1).map { fun2(it) }\n" + // 调用不是声明
                "}\n"
        val got = FUN_DECL_RE.findAll(codeOnly(sample)).map { it.groupValues[1] }.toSortedSet()
        assertEquals("只该认出两支真声明：$got", sortedSetOf("hiddenOne", "realOne"), got)
    }

    // ---- 扫描器 ---------------------------------------------------------------

    /**
     * 剥注释、留字面量，走全仓唯一一份词法扫描器 [KotlinSourceScanner.codeOnlyKeepingLiterals]。
     *
     * 必须留字面：③ 那条反面断言扫的是哨兵字面（引号开头的 `ATERM_SENT` 之类），用的是
     * `assertEquals(emptySet, leaked)`，换成「删字面」那支就是恒绿（①② 扫 `TmuxCommands.<名>` 则两支都认得出）。
     * 它看不见什么写在 [KotlinSourceScanner] 的头注里。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)

    private fun repoRoot(): File =
        generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
            .take(MAX_WALK_UP)
            .firstOrNull { File(it, "settings.gradle.kts").isFile }
            ?: error("找不到仓根（往上 $MAX_WALK_UP 层都没有 settings.gradle.kts）：${File(".").absolutePath}")

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

        private const val OBJECT = "TmuxCommands"
        private const val GATEWAY = "app/src/main/kotlin/com/ccmonitor/mobile/ssh/TmuxCommands.kt"

        /** 顶层/成员函数声明（含 `private`）。不认调用：`fun` 后面必须紧跟名字与 `(`。 */
        private val FUN_DECL_RE = Regex("""(?m)^\s*(?:private\s+|internal\s+)?fun\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(""")

        private const val BACKEND = "app/src/main/kotlin/com/ccmonitor/mobile/ssh/TmuxBackend.kt"
        private const val SINK = "app/src/main/kotlin/com/ccmonitor/mobile/ssh/TmuxSendKeysSink.kt"
        private const val LAUNCHER = "app/src/main/kotlin/com/ccmonitor/mobile/ssh/PipeLauncher.kt"
        private const val TMUX_DIALOG = "app/src/main/kotlin/com/ccmonitor/mobile/ui/session/TmuxManager.kt"

        /**
         * 定值钉：网关对外那 16 支各自的生产调用方。
         *
         * | 类 | 函数 | 谁在用 |
         * |---|---|---|
         * | 进入/新建 | `tmuxNewOrAttach` · `tmuxNewDetachedCommand` · `tmuxNewDetachedRunning` · `tmuxStartOnceCommand` | `TmuxBackend`（`SessionBackend` 的实现，别处只经它） |
         * | 探测/查询 | `tmuxForegroundProbeCommand` · `tmuxPaneChildProbeCommand` · `tmuxSessionCwdCommand` · `tmuxListCommand` · `tmuxCapturePaneCommand` | `TmuxBackend` |
         * | resume | `tmuxResumeCommand` | `TmuxBackend`（档案整份交进去） |
         * | kill/投递 | `tmuxKillCommand` · `tmuxSendModelCommand` | `TmuxBackend` |
         * | 解析 | `isResumeForegroundShell` · `paneConfirmedNoChild` | `TmuxBackend`（`SessionBackend` 把它们也收进了接口） |
         * | 解析 | `tmuxKillSucceeded` | `TmuxManager`，不经 `TmuxBackend`：`SessionBackend` 没有 `killSucceeded` 这一格 |
         * | 上行 | `buildSendCommand` | `TmuxSendKeysSink`（它只管「产品行为」那一半） |
         */
        private val PINNED_OUTWARD_CALLERS =
            linkedMapOf(
                "tmuxNewOrAttach" to setOf(BACKEND),
                "tmuxNewDetachedCommand" to setOf(BACKEND),
                "tmuxNewDetachedRunning" to setOf(BACKEND),
                "tmuxStartOnceCommand" to setOf(BACKEND),
                "tmuxForegroundProbeCommand" to setOf(BACKEND),
                "tmuxPaneChildProbeCommand" to setOf(BACKEND),
                "tmuxSessionCwdCommand" to setOf(BACKEND),
                "tmuxListCommand" to setOf(BACKEND),
                "tmuxCapturePaneCommand" to setOf(BACKEND),
                "tmuxResumeCommand" to setOf(BACKEND),
                "tmuxKillCommand" to setOf(BACKEND),
                "tmuxSendModelCommand" to setOf(BACKEND),
                "isResumeForegroundShell" to setOf(BACKEND),
                "paneConfirmedNoChild" to setOf(BACKEND),
                "tmuxKillSucceeded" to setOf(TMUX_DIALOG),
                "buildSendCommand" to setOf(SINK),
            )

        /**
         * 只有网关自己用的那几支：如实登记，不假装它们有外部消费方。
         *
         * | 函数 | 谁在用（网关内） | 为什么它是公开的 |
         * |---|---|---|
         * | `tmuxAttachCommand` | `tmuxResumeCommand` 的收尾段 | 它是 resume 那条串的一格，判据要能单独钉住它的两种形态（attach vs switch-client 防嵌套） |
         * | `tmuxMobileSetup` | `tmuxNewOrAttach` · `tmuxAttachCommand` | 按会话名出串，判据要能单独钉住「只对那一个会话」，不碰 tmux 全局 |
         * | `exactTarget` | `buildGuardedCommand` | `=name:` 的精确匹配包装是结构性的门，判据要能单独证「空 target 被拒」 |
         * | `buildGuardedCommand` | `buildSendCommand` | 身份门那一段单独可测（`needSid` 两支形状完全不同） |
         * | `isCcmTmuxName` | `buildSendCommand` 决定 `needSid` | 「认老 `cc-` 前缀」的判据（`TmuxSessionNameSourceTest` ③c 钉着它是只认不造那一侧） |
         * | `hasNewCcSuffix` | `isCcmTmuxName` | `private`，跟着上一条 |
         */
        private val GATEWAY_INTERNAL_ONLY =
            setOf("tmuxAttachCommand", "tmuxMobileSetup", "exactTarget", "buildGuardedCommand", "isCcmTmuxName", "hasNewCcSuffix")

        /** 定值钉：焊在命令串里的哨兵，各自今天的读方。 */
        private val PINNED_SENTINEL_READERS =
            linkedMapOf(
                "TMUX_ALREADY_MARKER" to setOf(LAUNCHER),
                "TMUX_STARTED_MARKER" to setOf(LAUNCHER),
                "TMUX_KILL_OK_MARKER" to setOf(TMUX_DIALOG),
                "SENT_MARKER" to setOf(SINK),
                "MODAL_SENTINEL" to setOf(SINK),
            )

        /**
         * 上面那几个哨兵的字面值：网关之外一次都不许出现（手抄一份就等于有两处各说各的）。
         *
         * 只列有具名常量的那五个。`NO_TMUX` / `CCM_NO_SESSION` / `CCM_GUARD_REJECTED`
         * 在网关里是内联在 `printf` 里的、没有常量可读，`TmuxSendKeysSink.classify` 那头也是手写字面，
         * 所以它们不在本条射程内；要闭合得先在网关里给它们起名字。
         */
        private val SENTINEL_LITERALS =
            listOf("__aterm_already__", "__aterm_started__", "__aterm_kill_ok__", "ATERM_SENT", "ATERM_MODAL_WAIT")
    }
}

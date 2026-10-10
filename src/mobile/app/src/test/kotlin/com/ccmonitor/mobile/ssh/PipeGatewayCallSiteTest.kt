package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 管道面的每条远端命令串都由那一个网关产出，而且它每一支都真的接上了。
 *
 * | # | 问什么 | 不问会怎样 |
 * |---|---|---|
 * | ① | 网关公开的每一支对外函数，在生产代码里有至少一个调用方，且住址逐字钉死 | 「已 build 零引用」：网关写好了、没人用、别的路照跑 |
 * | ② | 网关里的函数一支不多一支不少（新增一支必须在这张表里表态） | 新长出来的函数会悄悄绕开①，表看着还全绿 |
 *
 * ①是「谁在用」，`PipeSurfaceLiteralScanTest` 是「还有谁在自己拼」，`PipeGatewayGoldenTest` 是「串本身没变」。
 * 三条互不替代：只有①时别处可以照拼；只有②时网关可以是死代码；只有定值时可以有第二个产地各自正确。
 *
 * 它读 `core-claude` 的源文件，所以住 `app`：`:app:testDebugUnitTest` 的运行时 classpath 含全部 core 模块的产物，
 * 任何模块的代码改动都会让它重跑（同 `NamespaceLiteralScanTest`）。
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 本文件 |
 * |---|---|
 * | 网关新增一支函数 | 红（②那条集合不等） |
 * | 某支函数失去全部生产调用方 | 红（①那条文件集不等，且要求非空） |
 * | 调用方存在但走的是死分支 | 抓不到：源码扫描只答「有没有引用」 |
 * | 反射 / 别名 / 通过局部 `val f = PipeCommands::startCommand` 转一手 | 抓不到（防手滑不防拼接） |
 */
class PipeGatewayCallSiteTest {
    /** ①：每支对外函数的生产调用方住址定值钉，且非空。 */
    @Test
    fun everyOutwardGatewayFunctionHasPinnedProductionCallers() {
        val root = repoRoot()
        val sources = productionSources(root).filter { it.relativeTo(root).invariantSeparatorsPath != GATEWAY }
        assertTrue("前提：得真扫到源文件（且已排除网关自己），实得 ${sources.size} 个", sources.size > 150)

        for ((fn, pinned) in PINNED_OUTWARD_CALLERS) {
            val found =
                sources
                    .filter { codeOnly(it.readText()).contains("PipeCommands.$fn") }
                    .map { it.relativeTo(root).invariantSeparatorsPath }
                    .toSortedSet()
            assertTrue(
                "`PipeCommands.$fn` 零生产调用方：网关函数要么接上、要么删掉。",
                found.isNotEmpty(),
            )
            assertEquals(
                "`PipeCommands.$fn` 的生产调用方住址变了。多出来的：确认它该经网关（那就把住址加进表）；" +
                    "少掉的：那一路是不是又自己拼串去了（`PipeSurfaceLiteralScanTest` 会从另一头红）。",
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
     * 扫的是 `PipeCommands.<fn>` 与 `fun` 声明这类代码构造，留不留字面两支都认得出。
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

        private const val GATEWAY = "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/command/PipeCommands.kt"

        /** 顶层/成员函数声明（含 `private`）。不认调用：`fun` 后面必须紧跟名字与 `(`。 */
        private val FUN_DECL_RE = Regex("""(?m)^\s*(?:private\s+|internal\s+)?fun\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(""")

        private const val CLAUDE_INVOCATION = "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/command/ClaudeInvocation.kt"
        private const val CHAT_ATTACHMENT = "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/command/ChatAttachment.kt"
        private const val PIPE_SESSION = "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/bridge/PipeSession.kt"
        private const val PIPE_DIAGNOSTICS = "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/bridge/PipeDiagnostics.kt"
        private const val PIPE_LAUNCHER = "app/src/main/kotlin/com/ccmonitor/mobile/ssh/PipeLauncher.kt"

        /**
         * 定值钉：网关对外那几支，各自的生产调用方。
         *
         * | 类 | 函数 | 谁在用 |
         * |---|---|---|
         * | 起 | `startCommand` | `ClaudeInvocation.pipeInvocation`（唯一产地：它出「跑什么」，网关出「管子」） |
         * | 写 | `idempotentAppendCommand` · `appendLineCommand` | `PipeSession`（它只出那一行 JSON 与针） |
         * | 写 | `mkdirCommand` | `ChatAttachment`（附件目录；`mkdir` 是写动词，收进网关） |
         * | 读 | `diagnosticsReadCommand` | `PipeDiagnostics` |
         * | 读 | `eventsPath` | `PipeSession.frames`（交给 `TailTransport` 的落点；那条 tail 命令归 `TailTransport`） |
         * | 布局 | `sessionDir` | `ChatAttachment` · `PipeLauncher`（UI 提示/排查用的转发） |
         * | 证据 | `guarded` · `bodyOrNullIfFailed` | `PipeSession`（`PipeUplinkSink`：写成功的肯定证据，见 `OK_MARKER`） |
         */
        private val PINNED_OUTWARD_CALLERS =
            linkedMapOf(
                "startCommand" to setOf(CLAUDE_INVOCATION),
                "idempotentAppendCommand" to setOf(PIPE_SESSION),
                "appendLineCommand" to setOf(PIPE_SESSION),
                "mkdirCommand" to setOf(CHAT_ATTACHMENT),
                "diagnosticsReadCommand" to setOf(PIPE_DIAGNOSTICS),
                "eventsPath" to setOf(PIPE_SESSION),
                "sessionDir" to setOf(CHAT_ATTACHMENT, PIPE_LAUNCHER),
                "guarded" to setOf(PIPE_SESSION),
                "bodyOrNullIfFailed" to setOf(PIPE_SESSION),
            )

        /**
         * 只有网关自己用的那几支：如实登记，不假装它们有外部消费方。
         *
         * `inPath` 被两条写命令用、`logPath` 被 [diagnosticsReadCommand] 用。
         * 它们公开是因为「会话目录 / 上行 / 下行 / 日志」是一套布局四件套，
         * 判据要能逐字钉住这四个落点（`PipeGatewayGoldenTest` 在钉）。
         */
        private val GATEWAY_INTERNAL_ONLY = setOf("inPath", "logPath")
    }
}

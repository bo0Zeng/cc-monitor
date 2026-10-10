package com.ccmonitor.mobile.agent

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * `AgentKind` 的两个字面只许住档案里。
 *
 * `agentKind` 若散在生产代码各处、每处自己分派，就会出现「别处都分了档，只有一处没分」
 * （例如 Codex 服务器落进 Claude 的聊天屏）。有人在消费方写回一句 `if (kind == AgentKind.Codex)`，本条当场红并点出文件名。
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 这条判据 |
 * |---|---|
 * | 消费方新增 `== AgentKind.Codex` / `when (kind) { AgentKind… }` | 红 |
 * | `AgentKind.valueOf("Cod" + "ex")` · `AgentKind.entries[1]` · 反射 | 抓不到：防手滑不防拼接 |
 * | `app/src/androidTest`、各模块 `src/debug` 源集 | 不在扫描面内 |
 * | 注释 / KDoc 里提到那两个名字 | 刻意不算（[codeOnly] 先剥注释），注释里分派不了东西 |
 *
 * 白名单 [PINNED_PROFILE_FILES] 是定值钉：多一个文件红（有人又散了一处），
 * 少一个也红（档案挪窝了，得重新过一遍眼再改这张表）。同
 * `NewUiSettingsTest.theDefaultSourceIsPinnedSoAnyChangeGetsReVetted` 的做法。
 */
class AgentProfileSingleAddressTest {
    /**
     * 档案之外，`app/src/main` + 全部 `core-…` 模块的 `src/main` 里零命中。
     *
     * 三条前提断言顶在前面：少了它们，一个「什么都没扫到」的扫描器会让本条恒绿。
     */
    @Test
    fun theAgentKindLiteralsLiveOnlyInTheProfile() {
        val root = repoRoot()
        val roots = productionSourceRoots(root)
        assertTrue("前提：得找到扫描面（app + core-*），实得 ${roots.map { it.name }}", roots.size >= 2)
        val sources = roots.flatMap { r -> r.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList() }
        assertTrue("前提：得真扫到源文件，否则这条测试是空跑（实得 ${sources.size} 个）", sources.size > 150)

        var hits = 0
        val found = sortedSetOf<String>()
        for (f in sources) {
            val n = countKindLiterals(codeOnly(f.readText()))
            hits += n
            if (n > 0) found += f.relativeTo(root).invariantSeparatorsPath
        }
        assertTrue("前提：整份语料一次都没命中 ⇒ 扫描器瞎了，判据恒绿（实得 $hits 处）", hits > 0)

        assertEquals(
            "agent 种类的判断要走档案（`AgentProfile.of(kind)`），不许在消费方写 `AgentKind.X` 字面。" +
                "多出来的文件请改掉；确属档案/序列化边界的，连同理由加进 PINNED_PROFILE_FILES。",
            PINNED_PROFILE_FILES,
            found.toSet(),
        )
    }

    /**
     * 扫描器自检（同 `AgentArchetypeTest.theScannerReadsRealTextArgumentsAndNothingElse` 的用意）。
     *
     * 上面那条全靠 [countKindLiterals] 认得准。它要是认错了（把注释算进来、或者把
     * `MyAgentKind.Codex` 这种别的类型也算成命中），判据要么恒绿要么开始误红，两种都比没有更坏。
     */
    @Test
    fun theScannerCountsRealDispatchAndNothingElse() {
        val sample =
            "// AgentKind.Codex —— 注释里的不算\n" +
                "/* AgentKind.ClaudeCode —— 块注释里的也不算 */\n" +
                "val a = AgentKind.Codex\n" +
                "val b = MyAgentKind.Codex\n" + // 前面挨着标识符字符 ⇒ 是别的类型
                "val c = AgentKind.CodexLike\n" + // 后面挨着标识符字符 ⇒ 是别的名字
                "val d = when (k) { AgentKind.ClaudeCode -> 1; else -> 2 }\n"
        val code = codeOnly(sample)
        assertEquals("只该认出 `a` 与 `d` 两处真分派：\n$code", 2, countKindLiterals(code))
        assertEquals("注释剥干净了没有：\n$code", 0, countKindLiterals(codeOnly("// AgentKind.Codex\n")))
        assertEquals("块注释剥干净了没有", 0, countKindLiterals(codeOnly("/* AgentKind.ClaudeCode */")))
        // 反过来也要证：真分派一定认得出，否则「剥得干净」可以靠「什么都不认」作弊
        assertEquals("裸一行真分派必须认出", 1, countKindLiterals(codeOnly("if (k == AgentKind.Codex) f()")))
    }

    // ---- 扫描器 --------------------------------------------------------------

    /**
     * 剥注释、留字面量，走全仓唯一一份词法扫描器 [KotlinSourceScanner.codeOnlyKeepingLiterals]。
     *
     * 本条扫的是 `AgentKind.Codex` 这类代码分派，字面留不留都认得出。
     * 它看不见什么写在 [KotlinSourceScanner] 的头注里。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)

    /** 数 `AgentKind.Codex` / `AgentKind.ClaudeCode` 两个字面的真实出现次数（两侧都护）。 */
    private fun countKindLiterals(code: String): Int {
        var n = 0
        for (mark in MARKS) {
            var i = 0
            while (true) {
                val at = code.indexOf(mark, i)
                if (at < 0) break
                i = at + mark.length
                // 前面紧挨标识符字符 ⇒ 是别的类型（`MyAgentKind.Codex`），不算
                if (at > 0 && isIdentChar(code[at - 1])) continue
                // 后面紧挨标识符字符 ⇒ 是别的名字（`AgentKind.CodexLike`），不算
                if (i < code.length && isIdentChar(code[i])) continue
                n++
            }
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
     * 扫描面：`app/src/main/kotlin` + 每个 `core-…` 模块的 `src/main/kotlin`。
     *
     * 这就是本仓全部 Kotlin 生产代码（仓根那个 `bridge/` 不含 `.kt`），但只含 `main` 源集，
     * `androidTest` / `debug` 不在内。
     */
    private fun productionSourceRoots(root: File): List<File> =
        (listOf(File(root, "app")) + (root.listFiles()?.filter { it.isDirectory && it.name.startsWith("core-") } ?: emptyList()))
            .map { File(it, "src/main/kotlin") }
            .filter { it.isDirectory }
            .sortedBy { it.path }

    companion object {
        private const val MAX_WALK_UP = 6

        private val MARKS = listOf("AgentKind.Codex", "AgentKind.ClaudeCode")

        /**
         * 允许出现 `AgentKind.X` 字面的文件（相对仓根）。只许缩短，不许悄悄变长。
         *
         * | 文件 | 为什么它可以有 |
         * |---|---|
         * | `AgentProfile.kt` | 它就是档案本身：一个 agent 一份档案在这里定义，`of(kind)` 那个 `when` 是全仓唯一一个 |
         *
         * Room 边界与 wire 边界都不需要字面：前者走 [com.ccmonitor.mobile.core.claude.agent.AgentProfile.ofStoredName]、
         * 后者走 `AgentProfile.fromWire`。谁要往回放宽，先读 `AgentProfile.kt` 的头注。
         */
        private val PINNED_PROFILE_FILES =
            setOf(
                "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/agent/AgentProfile.kt",
            )
    }
}

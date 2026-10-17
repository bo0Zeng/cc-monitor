package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * tmux 会话名由那台核心铸（`session-open` 没给名 ⇒ 那台铸），手机不造 `cc-…` 名：`"cc-`（后面不是 `monitor` 的那种）在生产代码里一处都没有。
 * 用户在 tmux 管理窗里手敲的名字原样交给那台，不加前缀。
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 本文件 |
 * |---|---|
 * | 在消费方复制一份 `"cc-"` 前缀去构名 | 红（点名文件） |
 * | `"cc" + "-"` · `String(charArrayOf('c','c','-'))` · 反射 | 抓不到：防手滑不防拼接 |
 * | `androidTest` / `src/debug` 源集 | 不在扫描面内 |
 */
class TmuxSessionNameSourceTest {
    @Test
    fun theMobileNeverMintsCcSessionNames() {
        val root = repoRoot()
        val sources = productionSources(root)
        assertTrue("前提：得真扫到源文件，否则这条是空跑（实得 ${sources.size} 个）", sources.size > 100)

        val found = sortedSetOf<String>()
        var hits = 0
        for (f in sources) {
            val n = countSessionCcPrefix(codeOnly(f.readText()))
            hits += n
            if (n > 0) found += f.relativeTo(root).invariantSeparatorsPath
        }
        assertEquals("手机不造 `cc-` 会话名（那台核心铸名）：命中 $hits 处", emptySet<String>(), found.toSet())
    }

    /** 扫描器自检：注释里的不算、`cc-monitor` 不算，代码里那一处要算 —— 否则上面那条要么恒绿要么误红。 */
    @Test
    fun theScannerReadsCodeAndNothingElse() {
        val sample =
            "// 注释里的 \"cc- 不算\n" +
                "val a = \"cc-\${sid.take(8)}\"\n" +
                "val b = \"cc-monitor-backend\"\n" // 二进制名，不是会话名
        assertEquals("会话名前缀只该认出 `a` 那一处（`b` 是 cc-monitor 的二进制名）", 1, countSessionCcPrefix(codeOnly(sample)))
        assertEquals("注释剥干净了没有", 0, countSessionCcPrefix(codeOnly("/* \"cc-x */\n")))
    }

    /** 剥注释、留字面量，走全仓唯一一份词法扫描器 [KotlinSourceScanner.codeOnlyKeepingLiterals]（会话名前缀住在字面里）。 */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)

    /** 数会话名用的 `"cc-` 前缀：后面紧跟 `monitor` 的是 cc-monitor 的二进制名，不算。 */
    private fun countSessionCcPrefix(code: String): Int {
        var n = 0
        var i = 0
        while (true) {
            val at = code.indexOf(CC_NAME_PREFIX, i)
            if (at < 0) break
            i = at + CC_NAME_PREFIX.length
            if (code.startsWith("monitor", i)) continue
            n++
        }
        return n
    }

    /** 仓根 = 往上走到含 `settings.gradle.kts` 的那一层（Gradle 跑测试时 cwd = 模块目录）。 */
    private fun repoRoot(): File =
        generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
            .take(MAX_WALK_UP)
            .firstOrNull { File(it, "settings.gradle.kts").isFile }
            ?: error("找不到仓根（往上 $MAX_WALK_UP 层都没有 settings.gradle.kts）：${File(".").absolutePath}")

    /** 扫描面：`app/src/main/kotlin` + 每个 `core-…` 模块的 `src/main/kotlin`（= 本仓全部 Kotlin 生产代码）。 */
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
        private const val CC_NAME_PREFIX = "\"cc-"
    }
}

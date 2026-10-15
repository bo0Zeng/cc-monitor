package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * **手机上只有一条重连循环**：共用的那一条（嵌的核心里的 `resident-client`，桌面同一份：拨号按进展计时 · 宿主报事就重拨 · 心跳判死）。
 * SSH 那层（`core-ssh`：终端 · 文件走的那条）不自己拨、不自己判死、不自己退避：握着的那条死了等共用那条够着那一台再跟着换一条，
 * 开链路的期限在核心里（`ack_within` 套共用那一处的 `dial_within`）。本文件是源码那一道：
 *
 * | # | 问什么 |
 * |---|---|
 * | ① | `core-ssh` 的生产代码里没有一个时限数：不 `delay` · 不 `withTimeout` · 不 `sleep`，没有 `…Ms` / `…_MS` 那种名字 |
 * | ② | 全部 Kotlin 生产代码里没有自己探活 · 自己重连的口（`probeAlive` · `probeAndReconnectStale` · `requestReconnectAll`） |
 *
 * 住在 `:app` 的测试里：它的运行时 classpath 含全部 `core-…` 模块，改哪个模块都会让它重跑（同 [NamespaceLiteralScanTest] 那条注意）。
 * 判别力押在 [theScannerSeesEachMark] 上（每一针都认得出真写法、认不出注释与字面）。
 */
class OneReconnectLoopScanTest {
    @Test
    fun theSshLayerHoldsNoTimeNumber() {
        val root = repoRoot()
        val files = kotlinUnder(File(root, "core-ssh/src/main/kotlin"))
        assertTrue("前提：得真扫到 core-ssh 的源文件（实得 ${files.size} 个）", files.size >= 10)
        val hits = sortedMapOf<String, List<String>>()
        for (f in files) {
            val found = timeMarks(KotlinSourceScanner.codeOnlyDroppingLiterals(f.readText()))
            if (found.isNotEmpty()) hits[f.relativeTo(root).invariantSeparatorsPath] = found
        }
        assertEquals(
            "SSH 那层又在自己数时间（那是第二条重连循环：看门狗 · 退避 · 探活期限）。期限在核心里、按进展算，重连跟着共用那一条。",
            emptyMap<String, List<String>>(),
            hits.toMap(),
        )
    }

    @Test
    fun nothingProbesOrReconnectsOnItsOwn() {
        val root = repoRoot()
        val files = productionSources(root)
        assertTrue("前提：得真扫到生产源文件（实得 ${files.size} 个）", files.size > 100)
        val hits = sortedMapOf<String, List<String>>()
        for (f in files) {
            val code = KotlinSourceScanner.codeOnlyDroppingLiterals(f.readText())
            val found = OWN_LOOP.filter { Regex("\\b$it\\b").containsMatchIn(code) }
            if (found.isNotEmpty()) hits[f.relativeTo(root).invariantSeparatorsPath] = found
        }
        assertEquals("又长出了自己探活 · 自己重连的口：", emptyMap<String, List<String>>(), hits.toMap())
    }

    /** 自检：每一针都认得出真写法；注释 · 字面里提一嘴不算；`MAX_FILE_BYTES = 64L shl 20` 那种大小的数不算时限。 */
    @Test
    fun theScannerSeesEachMark() {
        val real =
            listOf(
                "fun f() { delay(5) }",
                "val x = withTimeoutOrNull(t) { 1 }",
                "Thread.sleep(1)",
                "val handshakeTimeoutMs: Long = 45_000",
                "private const val PROBE_TIMEOUT_MS = 5_000L",
            )
        for (line in real) {
            assertTrue("认不出：$line", timeMarks(KotlinSourceScanner.codeOnlyDroppingLiterals(line)).isNotEmpty())
        }
        val quiet =
            listOf(
                "// delay(5) 写在注释里",
                "val s = \"withTimeout(\"",
                "const val MAX_FILE_BYTES = 64L shl 20",
                "val delayed = true",
            )
        for (line in quiet) {
            assertEquals("误认：$line", emptyList<String>(), timeMarks(KotlinSourceScanner.codeOnlyDroppingLiterals(line)))
        }
        assertTrue(Regex("\\bprobeAlive\\b").containsMatchIn(KotlinSourceScanner.codeOnlyDroppingLiterals("x.probeAlive(1)")))
    }

    private fun timeMarks(code: String): List<String> = TIME_MARKS.flatMap { r -> r.findAll(code).map { it.value }.toList() }

    private fun kotlinUnder(dir: File): List<File> = dir.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()

    /** 仓根 = 往上走到含 `settings.gradle.kts` 的那一层（Gradle 跑测试时 cwd = 模块目录）。 */
    private fun repoRoot(): File =
        generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
            .take(MAX_WALK_UP)
            .firstOrNull { File(it, "settings.gradle.kts").isFile }
            ?: error("找不到仓根（往上 $MAX_WALK_UP 层都没有 settings.gradle.kts）：${File(".").absolutePath}")

    /** `app` ＋ 每个 `core-…` 模块的 `src/main/kotlin`。 */
    private fun productionSources(root: File): List<File> =
        (listOf(File(root, "app")) + (root.listFiles()?.filter { it.isDirectory && it.name.startsWith("core-") } ?: emptyList()))
            .map { File(it, "src/main/kotlin") }
            .filter { it.isDirectory }
            .flatMap(::kotlinUnder)

    companion object {
        private const val MAX_WALK_UP = 6

        private val TIME_MARKS =
            listOf(
                Regex("\\bdelay\\("),
                Regex("\\bwithTimeout(OrNull)?\\b"),
                Regex("\\bsleep\\("),
                Regex("\\b\\w*(Ms|_MS)\\b"),
            )

        private val OWN_LOOP = listOf("probeAlive", "probeAndReconnectStale", "requestReconnectAll")
    }
}

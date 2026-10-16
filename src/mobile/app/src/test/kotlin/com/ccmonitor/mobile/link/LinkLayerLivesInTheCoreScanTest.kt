package com.ccmonitor.mobile.link

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 手机没有 Kotlin 那一份连远端后端的链路层：接常驻流 · 握手 · 帧客户端（发号 · 期限 · 撤单）· 重连退避 · 版本门 · 在看名单
 * 整条在嵌的 Rust 核心里（`resident-client`，桌面 monitor 同一份）。Kotlin 这一侧只把核心交回来的事落成几条流（`ResidentFeed`）。
 *
 * | # | 问什么 |
 * |---|---|
 * | ① | 那几样从前的 Kotlin 链路层（`ResidentLink` · `FrameClient` · `BackendFeed` · `OneShot` · `BackendGate` · `WatchList` · `BackendBin` · `LineFramer` · `RemoteDuplex`）一个都没有声明 |
 * | ② | 链路层的线上词（请求信封的 `within_ms` · 撤单的 `cancel` · 在看名单的 `stream-watch` · 接流与起常驻的子命令）在生产字面里一个都没有 |
 * | ③ | 退避的数不在 Kotlin 里：没有 `Backoff` 声明，也没有「活过多久才算站住」那个名字（`HEALTHY_MS`）—— 两端同一份在核心里 |
 *
 * 判别力押在阳性对照上（每一条先喂一份合成样本证明认得出）。看不见拼接 · 反射，防手滑不防故意。
 * 住 `app`：`:app:testDebugUnitTest` 的运行时 classpath 含全部 core 模块产物，任何模块的代码改动都会让它重跑。
 */
class LinkLayerLivesInTheCoreScanTest {
    @Test
    fun theOldKotlinLinkLayerIsDeclaredNowhere() {
        assertEquals("阳性对照：认得出声明", listOf("FrameClient"), declared("internal class FrameClient private constructor(\nobject BackendBinX"))
        val hits = sources().mapNotNull { (rel, text) -> declared(text).takeIf { it.isNotEmpty() }?.let { "$rel: $it" } }
        assertEquals("Kotlin 里又长出了链路层（那一份在嵌的核心里，桌面同一份）", emptyList<String>(), hits)
    }

    @Test
    fun theLinkWireWordsAppearInNoProductionLiteral() {
        assertEquals("阳性对照：认得出整段字面", listOf("within_ms"), wireWords("put(\"within_ms\", 3)\nval x = \"within_ms_not\""))
        val hits = sources().mapNotNull { (rel, text) -> wireWords(text).takeIf { it.isNotEmpty() }?.let { "$rel: $it" } }
        assertEquals("链路层的线上词又出现在 Kotlin 生产字面里：一问一答经 `CoreLink.call` 交给核心", emptyList<String>(), hits)
    }

    @Test
    fun theReconnectBackoffIsNotInKotlin() {
        assertEquals("阳性对照", listOf("Backoff", "HEALTHY_MS"), backoffNames("class Backoff(\nconst val HEALTHY_MS: Long = 30_000L"))
        val hits = sources().mapNotNull { (rel, text) -> backoffNames(text).takeIf { it.isNotEmpty() }?.let { "$rel: $it" } }
        assertEquals("重连退避的数只在核心里（两端同一份）", emptyList<String>(), hits)
    }

    // ---- 扫描器 ---------------------------------------------------------------

    private fun declared(text: String): List<String> {
        val code = KotlinSourceScanner.codeOnlyDroppingLiterals(text)
        return OLD_LINK_LAYER.filter { name -> DECL.any { kw -> Regex("""\b$kw\s+$name\b""").containsMatchIn(code) } }
    }

    private fun wireWords(text: String): List<String> {
        val literals = KotlinSourceScanner.literalCorpusOf(text).split(KotlinSourceScanner.LITERAL_SEPARATOR).toSet()
        return WIRE_WORDS.filter { it in literals }
    }

    private fun backoffNames(text: String): List<String> {
        val code = KotlinSourceScanner.codeOnlyDroppingLiterals(text)
        return BACKOFF_NAMES.filter { Regex("""\b$it\b""").containsMatchIn(code) }
    }

    /** 扫描面：`app` ＋ 每个 `core-…` 模块的 `src/main/kotlin` 下的全部 `.kt`（仓相对路径 → 原文）。 */
    private fun sources(): List<Pair<String, String>> {
        val root =
            generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
                .take(MAX_WALK_UP)
                .firstOrNull { File(it, "settings.gradle.kts").isFile }
                ?: error("找不到仓根：${File(".").absolutePath}")
        val modules = listOf(File(root, "app")) + (root.listFiles()?.filter { it.isDirectory && it.name.startsWith("core-") } ?: emptyList())
        val files = modules.map { File(it, "src/main/kotlin") }.filter { it.isDirectory }.flatMap { r -> r.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList() }
        assertTrue("前提：得真扫到源文件（实得 ${files.size} 个）", files.size > 100)
        return files.map { it.relativeTo(root).invariantSeparatorsPath to it.readText() }
    }

    private companion object {
        const val MAX_WALK_UP = 6
        val DECL = listOf("class", "object", "interface")
        val OLD_LINK_LAYER = listOf("ResidentLink", "FrameClient", "BackendFeed", "OneShot", "BackendGate", "WatchList", "BackendBin", "LineFramer", "RemoteDuplex")

        // 「attach」不在表里：它也是终端那几问里的一个动作词（`"kind" to "attach"`），不是握手。
        val WIRE_WORDS = listOf("within_ms", "cancel", "stream-watch", "--resident-attach", "--resident-ensure")
        val BACKOFF_NAMES = listOf("Backoff", "HEALTHY_MS")
    }
}

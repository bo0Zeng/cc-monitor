package com.ccmonitor.mobile.ui.copy

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import com.ccmonitor.mobile.ui.overview.SECTION_NEEDS_MANUAL
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 文案里不许有第二人称，「需要你」一律叫「需手动」。两条规矩两个产品共用。
 *
 * 规矩不落到可执行层面就会一句句腐回去（同 `UiWordlistTest` 头注里「文案会一句句把技术概念漏回 UI」）；
 * `UiWordlistTest` 的禁用词表里不含第二人称，所以单开这一份。
 *
 * 射程：两个模块的 `src/main`：`app`（界面全部包，不只三个面）＋ `core-claude`
 * （等待文案 `WaitingCopy` 住这儿，漏掉它这条判据就是空的）。测试源不扫：判据自己要钉文案，断言里必然出现文案。
 *
 * ### 它不防什么
 *
 * 1. 拼接绕得过：`"等" + "你"`、`"$a你$b"` 里的模板段。
 * 2. 扫不到资源文件：`strings.xml` 看不见。
 * 3. 只看字面量，不判是否真上屏：日志、异常消息、命令行参数里的字也会被抓。
 *    这个方向是安全的（多抓不漏抓）；真有非上屏的必要例外，加进 [allowed] 并写理由。
 * 4. 不管第一人称：「我们」不在这条规矩里。
 */
class SecondPersonBanTest {
    /** 你·您·你的，所有档。「你的」是「你」的子串，钉「你」即可。 */
    private val secondPerson = listOf("你", "您")

    /**
     * 允许的例外：`字面量片段 to 理由`。它是空的，这是故意的：
     * 加一条进来等于承认有一句上屏文案带第二人称，要先说清为什么非它不可。
     */
    private val allowed = emptyMap<String, String>()

    private fun mainDir(module: String): File {
        val rel = "$module/src/main/kotlin/com/ccmonitor/mobile"
        // 单测的工作目录是模块目录（`app/`）；从仓根跑时路径已经对了。
        return File(rel).takeIf { it.isDirectory } ?: File("../$rel")
    }

    private data class Hit(
        val where: String,
        val word: String,
        val literal: String,
    )

    private fun scan(module: String): List<Hit> {
        val dir = mainDir(module)
        assertTrue(
            "找不到 $module 的 main 源目录（判据自己失效了，比不合规更坏）：${dir.absolutePath}",
            dir.isDirectory,
        )
        return dir
            .walkTopDown()
            .filter { it.isFile && it.extension == "kt" }
            .flatMap { f ->
                KotlinSourceScanner
                    .literalsOf(f.readText())
                    .asSequence()
                    .filter { lit -> allowed.keys.none { lit.body.contains(it) } }
                    .flatMap { lit ->
                        secondPerson
                            .asSequence()
                            .filter { lit.body.contains(it) }
                            .map { Hit("${f.name}:${lit.line}", it, lit.body) }
                    }
            }.toList()
    }

    @Test
    fun noUserFacingLiteralEverAddressesTheReaderAsYou() {
        val hits = listOf("app", "core-claude").flatMap { scan(it) }
        assertEquals(
            "文案里出现了第二人称（你·您·你的，所有档）：每条都要改写成无人称：\n" +
                hits.joinToString("\n") { "  ${it.where}  「${it.word}」 in ${it.literal}" },
            emptyList<Hit>(),
            hits,
        )
    }

    /**
     * 另一条规矩：「需要你」一律叫「需手动」。
     *
     * 单独钉，是因为它是总览屏的分区名：上面那条机检只要有人把它拼起来就漏，
     * 而这个常量是它唯一的上屏入口，逐字钉得住。
     */
    @Test
    fun theSectionIsCalledNeedsManualNotNeedsYou() {
        assertEquals("需手动", SECTION_NEEDS_MANUAL)
        assertFalse(
            "分区名一律叫「需手动」，不许是「需要你」",
            SECTION_NEEDS_MANUAL.contains("需要你"),
        )
    }
}

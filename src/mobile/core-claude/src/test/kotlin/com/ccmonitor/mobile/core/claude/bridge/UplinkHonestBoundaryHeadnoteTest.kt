package com.ccmonitor.mobile.core.claude.bridge

import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 上行的诚实边界（幂等治不了的情形）必须逐字留在被守代码的头注里。
 *
 * 比的是多行整段，且只在首个顶层声明之前那段头注里找：只断言「文件里有这几个字」的话，
 * 任何人在别处提一句就绿。把段落挪出头注、改写措辞、删掉，三种都红。
 *
 * 它只证明「那段字还在」，证明不了代码行为仍如它所说；那一侧由 `UplinkIdempotentAppendTest` 接住。
 */
class UplinkHonestBoundaryHeadnoteTest {
    /**
     * 静默分叉住 `UplinkSink.kt` 的头注。
     *
     * 各条上行路共有：幂等治「同一条消息发两遍」，治不了「两处同时打字」。
     */
    @Test
    fun theSilentForkBoundaryStaysInTheUplinkSinkHeadnote() {
        assertHeadnoteContains(
            file = "bridge/UplinkSink.kt",
            anchor = "interface UplinkSink {",
            id = "静默分叉",
            block =
                """
                 | * ### 静默分叉：幂等治不了它
                 | *
                 | * 手机与电脑同时对着一条对话打字：两条上行各自拿到成功、两侧判据各自全绿、
                 | * 两条都进了同一条线性上下文；无报错、无日志，用户侧唯一症状是「AI 突然变笨了」。
                 | * 幂等治的是「同一条消息发两遍」，治不了这个 —— 那是两条真的不同的消息进同一条上下文，
                 | * 我们把它们串行化也串不成一条。
                 | * 要解它得靠后端的排队语义；在后端做出队列之前，这段不许删。
                """.trimMargin(),
        )
    }

    /**
     * 查重-then-追加剩下的窗口与长行被撕开，住管道网关 `command/PipeCommands.kt` 的头注，
     * 与那条命令串在一起。
     *
     * 管道路特有：同一条 exec 消掉了网络往返那半，远端本机那一段窗口还在；真实代价是残行。
     */
    @Test
    fun theCheckThenAppendWindowAndTornLineBoundaryStaysInThePipeCommandsHeadnote() {
        assertHeadnoteContains(
            file = "command/PipeCommands.kt",
            anchor = "object PipeCommands {",
            id = "查重-then-追加的窗口与撕行",
            block =
                """
                 | * 注意：[idempotentAppendCommand] 把查重与追加放进同一条远端命令，没有网络往返，但远端本机仍有窗口：
                 | * 追加写到一半被打断会留下残行。而且 dash 的 `printf` 超过 8 KiB 会分多次 `write(2)`，`O_APPEND`
                 | * 只保证单次原子，并发写方（中断不受在飞门约束、两台设备同时打字）可能把长行撕开。
                 | * 根治要先写临时文件再 `mv`。
                """.trimMargin(),
        )
    }

    /**
     * 判据自己不许是空真：把「删掉头注」与「把段落挪出头注区」两种变异就地造出来，断言两种都判不过。
     */
    @Test
    fun theHeadnoteJudgeItselfWouldGoRedIfTheParagraphWereRemovedOrMovedOut() {
        val block = "### 诚实边界：某条刻意不钉"
        val anchor = "class Foo {"

        val deleted = listOf("/**", " * 别的话", " */", anchor, "}")
        assertTrue("头注被删掉 ⇒ 必须判不过", !headnoteOf(deleted, anchor).contains(block))

        // 段落还在文件里，但挪到了声明之后 ⇒ 也必须判不过（只测「文件里有这几个字」会在这漏掉）
        val movedOut = listOf("/**", " * 别的话", " */", anchor, "    // $block", "}")
        assertTrue("段落挪出头注区 ⇒ 必须判不过", !headnoteOf(movedOut, anchor).contains(block))
        assertTrue("前提：段落确实还在文件里（所以「扫全文」那种写法会误绿）", movedOut.any { it.contains(block) })

        val kept = listOf("/**", " * $block", " */", anchor, "}")
        assertTrue("前提：段落留在头注里时必须判得过", headnoteOf(kept, anchor).contains(block))
    }

    private companion object {
        /** 首个顶层声明之前那一段 = 头注区。 */
        fun headnoteOf(
            lines: List<String>,
            anchor: String,
        ): String {
            val at = lines.indexOfFirst { it.startsWith(anchor) }
            require(at >= 0) { "找不到锚点声明：$anchor" }
            return lines.take(at).joinToString("\n")
        }

        /** `core-claude` 的源码根：工作目录可能是仓根，也可能是模块目录。 */
        fun sourceRoot(): File {
            val fromModule = File("src/main/kotlin/com/ccmonitor/mobile/core/claude")
            if (fromModule.isDirectory) return fromModule
            val fromRepo = File("core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude")
            require(fromRepo.isDirectory) { "找不到 core-claude 源码根，cwd=${File(".").absolutePath}" }
            return fromRepo
        }

        fun assertHeadnoteContains(
            file: String,
            anchor: String,
            id: String,
            block: String,
        ) {
            val src = File(sourceRoot(), file)
            assertTrue("被守的源文件不在：${src.absolutePath}", src.isFile)
            val headnote = headnoteOf(src.readLines(), anchor)
            assertTrue(
                "诚实边界「$id」必须逐字留在 $file 的头注里。\n" +
                    "删它、改写它、或把它挪到 `$anchor` 之后，这条都会红 —— 那是有意的。\n" +
                    "要的整段：\n$block\n" +
                    "----- 实得头注 -----\n$headnote",
                headnote.contains(block),
            )
        }
    }
}

package com.ccmonitor.mobile.ui.overview

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 词表机检：禁用的技术词不许出现在给用户看的字面量里。
 *
 * 「终端 / tmux / 按键排」这些对新用户应该不可见；词表是把这条落到可执行层面的唯一手段，
 * 否则文案会一句句把技术概念漏回 UI。这条机检抓的是手滑与顺手：有人图省事写了「连接失败」「会话已结束」。
 *
 * ### 它不防什么
 *
 * 1. 字符串拼接绕得过：`"连" + "接"`、`"$a$b"`。
 * 2. 扫不到资源文件：`strings.xml` 里的任何一句它都看不见。
 * 3. 只扫 `Text(` 的实参：`Toast`、`contentDescription`、`placeholder = "…"`、异常消息里的字它看不见。
 *    `SessionScreen.kt` 里有 `Text(context, "…")` 这种 `Toast.makeText` 形态，不是 Compose 的 `Text`，
 *    但本扫描器会把它算进来，这个方向是安全的（多抓不漏抓）。
 *
 * ### 射程：新界面的三个面，不是整个 `app/src/main`
 *
 * `ui/session` / `ui/host` / `ui/identity` 是终端那一路，一个终端屏上写「tmux 会话」是对的。
 * 词表管的是聊天 / 总览 / 设置三个面。射程分两档：
 * - [cleanPackages]：零命中，任何手滑当场红。
 * - [ratchets]：还不干净的两个包，各上一道棘轮（只许减不许增）：`ui/claude` 是聊天面的正文渲染器
 *   （`ClaudeReadingPane`），`ui/settings` 剩一句设置项说明里的「连接」。
 *
 * 扫描器走共享实现 [KotlinSourceScanner.literalsInCallsTo]；它认 Kotlin 的嵌套块注释，
 * 回归锚钉在 [theScannerDoesNotMistakeANestedKdocForScreenCopy]，词法层那条钉在
 * `KotlinSourceScannerTest.aNestedBlockCommentNeverLeaksIntoTheCallLiterals`（两个模块各跑一遍）。
 */
class UiWordlistTest {
    /** 禁用词，逐字。 */
    private val banned = listOf("连接", "SSH", "tmux", "会话", "exec", "bridge", "daemon", "offset")

    /** 已经干净的包。零命中。 */
    private val cleanPackages = listOf("overview", "chat")

    /**
     * 还欠着的包 → 现有的违规条数。
     *
     * 这些数只许往下改。往上改 = 又往 UI 上漏了一个技术词。
     */
    private val ratchets = mapOf("claude" to 10, "settings" to 1)

    private fun uiDir(pkg: String): File {
        val rel = "src/main/kotlin/com/ccmonitor/mobile/ui/$pkg"
        // 单测的工作目录是模块目录（`app/`）；从仓根跑时退回带模块前缀的路径。
        return File(rel).takeIf { it.isDirectory } ?: File("app/$rel")
    }

    private data class Hit(
        val where: String,
        val word: String,
        val literal: String,
    )

    private fun scan(pkg: String): List<Hit> {
        val dir = uiDir(pkg)
        assertTrue("找不到 $pkg 的源码目录（判据自己失效了，比不合规更坏）：${dir.absolutePath}", dir.isDirectory)
        return dir
            .walkTopDown()
            .filter { it.isFile && it.extension == "kt" }
            .flatMap { f ->
                textLiterals(f.readText()).flatMap { (line, literal) ->
                    banned.filter { literal.contains(it) }.map { Hit("${f.name}:$line", it, literal) }
                }
            }.toList()
    }

    @Test
    fun theOverviewAndChatFacesNeverSayATechnicalWordOutLoud() {
        val hits = cleanPackages.flatMap { scan(it) }
        assertEquals(
            "词表命中：每条都要改成人话：\n" +
                hits.joinToString("\n") { "  ${it.where}  「${it.word}」 in ${it.literal}" },
            emptyList<Hit>(),
            hits,
        )
    }

    @Test
    fun thePackagesStillOwingCopyWorkNeverGetWorse() {
        ratchets.forEach { (pkg, baseline) ->
            val hits = scan(pkg)
            assertTrue(
                "ui/$pkg 的词表违规从 $baseline 涨到了 ${hits.size}：棘轮只许往下走：\n" +
                    hits.joinToString("\n") { "  ${it.where}  「${it.word}」 in ${it.literal}" },
                hits.size <= baseline,
            )
        }
    }

    /**
     * 判据的阴性对照：扫描器自己得真的能抓到东西。
     *
     * 没有这条的话，一个「永远返回空列表」的扫描器 bug 会让上面两条恒绿。
     */
    @Test
    fun theScannerActuallyFindsABannedWordWhenThereIsOne() {
        val src =
            """
            fun f() {
                Text("连接失败: ${'$'}message")
                Text("这句是干净的")
                // Text("会话") —— 注释里的不算
            }
            """.trimIndent()
        val found = textLiterals(src).map { it.second }
        assertTrue("扫描器漏了 Text 里的字面量：$found", found.any { it.contains("连接失败") })
        assertTrue("扫描器漏了第二个 Text：$found", found.any { it.contains("这句是干净的") })
        assertEquals("注释里的 Text 不该被算进来：$found", 2, found.size)
    }

    /**
     * 嵌套 KDoc 里的 `Text(…)` 不是界面文案。
     *
     * 用「找下一个闭合」跳块注释的扫描器不认嵌套，在这份样本上会吐出
     * `[(3, 会话已结束), (5, 真实文案)]`：注释里的引文被算成上屏文案（假阳），会让人去改一句根本没上屏的话。
     * 退回那种跳法 ⇒ 本条当场红。
     *
     * 块注释的开/闭在这里逐字拼、不写成连续字面：写成字面的话本文件会被
     * `SharedScannerGateTest` 的⑤（按形状认手搓剥注释器）点名。
     */
    @Test
    fun theScannerDoesNotMistakeANestedKdocForScreenCopy() {
        val star = "*"
        val sample =
            "fun f() {\n" +
                "/" + star + star + " 外层 KDoc，里面嵌一层：" + "/" + star + " 内层说明 " + star + "/" + " 外层还没完\n" +
                " " + star + " Text(\"会话已结束\")  —— 注释里的引文，不该上屏\n" +
                " " + star + "/\n" +
                "    Text(\"这句是干净的\")\n" +
                "}\n"
        val got = textLiterals(sample)
        assertEquals("嵌套块注释提前收尾了，注释后半段被当代码扫 ⇒ 一条假阳：$got", listOf(5 to "这句是干净的"), got)
        // 靶子还在：样本真的带嵌套，而且「提前收尾」之后那一段里真的有一个禁用词引文。
        val close = star + "/"
        val firstClose = sample.indexOf(close)
        assertTrue(
            "样本不再带嵌套 / 提前收尾之后那段里没有禁用词了 ⇒ 这条锚失去靶子，请换一份样本",
            firstClose > 0 && banned.any { w -> sample.substring(firstClose + close.length).contains(w) },
        )
    }
}

/**
 * 从一份 Kotlin 源码里挑出每个 `Text(` 调用的实参里的字符串字面量，走全仓唯一一份
 * 词法扫描器 [KotlinSourceScanner.literalsInCallsTo]。
 *
 * 载荷就是字面内容：本文件问的是「上屏的那句话里有没有技术词」。接到 `codeOnlyDroppingLiterals` 上的话
 * 一句文案都抠不出来、词表全零命中然后全绿；[UiWordlistTest.theScannerActuallyFindsABannedWordWhenThereIsOne]
 * 钉着「扫描器真能抓到东西」。
 *
 * @return `行号 to 字面量内容`（不含引号；模板 `${'$'}{...}` 原样留在串里，
 *   所以 `Text("${'$'}{n} 个会话")` 抓得到「会话」）。行号是那条字面自己所在的行；
 *   这个数只进报错信息，不进判定。
 */
internal fun textLiterals(source: String): List<Pair<Int, String>> =
    KotlinSourceScanner.literalsInCallsTo(source, "Text").literals.map { it.line to it.body }

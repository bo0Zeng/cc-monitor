package com.ccmonitor.mobile.copy

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import com.squareup.moshi.Moshi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * **手机生产源码里不写死界面中文**：手机各模块 `src/main/` 下的 `.kt` 里每一个含汉字的字符串字面量，要么按位置放过，
 * 要么逐条登记在 `tests/copy/mobile-literal-exempt.json`（按 文件 · 那句 · 用途 认）；其余一律红 —— 改成 `copyText("键", …)` 取文。
 *
 * # 为什么
 *
 * 手机、桌面、后端读同一张 `src/shared/copy/table.json`。桌面那一半由普查（`copy-ledgers.vitest.ts` 的 CP2：全仓对外字面量零条）
 * 钉住「抽完的文件不许长回字面量」；手机抽完了，就不另起待办表，直接判零。
 * 口径与桌面那一条一致：日志不对外（桌面放过 console / tracing）、前置条件的报错是程序员错（桌面放过 panic / assert），
 * 其余对外的字只从表里来。
 *
 * # 判什么
 *
 * 1. 人群：生产 Kotlin（[KotlinSourceScanner] 按词法读，注释不算）里含汉字的字面量；模板那一截记 `{…}`（[KotlinSourceScanner.templateShape]）。
 * 2. 按位置放过（[PASS_CALLS]）：`Log.v/d/i/w/e/wtf(…)` 日志 · `testTag(…)` · `require/check(…) { … }` 前置条件。
 * 3. 其余每一处的（文件 · 字面量）都要在豁免表里、用途取自闭集 [USES]；否则红。
 * 4. 豁免表无死条：登记的那句在那份文件里真有一处命中、文件存在。
 * 5. 正控：同一个读法对着样本认出界面字、放过日志 / testTag / 前置条件 / 注释，模板记成 `{…}`。
 *
 * # 不判什么
 *
 * - 只认含汉字的：纯英文、纯符号（`⊕` `▸`）不在人群里（与桌面普查「含汉字」那一刀同）。
 * - 不判豁免登记得对不对：用途写成「内部」的那一句到底上不上屏，靠人复核（登记时写清去向）。
 * - 拼接绕过（`"连" + "接"` 各一个字也照样数得到；`String(charArrayOf(…))` 数不到）：防手滑，不防故意。
 * - 模板插值里再嵌字面（`"a ${q("b")} c"`）会让引号配对翻面（扫描器头注那一条），那种字面切成两截各算各的。
 * - `res/values/strings.xml` 不在这里：清单要求住在 xml 的那几条照留，见豁免表的说明。
 */
class MobileLiteralsTest {
    private data class Hit(
        val file: String,
        val line: Int,
        val text: String,
    )

    private data class Exempt(
        val text: String,
        val use: String,
        val why: String,
    )

    @Test
    fun positiveControlCatchesUiTextAndPassesLogsTagsPreconditionsAndComments() {
        val dollar = "$"
        val sample =
            listOf(
                "// 注释里的「连接」不算",
                "/* 块注释 /* 嵌套 */ 也不算 */",
                "Text(\"连接失败：$dollar{e.message}\")",
                "android.util.Log.w(TAG, \"日志不算 ${dollar}x\")",
                "Modifier.testTag(\"标签不算\")",
                "require(ok) { \"前置条件不算\" }",
                "val s = \"\"\"原样串 ${dollar}n 行\"\"\"",
                "val c = '\"' // 字符字面不吞后文",
                "title = \"新建会话\"",
                "check(f(x)) { \"带括号的条件也认得出\" }",
                "val m = mapOf(1 to \"映射里的照算\")",
                "Text(\"转义的 \\${dollar}x 不是模板\")",
            ).joinToString("\n")
        val got = hitsOf(listOf("x.kt" to sample)).map { "${it.line}:${it.text}" }
        assertEquals(
            listOf("3:连接失败：{…}", "7:原样串 {…} 行", "9:新建会话", "11:映射里的照算", "12:转义的 \\${dollar}x 不是模板"),
            got,
        )
    }

    @Test
    fun noHardCodedUiChineseOutsideTheExemptListAndNoDeadExemptions() {
        val root = repoRoot()
        val files = productionKtFiles(root)
        assertTrue("生产 Kotlin 一份都没扫到（实得 ${files.size}）⇒ 扫描面写错了，本条会恒绿", files.size > MIN_FILES)
        val all = files.sumOf { (_, text) -> hanLiteralsOf(text).size }
        assertTrue("生产 Kotlin 里一条含汉字的字面都没认出 ⇒ 读法瞎了，本条会恒绿", all > 0)

        val exempt = exemptions(root)
        val bad = ArrayList<String>()
        val used = HashSet<Pair<String, String>>()
        for (h in hitsOf(files)) {
            if (exempt[h.file].orEmpty().any { it.text == h.text }) {
                used += h.file to h.text
            } else {
                bad += "${h.file}:${h.line} · ${h.text}"
            }
        }
        assertEquals(
            "这几处在手机界面上写死了中文 —— 改成 copyText(\"键\", \"名\" to 值) 取文（src/shared/copy/table.json）；" +
                "不上屏的（词表 · 远端命令 · 内部诊断）才进 tests/copy/mobile-literal-exempt.json",
            emptyList<String>(),
            bad,
        )

        val dead = ArrayList<String>()
        val badUse = ArrayList<String>()
        for ((f, list) in exempt) {
            if (!File(root, f).isFile) dead += "$f（文件不在）"
            for (e in list) {
                if (e.use !in USES) badUse += "$f · ${e.text} · ${e.use}"
                if (e.why.isBlank()) badUse += "$f · ${e.text}（没写去向）"
                if ((f to e.text) !in used) dead += "$f · ${e.text}"
            }
        }
        assertEquals("用途不在闭集里（${USES.keys.joinToString(" · ")}）或没写去向", emptyList<String>(), badUse)
        assertEquals("死豁免：那份文件里已没有这句 —— 从 mobile-literal-exempt.json 删掉", emptyList<String>(), dead)
    }

    private fun hanLiteralsOf(text: String): List<KotlinSourceScanner.CalledLiteral> =
        KotlinSourceScanner.literalsWithEnclosingCall(text).filter { HAN.containsMatchIn(it.literal.body) }

    private fun hitsOf(files: List<Pair<String, String>>): List<Hit> =
        files.flatMap { (file, text) ->
            hanLiteralsOf(text)
                .filterNot { PASS_CALLS.containsMatchIn(it.call) }
                .map { Hit(file, it.literal.line, KotlinSourceScanner.templateShape(it.literal.body)) }
        }

    @Suppress("UNCHECKED_CAST")
    private fun exemptions(root: File): Map<String, List<Exempt>> {
        val doc =
            Moshi
                .Builder()
                .build()
                .adapter(Any::class.java)
                .fromJson(File(root, EXEMPT_PATH).readText()) as Map<String, Any?>
        val files = doc["files"] as Map<String, List<Map<String, Any?>>>
        return files.mapValues { (_, list) ->
            list.map { Exempt(it["text"] as String, it["use"] as String, (it["why"] as String?).orEmpty()) }
        }
    }

    /** 手机各模块 `src/main/` 下的 `.kt`：（相对仓根的路径, 正文）。 */
    private fun productionKtFiles(root: File): List<Pair<String, String>> =
        File(root, "src/mobile")
            .walkTopDown()
            .onEnter { it.name !in SKIPPED_DIRS }
            .filter { it.isFile && it.extension == "kt" }
            .map { it.relativeTo(root).invariantSeparatorsPath to it }
            .filter { (rel, _) -> "/src/main/" in rel }
            .sortedBy { it.first }
            .map { (rel, f) -> rel to f.readText() }
            .toList()

    private fun repoRoot(): File =
        generateSequence(File("").absoluteFile) { it.parentFile }
            .firstOrNull { File(it, EXEMPT_PATH).isFile }
            ?: error("找不到仓根（往上没有 $EXEMPT_PATH）：${File("").absolutePath}")

    private companion object {
        const val EXEMPT_PATH = "tests/copy/mobile-literal-exempt.json"

        /** 生产 Kotlin 份数下限：只用来证明扫描面是真的，不钉准数。 */
        const val MIN_FILES = 100

        val HAN = Regex("[㐀-䶿一-鿿豈-﫿]")

        /** 按位置放过的调用：日志 · testTag · 前置条件（程序员错，同桌面放过 panic / assert）。 */
        val PASS_CALLS = Regex("""(^|\.)(Log\.(v|d|i|w|e|wtf)|testTag|require|check|requireNotNull|checkNotNull)$""")

        /** 豁免的用途闭集：这几类字面量不上屏，不该进表。 */
        val USES =
            mapOf(
                "词表" to "判据或过滤用的词表（禁词 · 认词），不显示",
                "命令" to "发给远端 shell / tmux 的命令串，字落在远端终端，不是手机界面上的字",
                "内部" to "内部诊断 · 断言 · 协议错的说明，只进日志 · 异常栈 · 复制详情的原话栏，不当句子上屏（登记时写清去向）",
            )

        // `detekt-rules`：手机的开发工具（本仓的 detekt 规则，只在 detekt 跑时装进去），不是产品代码，它的字不上屏。
        val SKIPPED_DIRS = setOf("build", ".git", ".gradle", ".idea", "libs", ".codepicture", "generated", "detekt-rules")
    }
}

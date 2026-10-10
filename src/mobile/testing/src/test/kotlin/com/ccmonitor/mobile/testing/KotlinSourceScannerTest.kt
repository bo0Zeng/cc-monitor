package com.ccmonitor.mobile.testing

import com.ccmonitor.mobile.testing.KotlinSourceScanner.LITERAL_SEPARATOR
import com.ccmonitor.mobile.testing.KotlinSourceScanner.codeOnlyDroppingLiterals
import com.ccmonitor.mobile.testing.KotlinSourceScanner.codeOnlyKeepingLiterals
import com.ccmonitor.mobile.testing.KotlinSourceScanner.literalCorpusOf
import com.ccmonitor.mobile.testing.KotlinSourceScanner.literalsInCallsTo
import com.ccmonitor.mobile.testing.KotlinSourceScanner.literalsOf
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * [KotlinSourceScanner] 的自检。
 *
 * 和实现住同一个共享目录，`:app` 与 `:core-claude` 各跑一遍：哪个模块的 `srcDir` 接线掉了，那个模块当场编译不过。
 *
 * 注意：这里只用手写样本，不读真实源文件。`:core-claude:test` 的输入不含 `app/`，
 * 读真实文件的回归判据放在 `:app` 里。
 */
class KotlinSourceScannerTest {
    /**
     * 「引号-星-斜杠-星-引号」那种字面（MIME 通配串）不许吃掉后面的代码。
     *
     * 正则剥注释会从这个字面中间开一个假块注释，一直吃到下一个真注释的收尾。
     */
    @Test
    fun aSlashStarWildcardInsideAStringDoesNotSwallowTheCodeAfterIt() {
        // 样本要同时带两样东西才复现得出：字面里的「星-斜杠」开了个假注释头，
        // 后面还得有一个真注释的收尾去给它配对。
        val sample =
            """
            val a = launcher.launch("*/*")
            val b = vm.connect(hostId)
            /* 一段真块注释 SHOULD_BE_STRIPPED */
            val c = after()
            """.trimIndent()
        for ((name, got) in branches(sample)) {
            assertTrue("[$name] 通配串之后那一行被吃掉了：\n$got", got.contains("vm.connect(hostId)"))
            assertTrue("[$name] 再往后那一行也被吃掉了：\n$got", got.contains("val c = after()"))
            assertFalse("[$name] 真注释没剥掉：\n$got", got.contains("SHOULD_BE_STRIPPED"))
        }
        // 反过来确认样本真的触发那个坑：坏正则在同一份样本上确实把中间那一行吃掉了。
        // 没有这一句，样本哪天被改得不再触发，上面几条会安静地失去判别力。
        assertFalse(
            "样本不再触发那个坑了，这条自检失去对照；换一份仍带「引号-星-斜杠-星-引号」且后面跟着真注释收尾的样本",
            brokenRegexStripper(sample).contains("vm.connect(hostId)"),
        )
    }

    /** Kotlin 的块注释是嵌套的：内层收尾不许把外层也一起关掉。 */
    @Test
    fun blockCommentsNest() {
        val sample = "val a = 1\n/* outer /* inner */ still comment MARK */\nval b = 2\n"
        for ((name, got) in branches(sample)) {
            assertFalse("[$name] 嵌套没认出来，外层注释的尾巴漏成代码了：\n$got", got.contains("MARK"))
            assertTrue("[$name] 注释前那一行该留着：\n$got", got.contains("val a = 1"))
            assertTrue("[$name] 注释后那一行该留着：\n$got", got.contains("val b = 2"))
        }
    }

    /** 行注释剥掉，同一行注释之前的代码留着。 */
    @Test
    fun lineCommentsAreStrippedAndTheCodeBeforeThemSurvives() {
        val sample = "val a = f() // 注释里的 g() 不算\nval b = g()\n"
        for ((name, got) in branches(sample)) {
            assertTrue("[$name] 注释之前的代码没留住：\n$got", got.contains("val a = f()"))
            assertFalse("[$name] 行注释没剥掉：\n$got", got.contains("注释里的"))
            assertEquals("[$name] `g()` 只该剩真代码那一处：\n$got", 1, occurrences(got, "g()"))
        }
    }

    /**
     * 「留字面」那支真的把字面留下来，「删字面」那支真的删掉。
     *
     * 载荷就是字面内容的判据，接到删字面那支上会一个都扫不到，然后全绿。
     */
    @Test
    fun theTwoBranchesDisagreeAboutStringLiteralsOnPurpose() {
        val sample = "val a = \"tmux new-session -d\"\nval b = run()\n"

        val kept = codeOnlyKeepingLiterals(sample)
        assertTrue("「留字面」那支必须逐字留住字面内容：\n$kept", kept.contains("\"tmux new-session -d\""))

        val dropped = codeOnlyDroppingLiterals(sample)
        assertFalse("「删字面」那支必须把字面内容删干净：\n$dropped", dropped.contains("tmux new-session"))
        assertFalse("连引号都不该留：\n$dropped", dropped.contains("\""))
        assertTrue("字面之外的代码两支都要留着：\n$dropped", dropped.contains("val b = run()"))
        assertTrue("字面那一行的赋值骨架也要留着：\n$dropped", dropped.contains("val a = "))
    }

    /**
     * 字符串里的两个斜杠不是行注释。
     *
     * sed 的替换串、URI 的协议分隔符都是这个形状；按行砍注释会把它们拦腰砍断，之后引号配对翻面，后面的字面全错位。
     */
    @Test
    fun twoSlashesInsideAStringAreNotALineComment() {
        val sample = "val u = \"android-keystore://aterm_key_master\"\nval v = 1\n"
        val kept = codeOnlyKeepingLiterals(sample)
        assertTrue("串被行注释规则砍掉了：\n$kept", kept.contains("android-keystore://aterm_key_master"))
        assertTrue("后一行该留着：\n$kept", kept.contains("val v = 1"))
        assertTrue("坏正则在这份样本上确实会砍断它，对照有效", !brokenRegexStripper(sample).contains("aterm_key_master"))
    }

    /** 字符字面里的引号不许把后面的引号配对带歪（`val c = '"'` 是合法 Kotlin）。 */
    @Test
    fun aQuoteInsideACharLiteralDoesNotDerailQuotePairing() {
        val sample = "val c = '\"'\nval d = \"PAYLOAD\"\nval e = 1\n"
        val kept = codeOnlyKeepingLiterals(sample)
        assertTrue("后面那个真字面被带歪了：\n$kept", kept.contains("\"PAYLOAD\""))
        assertTrue("再后面那行代码也被带歪了：\n$kept", kept.contains("val e = 1"))
    }

    /** 原始串（三引号）里的注释语法不是注释；它的内容在「留字面」那支里要原样留着。 */
    @Test
    fun rawStringsAreLiteralsNotComments() {
        val sample = "val r = \"\"\"a /* b */ c // d\"\"\"\nval s = 2\n"
        val kept = codeOnlyKeepingLiterals(sample)
        assertTrue("原始串被当注释剥了：\n$kept", kept.contains("a /* b */ c // d"))
        assertTrue("后一行该留着：\n$kept", kept.contains("val s = 2"))
        val dropped = codeOnlyDroppingLiterals(sample)
        assertFalse("「删字面」那支要把原始串删掉：\n$dropped", dropped.contains("a /* b */ c"))
        assertTrue("但后一行仍要留着：\n$dropped", dropped.contains("val s = 2"))
    }

    /** 剥完行数不变：判据的报错会报 `文件:行号`，行号得对得上。 */
    @Test
    fun theLineCountSurvivesStrippingComments() {
        val sample =
            "val a = 1\n" +
                "/* 一行\n" +
                "   两行\n" +
                "   三行 */\n" +
                "val b = \"\"\"raw\nstring\"\"\"\n" +
                "val c = 3\n"
        val lines = sample.lineSequence().count()
        for ((name, got) in branches(sample)) {
            assertEquals("[$name] 行数漂了，报错里的行号会指错地方：\n$got", lines, got.lineSequence().count())
        }
    }

    /** 剥掉的块注释不许把两边的 token 粘成一个（`A/* x */.b` 不是 `A.b`）。 */
    @Test
    fun strippedBlockCommentsDoNotGlueTokensTogether() {
        val sample = "val x = Foo/* 注释 */.bar()\n"
        for ((name, got) in branches(sample)) {
            assertFalse("[$name] 注释两边被粘成了一个假调用 `Foo.bar(`：\n$got", got.contains("Foo.bar("))
        }
    }

    /** 未闭合的字面不许吃掉整个文件（遇换行即止）。 */
    @Test
    fun anUnterminatedLiteralStopsAtTheLineEnd() {
        val sample = "val bad = \"没有收尾\nval good = 7\n"
        for ((name, got) in branches(sample)) {
            assertTrue("[$name] 一个落单的引号吃掉了后面的代码：\n$got", got.contains("val good = 7"))
        }
    }

    // ---- 抠字面那三支 ------------------------------------------------------------

    /**
     * [literalCorpusOf] 抠的是字面的内容，而且真的抠到了。
     *
     * 正反都断：真字面必须在语料里，注释里的引文一个都不许在。
     */
    @Test
    fun theLiteralCorpusKeepsLiteralBodiesAndDropsComments() {
        val sample =
            "val a = \"tmux ls\"\n" +
                "val c = '\"'\n" +
                "// 行注释里的 \"骗人的\" 不算\n" +
                "val b = \"\"\"raw --list-projects\"\"\"\n"
        val corpus = literalCorpusOf(sample)
        assertTrue("普通串的内容没抠出来：${corpus.replace(LITERAL_SEPARATOR, '|')}", corpus.contains("tmux ls"))
        assertTrue("原始串的内容没抠出来：${corpus.replace(LITERAL_SEPARATOR, '|')}", corpus.contains("raw --list-projects"))
        assertFalse("注释里的引文进语料了（那会变成一片假阳）", corpus.contains("骗人的"))
        assertFalse("引号本身不该进语料（抠的是内容）", corpus.contains("\""))
        assertEquals(
            "语料就该是「内容 + 隔断符」逐条拼起来",
            "tmux ls" + LITERAL_SEPARATOR + "raw --list-projects" + LITERAL_SEPARATOR,
            corpus,
        )
    }

    /**
     * 隔断符不许是空格，否则相邻两条字面会粘出一个假命中。
     */
    @Test
    fun adjacentLiteralsCannotGlueIntoAFalseHit() {
        assertEquals(
            "跨字面拼出了锚点，隔断符失效",
            0,
            occurrences(literalCorpusOf("val x = \"--list\" + \"-projects\""), "--list-projects"),
        )
        assertEquals(
            "而同一条字面里的锚点必须认出来（否则上面那条是靠「什么都抠不到」绿的）",
            1,
            occurrences(literalCorpusOf("val x = \"--list-projects\""), "--list-projects"),
        )
    }

    /** [literalsOf] 的行号是那条字面自己所在的行（判据的报错要指得准）。 */
    @Test
    fun everyLiteralKnowsWhichLineItLivesOn() {
        val sample = "val a = 1\nval b = \"二\"\nval c = 2\nval d = \"四\"\n"
        assertEquals(
            "行号漂了，判据报错里的 `文件:行号` 会指错地方",
            listOf(2 to "二", 4 to "四"),
            literalsOf(sample).map { it.line to it.body },
        )
    }

    /**
     * [literalsInCallsTo] 只收点名那个调用实参里的字面，正反都断。
     *
     * 收多了判据误红，收少了判据恒绿。
     */
    @Test
    fun onlyTheLiteralsInsideTheNamedCallAreCollected() {
        val sample =
            "Text(\"甲\")\n" +
                "BasicText(\"不算\")\n" +
                "Foo.Text(\"也不算\")\n" +
                "Text(if (k) \"乙\" else \"丙\")\n" +
                "val x = \"Text(\\\"字面里的假调用\\\")\"\n" +
                "Text(\"丁(括号在字面里\")\n"
        val got = literalsInCallsTo(sample, "Text")
        assertEquals("认出的调用处数不对：${got.literals.map { it.body }}", 3, got.callSites)
        assertEquals(
            "收到的字面不对：${got.literals.map { it.body }}",
            listOf("甲", "乙", "丙", "丁(括号在字面里"),
            got.literals.map { it.body },
        )
    }

    /**
     * 嵌套块注释里的 `Text(…)` 不是上屏文案。
     *
     * 用「找下一个闭合」跳块注释的扫描器会在内层收尾处提前收工，把注释后半段当代码扫，
     * 注释里的引文就被算成了界面文案。
     */
    @Test
    fun aNestedBlockCommentNeverLeaksIntoTheCallLiterals() {
        val star = "*"
        val sample =
            "val a = 1\n" +
                "/" + star + star + " 外层 KDoc，里面嵌一层：" + "/" + star + " 内层说明 " + star + "/" + " 外层还没完\n" +
                " " + star + " Text(\"注释里的引文\")  这不该上屏\n" +
                " " + star + "/\n" +
                "Text(\"真实文案\")\n"
        val got = literalsInCallsTo(sample, "Text")
        assertEquals("注释里的 `Text(` 被当成了真调用：${got.literals.map { it.line to it.body }}", 1, got.callSites)
        assertEquals(
            "嵌套块注释提前收尾了，注释后半段被当代码扫了",
            listOf(5 to "真实文案"),
            got.literals.map { it.line to it.body },
        )

        // 确认样本真的带嵌套，而且从内层收尾处接着扫的那一段里真的有一个 `Text(` 引文。
        // 没有这两句，样本哪天被改平了，上面两条会安静地失去判别力。
        val open = "/" + star
        val close = star + "/"
        val firstOpen = sample.indexOf(open)
        val firstClose = sample.indexOf(close)
        assertTrue(
            "样本不再带嵌套了（要的是「内层开 → 内层收尾 → 外层仍未闭合」），这条自检失去对照",
            firstOpen >= 0 && firstClose > firstOpen && sample.indexOf(open, firstOpen + open.length) in (firstOpen + 1) until firstClose,
        )
        assertTrue(
            "提前收尾之后那一段里没有引文了，对照是空的：不认嵌套的扫描器在这份样本上也不会错",
            sample.substring(firstClose + close.length).contains("Text(\"注释里的引文\")"),
        )
    }

    // ---- 帮手 -----------------------------------------------------------------

    private fun branches(sample: String): List<Pair<String, String>> =
        listOf(
            "keep" to codeOnlyKeepingLiterals(sample),
            "drop" to codeOnlyDroppingLiterals(sample),
        )

    private fun occurrences(
        text: String,
        mark: String,
    ): Int {
        var n = 0
        var i = 0
        while (true) {
            val at = text.indexOf(mark, i)
            if (at < 0) return n
            n++
            i = at + mark.length
        }
    }

    /**
     * 用正则剥注释的坏实现，作为样本的对照。
     *
     * 自检既要证扫描器在样本上对，也要证这个正则在同一份样本上真的错；否则样本不再触发那个坑时，自检会安静地失去判别力。
     *
     * 注意：正则源分段拼出来、不写成一个连续的字面。另有判据禁止仓里出现那个连续串。
     */
    private fun brokenRegexStripper(text: String): String {
        val blockPattern = "/" + "\\" + "*.*?" + "\\" + "*" + "/"
        val noBlock = text.replace(Regex(blockPattern, RegexOption.DOT_MATCHES_ALL), " ")
        return noBlock.lineSequence().joinToString("\n") { it.substringBefore("/" + "/") }
    }
}

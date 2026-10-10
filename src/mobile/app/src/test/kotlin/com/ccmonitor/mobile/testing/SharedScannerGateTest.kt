package com.ccmonitor.mobile.testing

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 那个坏正则不许再回潮，共享扫描器不许再分叉。
 *
 * 坏正则指「非贪婪匹配块注释 + 按行 `substringBefore` 砍行注释」。它会被字符串字面骗到：
 * 字面里的「星-斜杠」被当成块注释的收尾、字面里的两个斜杠被当成行注释的起点。
 * 在 `SftpScreen.kt` 上（上传用的 MIME 通配串就是那个形状）它会把后面一大段代码当注释吃掉，
 * `vm.connect(` 从 1 处变 0 处，任何在那个文件上断言「零命中 = 守住了」的判据静默失效。
 *
 * | # | 守什么 | 它要防的 bug 存在时会不会红 |
 * |---|---|---|
 * | ① [theBrokenCommentRegexIsGoneFromTheWholeRepo] | 全仓零份坏正则 | 任何一处写回那个正则 ⇒ 红并点名文件行号 |
 * | ② [theScannerHasExactlyOneImplementationInTheWholeRepo] | 共享实现的每支入口只许一份 | 有人复制一份出去各自改 ⇒ 红 |
 * | ③ [theRealWorldAnchorStillShowsTheBugAndTheFix] | 真实文件上「坏的真坏、新的真对」 | 新扫描器退化成按行砍 ⇒ 红；坏正则不再吃代码（锚点失效）⇒ 红并要求换锚 |
 * | ④ 凡声明 `codeOnly` 的文件必须引用共享扫描器 | 换个写法的等价坏实现 | 红 |
 * | ⑤ 手搓剥注释器按形状认 | 改个名字绕过④ | 红 |
 * | ⑥ [noHandRolledLexerSneaksIntoTheTestSources] | test 源集只许有一处 `while` 式词法扫描 | 新写一份「抠字面」的手搓扫描器 ⇒ 红（⑤ 按构造看不见这一族） |
 *
 * 两头断，别只断「零命中」：①②除了断「零命中 / 恰好一份」，还断真扫到了够多的文件、
 * 三类住址（`app` 的 test、`core-*` 的 test、仓根共享目录）都在面里、以及探测器本身不瞎
 * （[theDetectorItselfIsNotBlind] 拿一份合成样本证它认得出那个正则）。
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 本文件 |
 * |---|---|
 * | 把那个正则逐字写回任何一个 `.kt` | 红（①） |
 * | 写一个换了写法的等价坏正则（如用 `[\s\S]*?` 代替 `.*?`） | ①认不出，但④会把它挡在外面 |
 * | 把剥注释器藏进一个叫别的名字的私有帮手里 | 红（⑤ 按形状认，不按名字） |
 * | 手写一份只抠字面、不碰注释的扫描器 | 红（⑥） |
 * | 复制一份共享实现出去改 | 红（②） |
 * | 在共享实现里面把词法写错 | 本文件只兜住③那一条真实读数；逐条词法由 `KotlinSourceScannerTest` 守（它在两个模块各跑一遍） |
 * | `build/` 下的生成代码 | 不在扫描面内（刻意排除） |
 */
class SharedScannerGateTest {
    /**
     * ① 全仓（任何 `.kt`）零份坏正则。
     *
     * 两头断：先证扫描面真的有东西、三类住址都在里面，再证零命中。
     */
    @Test
    fun theBrokenCommentRegexIsGoneFromTheWholeRepo() {
        val root = repoRoot()
        val sources = kotlinSources(root)
        assertScanSurfaceIsReal(root, sources)

        val offenders =
            sources.flatMap { f ->
                f
                    .readText()
                    .lineSequence()
                    .withIndex()
                    .filter { (_, line) -> line.contains(brokenRegexSource()) }
                    .map { (i, _) -> "${f.relativeTo(root).invariantSeparatorsPath}:${i + 1}" }
                    .toList()
            }
        assertEquals(
            "那个会被字符串字面骗到的块注释正则又回来了。" +
                "请改成调 `KotlinSourceScanner`：载荷在字面里的用 `codeOnlyKeepingLiterals`，" +
                "只问「代码里有没有这个调用」的用 `codeOnlyDroppingLiterals`。\n" + offenders.joinToString("\n"),
            emptyList<String>(),
            offenders,
        )
    }

    /**
     * ② 共享扫描器全仓只许有一份实现（「两份拷贝各自漂移」的防线）。
     *
     * 共享方式是「共享源目录 + 两个模块各加一行 `srcDir`」，物理上只有一个文件；
     * 本条把「物理上只有一个」钉成判据。
     */
    @Test
    fun theScannerHasExactlyOneImplementationInTheWholeRepo() {
        val root = repoRoot()
        val sources = kotlinSources(root)
        assertScanSurfaceIsReal(root, sources)

        for (decl in IMPLEMENTATION_MARKERS) {
            val where =
                sources
                    .filter { it.readText().contains(decl) }
                    .map { it.relativeTo(root).invariantSeparatorsPath }
                    .sorted()
            assertEquals(
                "`$decl` 必须恰好有一个实现处（共享源目录那一份）。多出来的就是一份拷贝，" +
                    "两份拷贝会各自漂移。",
                listOf(SHARED_IMPL),
                where,
            )
        }
    }

    /**
     * ④ 凡是声明 `codeOnly` 的文件，都必须引用共享扫描器。
     *
     * ①只认那串逐字的正则源 ⇒ 换个写法的等价坏实现它认不出。本条从另一头堵：
     * 不管用什么技术，只要还叫 `codeOnly`，就必须是对共享实现的转发。
     */
    @Test
    fun everyFileThatDeclaresCodeOnlyDelegatesToTheSharedScanner() {
        val root = repoRoot()
        val sources = kotlinSources(root)
        assertScanSurfaceIsReal(root, sources)

        val declaring =
            sources
                .map { it to it.readText() }
                .filter { (_, text) -> text.contains(CODE_ONLY_DECL) }
        assertTrue(
            "全仓一处 `${CODE_ONLY_DECL.trim()}` 都没扫到 ⇒ 扫描面写错了，本条会恒绿。",
            declaring.size >= MIN_CODE_ONLY_SITES,
        )
        val rogue =
            declaring
                .filterNot { (_, text) -> text.contains(SCANNER_TYPE) }
                .map { (f, _) -> f.relativeTo(root).invariantSeparatorsPath }
                .sorted()
        assertEquals(
            "有文件自己造了一份 `codeOnly` 而没走共享扫描器" +
                "（私有的剥注释器多半就是那个坏正则）。",
            emptyList<String>(),
            rogue,
        )
    }

    /**
     * ③ 真实文件上的锚：坏的真的会吃代码，新的真的不吃。
     *
     * `SftpScreen.kt` 上坏正则剥完会少掉几十行、`vm.connect(` 从 1 处变 0 处；
     * 共享扫描器一行不少、`vm.connect(` 仍是 1 处。
     *
     * 这里不钉绝对行数（会随 UI 改动乱红），钉的是那个关系。哪天这个文件不再是那个形状
     * （通配串没了 / 后面没有真注释去给它配对），本条会红并要求换一个锚：没有靶子的「零命中 = 守住了」就是假绿。
     */
    @Test
    fun theRealWorldAnchorStillShowsTheBugAndTheFix() {
        val anchor = File(repoRoot(), ANCHOR)
        assertTrue("前提：锚点文件不在了：${anchor.absolutePath}", anchor.isFile)
        val raw = anchor.readText()
        val rawLines = raw.lineSequence().count()
        assertTrue("前提：锚点文件得真有内容（实得 $rawLines 行）", rawLines > 100)
        assertEquals("前提：锚点文件里 `$ANCHOR_MARK` 得恰好一处，否则这条锚失效了", 1, occurrences(raw, ANCHOR_MARK))

        // 新扫描器：一行不少、那个调用还在。
        val fixed = KotlinSourceScanner.codeOnlyKeepingLiterals(raw)
        assertEquals("共享扫描器把代码吃掉了（行数缩水）—— 它退化成按行砍了吗？", rawLines, fixed.lineSequence().count())
        assertEquals("共享扫描器把 `$ANCHOR_MARK` 吃掉了：这正是坏正则的 bug", 1, occurrences(fixed, ANCHOR_MARK))

        // 靶子仍在：坏正则在这个文件上确实会吃掉代码。没有这一句，上面两条可以靠「锚点文件变干净了」假绿。
        val broken = brokenRegexStripper(raw)
        assertTrue(
            "锚点失效了：坏正则在 `$ANCHOR` 上不再吃代码（实得 ${broken.lineSequence().count()} / $rawLines 行）。" +
                "请换一个仍带「引号-星-斜杠-星-引号」且后面跟着真注释收尾的文件当锚。",
            broken.lineSequence().count() < rawLines,
        )
        assertEquals(
            "锚点失效了：坏正则在 `$ANCHOR` 上不再把 `$ANCHOR_MARK` 吃掉 ⇒ 这条判据失去靶子，请换锚。",
            0,
            occurrences(broken, ANCHOR_MARK),
        )
    }

    /**
     * ⑤ 手搓剥注释器按「形状」认，不按名字。
     *
     * ④ 只认名字：一份叫 `backendLiteralsOf` 或 `textLiterals` 的手搓扫描器它一声不吭。
     *
     * 形状：任何手写的剥注释器都必须在源码里同时提到块注释的开与闭（无论是写成字面还是提成常量）。
     * 这个特征窄且硬：全仓 test 源集只有共享实现自己命中，没有误报，不是「要人天天加白名单」的那种判据。
     *
     * [HAND_ROLLED_SCANNERS] 上只有共享实现自己。新写一份当场红；要往回加一条，
     * 得连理由一起写进表里并登记成欠账（别只改表）。
     *
     * 它按构造看不见一族：只抠字面、不碰注释的扫描器压根不提这两个记号，那一族由
     * [noHandRolledLexerSneaksIntoTheTestSources] 认。
     */
    @Test
    fun noNewHandRolledCommentScannerSneaksInUnderADifferentName() {
        val root = repoRoot()
        val sources = kotlinSources(root).filter { "/src/test/" in it.invariantSeparatorsPath() }
        assertTrue("test 源集一个都没扫到（实得 ${sources.size}）⇒ 扫描面写错了，本条会恒绿", sources.size > MIN_TEST_SOURCES)

        val found =
            sources
                .filter { looksLikeACommentScanner(it.readText()) }
                .map { it.relativeTo(root).invariantSeparatorsPath }
                .sorted()
        assertTrue(
            "全仓一份手搓剥注释器都没扫到 ⇒ 探测器瞎了（共享实现自己就该命中），本条会恒绿。",
            found.isNotEmpty(),
        )
        assertEquals(
            "多出来一份手搓的剥注释器（按形状认，改名绕不过去）。\n" +
                "  全仓只许一份，新写的请改调 `KotlinSourceScanner`：\n" +
                "    载荷在字面里 ⇒ `codeOnlyKeepingLiterals`；只问「代码里有没有这个调用」⇒ `codeOnlyDroppingLiterals`。\n" +
                "  共享实现里缺所需的那一支（比如「抠字面内容」），那就把它加进共享实现并配自检，\n" +
                "  别在自己文件里再长一份。\n" +
                "  真有非共享不可的理由，加进 `HAND_ROLLED_SCANNERS` 并写明理由 + 登记成欠账（别只改表）。",
            HAND_ROLLED_SCANNERS,
            found.toSet(),
        )
    }

    /**
     * ⑤的探测器自检：没有它，⑤ 就是一条「零命中 = 守住了」。
     *
     * 正反两向都断：合成一份手搓扫描器必须认出来；干净的转发写法必须不认成。
     */
    @Test
    fun theShapeDetectorRecognisesAHandRolledScannerAndNotADelegation() {
        val blockOpen = "\"" + "/" + "*" + "\""
        val blockClose = "\"" + "*" + "/" + "\""
        val planted =
            "private fun strip(t: String): String {\n" +
                "    if (two == " + blockOpen + ") depth++\n" +
                "    if (two == " + blockClose + ") depth--\n" +
                "}\n"
        assertTrue("探测器认不出合成的手搓扫描器 ⇒ ⑤是恒绿的「零命中」", looksLikeACommentScanner(planted))
        assertTrue(
            "干净的转发写法不许被认成手搓（否则是全红，不是能分辨）",
            !looksLikeACommentScanner("private fun codeOnly(t: String) = KotlinSourceScanner.codeOnlyDroppingLiterals(t)"),
        )
        assertTrue(
            "只提到开、没提到闭的文件不算（半边特征太宽，会把一堆正常文件卷进来）",
            !looksLikeACommentScanner("val x = " + blockOpen),
        )
    }

    /**
     * ⑥ test 源集里只许有一处 `while` 式词法扫描。
     *
     * ⑤ 认的是「源码同时提到块注释的开与闭」。只抠字面、不碰注释的扫描器（把「剥注释」那半接了共享实现，
     * 自己只留下「抠字面」那半）压根不提这两个记号，⑤ 对它们一声不吭。
     *
     * 形状：一份手搓的字面抠取器必须干两件事：逐字符走（`while (i < …​.length)`）、
     * 并且把一个双引号当字符比（`'` 引号 `'`）。两条都占齐才算，单独一条都太宽。
     * 全仓 test 源集只有共享实现自己命中，零误报。
     *
     * ### 判别力边界
     *
     * | 绕过形态 | 本条 |
     * |---|---|
     * | 新写一份逐字符的字面抠取器 | 红 |
     * | 用正则代替逐字符走（没有 `while`） | 认不出，但那条路由 ① 与 ④ 各堵了一半 |
     * | 用 `for (c in text)` / `fold` 代替 `while` | 认不出：窄是故意的，宽了就要天天加白名单，然后被人删掉 |
     * | 写在 `src/main` 里（生产代码里的解析器） | 不在扫描面内：那是业务（`TerminalSession` 的转义解析就是） |
     */
    @Test
    fun noHandRolledLexerSneaksIntoTheTestSources() {
        val root = repoRoot()
        val sources = kotlinSources(root).filter { "/src/test/" in it.invariantSeparatorsPath() }
        assertTrue("test 源集一个都没扫到（实得 ${sources.size}）⇒ 扫描面写错了，本条会恒绿", sources.size > MIN_TEST_SOURCES)

        val found =
            sources
                .filter { looksLikeAHandRolledLexer(it.readText()) }
                .map { it.relativeTo(root).invariantSeparatorsPath }
                .sorted()
        assertTrue(
            "全仓一份逐字符扫描器都没扫到 ⇒ 探测器瞎了（共享实现自己就该命中），本条会恒绿。",
            found.isNotEmpty(),
        )
        assertEquals(
            "多出来一份手搓的词法扫描器。全仓的剥注释器与字面抠取器只许一份，\n" +
                "  新写的请改调 `KotlinSourceScanner`：\n" +
                "    只问「代码里有没有这个调用」⇒ `codeOnlyDroppingLiterals`；\n" +
                "    载荷在字面里 ⇒ `literalCorpusOf`（语料）/ `literalsOf`（带行号）；\n" +
                "    只要某个调用实参里的字面 ⇒ `literalsInCallsTo(text, \"Text\")`。\n" +
                "  共享实现里缺所需的那一支，那就把它加进共享实现并在共享目录里配自检\n" +
                "  （那样 `:app` 与 `:core-claude` 各跑一遍，哪边 `srcDir` 掉了哪边当场编译不过）。",
            HAND_ROLLED_LEXERS,
            found.toSet(),
        )
    }

    /**
     * ⑥的探测器自检：没有它，⑥ 就是一条「零命中 = 守住了」。
     *
     * 正反都断：合成一份手搓字面抠取器必须认出来；干净的转发、以及只占一半特征的写法必须不认成。
     *
     * 两个 needle 在本文件里逐字拼、不写成连续字面：写成字面的话本文件会被自己的 ⑥ 点名
     * （同 [brokenRegexSource] 的理由）。
     */
    @Test
    fun theLexerDetectorRecognisesAHandRolledScannerAndNotADelegation() {
        val quoteChar = "'" + "\"" + "'"
        val loop = "whi" + "le (i < code.length) {"
        val planted =
            "private fun grabLiterals(code: String): String {\n" +
                "    var i = 0\n" +
                "    " + loop + "\n" +
                "        if (code[i] == " + quoteChar + ") out.append(code[i])\n" +
                "        i++\n" +
                "    }\n" +
                "}\n"
        assertTrue("探测器认不出合成的手搓字面抠取器 ⇒ ⑥是恒绿的「零命中」", looksLikeAHandRolledLexer(planted))
        assertTrue(
            "干净的转发写法不许被认成手搓（否则是全红，不是能分辨）",
            !looksLikeAHandRolledLexer("private fun grabLiterals(t: String) = KotlinSourceScanner.literalCorpusOf(t)"),
        )
        assertTrue(
            "只有循环、没把引号当字符比的，不算（仓里一堆 `while` 循环，宽了就要天天加白名单）",
            !looksLikeAHandRolledLexer("var i = 0\n" + loop + "\n i++ \n}"),
        )
        assertTrue(
            "只提到引号字符、没有逐字符走的，也不算（一堆测试会提到引号）",
            !looksLikeAHandRolledLexer("assertEquals(" + quoteChar + ", shellQuote(q))"),
        )
    }

    /**
     * 探测器自检：①那条的判别力全押在「它认得出那串正则源」上。
     *
     * 探测器要是认不出（needle 拼错了、或哪天被改短了），①就是一条恒绿的「零命中」。
     */
    @Test
    fun theDetectorItselfIsNotBlind() {
        val planted =
            "    private fun codeOnly(text: String): String {\n" +
                "        val noBlock = text.replace(Regex(" + "\"\"\"" + brokenRegexSource() + "\"\"\"" + "), \" \")\n" +
                "    }\n"
        assertTrue("探测器认不出那串正则源 ⇒ ①那条是恒绿的「零命中」", planted.contains(brokenRegexSource()))
        assertTrue("探测器也得认得出 `codeOnly` 的声明（④那条靠它）", planted.contains(CODE_ONLY_DECL))
        assertTrue(
            "而干净的代码不许被误判成坏正则（否则是全红，不是能分辨）",
            !"private fun codeOnly(t: String) = KotlinSourceScanner.codeOnlyKeepingLiterals(t)".contains(brokenRegexSource()),
        )
    }

    // ---- 扫描面 ---------------------------------------------------------------

    /**
     * 两头断里的「这一头」：扫描面真的有东西，而且三类住址都在里面。
     *
     * 三类分别是 `:app` 的 test、`core-*` 的 test、仓根那个共享源目录；
     * 共享目录不属于任何 Gradle 模块，最容易在「按模块列目录」的写法里被漏掉。
     */
    private fun assertScanSurfaceIsReal(
        root: File,
        sources: List<File>,
    ) {
        assertTrue("扫描面太小（实得 ${sources.size} 个 .kt）⇒ 路径写错了，本文件的判据会恒绿", sources.size > MIN_SOURCES)
        val paths = sources.map { it.relativeTo(root).invariantSeparatorsPath }.toSet()
        for (must in SURFACE_WITNESSES) {
            assertTrue("扫描面没盖到 `$must` ⇒ 这一类住址被漏掉了，判据会在那儿恒绿", must in paths)
        }
    }

    /**
     * 全仓 `.kt`（排除构建产物与第三方制品目录）。
     *
     * 刻意不按模块列目录：那种写法会漏掉仓根的共享源目录（共享扫描器就住那儿）。
     */
    private fun kotlinSources(root: File): List<File> =
        root
            .walkTopDown()
            .onEnter { it.name !in SKIPPED_DIRS }
            .filter { it.isFile && it.extension == "kt" }
            .sortedBy { it.path }
            .toList()

    private fun repoRoot(): File =
        generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
            .take(MAX_WALK_UP)
            .firstOrNull { File(it, "settings.gradle.kts").isFile }
            ?: error("找不到仓根（往上 $MAX_WALK_UP 层都没有 settings.gradle.kts）：${File(".").absolutePath}")

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
     * 被禁的那串正则源，逐字拼出来、不写成一个连续的字面。
     *
     * 写成字面的话，本文件会被自己的判据①点名；下一步通常是给自己开个白名单，白名单一开这条就废了。
     */
    private fun brokenRegexSource(): String = "/" + "\\" + "*" + "." + "*" + "?" + "\\" + "*" + "/"

    /**
     * ⑤的形状探测：源码同时提到块注释的开与闭（写成字面或提成常量都算）。
     *
     * 两个 needle 逐字拼、不写成连续字面（同 [brokenRegexSource] 的理由）。
     *
     * 判别力边界：它认的是「提到了这两个记号」，不是「真的在剥注释」。
     * 用别的写法表达同一件事（比如只按 `Char` 比、或把记号拼出来）抓不到。
     */
    private fun looksLikeACommentScanner(text: String): Boolean {
        val open = "\"" + "/" + "*" + "\""
        val close = "\"" + "*" + "/" + "\""
        val mentionsOpen = text.contains(open) || text.contains("BLOCK_" + "OPEN")
        val mentionsClose = text.contains(close) || text.contains("BLOCK_" + "CLOSE")
        return mentionsOpen && mentionsClose
    }

    /**
     * ⑥的形状探测：源码同时占齐「逐字符走一遍」与「把双引号当字符比」两个特征。
     *
     * 引号那个 needle 逐字拼、不写成连续字面；循环那个 needle 是一条正则源，
     * 它自己不匹配自己（`while` 后面紧跟的是反斜杠不是空白或左括号），本文件不会被自己点名。
     *
     * 判别力边界：它认的是「这两个特征都在这份文件里」，不是「它们在同一个函数里」。
     * 一份文件同时有个无关的 `while` 循环和一句提到引号的断言会被误判；真出现了就换形状（别加白名单）。
     */
    private fun looksLikeAHandRolledLexer(text: String): Boolean {
        val quoteChar = "'" + "\"" + "'"
        return text.contains(quoteChar) && CHAR_BY_CHAR_LOOP.containsMatchIn(text)
    }

    private fun File.invariantSeparatorsPath(): String = path.replace(File.separatorChar, '/')

    /** 那个坏实现，留一份当③的靶子（同样逐字拼，不写成连续字面）。 */
    private fun brokenRegexStripper(text: String): String {
        val noBlock = text.replace(Regex(brokenRegexSource(), RegexOption.DOT_MATCHES_ALL), " ")
        return noBlock.lineSequence().joinToString("\n") { it.substringBefore("/" + "/") }
    }

    private companion object {
        private const val MAX_WALK_UP = 6

        /** 扫描面下限。 */
        private const val MIN_SOURCES = 200

        /** 转发到共享扫描器的 `codeOnly` 声明数。 */
        private const val MIN_CODE_ONLY_SITES = 15

        private val SKIPPED_DIRS = setOf("build", ".git", ".gradle", ".idea", "libs", ".codepicture")

        private const val SHARED_IMPL = "testing/src/test/kotlin/com/ccmonitor/mobile/testing/KotlinSourceScanner.kt"

        private const val SCANNER_TYPE = "KotlinSourceScanner"

        private const val CODE_ONLY_DECL = "fun codeOnly(text: String)"

        /**
         * 共享实现的公开入口：各自只许声明一次。
         *
         * 同样逐字拼、不写成连续字面（同 [brokenRegexSource] 的理由）。
         */
        private val IMPLEMENTATION_MARKERS =
            listOf(
                "fun " + "codeOnlyKeepingLiterals(",
                "fun " + "codeOnlyDroppingLiterals(",
                // 三支「抠字面」入口：同样各自只许声明一次。
                "fun " + "literalsOf(",
                "fun " + "literalCorpusOf(",
                "fun " + "literalsInCallsTo(",
            )

        /**
         * 扫描面的三个见证人：`:app` 的 test、`core-*` 的 test、仓根共享目录。
         *
         * 它们少一个，就说明这条判据在那一类住址上是瞎的。
         */
        private val SURFACE_WITNESSES =
            listOf(
                SHARED_IMPL,
                "app/src/test/kotlin/com/ccmonitor/mobile/ssh/HostConnectorGatewayTest.kt",
                "core-claude/src/test/kotlin/com/ccmonitor/mobile/core/claude/CoreClaudeKnowsNoTmuxTest.kt",
            )

        /** ⑤⑥的扫描面下限。 */
        private const val MIN_TEST_SOURCES = 100

        /**
         * ⑥ 的「逐字符走一遍」特征：`whi`+`le` 后面跟一个「下标 < 某某点 length」的条件。
         *
         * 本式写成正则源之后不匹配自己（源码里 `whi`+`le` 之后紧跟的是一个反斜杠，
         * 而本式要求紧跟空白或左括号）。头注里也刻意不把那个形写成一串连续的字面。
         */
        private val CHAR_BY_CHAR_LOOP = Regex("""while\s*\(\s*\w+\s*<\s*[\w.]*\.length""")

        /**
         * 全仓唯一许可的那一处 `while` 式词法扫描。
         *
         * 往这里加一条之前先回答：共享实现里为什么没有所需的那一支？
         * 真有非共享不可的理由，连理由一起写进来并登记成欠账（别只改表）。
         */
        private val HAND_ROLLED_LEXERS = setOf(SHARED_IMPL)

        /**
         * 全仓唯一许可的手搓剥注释器：共享实现自己。
         *
         * 往这里加一条 = 又长出一份独立的手写扫描器。加之前先回答：
         * 共享实现里为什么没有所需的那一支？真有非共享不可的理由，连理由一起写进来并登记成欠账。
         */
        private val HAND_ROLLED_SCANNERS =
            setOf(
                // 共享实现本身：它就是那一份，必须命中（命不中说明探测器瞎了）。
                SHARED_IMPL,
            )

        private const val ANCHOR = "app/src/main/kotlin/com/ccmonitor/mobile/ui/sftp/SftpScreen.kt"
        private const val ANCHOR_MARK = "vm.connect("
    }
}

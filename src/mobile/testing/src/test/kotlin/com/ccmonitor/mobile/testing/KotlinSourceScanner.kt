package com.ccmonitor.mobile.testing

/**
 * 全仓唯一一份「把 Kotlin 源码剥成可扫文本」的词法扫描器，给扫源码文本的测试判据共用。
 *
 * 单遍词法，先认字面再认注释，所以字符串里的「星-斜杠」和两个斜杠不会被当成注释。
 * 用正则剥注释（非贪婪匹配块注释、按行砍行注释）做不到这一点：字面里的这两种形状会让
 * 后面的代码被整段吃掉，依赖「零命中」的判据就静默失效。
 *
 * 两支剥注释的入口语义不同，调用方自己选：
 * - [codeOnlyKeepingLiterals]：字面逐字留着。载荷住在字面里的判据用（命令串片段、路径前缀、上屏文案）。
 * - [codeOnlyDroppingLiterals]：字面整段删掉。只问「代码里有没有这个调用/声明」的判据用。
 *
 * 注意：选错不会编译失败，只会静默全绿。拿删字面那支去扫字面载荷，什么都扫不到。
 *
 * 另有三支抠字面的入口，共用同一个词法主循环 [lex]：
 * - [literalsOf]：全部字符串字面，带行号与原文下标；
 * - [literalCorpusOf]：字面内容用 [LITERAL_SEPARATOR] 拼成一份可 `indexOf` 的语料；
 * - [literalsInCallsTo]：某个名字的调用实参里的字面（例如 `Text(…)`）。
 *
 * 看不见的东西：
 * - 反射、拼接出来的名字、`Class.forName`：只答「这段文本在不在」，不答运行时会发生什么；
 * - 死分支里的调用点：不走控制流；
 * - 语法树、作用域、类型：一概没有，防手滑不防拼接；
 * - 非 Kotlin 的注释语法（`#`、`<!-- -->`）：只认行注释、块注释（嵌套）、KDoc；
 * - 跨字面拼出来的载荷（`"--list" + "-projects"`）：隔断符保证拼不出假命中，也意味着真这么写就看不见。
 *
 * 容易误会的边界：
 * - 模板插值里嵌套的字面（`"a ${'$'}{q("b")} c"`）会让引号配对「翻面」：`"a ${'$'}{q("` 被当成一段字面、
 *   `b` 被当成代码、`")} c"` 又是一段字面。两支都如此；模板插值里的括号也因此会露进骨架。
 * - 原始串按最近的三引号收尾，`""""`（内容以引号结尾）这种贴边写法会认错。
 * - 未闭合的单行字面遇换行即止，换行本身留着，不让一个落单的引号吃掉整个文件。
 * - 字符字面 `'x'` 会被认出来（它可能装着一个引号），但不进字面语料。
 * - [literalsInCallsTo] 只按「名字 + 紧跟左括号」认调用，不看类型：`Toast.makeText(` 不算
 *   （前一个字符是标识符字符），`BasicText(` 与 `Foo.Text(` 也不算。
 *
 * 这份文件住在仓根 `testing/src/test/kotlin`，`:app` 与 `:core-claude` 的 test 源集各用一行 `srcDir` 接进来。
 * 它的自检 [KotlinSourceScannerTest] 住在同一目录，所以两个模块各跑一遍；哪边接线掉了，那边当场编译不过。
 */
object KotlinSourceScanner {
    /** 剥掉行注释、块注释与 KDoc，字符串和字符字面逐字留着。 */
    fun codeOnlyKeepingLiterals(text: String): String = stripComments(text, keepLiterals = true)

    /**
     * 剥注释，并把字符串和字符字面整段删掉，只剩代码。
     *
     * 给问「代码里有没有这个调用/声明」的判据用：字面里提一嘴 `manager.connect(` 不是调用。
     */
    fun codeOnlyDroppingLiterals(text: String): String = stripComments(text, keepLiterals = false)

    /**
     * 一条字符串字面在源码里的位置与内容。
     *
     * @param line 起始引号所在行（1 起）。
     * @param body 字面的内容，不含两头的引号；原始串（三引号）同理。
     *   未闭合的字面（遇换行即止 / 文件到头）取到止处为止。
     * @param from 起始引号在原文里的下标。
     * @param toExclusive 收尾之后的下标（开区间右端）。
     */
    data class Literal(
        val line: Int,
        val body: String,
        val from: Int,
        val toExclusive: Int,
    )

    /**
     * [literalsInCallsTo] 的结果：认出的调用处数，以及这些调用实参里的字面。
     *
     * 注意：调用方要拿 `callSites` 做前提自检。零处调用加零条字面，和「真的一条违规都没有」长得一样。
     */
    data class CallLiterals(
        val callSites: Int,
        val literals: List<Literal>,
    )

    /** 字面语料的隔断符：锚点里不可能出现的字符。写成空格会让相邻两条字面粘出假命中。 */
    const val LITERAL_SEPARATOR: Char = '\u0000'

    /**
     * 一份源码里的全部字符串字面（注释里的不算、字符字面不算），带行号与原文下标。
     *
     * 给「载荷就住在字面里」的判据用：命令串片段、路径前缀、会话名、上屏文案。
     */
    fun literalsOf(text: String): List<Literal> {
        val out = ArrayList<Literal>()
        var line = 1
        var seen = 0
        lex(text) { kind, from, to ->
            line += countNewlines(text, seen, from)
            seen = from
            if (kind == Lexeme.STRING || kind == Lexeme.RAW_STRING) {
                out += Literal(line, bodyOf(text, kind, from, to), from, to)
            }
        }
        return out
    }

    /**
     * [literalsOf] 的内容用 [LITERAL_SEPARATOR] 拼成一份可 `indexOf` 的语料，每条后面跟一个隔断符。
     *
     * 给问「这个 shell 片段 / 后端子命令在生产代码的字面里出现了几次」的判据用。
     */
    fun literalCorpusOf(text: String): String {
        val out = StringBuilder()
        for (l in literalsOf(text)) out.append(l.body).append(LITERAL_SEPARATOR)
        return out.toString()
    }

    /**
     * 挑出名为 [callee] 的调用的实参括号里的字面。
     *
     * 在骨架（注释与字面内容都换成空格、长度与原文对齐的视图）上找 `callee(`，
     * 前一个字符是标识符字符或点号的不算（挡掉 `BasicText(` 与 `Foo.Text(`），
     * 然后括号配平走到配对的 `)`，把落在这段里的字面收走。
     * 字面内容在骨架里是空格，所以字面里的 `(`、`)`、`callee(` 都骗不了它。
     */
    fun literalsInCallsTo(
        text: String,
        callee: String,
    ): CallLiterals {
        val mark = "$callee("
        val skeleton = codeSkeleton(text)
        val literals = literalsOf(text)
        val found = ArrayList<Literal>()
        var sites = 0
        var i = 0
        while (true) {
            val at = skeleton.indexOf(mark, i)
            if (at < 0) break
            i = at + mark.length
            val prev = if (at == 0) ' ' else skeleton[at - 1]
            if (prev.isLetterOrDigit() || prev == '_' || prev == '.') continue
            sites++
            var depth = 1
            var j = i
            while (j < skeleton.length && depth > 0) {
                when (skeleton[j]) {
                    '(' -> depth++
                    ')' -> depth--
                    else -> Unit
                }
                j++
            }
            for (l in literals) if (l.from in at until j) found += l
        }
        return CallLiterals(sites, found)
    }

    // ---- 唯一的词法主循环 -------------------------------------------------------

    /** [lex] 吐出来的词素种类。 */
    private enum class Lexeme {
        /** 既不是注释也不是字面的那些字符（成段吐出）。 */
        CODE,

        /** 行注释，不含收尾的换行（换行作为代码字符另行吐出，行号不漂）。 */
        LINE_COMMENT,

        /** 块注释（嵌套按 Kotlin 语义计数），含两头的记号。 */
        BLOCK_COMMENT,

        /** 开了没关的块注释，一路吃到文件尾。 */
        UNCLOSED_BLOCK_COMMENT,

        /** 普通串 `"…"`（`\` 转义下一个字符，遇换行即止）。 */
        STRING,

        /** 原始串 `"""…"""`。 */
        RAW_STRING,

        /** 字符字面 `'x'` —— 它可能装着一个引号，必须认出来，但不进字面语料。 */
        CHAR,
    }

    /**
     * 词法主循环：单遍、不回溯、不用正则。
     *
     * 字面在注释之前认：字面里的「星-斜杠」和两个斜杠因此骗不了它。
     * 吐出来的词素首尾相接、不重不漏地盖满 `[0, text.length)`，[codeSkeleton] 的等长押在这条性质上。
     */
    @Suppress("NestedBlockDepth", "CyclomaticComplexMethod")
    private fun lex(
        text: String,
        emit: (Lexeme, Int, Int) -> Unit,
    ) {
        var i = 0
        var depth = 0
        var runStart = 0
        var blockStart = 0

        fun flushCode(upTo: Int) {
            if (upTo > runStart) emit(Lexeme.CODE, runStart, upTo)
            runStart = upTo
        }

        while (i < text.length) {
            val two = if (i + 1 < text.length) text.substring(i, i + 2) else ""
            when {
                depth > 0 && two == BLOCK_OPEN -> {
                    depth++
                    i += 2
                }
                depth > 0 && two == BLOCK_CLOSE -> {
                    depth--
                    i += 2
                    if (depth == 0) {
                        emit(Lexeme.BLOCK_COMMENT, blockStart, i)
                        runStart = i
                    }
                }
                depth > 0 -> i++
                two == BLOCK_OPEN -> {
                    flushCode(i)
                    blockStart = i
                    depth++
                    i += 2
                }
                two == LINE_OPEN -> {
                    flushCode(i)
                    val start = i
                    while (i < text.length && text[i] != '\n') i++
                    emit(Lexeme.LINE_COMMENT, start, i)
                    runStart = i
                }
                text.startsWith(RAW_QUOTE, i) -> {
                    flushCode(i)
                    val end = text.indexOf(RAW_QUOTE, i + RAW_QUOTE.length)
                    val stop = if (end < 0) text.length else end + RAW_QUOTE.length
                    emit(Lexeme.RAW_STRING, i, stop)
                    i = stop
                    runStart = i
                }
                text[i] == '"' || text[i] == '\'' -> {
                    flushCode(i)
                    val char = text[i] == '\''
                    val stop = endOfSingleLineLiteral(text, i)
                    emit(if (char) Lexeme.CHAR else Lexeme.STRING, i, stop)
                    i = stop
                    runStart = i
                }
                else -> i++
            }
        }
        if (depth > 0) emit(Lexeme.UNCLOSED_BLOCK_COMMENT, blockStart, text.length) else flushCode(i)
    }

    // ---- 各入口对词素的处理 ----------------------------------------------------

    /**
     * 剥注释：块注释用一个空格占位（不把两边的 token 粘成一个），
     * 内部的换行逐个留下（行号不漂，判据的报错信息才指得准）。
     */
    private fun stripComments(
        text: String,
        keepLiterals: Boolean,
    ): String {
        val out = StringBuilder(text.length)
        lex(text) { kind, from, to ->
            when (kind) {
                Lexeme.CODE -> out.append(text, from, to)
                Lexeme.LINE_COMMENT -> Unit
                Lexeme.BLOCK_COMMENT -> {
                    appendNewlinesOf(out, text, from, to)
                    out.append(' ')
                }
                Lexeme.UNCLOSED_BLOCK_COMMENT -> appendNewlinesOf(out, text, from, to)
                else -> if (keepLiterals) out.append(text, from, to) else appendNewlinesOf(out, text, from, to)
            }
        }
        return out.toString()
    }

    /**
     * 与原文等长的骨架：注释与字面的内容逐字符换成空格（换行留着），代码原样。
     *
     * 等长是它的用处：[literalsInCallsTo] 在骨架上做括号配平，再拿 [Literal.from] 这个原文下标去筛。
     */
    private fun codeSkeleton(text: String): String {
        val out = StringBuilder(text.length)
        lex(text) { kind, from, to ->
            if (kind == Lexeme.CODE) {
                out.append(text, from, to)
            } else {
                for (k in from until to) out.append(if (text[k] == '\n') '\n' else ' ')
            }
        }
        return out.toString()
    }

    /** 一条字面的内容：闭合的去掉两头的记号，没闭合的取到止处为止。 */
    private fun bodyOf(
        text: String,
        kind: Lexeme,
        from: Int,
        to: Int,
    ): String {
        val delim = if (kind == Lexeme.RAW_STRING) RAW_QUOTE else text[from].toString()
        val q = delim.length
        val closed = to - from >= 2 * q && text.startsWith(delim, to - q)
        return text.substring(from + q, if (closed) to - q else to)
    }

    private fun appendNewlinesOf(
        out: StringBuilder,
        text: String,
        from: Int,
        to: Int,
    ) {
        repeat(countNewlines(text, from, to)) { out.append('\n') }
    }

    private fun countNewlines(
        text: String,
        from: Int,
        to: Int,
    ): Int {
        var n = 0
        for (i in from until to) if (text[i] == '\n') n++
        return n
    }

    /**
     * 一条单行字符串/字符字面的收尾下标（开区间右端）。
     *
     * 遇换行即止且不吞掉那个换行：Kotlin 的转义字符串不跨行，一个落单的引号不该吃掉整个文件。
     */
    private fun endOfSingleLineLiteral(
        text: String,
        start: Int,
    ): Int {
        val quote = text[start]
        var i = start + 1
        while (i < text.length && text[i] != quote && text[i] != '\n') {
            if (text[i] == '\\') i++
            i++
        }
        return if (i < text.length && text[i] == '\n') i else minOf(i + 1, text.length)
    }

    private const val BLOCK_OPEN = "/*"
    private const val BLOCK_CLOSE = "*/"
    private const val LINE_OPEN = "//"
    private const val RAW_QUOTE = "\"\"\""
}

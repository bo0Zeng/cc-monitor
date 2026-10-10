package com.ccmonitor.mobile.core.remote

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * [shellQuote] 是全仓唯一的 POSIX 单引号转义，这里直接表驱动地测它。
 * 结果对任意输入都是一个 shell 词：不会被 word-split、不触发 glob / 变量展开 / 命令替换。
 */
class ShellQuoteTest {
    @Test
    fun quotesPlainWord() = assertEquals("'main'", shellQuote("main"))

    @Test
    fun quotesEmptyStringToAnEmptyWord() =
        // 空串必须变成 `''` 而不是空字符串，否则那个参数会整个消失、命令的参数个数就变了。
        assertEquals("''", shellQuote(""))

    @Test
    fun escapesSingleQuote() = assertEquals("""'it'\''s'""", shellQuote("it's"))

    @Test
    fun escapesConsecutiveSingleQuotes() =
        assertEquals("""'a'\'''\''b'""", shellQuote("a''b"))

    @Test
    fun quotesOnlyASingleQuote() = assertEquals("""''\'''""", shellQuote("'"))

    @Test
    fun handlesSpaces() = assertEquals("'a b'", shellQuote("a b"))

    /** 不 quote 就会变成变量展开、命令替换、glob 的输入；单引号内它们全是字面量。 */
    @Test
    fun neutralizesShellMetacharacters() {
        listOf(
            "\$HOME" to "'\$HOME'",
            "`whoami`" to "'`whoami`'",
            "\$(rm -rf /)" to "'\$(rm -rf /)'",
            "*" to "'*'",
            "a;b" to "'a;b'",
            "a|b" to "'a|b'",
            "a&b" to "'a&b'",
            "a>b" to "'a>b'",
            "~/x" to "'~/x'",
        ).forEach { (input, expected) ->
            assertEquals("输入 $input 未被正确中和", expected, shellQuote(input))
        }
    }

    /** 换行在单引号内是字面换行 —— 合法且安全（`$'...'` 才需要特殊处理，我们不用那个方言）。 */
    @Test
    fun keepsNewlineLiteralInsideQuotes() = assertEquals("'a\nb'", shellQuote("a\nb"))

    /** 中文/emoji 等非 ASCII 原样保留 —— 不做任何编码转换。 */
    @Test
    fun leavesNonAsciiUntouched() = assertEquals("'项目/文档 🚀'", shellQuote("项目/文档 🚀"))

    /**
     * 结构不变量：输出必然以 `'` 开头、以 `'` 结尾，
     * 且内部出现的每个 `'` 都被 `'\''` 包裹（输出里没有未转义的裸单引号）。
     */
    @Test
    fun outputIsAlwaysExactlyOneShellWord() {
        listOf("", "a", "'", "''", "a'b'c", "\$x", "a b", "\n", "项目").forEach { input ->
            val out = shellQuote(input)
            assertTrue("$input → $out 应以单引号开头", out.startsWith("'"))
            assertTrue("$input → $out 应以单引号结尾", out.endsWith("'"))
            // 去掉所有 `'\''` 序列后，剩余部分里不该再有单引号（除首尾那两个）
            val stripped = out.replace("""'\''""", "")
            assertEquals(
                "$input → $out 内部残留了未转义的单引号",
                2,
                stripped.count { it == '\'' },
            )
        }
    }
}

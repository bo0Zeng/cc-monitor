package com.ccmonitor.mobile.link

import com.ccmonitor.mobile.core.wire.gen.Commands
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 命令名只住生成的那一份（`Commands.<命令>.NAME`，照后端命令表生成）：app 生产段不另写一串等于某条命令名的字面。
 * 冒烟按它点的测试标签（`object …Tags` 里那几串）不是命令名，只是碰巧同形，不算。
 */
class CommandNameLiteralsTest {
    @Test
    fun `app 生产段不手写命令名`() {
        val names = Commands.ALL.keys
        assertTrue("前提：生成的命令表读到了", names.size > 100)
        val root = File("src/main/kotlin").takeIf { it.isDirectory } ?: File("app/src/main/kotlin")
        val files = root.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
        assertTrue("前提：扫到了 app 生产段（${root.absolutePath}）", files.size > 50)
        val hits =
            files.flatMap { f ->
                val text = f.readText()
                val tags = tagsBodies(text)
                KotlinSourceScanner
                    .literalsOf(text)
                    .filter { l -> l.body in names && tags.none { l.from in it } }
                    .map { "${f.name}:${it.line} \"${it.body}\"" }
            }
        assertEquals("这几处手写了命令名（改用 Commands.<命令>.NAME）", emptyList<String>(), hits)
    }

    @Test
    fun `测试标签只放过 Tags 对象自己那一对花括号里的`() {
        val text = "object FooTags {\n    const val A = \"files-ls\"\n}\nval b = \"files-ls\"\n"
        val tags = tagsBodies(text)
        val at = KotlinSourceScanner.literalsOf(text).map { l -> tags.any { l.from in it } }
        assertEquals(listOf(true, false), at)
    }

    /** 名字以 `Tags` 结尾的 object 的 `{…}`（花括号配平；这几个对象只放 `const val`，字面里不带花括号）。 */
    private fun tagsBodies(text: String): List<IntRange> = TAGS_HEAD.findAll(text).map { bodyFrom(text, it.range.last) }.toList()

    /** 从 [open] 那个 `{` 起配平到它的 `}`。 */
    private fun bodyFrom(
        text: String,
        open: Int,
    ): IntRange {
        var depth = 0
        for (i in open until text.length) {
            if (text[i] == '{') depth++
            if (text[i] == '}' && --depth == 0) return open..i
        }
        return open until text.length
    }

    private companion object {
        val TAGS_HEAD = Regex("""\bobject\s+\w+Tags\s*\{""")
    }
}

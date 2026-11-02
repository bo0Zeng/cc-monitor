package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.claude.link.Json
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.Doc
import org.junit.Assert.assertEquals
import org.junit.Test
import java.io.File

/**
 * 助手正文的块树上取字（[codeBlocks] · [plainText]）：阅读面「复制代码块」与会话内搜索都从树上取，不看 Markdown 原文。
 * 树取自后端的金样（`tests/__fixtures__/md-core.golden.json`，`md_core::parse` 每种写法一份）。
 */
class MarkdownTest {
    private val golden: Map<String, Doc> by lazy {
        val root =
            generateSequence(File("").absoluteFile) { it.parentFile }
                .first { File(it, "tests/__fixtures__").isDirectory && File(it, "src/backend/lib.rs").isFile }

        @Suppress("UNCHECKED_CAST")
        val cases = Json.value(File(root, "tests/__fixtures__/md-core.golden.json").readText()) as List<Map<String, Any?>>
        cases.associate { (it["name"] as String) to requireNotNull(Wire.bind<Doc>(it["doc"])) { "金样的树装不进线上类：${it["name"]}" } }
    }

    private fun doc(json: String): Doc = requireNotNull(Wire.bind<Doc>(Json.value(json))) { "树装不进线上类：$json" }

    @Test
    fun `两种围栏的代码块都照树上的原文复制`() {
        assertEquals(listOf("ls -la ~/a"), golden.getValue("bash 代码块").codeBlocks())
        assertEquals(listOf("fn main() {}"), golden.getValue("~~~ 围栏").codeBlocks())
    }

    @Test
    fun `列表 · 引用里的代码块照先序都在；切开的后几块接回前一块`() {
        val d =
            doc(
                """
                {"blocks":[
                  {"t":"list","start":null,"tight":true,"items":[{"task":null,"blocks":[{"t":"code","lang":"sh","x":"a","cont":false}]}]},
                  {"t":"quote","blocks":[{"t":"code","lang":null,"x":"b1","cont":false}]},
                  {"t":"code","lang":null,"x":"b2","cont":true},
                  {"t":"p","k":["正文"]}
                ],"segs":[2]}
                """.trimIndent(),
            )
        assertEquals(listOf("a", "b1\nb2"), d.codeBlocks())
    }

    @Test
    fun `没有代码块就没得复制`() {
        assertEquals(emptyList<String>(), golden.getValue("尖括号词").codeBlocks())
    }

    @Test
    fun `搜索的字照树上屏的字：尖括号是字，链接取字，块之间换行`() {
        assertEquals("把 <Composer> 和 Vec<String> 换掉", golden.getValue("尖括号词").plainText())
        assertEquals("见 https://example.com/x 与 www.example.org", golden.getValue("裸网址与 www").plainText())
        assertEquals("引一句", golden.getValue("引用与分隔线").plainText())
        assertEquals("名 值\na | b 2", golden.getValue("表格里的转义竖线").plainText())
        assertEquals("ls -la ~/a", golden.getValue("bash 代码块").plainText())
        assertEquals("待办\n已做", golden.getValue("任务列表").plainText())
    }
}

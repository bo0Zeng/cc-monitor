package com.ccmonitor.mobile.ui.claude

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.BaselineShift
import androidx.compose.ui.text.style.TextDecoration
import com.ccmonitor.mobile.core.wire.gen.AlertKind
import com.ccmonitor.mobile.core.wire.gen.Doc
import com.ccmonitor.mobile.core.wire.gen.MdAlign
import com.ccmonitor.mobile.core.wire.gen.MdBlock
import com.ccmonitor.mobile.core.wire.gen.MdItem
import com.ccmonitor.mobile.core.wire.gen.SegHl
import com.squareup.moshi.JsonReader
import okio.Buffer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 排版器只照树排（[layoutSeg]）：树取自后端的金样（`tests/__fixtures__/md-core.golden.json`，`md_core::parse` 每种写法一份），
 * 高亮取同一份金样里那一段的 `hl`。字一个不添一个不丢（树里的字原样上屏，尖括号是字），样式只照节点的种类与高亮三元组。
 */
class MdDocTest {
    /** 测试用的样式：每样一种好认的值（色值只在测试里写，生产码走令牌）。 */
    private val style =
        MdStyle(
            code = SpanStyle(background = Color(0xFF000001)),
            link = TextLinkStyles(SpanStyle(color = Color(0xFF000002))),
            faint = SpanStyle(color = Color(0xFF000003)),
            fnRef = SpanStyle(baselineShift = BaselineShift.Superscript),
            hl = List(13) { SpanStyle(color = Color(0xFF000100 + it)) },
        )

    private class Case(
        val name: String,
        val doc: Doc,
        val hl: SegHl?,
    )

    private val golden: List<Case> by lazy {
        val root =
            generateSequence(File("").absoluteFile) { it.parentFile }
                .first { File(it, "tests/__fixtures__").isDirectory && File(it, "src/backend/lib.rs").isFile }
        val r = JsonReader.of(Buffer().writeUtf8(File(root, "tests/__fixtures__/md-core.golden.json").readText()))
        val out = mutableListOf<Case>()
        r.beginArray()
        while (r.hasNext()) {
            var name = ""
            var doc: Doc? = null
            var hl: SegHl? = null
            r.beginObject()
            while (r.hasNext()) {
                when (r.nextName()) {
                    "name" -> name = r.nextString()
                    "doc" -> doc = Doc.ADAPTER.fromJson(r)
                    "hl" -> {
                        val rows = mutableListOf<List<Long>>()
                        r.beginArray()
                        while (r.hasNext()) {
                            val row = mutableListOf<Long>()
                            r.beginArray()
                            while (r.hasNext()) row += r.nextLong()
                            r.endArray()
                            rows += row
                        }
                        r.endArray()
                        hl = SegHl(rows)
                    }
                    else -> r.skipValue()
                }
            }
            r.endObject()
            out += Case(name, doc!!, hl)
        }
        r.endArray()
        assertTrue("金样一份都没读出来（判据自己失效了）", out.size >= 20)
        out
    }

    private fun case(name: String): Case = golden.first { it.name == name }

    private fun one(name: String): MdPiece = layoutSeg(case(name).doc, 0, case(name).hl, style).single()

    private fun text(name: String): AnnotatedString = (one(name) as MdPiece.Text).text

    private fun AnnotatedString.spansOf(s: SpanStyle): List<IntRange> =
        spanStyles.filter { it.item == s }.map { it.start until it.end }

    @Test
    fun everyGoldenTreeLaysOutBlockForBlock() {
        for (c in golden) {
            val pieces = layoutSeg(c.doc, 0, c.hl, style)
            assertEquals(c.name, c.doc.blocks.size, pieces.size)
            c.doc.blocks.zip(pieces).forEach { (b, p) ->
                val ok =
                    when (b) {
                        is MdBlock.P -> p is MdPiece.Text && p.level == 0
                        is MdBlock.H -> p is MdPiece.Text && p.level == b.level
                        is MdBlock.Code -> p is MdPiece.Code && p.text.text == b.x && p.lang == b.lang
                        is MdBlock.Math -> p is MdPiece.Math && p.tex == b.tex
                        is MdBlock.List -> p is MdPiece.Bullets && p.items.size == b.items.size
                        is MdBlock.Quote -> p is MdPiece.Quote && p.pieces.size == b.blocks.size
                        is MdBlock.Alert -> p is MdPiece.Alert && p.kind == b.kind
                        is MdBlock.Table -> p is MdPiece.Table && p.head.size == b.head.size
                        MdBlock.Rule -> p == MdPiece.Rule
                        is MdBlock.Notes -> p is MdPiece.Notes && p.items.map { it.n } == b.items.map { it.n }
                    }
                assertTrue("${c.name}：${b.t} 排成了 $p", ok)
            }
        }
    }

    @Test
    fun textIsTheTreesCharactersVerbatim() {
        assertEquals("把 <Composer> 和 Vec<String> 换掉", text("尖括号词").text)
        assertEquals("花了 \$5 到 \$10", text("钱数不是公式").text)
        assertEquals("~/a~/b 与 删", text("单波浪号与删除线").text)
        assertEquals("上行\n中行\n下行", text("两种硬换行").text)
        assertEquals("下 <kbd>Ctrl</kbd>", text("br 与 kbd").text.substringAfter('\n'))
    }

    @Test
    fun inlineNodesBecomeSpans() {
        val del = text("单波浪号与删除线")
        assertEquals(listOf(8 until 9), del.spansOf(SpanStyle(textDecoration = TextDecoration.LineThrough)))

        val bold = text("中文旁的加粗")
        assertEquals(listOf(2 until 6), bold.spansOf(SpanStyle(fontWeight = FontWeight.Bold)))

        val h = one("标题") as MdPiece.Text
        assertEquals(2, h.level)
        assertEquals(listOf(3 until 5), h.text.spansOf(SpanStyle(fontWeight = FontWeight.Bold)))

        val fn = text("脚注")
        assertEquals("正文1", fn.text)
        assertEquals(listOf(2 until 3), fn.spansOf(style.fnRef))

        val table = one("表格里的转义竖线") as MdPiece.Table
        assertEquals(listOf(MdAlign.LEFT, MdAlign.RIGHT), table.align)
        assertEquals(listOf("名", "值"), table.head.map { it.text })
        assertEquals("a | b", table.rows[0][0].text)
        assertEquals(listOf(0 until 5), table.rows[0][0].spansOf(style.code))
    }

    @Test
    fun linksCarryTheTreesHrefOnly() {
        val t = text("裸网址与 www")
        val links = t.getLinkAnnotations(0, t.length)
        assertEquals(listOf("https://example.com/x", "http://www.example.org"), links.map { (it.item as LinkAnnotation.Url).url })
        assertEquals(listOf(style.link, style.link), links.map { (it.item as LinkAnnotation.Url).styles })
        assertEquals("www.example.org", t.text.substring(links[1].start, links[1].end))
    }

    @Test
    fun inlineMathIsAnInlineSlotNamingItsTex() {
        val t = text("行内公式两种写法")
        assertEquals(listOf("E=mc^2", "a+b"), mathTexOf(t))
        assertTrue(t.text.startsWith("行内 "))
    }

    @Test
    fun codeIsColouredByTheTriplesOnly() {
        // `ls -la ~/a` ⇐ [0,2,函数][0,4,变量][1,1,变量]
        val code = one("bash 代码块") as MdPiece.Code
        assertEquals("bash", code.lang)
        assertEquals(listOf(0 until 2), code.text.spansOf(style.hl[4]))
        assertEquals(listOf(2 until 6, 7 until 8), code.text.spansOf(style.hl[7]))
        assertEquals(3, code.text.spanStyles.size)

        // 不给高亮 ⇒ 纯字。
        val plain = layoutSeg(case("bash 代码块").doc, 0, null, style).single() as MdPiece.Code
        assertEquals("ls -la ~/a", plain.text.text)
        assertTrue(plain.text.spanStyles.isEmpty())
    }

    @Test
    fun badTriplesClampAndUnknownClassesStayPlain() {
        val x = "abcdef"
        // 越过串尾的截在串尾；认不出的类（99）当没着色；不成三的尾巴丢掉。
        val a = highlighted(x, listOf(1L, 2L, 0L, 0L, 1L, 99L, 1L, 100L, 1L, 7L), style)
        assertEquals(x, a.text)
        assertEquals(listOf(1 until 3), a.spansOf(style.hl[0]))
        assertEquals(listOf(4 until 6), a.spansOf(style.hl[1]))
        assertEquals(2, a.spanStyles.size)
    }

    @Test
    fun listsKeepTheTreesNumberingAndTasks() {
        val ol = one("有序列表起号") as MdPiece.Bullets
        assertEquals(listOf(3, 4), ol.items.map { it.number })

        val tasks = one("任务列表") as MdPiece.Bullets
        assertEquals(listOf(false, true), tasks.items.map { it.task })
        assertEquals(listOf(null, null), tasks.items.map { it.number })

        val alert = one("提示块") as MdPiece.Alert
        assertEquals(AlertKind.WARNING, alert.kind)
        assertEquals("小心", (alert.pieces.single() as MdPiece.Text).text.text)
    }

    @Test
    fun highlightGoesToCodeBlocksInPreorderWithinTheSegment() {
        val bash = case("bash 代码块")
        val py = case("没闭合的代码块")
        val bashCode = bash.doc.blocks.single()
        val pyCode = py.doc.blocks.single()
        // 先序：列表里的那块是第 0 块，顶层那块是第 1 块。
        val doc = Doc(listOf(MdBlock.List(null, true, listOf(MdItem(null, listOf(bashCode)))), pyCode), emptyList())
        val pieces = layoutSeg(doc, 0, SegHl(listOf(bash.hl!!.hl[0], py.hl!!.hl[0])), style)
        val nested = ((pieces[0] as MdPiece.Bullets).items[0].pieces[0]) as MdPiece.Code
        assertEquals(listOf(0 until 2), nested.text.spansOf(style.hl[4]))
        val top = pieces[1] as MdPiece.Code
        // `print(1)` ⇐ [0,5,函数][1,1,数]
        assertEquals(listOf(0 until 5), top.text.spansOf(style.hl[4]))
        assertEquals(listOf(6 until 7), top.text.spansOf(style.hl[3]))
    }

    @Test
    fun onlySegmentZeroIsBuiltUntilAskedForMore() {
        val a = case("标题").doc.blocks
        val b = case("引用与分隔线").doc.blocks
        val c = case("bash 代码块").doc.blocks
        val doc = Doc(a + b + c, listOf(a.size, a.size + b.size))
        assertEquals(0 until 1, segRange(doc, 0))
        assertEquals(1 until 3, segRange(doc, 1))
        assertEquals(3 until 4, segRange(doc, 2))

        val st = MdDocState(doc, null, style)
        assertEquals(3, st.segCount)
        assertEquals(1, st.built)
        assertEquals(listOf(1), st.pieces.map { it.size })
        assertTrue(st.buildNext(null))
        assertTrue(st.buildNext(case("bash 代码块").hl))
        assertEquals(listOf(1, 2, 1), st.pieces.map { it.size })
        assertEquals(3, (st.pieces[2].single() as MdPiece.Code).text.spanStyles.size)
        assertFalse(st.buildNext(null))
        assertEquals(3, st.built)

        val single = MdDocState(case("标题").doc, null, style)
        assertEquals(1, single.segCount)
        assertFalse(single.buildNext(null))
    }

    @Test
    fun italicAndImageAltAreText() {
        val doc =
            Doc(
                listOf(
                    MdBlock.P(
                        listOf(
                            com.ccmonitor.mobile.core.wire.gen.Inl.InlNode(
                                com.ccmonitor.mobile.core.wire.gen.InlNode.I(listOf(com.ccmonitor.mobile.core.wire.gen.Inl.Text("斜"))),
                            ),
                            com.ccmonitor.mobile.core.wire.gen.Inl.InlNode(com.ccmonitor.mobile.core.wire.gen.InlNode.Img("a.png", "图注")),
                        ),
                    ),
                ),
                emptyList(),
            )
        val t = (layoutSeg(doc, 0, null, style).single() as MdPiece.Text).text
        assertEquals("斜图注", t.text)
        assertEquals(listOf(0 until 1), t.spansOf(SpanStyle(fontStyle = FontStyle.Italic)))
        assertEquals(listOf(1 until 3), t.spansOf(style.faint))
        assertNull(t.getLinkAnnotations(0, t.length).firstOrNull())
    }
}

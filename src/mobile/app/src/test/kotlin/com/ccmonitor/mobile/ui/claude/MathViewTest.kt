package com.ccmonitor.mobile.ui.claude

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import com.ccmonitor.mobile.core.wire.gen.Doc
import com.ccmonitor.mobile.core.wire.gen.Inl
import com.ccmonitor.mobile.core.wire.gen.InlNode
import com.ccmonitor.mobile.core.wire.gen.MathBox
import com.ccmonitor.mobile.core.wire.gen.MathItem
import com.ccmonitor.mobile.core.wire.gen.MdBlock
import com.squareup.moshi.JsonReader
import okio.Buffer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 公式照核心排好的画单画（`MathView.kt`）：金样里的公式都带非空画单、排进块里 · 行内槽里不丢；槽的大小 ＝ 画单 × 字号；
 * 每种字体都对得上 `src/shared/fonts/katex/` 里的那一份（手机构建时铺进 assets）。
 */
class MathViewTest {
    private val style =
        MdStyle(
            code = SpanStyle(),
            link = TextLinkStyles(SpanStyle(color = Color(0xFF000002))),
            faint = SpanStyle(),
            fnRef = SpanStyle(),
            hl = List(13) { SpanStyle() },
        )

    private val root: File =
        generateSequence(File("").absoluteFile) { it.parentFile }
            .first { File(it, "tests/__fixtures__").isDirectory && File(it, "src/backend/lib.rs").isFile }

    private val golden: Map<String, Doc> by lazy {
        val r = JsonReader.of(Buffer().writeUtf8(File(root, "tests/__fixtures__/md-core.golden.json").readText()))
        val out = mutableMapOf<String, Doc>()
        r.beginArray()
        while (r.hasNext()) {
            var name = ""
            var doc: Doc? = null
            r.beginObject()
            while (r.hasNext()) {
                when (r.nextName()) {
                    "name" -> name = r.nextString()
                    "doc" -> doc = Doc.ADAPTER.fromJson(r)
                    else -> r.skipValue()
                }
            }
            r.endObject()
            out[name] = doc!!
        }
        r.endArray()
        out
    }

    private fun glyphs(m: MathBox) = m.items.filterIsInstance<MathItem.G>()

    @Test
    fun everyGoldenBlockFormulaLaysOutWithANonEmptyBox() {
        val blocks = golden.values.flatMap { d -> layoutSeg(d, 0, null, style) }.filterIsInstance<MdPiece.Math>()
        assertTrue("金样里一条块级公式都没有（判据空转）", blocks.size >= 2)
        for (p in blocks) {
            val m = p.m
            assertNotNull("${p.tex} 没带画单", m)
            assertTrue("${p.tex} 的画单没有字形", glyphs(m!!).isNotEmpty())
            assertTrue("${p.tex} 的框是空的", m.w > 0 && m.h + m.d > 0)
        }
    }

    @Test
    fun inlineFormulasReachTheirSlotsWithTheirBoxes() {
        val doc = golden.getValue("行内公式两种写法")
        val boxes = inlineMathOf(doc.blocks)
        assertEquals(setOf("E=mc^2", "a+b"), boxes.keys)
        assertEquals(5, glyphs(boxes.getValue("E=mc^2")).size)
        // 排进段落那一串字里的槽，名字照旧是 TeX 原文（取画单就按它取）。
        val t = (layoutSeg(doc, 0, null, style).single() as MdPiece.Text).text
        assertEquals(boxes.keys.toList(), mathTexOf(t))
    }

    @Test
    fun inlineMathIsCollectedFromNestedBlocksAndSkipsMissingBoxes() {
        val box = MathBox(1000, 700, 0, listOf(MathItem.G("Math-Italic", 0x61, 0, 700, 1000)))
        val m = Inl.InlNode(InlNode.M(tex = "a", m = box))
        val none = Inl.InlNode(InlNode.M(tex = "\\bad", m = null))
        val blocks = listOf(MdBlock.Quote(blocks = listOf(MdBlock.P(k = listOf(Inl.InlNode(InlNode.B(k = listOf(m))), none)))))
        assertEquals(mapOf("a" to box), inlineMathOf(blocks))
    }

    @Test
    fun aMissingBlockBoxFallsBackToTheTex() {
        val p = layoutSeg(Doc(blocks = listOf(MdBlock.Math(tex = "\\frac{a", m = null)), segs = emptyList()), 0, null, style).single()
        assertNull((p as MdPiece.Math).m)
    }

    @Test
    fun theSlotIsTheBoxTimesTheFontSize() {
        val m = MathBox(w = 3840, h = 814, d = 200, items = emptyList())
        val s = mathSizePx(m, 20f)
        assertEquals(76.8f, s.width, 1e-3f)
        assertEquals(20.28f, s.height, 1e-3f)
        assertEquals(4f, s.depth, 1e-3f)
    }

    @Test
    fun aStemNamesItsFileInTheSharedFolderAndNoneNamesNoFile() {
        // 认得哪几族只在 md-core 列（那边逐族对过文件）；这里只钉「主干 ⇒ assets 里那一份」这一拼法。
        val dir = File(root, "src/shared/fonts/katex")
        for (stem in listOf("Math-Italic", "Main-Regular", "Size2-Regular")) {
            val asset = mathFontAsset(stem)
            assertEquals("katex/KaTeX_$stem.ttf", asset)
            assertTrue("$stem ⇒ $asset 在 $dir 里没有", File(dir, asset!!.removePrefix("katex/")).isFile)
        }
        assertNull("系统字体不该有文件", mathFontAsset(null))
    }
}

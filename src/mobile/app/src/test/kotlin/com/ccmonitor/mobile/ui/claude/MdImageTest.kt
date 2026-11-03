package com.ccmonitor.mobile.ui.claude

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import com.ccmonitor.mobile.core.wire.gen.Doc
import com.ccmonitor.mobile.core.wire.gen.Inl
import com.ccmonitor.mobile.core.wire.gen.InlNode
import com.ccmonitor.mobile.core.wire.gen.MdBlock
import org.junit.Assert.assertEquals
import org.junit.Test

/** 正文里的图照桌面画真图：排版时那一格是一个带着 src 的行内槽，替代字仍是那一格的字（取到之前 · 取不到就画它）。 */
class MdImageTest {
    private val style =
        MdStyle(
            code = SpanStyle(),
            link = TextLinkStyles(SpanStyle(color = Color(0xFF000002))),
            faint = SpanStyle(color = Color(0xFF000003)),
            fnRef = SpanStyle(),
            hl = List(13) { SpanStyle() },
        )

    @Test
    fun imageIsAnInlineSlotCarryingItsSrc() {
        val doc =
            Doc(
                listOf(MdBlock.P(listOf(Inl.Text("看 "), Inl.InlNode(InlNode.Img("https://e.x/a.png", "图注")), Inl.Text(" 完")))),
                emptyList(),
            )
        val t = (layoutSeg(doc, 0, null, style).single() as MdPiece.Text).text
        assertEquals("看 图注 完", t.text)
        assertEquals(listOf("https://e.x/a.png"), imagesOf(t))
        val slot = t.getStringAnnotations(IMG_TAG, 0, t.length).single()
        assertEquals(2 until 4, slot.start until slot.end)
        assertEquals(listOf(2 until 4), t.spanStyles.filter { it.item == style.faint }.map { it.start until it.end })
    }
}

package com.ccmonitor.mobile.ui.claude

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.font.FontWeight
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.ui.chat.ChatSession
import com.ccmonitor.mobile.ui.chat.ChatTarget
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 思考展开那一刻问后端解析（`md-doc {as: prose}`），回来照树排（同桌面 `buildThinkingBody`）；没问成 ⇒ 照原文的字。 */
@OptIn(ExperimentalCoroutinesApi::class)
class ThinkingTextTest {
    private val style =
        MdStyle(
            code = SpanStyle(),
            link = TextLinkStyles(SpanStyle(color = Color(0xFF000002))),
            faint = SpanStyle(),
            fnRef = SpanStyle(),
            hl = List(13) { SpanStyle() },
        )

    /** 后端对 `**粗** 一下\n\n- 一项` 交的树（线上那一形）。 */
    private val tree =
        mapOf(
            "blocks" to
                listOf(
                    mapOf("t" to "p", "k" to listOf(mapOf("t" to "b", "k" to listOf("粗")), " 一下")),
                    mapOf(
                        "t" to "list",
                        "start" to null,
                        "tight" to true,
                        "items" to listOf(mapOf("task" to null, "blocks" to listOf(mapOf("t" to "p", "k" to listOf("一项"))))),
                    ),
                ),
            "segs" to emptyList<Int>(),
        )

    @Test
    fun thinkingIsLaidOutFromTheTree() =
        runTest {
            val link = FakeCoreLink()
            link.answer = { cmd, _ -> if (cmd == "md-doc") Reply.Ok(tree) else Reply.LinkDown }
            val s = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope)
            val text = "**粗** 一下\n\n- 一项"
            val doc = s.mdDoc(text)
            assertEquals(listOf(mapOf("text" to text, "as" to "prose", "refs" to emptyList<Any>())), link.calls("md-doc"))

            val pieces = layoutSeg(doc!!, 0, null, style)
            val p = pieces[0] as MdPiece.Text
            assertEquals("粗 一下", p.text.text)
            val bold = p.text.spanStyles.filter { it.item == SpanStyle(fontWeight = FontWeight.Bold) }.map { it.start until it.end }
            assertEquals(listOf(0 until 1), bold)
            assertTrue("列表成列表块", pieces[1] is MdPiece.Bullets)
        }

    @Test
    fun unaskedThinkingIsNull() =
        runTest {
            val link = FakeCoreLink()
            val s = ChatSession(link, link, ChatTarget.Existing("s1"), "box", backgroundScope)
            assertNull("没问成 ⇒ null（卡上照原文的字）", s.mdDoc("x"))
        }
}

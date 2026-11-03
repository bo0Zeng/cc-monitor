package com.ccmonitor.mobile.ui.claude

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import com.ccmonitor.mobile.core.wire.gen.Doc
import com.ccmonitor.mobile.core.wire.gen.Inl
import com.ccmonitor.mobile.core.wire.gen.MdBlock
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** 长正文照段界排：只排第 0 段 ＋「展开」，点了才一帧接一段补完（两端一个样子）。 */
@OptIn(ExperimentalCoroutinesApi::class)
class MarkdownTextTest {
    private val style =
        MdStyle(
            code = SpanStyle(),
            link = TextLinkStyles(SpanStyle(color = Color(0xFF000002))),
            faint = SpanStyle(),
            fnRef = SpanStyle(),
            hl = List(13) { SpanStyle() },
        )

    private fun p(s: String) = MdBlock.P(k = listOf(Inl.Text(s)))

    private val threeSegs = Doc(blocks = listOf(p("一"), p("二"), p("三")), segs = listOf(1, 2))

    @Test
    fun longProseWaitsForExpand() =
        runTest {
            val state = MdDocState(threeSegs, style)
            val job = launch { state.drive({ yield() }, { null }) }
            advanceUntilIdle()
            assertEquals(1, state.built)
            assertTrue(state.more)

            state.expand()
            advanceUntilIdle()
            assertEquals(3, state.built)
            assertFalse(state.more)
            job.cancel()
        }

    @Test
    fun oneSegHasNoExpand() =
        runTest {
            val state = MdDocState(Doc(blocks = listOf(p("一")), segs = emptyList()), style)
            val job = launch { state.drive({ yield() }, { null }) }
            advanceUntilIdle()
            assertEquals(1, state.built)
            assertFalse(state.more)
            job.cancel()
        }
}

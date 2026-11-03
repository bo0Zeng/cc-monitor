package com.ccmonitor.mobile.ui.claude

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import com.ccmonitor.mobile.core.copy.sizeText
import com.ccmonitor.mobile.core.wire.gen.Doc
import com.ccmonitor.mobile.core.wire.gen.Inl
import com.ccmonitor.mobile.core.wire.gen.MdBlock
import com.squareup.moshi.JsonReader
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.yield
import okio.Buffer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

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

    /** 「展开 {size}」那一格经核心 `copy_core::size_text`（JNI），与后端 · 界面对同一份金样。 */
    @Test
    fun expandSizeIsCoreSizeText() {
        val root =
            generateSequence(File("").absoluteFile) { it.parentFile }
                .first { File(it, "tests/__fixtures__").isDirectory && File(it, "src/backend/lib.rs").isFile }
        val r = JsonReader.of(Buffer().writeUtf8(File(root, "tests/__fixtures__/size-text.golden.json").readText()))
        var n = 0
        r.beginObject()
        while (r.hasNext()) {
            if (r.nextName() != "cases") {
                r.skipValue()
                continue
            }
            r.beginArray()
            while (r.hasNext()) {
                var bytes = 0L
                var want = ""
                r.beginObject()
                while (r.hasNext()) {
                    when (r.nextName()) {
                        "bytes" -> bytes = r.nextLong()
                        "want" -> want = r.nextString()
                        else -> r.skipValue()
                    }
                }
                r.endObject()
                assertEquals("$bytes", want, sizeText(bytes))
                n++
            }
            r.endArray()
        }
        r.endObject()
        assertTrue(n > 0)
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

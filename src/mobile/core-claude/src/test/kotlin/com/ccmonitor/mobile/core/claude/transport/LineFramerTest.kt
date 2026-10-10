package com.ccmonitor.mobile.core.claude.transport

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class LineFramerTest {
    private fun bytes(s: String) = s.toByteArray(Charsets.UTF_8)

    @Test fun simpleLines() {
        val f = LineFramer()
        assertEquals(listOf("a", "b"), f.feed(bytes("a\nb\n")))
        assertNull(f.flush())
    }

    @Test fun chunkSplitMidLine() {
        val f = LineFramer()
        assertEquals(emptyList<String>(), f.feed(bytes("hel")))
        assertEquals(listOf("hello"), f.feed(bytes("lo\n")))
    }

    @Test fun residualLineViaFlush() {
        val f = LineFramer()
        assertEquals(listOf("done"), f.feed(bytes("done\npartial")))
        assertEquals("partial", f.flush())
        assertNull(f.flush()) // flush 后清空
    }

    @Test fun crlfStripped() {
        val f = LineFramer()
        assertEquals(listOf("x", "y"), f.feed(bytes("x\r\ny\r\n")))
    }

    @Test fun multibyteUtf8SplitAcrossChunks() {
        // "ab你" = 61 62 E4 BD A0；把 你 的 3 字节劈在 chunk 边界
        val full = bytes("ab你\n")
        val f = LineFramer()
        assertEquals(emptyList<String>(), f.feed(full.copyOfRange(0, 4))) // a b E4 BD
        assertEquals(listOf("ab你"), f.feed(full.copyOfRange(4, full.size))) // A0 0A
    }

    @Test fun emptyLinesPreserved() {
        val f = LineFramer()
        assertEquals(listOf("a", "", "b"), f.feed(bytes("a\n\nb\n")))
    }

    @Test fun emptyChunkNoOp() {
        val f = LineFramer()
        assertEquals(emptyList<String>(), f.feed(ByteArray(0)))
    }

    @Test fun jsonLineRoundTrips() {
        val json = """{"type":"user","message":{"content":"héllo 世界"}}"""
        val f = LineFramer()
        assertEquals(listOf(json), f.feed(bytes(json + "\n")))
    }

    // ---- 逐行字节 offset ----

    @Test fun feedFramedTracksByteOffset() {
        val f = LineFramer()
        val framed = f.feedFramed(bytes("ab\ncde\n")) // 行1=3字节(ab\n)，行2=4字节(cde\n)
        assertEquals(listOf("ab", "cde"), framed.map { it.raw })
        assertEquals(listOf(3L, 7L), framed.map { it.endOffset })
        assertEquals(7L, f.consumedBytes)
    }

    @Test fun feedFramedCountsCrlfAndMultibyteRawBytes() {
        val f = LineFramer()
        // "你\r\n" = 3(你)+1(\r)+1(\n)=5 字节；"x\n"=2 字节。offset 数原始字节（含 \r），string 去 \r。
        val framed = f.feedFramed(bytes("你\r\nx\n"))
        assertEquals(listOf("你", "x"), framed.map { it.raw })
        assertEquals(listOf(5L, 7L), framed.map { it.endOffset })
    }

    @Test fun feedFramedOffsetSpansChunksResidualNotCounted() {
        val f = LineFramer()
        assertEquals(emptyList<LineFramer.Framed>(), f.feedFramed(bytes("hel")))
        assertEquals(0L, f.consumedBytes) // 残行未提交 → 不计入 offset
        val framed = f.feedFramed(bytes("lo\n"))
        assertEquals(listOf("hello"), framed.map { it.raw })
        assertEquals(6L, f.consumedBytes) // "hello\n"=6 字节
        assertEquals(listOf(6L), framed.map { it.endOffset })
    }
}

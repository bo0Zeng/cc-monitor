package com.ccmonitor.mobile.core.copy

import com.squareup.moshi.Moshi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 手机写大小 · 短时长 · 会走的钟只经 [SizeFormat] · [DurationFormat]，与核心（Rust copy-core）· 桌面（TS）读同一份金样
 * （`size-text.golden.json` · `short-duration.golden.json` 的 cases / rel / clock 三段）。
 */
class FormatGoldenTest {
    @Suppress("UNCHECKED_CAST")
    private fun golden(name: String): Map<String, Any?> =
        Moshi
            .Builder()
            .build()
            .adapter(Any::class.java)
            .fromJson(
                generateSequence(File("").absoluteFile) { it.parentFile }
                    .map { File(it, "tests/__fixtures__/$name") }
                    .first { it.isFile }
                    .readText(),
            ) as Map<String, Any?>

    @Suppress("UNCHECKED_CAST")
    private fun Map<String, Any?>.list(k: String) = this[k] as List<Map<String, Any?>>

    private fun Map<String, Any?>.long(k: String): Long? = (this[k] as Double?)?.toLong()

    @Test
    fun `大小：每一条与金样逐字相同`() {
        val cases = golden("size-text.golden.json").list("cases")
        assertTrue(cases.size > 10)
        for (c in cases) assertEquals("${c["bytes"]}", c["want"], SizeFormat.text(c.long("bytes")!!))
    }

    @Test
    fun `短时长 · 距今 · 会走的钟：每一条与金样逐字相同`() {
        val g = golden("short-duration.golden.json")
        for (c in g.list("cases")) assertEquals("${c["ms"]}", c["want"], DurationFormat.short(c.long("ms")!!))
        for (c in g.list("rel")) assertEquals("${c["aheadMs"]}", c["want"], DurationFormat.ahead(c.long("aheadMs")!!))
        for (c in g.list("clock")) {
            assertEquals(c["text"].toString(), c["want"], DurationFormat.clock(c["text"] as String, c.long("from")!!, c.long("to"), c.long("now")!!))
        }
    }

    /** 同桌面 `duration-format.ts` 的 `waitedNow`：那台算好的 `waitedMs` ＋ 本机从收到那份起走过的（两段各在一台钟上量，不跨机器减）。 */
    @Test
    fun `已等走表：那台给的 waitedMs 加上本机从收到起走过的；没有起点 ⇒ null；本机钟往回走按 0`() {
        assertEquals(DurationFormat.short(240_000), DurationFormat.waited(180_000, receivedAt = 1_000, now = 61_000))
        assertEquals(DurationFormat.short(5_000), DurationFormat.waited(5_000, receivedAt = 10_000, now = 9_000))
        assertNull(DurationFormat.waited(null, receivedAt = 0, now = 1_000))
    }
}

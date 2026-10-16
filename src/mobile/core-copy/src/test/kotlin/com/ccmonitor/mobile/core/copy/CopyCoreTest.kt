package com.ccmonitor.mobile.core.copy

import com.squareup.moshi.Moshi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.io.File

/**
 * 取文 · 大小 · 短时长 · 距今经手机那份 `.so` 里的 `copy-core`（单测加载同一份 crate 的宿主编译，写法本身的对拍在 Rust 那边）：
 * 这里只钉经 JNI 来回不走样；会走的钟与已等走表是手机这一侧按此刻填的，照金样 `short-duration.golden.json` 的 clock 段。
 */
class CopyCoreTest {
    @Test
    fun `取文经核心：参数按名对上，非 ASCII 来回不走样，表里没有的键写成〔键〕`() {
        assertEquals("1h4m", copyText("durationFormat.short.hourMin", "h" to 1, "m" to 4))
        assertEquals("七 B", copyText("sizeFormat.unit.b", "n" to "七"))
        assertEquals("〔no.such.key〕", copyText("no.such.key"))
    }

    @Test
    fun `大小 · 短时长 · 距今经核心：负数按 0，已到 ⇒ null`() {
        assertEquals("1.5 KB", SizeFormat.text(1536))
        assertEquals("0 B", SizeFormat.text(-1))
        assertEquals("1h4m", DurationFormat.short(3_840_000))
        assertEquals("<1s", DurationFormat.short(-5))
        assertEquals("+2m", DurationFormat.ahead(61_000))
        assertNull(DurationFormat.ahead(0))
        assertNull(DurationFormat.ahead(-5_000))
    }

    @Test
    fun `会走的钟：每一条与金样 clock 段逐字相同`() {
        @Suppress("UNCHECKED_CAST")
        val g =
            Moshi
                .Builder()
                .build()
                .adapter(Any::class.java)
                .fromJson(
                    generateSequence(File("").absoluteFile) { it.parentFile }
                        .map { File(it, "tests/__fixtures__/short-duration.golden.json") }
                        .first { it.isFile }
                        .readText(),
                ) as Map<String, Any?>

        @Suppress("UNCHECKED_CAST")
        val clock = g["clock"] as List<Map<String, Any?>>

        fun Map<String, Any?>.long(k: String): Long? = (this[k] as Double?)?.toLong()
        for (c in clock) {
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

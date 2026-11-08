package com.ccmonitor.mobile.core.data.db

import com.ccmonitor.mobile.core.rs.TermKeys
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 手机这一侧对拍 xterm 键码的金样（`tests/__fixtures__/xterm-keys.tsv`，cmd-wire 的判据读同一份）：
 * 金样里的记号原样交 [TermKeys.bytes]（JNI 进核心那一张表），字节对得上（解码那一向由 cmd-wire 的判据读同一份守）。
 * 手机写记号那一向（[token]）由 `KeyDispatchTest` 照字节钉。
 */
class KeyTokensTest {
    private data class Row(
        val line: Int,
        val token: String,
        val bytes: ByteArray,
        val dir: String,
    )

    private val rows: List<Row> by lazy {
        val f =
            generateSequence(File("").absoluteFile) { it.parentFile }
                .map { File(it, "tests/__fixtures__/xterm-keys.tsv") }
                .first { it.isFile }
        f.readLines().mapIndexed { i, l ->
            val c = l.split('\t')
            assertEquals("第 ${i + 1} 行该是三列", 3, c.size)
            Row(i + 1, c[0], c[1].chunked(2).map { it.toInt(16).toByte() }.toByteArray(), c[2])
        }
    }

    @Test
    fun everyGoldenTokenEncodesToItsBytes() {
        val enc = rows.filter { it.dir != "<" }
        assertTrue("金样里编码那几行太少", enc.size > 40)
        for (r in enc) {
            val got = checkNotNull(TermKeys.bytes(arrayOf(r.token))) { "第 ${r.line} 行记号核心读不出：${r.token}" }
            assertArrayEquals("第 ${r.line} 行 ${r.token}", r.bytes, got)
        }
    }
}

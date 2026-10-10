package com.ccmonitor.mobile.core.ui.copy

import com.squareup.moshi.Moshi
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** 插值与桌面两个读口对同一份金样（`tests/__fixtures__/copy-interpolation.golden.json`）。 */
class CopyRenderGoldenTest {
    private val golden: File =
        generateSequence(File("").absoluteFile) { it.parentFile }
            .map { File(it, "tests/__fixtures__/copy-interpolation.golden.json") }
            .first { it.isFile }

    @Suppress("UNCHECKED_CAST")
    private val cases: List<Map<String, Any?>> =
        (
            Moshi
                .Builder()
                .build()
                .adapter(Any::class.java)
                .fromJson(golden.readText()) as Map<String, Any?>
        )["cases"] as List<Map<String, Any?>>

    @Test
    fun `每一条插值与金样的 want 逐字相同`() {
        assertTrue("金样一条都没读到", cases.size > 5)
        for (c in cases) {
            @Suppress("UNCHECKED_CAST")
            val args = (c["args"] as Map<String, Any?>).mapValues { it.value.toString().removeSuffix(".0") }
            assertEquals(c["key"].toString(), c["want"], CopyRender.render(c["zh"] as String, args))
        }
    }
}

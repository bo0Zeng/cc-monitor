package com.ccmonitor.mobile.ui.common

import com.ccmonitor.mobile.ui.Faces
import org.junit.Assert.assertEquals
import org.junit.Test

/** 会话那颗点照那台给的脸画（`face.dot`），手机不另折：整份脸金样每一形落到哪一颗，形状照桌面 `kit/status-dot` 那一套。 */
class StatusDotTest {
    @Test
    fun `整份脸金样每一形的点落到它那一颗：在的实心 · 退了结束了是环 · 说不清虚线环 · 记录已不在短横`() {
        val want =
            mapOf(
                "running" to DotForm.SOLID,
                "needs-you" to DotForm.SOLID,
                "background" to DotForm.SOLID,
                "idle" to DotForm.SOLID,
                "exited" to DotForm.RING,
                "ended" to DotForm.RING,
                "unknown" to DotForm.DASHED,
                "gone" to DotForm.DASH,
            )

        @Suppress("UNCHECKED_CAST")
        val got = Faces.raw.keys.associate { k -> ((Faces.raw[k] as Map<String, Any?>)["dot"] as String) to formOf(Faces.of(k).dot) }
        assertEquals("线上那八颗点都在金样里、各落到它那一颗", want, got)
    }
}

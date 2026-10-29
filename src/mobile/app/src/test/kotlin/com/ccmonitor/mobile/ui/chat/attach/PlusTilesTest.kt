package com.ccmonitor.mobile.ui.chat.attach

import com.ccmonitor.mobile.core.ui.icon.Ph
import org.junit.Assert.assertEquals
import org.junit.Test

/** ［+］面板头一排三格：拍照 · 图片 · 文件各用各的图标（不拿圆 · 方块占位）。 */
class PlusTilesTest {
    @Test
    fun `三格的先后与图标：拍照 Camera · 图片 Image · 文件 File`() {
        assertEquals(listOf(AttachSource.CAMERA to Ph.Camera, AttachSource.IMAGE to Ph.Image, AttachSource.FILE to Ph.File), PLUS_TILES.map { it.source to it.icon })
    }
}

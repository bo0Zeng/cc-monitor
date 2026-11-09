package com.ccmonitor.mobile.core.terminal

import org.connectbot.terminal.TerminalDimensions
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 建 emulator 时交给 termlib 的回调（[emulatorHooks]）：里面程序经 OSC 52 写剪贴板，termlib 解好文本交 `onClipboardCopy`，
 * 这里要原样交到 app 那一侧（写系统剪贴板），且不往远端回任何字节。
 * 读剪贴板的询问（`OSC 52 ; c ; ?`）termlib 0.1.2 的 OscParser 自己丢掉、不交这条回调，也不回；
 * [TerminalSession] 本体含 termlib 原生 emulator（JNI + Looper），纯 JVM 喂不进字节，整条在设备上验。
 */
class ClipboardCopyTest {
    private class Wire {
        val sent = mutableListOf<ByteArray>()
        val resized = mutableListOf<Pair<Int, Int>>()
        val copied = mutableListOf<String>()

        fun hooks(latch: ModifierLatch = ModifierLatch()) =
            emulatorHooks(
                modifiers = latch,
                send = { sent += it },
                resizeRemote = { c, r -> resized += c to r },
                copyToClipboard = { copied += it },
            )
    }

    @Test
    fun osc52CopyReachesAppSideAndSendsNothingBack() {
        val w = Wire()
        w.hooks().onClipboardCopy("选中的字 hello")
        assertEquals(listOf("选中的字 hello"), w.copied)
        assertTrue("写剪贴板不往远端回字节", w.sent.isEmpty())
    }

    @Test
    fun keyboardAndResizeStillGoRemoteAndNeverTouchClipboard() {
        val w = Wire()
        val latch = ModifierLatch().apply { on(Mod.CTRL, ModGesture.TAP) }
        val h = w.hooks(latch)
        h.onKeyboardInput("c".toByteArray())
        h.onResize(TerminalDimensions(rows = 30, columns = 100))
        assertArrayEquals(byteArrayOf(3), w.sent.single())
        assertEquals(listOf(100 to 30), w.resized)
        assertTrue("键盘与改尺寸不碰剪贴板", w.copied.isEmpty())
    }
}

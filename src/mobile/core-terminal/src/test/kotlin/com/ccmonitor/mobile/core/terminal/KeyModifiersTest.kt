package com.ccmonitor.mobile.core.terminal

import org.junit.Assert.assertArrayEquals
import org.junit.Test

/** 粘性修饰键 [applyKeyModifiers] 纯函数——把 Ctrl/Alt 应用到一次软键盘按键。 */
class KeyModifiersTest {
    private fun b(vararg v: Int) = v.map { it.toByte() }.toByteArray()

    @Test
    fun noModifiersPassthrough() {
        val raw = b('c'.code)
        assertArrayEquals(raw, applyKeyModifiers(raw, KeyModifiers()))
    }

    @Test
    fun ctrlMapsLetterToControlCode() {
        // Ctrl+C = 0x03（中断）；Ctrl+D = 0x04；Ctrl+Z = 0x1A。
        assertArrayEquals(b(0x03), applyKeyModifiers(b('c'.code), KeyModifiers(ctrl = true)))
        assertArrayEquals(b(0x03), applyKeyModifiers(b('C'.code), KeyModifiers(ctrl = true))) // 大小写同码
        assertArrayEquals(b(0x04), applyKeyModifiers(b('d'.code), KeyModifiers(ctrl = true)))
        assertArrayEquals(b(0x1A), applyKeyModifiers(b('z'.code), KeyModifiers(ctrl = true)))
    }

    @Test
    fun altPrefixesEsc() {
        // Alt/Meta = ESC(0x1B) 前缀（readline M-x 等）。
        assertArrayEquals(b(0x1B, 'x'.code), applyKeyModifiers(b('x'.code), KeyModifiers(alt = true)))
    }

    @Test
    fun ctrlAndAltCombine() {
        // Ctrl+Alt+C = ESC + 0x03。
        assertArrayEquals(b(0x1B, 0x03), applyKeyModifiers(b('c'.code), KeyModifiers(ctrl = true, alt = true)))
    }

    @Test
    fun multiByteInputUnmodified() {
        // IME 组字/emoji/粘贴多字节 → 不动（避免破坏），即便有修饰。
        val multi = b(0xE4, 0xBD, 0xA0) // "你" UTF-8
        assertArrayEquals(multi, applyKeyModifiers(multi, KeyModifiers(ctrl = true)))
        assertArrayEquals(multi, applyKeyModifiers(multi, KeyModifiers(alt = true)))
    }

    @Test
    fun emptyInputUnmodified() {
        assertArrayEquals(ByteArray(0), applyKeyModifiers(ByteArray(0), KeyModifiers(ctrl = true)))
    }

    @Test
    fun anyReflectsFlags() {
        assert(!KeyModifiers().any)
        assert(KeyModifiers(ctrl = true).any)
        assert(KeyModifiers(alt = true).any)
        assert(KeyModifiers(shift = true).any)
    }

    // === Shift 分支 ===

    @Test
    fun shiftUppercasesLetterOnly() {
        assertArrayEquals(b('A'.code), applyKeyModifiers(b('a'.code), KeyModifiers(shift = true)))
        assertArrayEquals(b('Z'.code), applyKeyModifiers(b('z'.code), KeyModifiers(shift = true)))
        assertArrayEquals(b('1'.code), applyKeyModifiers(b('1'.code), KeyModifiers(shift = true))) // 非字母不动
    }

    @Test
    fun ctrlShiftSameAsCtrl() {
        // Ctrl 忽略大小写 → Ctrl+Shift+C == Ctrl+C == 0x03。
        assertArrayEquals(b(0x03), applyKeyModifiers(b('c'.code), KeyModifiers(ctrl = true, shift = true)))
    }

    // === Ctrl 限标准控制字符集（盲取 `& 0x1F` 会把 Ctrl+1→XON/Ctrl+3→XOFF，触发终端流控冻结）===

    @Test
    fun ctrlDigitUnchangedNoFlowControlFreeze() {
        assertArrayEquals(b('1'.code), applyKeyModifiers(b('1'.code), KeyModifiers(ctrl = true))) // 不能是 0x11=XON
        assertArrayEquals(b('3'.code), applyKeyModifiers(b('3'.code), KeyModifiers(ctrl = true))) // 不能是 0x13=XOFF
        assertArrayEquals(b('0'.code), applyKeyModifiers(b('0'.code), KeyModifiers(ctrl = true)))
        assertArrayEquals(b('9'.code), applyKeyModifiers(b('9'.code), KeyModifiers(ctrl = true)))
    }

    @Test
    fun ctrlStandardPunctuationStillMapped() {
        assertArrayEquals(b(0x1B), applyKeyModifiers(b('['.code), KeyModifiers(ctrl = true))) // Ctrl+[ = ESC
        assertArrayEquals(b(0x1C), applyKeyModifiers(b('\\'.code), KeyModifiers(ctrl = true)))
        assertArrayEquals(b(0x1D), applyKeyModifiers(b(']'.code), KeyModifiers(ctrl = true)))
        assertArrayEquals(b(0x1E), applyKeyModifiers(b('^'.code), KeyModifiers(ctrl = true)))
        assertArrayEquals(b(0x1F), applyKeyModifiers(b('_'.code), KeyModifiers(ctrl = true)))
        assertArrayEquals(b(0x00), applyKeyModifiers(b('@'.code), KeyModifiers(ctrl = true))) // Ctrl+@ = NUL
        assertArrayEquals(b(0x00), applyKeyModifiers(b(' '.code), KeyModifiers(ctrl = true))) // Ctrl+Space = NUL
        assertArrayEquals(b(0x7F), applyKeyModifiers(b('?'.code), KeyModifiers(ctrl = true))) // Ctrl+? = DEL
    }

    @Test
    fun ctrlNonStandardPunctuationUnchanged() {
        listOf('.', ',', ';', '-', '=', '/', '!').forEach { c ->
            assertArrayEquals("Ctrl+$c 无标准控制码，应不变", b(c.code), applyKeyModifiers(b(c.code), KeyModifiers(ctrl = true)))
        }
        // Ctrl+Alt+数字：Ctrl 部分不变、Alt 仍前缀 ESC。
        assertArrayEquals(b(0x1B, '1'.code), applyKeyModifiers(b('1'.code), KeyModifiers(ctrl = true, alt = true)))
    }
}

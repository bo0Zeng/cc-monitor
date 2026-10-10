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

    // === Shift 分支 + customButtonBytes ===

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

    @Test
    fun customButtonNoModifierIsCommandLine() {
        // 无修饰 → 命令行文本 + 回车。
        assertArrayEquals("ls -la\n".toByteArray(), customButtonBytes("ls -la", KeyModifiers()))
    }

    @Test
    fun customButtonCtrlKeySendsControlCode() {
        // Ctrl+C 按钮 → 0x03（不补回车）。
        assertArrayEquals(b(0x03), customButtonBytes("c", KeyModifiers(ctrl = true)))
        assertArrayEquals(b(0x1B, 'x'.code), customButtonBytes("x", KeyModifiers(alt = true))) // Alt+x
        assertArrayEquals(b('A'.code), customButtonBytes("a", KeyModifiers(shift = true))) // Shift+a → 'A'
    }

    @Test
    fun customButtonNamedKeys() {
        assertArrayEquals(b(0x1B, '['.code, 'Z'.code), customButtonBytes("Tab", KeyModifiers(shift = true))) // Shift+Tab=backtab
        assertArrayEquals(b(0x09), customButtonBytes("tab", KeyModifiers(ctrl = true))) // Ctrl+Tab：无标准 Ctrl 码 → 0x09 不变
        assertArrayEquals(b(0x1B, 0x0D), customButtonBytes("enter", KeyModifiers(alt = true))) // Alt+Enter
        assertArrayEquals(b(0x1B), customButtonBytes("esc", KeyModifiers(ctrl = true))) // Ctrl+Esc：无标准 Ctrl 码 → 0x1B 不变
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

    // === customButton 非 ASCII 首字符不被单字节截断 ===

    @Test
    fun customButtonNonAsciiFirstCharNotTruncated() {
        // 截成单字节会把 '中'(U+4E2D) 变成 0x2D('-')；应整码点 UTF-8 原样透传（多字节守卫）。
        assertArrayEquals("中".toByteArray(Charsets.UTF_8), customButtonBytes("中", KeyModifiers(ctrl = true)))
        assertArrayEquals("你".toByteArray(Charsets.UTF_8), customButtonBytes("你好", KeyModifiers(alt = true))) // 只取首码点
        assertArrayEquals("😀".toByteArray(Charsets.UTF_8), customButtonBytes("😀ok", KeyModifiers(ctrl = true))) // 代理对整码点
        assertArrayEquals("ASCII 首字符行为不变", b(0x03), customButtonBytes("c", KeyModifiers(ctrl = true)))
    }
}

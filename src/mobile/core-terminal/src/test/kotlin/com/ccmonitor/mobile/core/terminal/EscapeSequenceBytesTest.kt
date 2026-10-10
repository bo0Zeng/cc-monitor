package com.ccmonitor.mobile.core.terminal

import org.junit.Assert.assertArrayEquals
import org.junit.Test

/** 转义序列按钮 [escapeSequenceBytes] 纯函数——识别 \e/\xNN/\n/\r/\t/\\，其余原样透传。 */
class EscapeSequenceBytesTest {
    private fun b(vararg v: Int) = v.map { it.toByte() }.toByteArray()

    @Test
    fun escBackslashE() = assertArrayEquals(b(0x1B, '['.code, 'A'.code), escapeSequenceBytes("\\e[A")) // 上方向键 CSI A

    // 功能键排种子（DEFAULT_SEED_BUTTONS 的 fn 组）用的 escape 命令 → xterm 字节。
    @Test
    fun fnKeySeedCommandsDecodeToXtermBytes() {
        assertArrayEquals(b(0x1B), escapeSequenceBytes("\\e")) // Esc
        assertArrayEquals(b(0x09), escapeSequenceBytes("\\t")) // Tab
        assertArrayEquals(b(0x1B, '['.code, 'D'.code), escapeSequenceBytes("\\e[D")) // ←
        assertArrayEquals(b(0x1B, '['.code, 'A'.code), escapeSequenceBytes("\\e[A")) // ↑
        assertArrayEquals(b(0x1B, '['.code, 'B'.code), escapeSequenceBytes("\\e[B")) // ↓
        assertArrayEquals(b(0x1B, '['.code, 'C'.code), escapeSequenceBytes("\\e[C")) // →
        assertArrayEquals(b(0x1B, '['.code, 'H'.code), escapeSequenceBytes("\\e[H")) // Home
        assertArrayEquals(b(0x1B, '['.code, 'F'.code), escapeSequenceBytes("\\e[F")) // End
    }

    /** vim 键排种子（DEFAULT_SEED_BUTTONS 的 vim 组）的 escape 命令 → 字节。 */
    @Test
    fun vimKeySeedCommandsDecodeToKeyBytes() {
        fun s(literal: String) = literal.toByteArray(Charsets.UTF_8)
        assertArrayEquals(b(0x1B), escapeSequenceBytes("\\e")) // Esc
        assertArrayEquals(s("i"), escapeSequenceBytes("i"))
        assertArrayEquals(s(":"), escapeSequenceBytes(":"))
        assertArrayEquals(s("/"), escapeSequenceBytes("/"))
        assertArrayEquals(s(":w\n"), escapeSequenceBytes(":w\\n")) // 保存（↵=\n）
        assertArrayEquals(s(":wq\n"), escapeSequenceBytes(":wq\\n")) // 保存退出
        assertArrayEquals(s(":q!\n"), escapeSequenceBytes(":q!\\n")) // 强退不保存
        assertArrayEquals(s("dd"), escapeSequenceBytes("dd"))
        assertArrayEquals(s("yy"), escapeSequenceBytes("yy"))
        assertArrayEquals(s("p"), escapeSequenceBytes("p"))
        assertArrayEquals(s("u"), escapeSequenceBytes("u"))
        assertArrayEquals(s("gg"), escapeSequenceBytes("gg"))
        assertArrayEquals(s("G"), escapeSequenceBytes("G"))
        assertArrayEquals(s("h"), escapeSequenceBytes("h"))
        assertArrayEquals(s("j"), escapeSequenceBytes("j"))
        assertArrayEquals(s("k"), escapeSequenceBytes("k"))
        assertArrayEquals(s("l"), escapeSequenceBytes("l"))
        assertArrayEquals(s("0"), escapeSequenceBytes("0"))
        assertArrayEquals(s("\$"), escapeSequenceBytes("\$"))
    }

    @Test
    fun escHexLowerAndUpper() {
        assertArrayEquals(b(0x1B, '['.code, 'D'.code), escapeSequenceBytes("\\x1b[D")) // 左方向键
        assertArrayEquals(b(0x1B), escapeSequenceBytes("\\x1B")) // 大写 hex
        assertArrayEquals(b(0x07), escapeSequenceBytes("\\x07")) // BEL
    }

    @Test
    fun namedEscapes() {
        assertArrayEquals(b(0x09), escapeSequenceBytes("\\t"))
        assertArrayEquals(b(0x0A), escapeSequenceBytes("\\n"))
        assertArrayEquals(b(0x0D), escapeSequenceBytes("\\r"))
        assertArrayEquals(b('\\'.code), escapeSequenceBytes("\\\\"))
    }

    @Test
    fun literalPassthroughNoBackslash() {
        // 无反斜杠的字面串原样透传（如 `[D` = 字面 `[` `D`，非方向键——真方向键是含 ESC 的 `\e[D`=1b 5b 44）。
        assertArrayEquals(b('['.code, 'D'.code), escapeSequenceBytes("[D"))
        assertArrayEquals(b('l'.code, 's'.code, 0x0A), escapeSequenceBytes("ls\\n"))
    }

    @Test
    fun unrecognizedEscapeKeepsBackslashLiteral() {
        // 未识别的 \z → 反斜杠字面 + z 普通处理。
        assertArrayEquals(b('\\'.code, 'z'.code), escapeSequenceBytes("\\z"))
        // 无效 \x（hex 不足/非法）→ 反斜杠字面，x 后续正常。
        assertArrayEquals(b('\\'.code, 'x'.code, 'g'.code), escapeSequenceBytes("\\xg"))
    }

    @Test
    fun trailingLoneBackslashPassthrough() = assertArrayEquals(b('a'.code, '\\'.code), escapeSequenceBytes("a\\"))

    @Test
    fun nonAsciiPassthroughUtf8() =
        assertArrayEquals("中".toByteArray(Charsets.UTF_8), escapeSequenceBytes("中")) // 多字节原样透传

    @Test
    fun emptyString() = assertArrayEquals(ByteArray(0), escapeSequenceBytes(""))
}

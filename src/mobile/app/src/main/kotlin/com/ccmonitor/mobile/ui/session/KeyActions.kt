package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.data.db.AppAction
import com.ccmonitor.mobile.core.data.db.KeyBinding
import com.ccmonitor.mobile.core.data.db.KeyMod
import com.ccmonitor.mobile.core.data.db.KeyPress
import com.ccmonitor.mobile.core.data.db.SpecialKey
import com.ccmonitor.mobile.core.terminal.KeyModifiers
import com.ccmonitor.mobile.core.terminal.applyKeyModifiers
import java.io.ByteArrayOutputStream

/**
 * App 动作束：键条上绑了 App 动作的触发经 [invoke] 触达终端屏已有的回调。
 * 要 tmux 的那几个（[AppAction.NEEDS_TMUX]）在没有 tmux 的服务器上点了不做、交给 [onNeedsTmux] 说一句。
 */
class KeyActions(
    val supportsTmux: Boolean,
    val onShowTabs: () -> Unit,
    val onCommandPalette: () -> Unit,
    val onTmuxSessions: () -> Unit,
    val onPaste: () -> Unit,
    val onCapture: () -> Unit,
    val onPushKey: () -> Unit,
    val onShowKeyboard: () -> Unit,
    val onEditKeys: () -> Unit,
    val onNextGroup: () -> Unit,
    val onGotoGroup: (String) -> Unit,
    val onNeedsTmux: (AppAction) -> Unit = {},
) {
    fun invoke(b: KeyBinding.App) {
        if (b.action in AppAction.NEEDS_TMUX && !supportsTmux) return onNeedsTmux(b.action)
        when (b.action) {
            AppAction.NEXT_GROUP -> onNextGroup()
            AppAction.GOTO_GROUP -> b.groupId?.let(onGotoGroup)
            AppAction.EDIT_KEYS -> onEditKeys()
            AppAction.PASTE -> onPaste()
            AppAction.SHOW_TABS -> onShowTabs()
            AppAction.COMMAND_PALETTE -> onCommandPalette()
            AppAction.TMUX_SESSIONS -> onTmuxSessions()
            AppAction.SHOW_KEYBOARD -> onShowKeyboard()
            AppAction.CAPTURE -> onCapture()
            AppAction.PUSH_KEY -> onPushKey()
        }
    }
}

/** 一个触发派发出去：送输入 ⇒ 字节出线；App 动作 ⇒ [KeyActions]。 */
fun dispatchBinding(
    b: KeyBinding,
    onSend: (ByteArray) -> Unit,
    actions: KeyActions,
) {
    when (b) {
        is KeyBinding.Input -> onSend(inputBytes(b.presses))
        is KeyBinding.App -> actions.invoke(b)
    }
}

/** 「送输入」的字节：一下接一下拼起来。 */
fun inputBytes(presses: List<KeyPress>): ByteArray {
    val out = ByteArrayOutputStream()
    presses.forEach { out.write(pressBytes(it)) }
    return out.toByteArray()
}

private fun modsOf(mods: Set<KeyMod>) =
    KeyModifiers(ctrl = KeyMod.CTRL in mods, alt = KeyMod.ALT in mods, shift = KeyMod.SHIFT in mods)

/** xterm 的修饰参数：1 ＋ Shift(1) ＋ Alt(2) ＋ Ctrl(4)；不带修饰 ⇒ null。 */
private fun xtermParam(mods: Set<KeyMod>): Int? =
    if (mods.isEmpty()) {
        null
    } else {
        1 + (if (KeyMod.SHIFT in mods) 1 else 0) + (if (KeyMod.ALT in mods) 2 else 0) + (if (KeyMod.CTRL in mods) 4 else 0)
    }

private fun csiLetter(
    c: Char,
    mods: Set<KeyMod>,
) = (xtermParam(mods)?.let { "\u001b[1;$it$c" } ?: "\u001b[$c").toByteArray()

private fun csiTilde(
    n: Int,
    mods: Set<KeyMod>,
) = (xtermParam(mods)?.let { "\u001b[$n;$it~" } ?: "\u001b[$n~").toByteArray()

private fun ss3(
    c: Char,
    mods: Set<KeyMod>,
) = (xtermParam(mods)?.let { "\u001b[1;$it$c" } ?: "\u001bO$c").toByteArray()

/** 单字节的特殊键（Esc · Tab · 回车 · 退格）：Alt 前缀 ESC；Shift-Tab 是 `ESC [ Z`；Ctrl · Shift 别的不变。 */
private fun single(
    b: Int,
    mods: Set<KeyMod>,
): ByteArray {
    if (b == 0x09 && KeyMod.SHIFT in mods) return byteArrayOf(0x1B, '['.code.toByte(), 'Z'.code.toByte())
    return if (KeyMod.ALT in mods) byteArrayOf(0x1B, b.toByte()) else byteArrayOf(b.toByte())
}

private val TILDE = mapOf(SpecialKey.DELETE to 3, SpecialKey.PAGE_UP to 5, SpecialKey.PAGE_DOWN to 6, SpecialKey.F5 to 15, SpecialKey.F6 to 17, SpecialKey.F7 to 18, SpecialKey.F8 to 19, SpecialKey.F9 to 20, SpecialKey.F10 to 21, SpecialKey.F11 to 23, SpecialKey.F12 to 24)

private val SINGLE = mapOf(SpecialKey.ESC to 0x1B, SpecialKey.TAB to 0x09, SpecialKey.ENTER to 0x0D, SpecialKey.BACKSPACE to 0x7F)
private val LETTER =
    mapOf(SpecialKey.UP to 'A', SpecialKey.DOWN to 'B', SpecialKey.RIGHT to 'C', SpecialKey.LEFT to 'D', SpecialKey.HOME to 'H', SpecialKey.END to 'F')
private val SS3 = mapOf(SpecialKey.F1 to 'P', SpecialKey.F2 to 'Q', SpecialKey.F3 to 'R', SpecialKey.F4 to 'S')

/** 一下的字节：字照 UTF-8；组合照粘性修饰那一套（[applyKeyModifiers]）；特殊键照 xterm。 */
fun pressBytes(p: KeyPress): ByteArray =
    when (p) {
        is KeyPress.Text -> p.text.toByteArray(Charsets.UTF_8)
        is KeyPress.Chord -> applyKeyModifiers(p.char.toByteArray(Charsets.UTF_8), modsOf(p.mods))
        is KeyPress.Special -> specialBytes(p.key, p.mods)
    }

private fun specialBytes(
    k: SpecialKey,
    mods: Set<KeyMod>,
): ByteArray {
    SINGLE[k]?.let { return single(it, mods) }
    LETTER[k]?.let { return csiLetter(it, mods) }
    SS3[k]?.let { return ss3(it, mods) }
    return csiTilde(TILDE.getValue(k), mods)
}

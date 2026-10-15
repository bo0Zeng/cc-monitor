package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.data.db.AppAction
import com.ccmonitor.mobile.core.data.db.KeyBinding
import com.ccmonitor.mobile.core.data.db.KeyGroup
import com.ccmonitor.mobile.core.data.db.KeyMod
import com.ccmonitor.mobile.core.data.db.KeyPress
import com.ccmonitor.mobile.core.data.db.SpecialKey
import com.ccmonitor.mobile.core.terminal.KeyModifiers
import com.ccmonitor.mobile.core.terminal.Mod
import com.ccmonitor.mobile.core.terminal.applyKeyModifiers
import java.io.ByteArrayOutputStream

/** 键条上点了做不成的：那台没有 tmux · 要切去的组已经删了。 */
enum class KeyActionFail { NO_TMUX, GROUP_GONE }

/**
 * App 动作束：键条上绑了 App 动作的触发经 [invoke] 触达终端屏已有的回调。
 * 要 tmux 的那几个（[AppAction.NEEDS_TMUX]）在没有 tmux 的服务器上不做、回 [KeyActionFail.NO_TMUX]；
 * [onGotoGroup] 回 false ＝ 那组已经不在。
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
    val onGotoGroup: (String) -> Boolean,
) {
    fun invoke(b: KeyBinding.App): KeyActionFail? {
        if (b.action in AppAction.NEEDS_TMUX && !supportsTmux) return KeyActionFail.NO_TMUX
        when (b.action) {
            AppAction.NEXT_GROUP -> onNextGroup()
            AppAction.GOTO_GROUP -> if (b.groupId?.let(onGotoGroup) != true) return KeyActionFail.GROUP_GONE
            AppAction.EDIT_KEYS -> onEditKeys()
            AppAction.PASTE -> onPaste()
            AppAction.SHOW_TABS -> onShowTabs()
            AppAction.COMMAND_PALETTE -> onCommandPalette()
            AppAction.TMUX_SESSIONS -> onTmuxSessions()
            AppAction.SHOW_KEYBOARD -> onShowKeyboard()
            AppAction.CAPTURE -> onCapture()
            AppAction.PUSH_KEY -> onPushKey()
        }
        return null
    }
}

/**
 * 一个触发派发出去：送输入 ⇒ 取这一刻点亮的修饰（[takeModifiers]，点亮一次的随之熄掉）、带在第一下上出线；
 * App 动作 ⇒ [KeyActions]，不吃修饰。回做不成的原因，做成了 ＝ null。
 */
fun dispatchBinding(
    b: KeyBinding,
    onSend: (ByteArray) -> Unit,
    actions: KeyActions,
    takeModifiers: () -> KeyModifiers,
): KeyActionFail? =
    when (b) {
        is KeyBinding.Input -> {
            onSend(inputBytes(withModifiers(b.presses, takeModifiers())))
            null
        }
        is KeyBinding.App -> actions.invoke(b)
    }

/** 键条上点亮的修饰带在第一下上：特殊键与组合并进它的修饰；一段字拆出头一个字成组合。 */
fun withModifiers(
    presses: List<KeyPress>,
    lit: KeyModifiers,
): List<KeyPress> {
    if (!lit.any || presses.isEmpty()) return presses
    val add = keyModsOf(lit)
    val rest = presses.drop(1)
    return when (val first = presses.first()) {
        is KeyPress.Special -> listOf(first.copy(mods = first.mods + add)) + rest
        is KeyPress.Chord -> listOf(first.copy(mods = first.mods + add)) + rest
        is KeyPress.Text -> {
            if (first.text.isEmpty()) return presses
            val cut = first.text.offsetByCodePoints(0, 1)
            val tail = first.text.substring(cut)
            listOf(KeyPress.Chord(first.text.substring(0, cut), add)) + (if (tail.isEmpty()) rest else listOf(KeyPress.Text(tail)) + rest)
        }
    }
}

private fun keyModsOf(m: KeyModifiers): Set<KeyMod> =
    buildSet {
        if (m.ctrl) add(KeyMod.CTRL)
        if (m.alt) add(KeyMod.ALT)
        if (m.shift) add(KeyMod.SHIFT)
    }

/** 库里的修饰键 ↔ 终端那一份。 */
fun KeyMod.term(): Mod =
    when (this) {
        KeyMod.CTRL -> Mod.CTRL
        KeyMod.ALT -> Mod.ALT
        KeyMod.SHIFT -> Mod.SHIFT
    }

/** 录入一个组合时的判定：送得出 · 送不出（修饰全没起作用）· 与修饰更少的某个组合送出相同。 */
sealed interface PressVerdict {
    data object Sendable : PressVerdict

    data object Unsendable : PressVerdict

    data class SameAs(
        val press: KeyPress,
    ) : PressVerdict
}

/**
 * 照实际送出的字节判：去掉几个修饰后字节不变 ⇒ 那几个没起作用。全去掉也不变 ⇒ 送不出；
 * 留下的修饰最少的那个组合（同样多照 Ctrl · Alt · Shift 的先后）就是「送出相同」的那一个。
 */
fun pressVerdict(p: KeyPress): PressVerdict {
    val mods =
        when (p) {
            is KeyPress.Text -> return PressVerdict.Sendable
            is KeyPress.Special -> p.mods
            is KeyPress.Chord -> p.mods
        }
    if (mods.isEmpty()) return PressVerdict.Sendable
    val sent = pressBytes(p)
    val order = KeyMod.entries.filter { it in mods }
    val fewest =
        subsets(order)
            .filter { it.size < mods.size }
            .sortedBy { it.size }
            .firstOrNull { pressBytes(withOnly(p, it)).contentEquals(sent) }
            ?: return PressVerdict.Sendable
    return if (fewest.isEmpty()) PressVerdict.Unsendable else PressVerdict.SameAs(withOnly(p, fewest))
}

private fun withOnly(
    p: KeyPress,
    mods: Set<KeyMod>,
): KeyPress =
    when (p) {
        is KeyPress.Text -> p
        is KeyPress.Special -> p.copy(mods = mods)
        is KeyPress.Chord -> if (mods.isEmpty()) KeyPress.Text(p.char) else p.copy(mods = mods)
    }

/** 按位取子集；先后照 [order]，同样大小的照它的先后排（[sortedBy] 稳定）。 */
private fun subsets(order: List<KeyMod>): List<Set<KeyMod>> =
    (0 until (1 shl order.size)).map { bits -> order.filterIndexed { i, _ -> bits and (1 shl i) != 0 }.toSet() }

/** 当前组：记着的那组，没记过 · 那组已删 ⇒ 照先后第一组；一个组都没有 ⇒ null。 */
fun currentGroup(
    groups: List<KeyGroup>,
    saved: String?,
): KeyGroup? = groups.firstOrNull { it.id == saved } ?: groups.minByOrNull { it.sortOrder }

/** 「切到下一组」照组的先后轮转；一个组都没有 ⇒ null。 */
fun nextGroupId(
    groups: List<KeyGroup>,
    saved: String?,
): String? {
    val sorted = groups.sortedBy { it.sortOrder }
    val cur = currentGroup(sorted, saved) ?: return null
    return sorted[(sorted.indexOf(cur) + 1) % sorted.size].id
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

/** 一下的字节：字照 UTF-8；组合与软键盘带修饰同一套（[applyKeyModifiers]）；特殊键照 xterm。 */
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

package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.data.db.AppAction
import com.ccmonitor.mobile.core.data.db.KeyBinding
import com.ccmonitor.mobile.core.data.db.KeyGroup
import com.ccmonitor.mobile.core.data.db.KeyMod
import com.ccmonitor.mobile.core.data.db.KeyPress
import com.ccmonitor.mobile.core.data.db.keyBytes
import com.ccmonitor.mobile.core.terminal.KeyModifiers
import com.ccmonitor.mobile.core.terminal.Mod

/** 键条上点了做不成的：那台做不了（[Unavailable.why] 那一句）· 要切去的组已经删了。 */
sealed interface KeyActionFail {
    data class Unavailable(
        val why: String,
    ) : KeyActionFail

    data object GroupGone : KeyActionFail
}

/**
 * App 动作束：键条上绑了 App 动作的触发经 [invoke] 触达终端屏已有的回调。
 * 要那台后端的那几个（终端名单 · 抓屏 · 推公钥）在 [noBackend] 时不做，回 [KeyActionFail.Unavailable]（终端名单两种宿主都列，不看有没有 tmux）；
 * [onGotoGroup] 回 false ＝ 那组已经不在。
 */
class KeyActions(
    val noBackend: String?,
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
        if (b.action in NEEDS_BACKEND) noBackend?.let { return KeyActionFail.Unavailable(it) }
        when (b.action) {
            AppAction.NEXT_GROUP -> onNextGroup()
            AppAction.GOTO_GROUP -> if (b.groupId?.let(onGotoGroup) != true) return KeyActionFail.GroupGone
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

    private companion object {
        val NEEDS_BACKEND: Set<AppAction> = setOf(AppAction.TMUX_SESSIONS, AppAction.CAPTURE, AppAction.PUSH_KEY)
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
            onSend(keyBytes(withModifiers(b.presses, takeModifiers())))
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
    val sent = keyBytes(listOf(p))
    val order = KeyMod.entries.filter { it in mods }
    val fewest =
        subsets(order)
            .filter { it.size < mods.size }
            .sortedBy { it.size }
            .firstOrNull { keyBytes(listOf(withOnly(p, it))).contentEquals(sent) }
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

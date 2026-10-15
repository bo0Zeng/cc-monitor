package com.ccmonitor.mobile.core.data.db

import com.ccmonitor.mobile.core.copy.copyText

/*
 * 键帽不另写时照单击自动写：`Esc` · `Ctrl-C` · `Ctrl-B c` · `git status ⏎` · `:w⏎`。
 * 组合键写法跟桌面同一种（`Ctrl-C`，横线连）。
 */

/** 这颗按键的键帽字：另写了照另写；修饰键写修饰名；其余照单击自动写。[groupName] 给「切到某一组」查组名。 */
fun TermKey.cap(groupName: (String) -> String? = { null }): String =
    label ?: modifier?.let(::modName) ?: tap?.let { autoCap(it, groupName) } ?: ""

/** 一个触发的自动写法。 */
fun autoCap(
    b: KeyBinding,
    groupName: (String) -> String? = { null },
): String =
    when (b) {
        is KeyBinding.App ->
            when (b.action) {
                AppAction.NEXT_GROUP -> SWITCH_KEY_LABEL
                AppAction.GOTO_GROUP -> b.groupId?.let(groupName) ?: appActionName(b.action)
                else -> appActionName(b.action)
            }
        is KeyBinding.Input -> inputCap(b.presses)
    }

/** 切到下一组那颗的键帽：画成左右箭头图标，名字本身是数据、读屏念它。 */
const val SWITCH_KEY_LABEL = "⇌"

/** App 动作的名字（文案表）。 */
fun appActionName(a: AppAction): String =
    when (a) {
        AppAction.NEXT_GROUP -> copyText("mobKeys.action.nextGroup")
        AppAction.GOTO_GROUP -> copyText("mobKeys.action.gotoGroup")
        AppAction.EDIT_KEYS -> copyText("mobKeys.action.editKeys")
        AppAction.PASTE -> copyText("mobKeys.action.paste")
        AppAction.SHOW_TABS -> copyText("mobKeys.action.tabs")
        AppAction.COMMAND_PALETTE -> copyText("mobKeys.action.palette")
        AppAction.TMUX_SESSIONS -> copyText("mobKeys.action.tmux")
        AppAction.SHOW_KEYBOARD -> copyText("mobKeys.action.keyboard")
        AppAction.CAPTURE -> copyText("mobKeys.action.capture")
        AppAction.PUSH_KEY -> copyText("mobKeys.action.pushKey")
    }

fun modName(m: KeyMod): String =
    when (m) {
        KeyMod.CTRL -> "Ctrl"
        KeyMod.ALT -> "Alt"
        KeyMod.SHIFT -> "Shift"
    }

fun specialName(k: SpecialKey): String = SPECIAL_NAMES[k] ?: k.name // F1–F12 照名字

private val SPECIAL_NAMES: Map<SpecialKey, String> =
    mapOf(
        SpecialKey.ESC to "Esc",
        SpecialKey.TAB to "Tab",
        SpecialKey.ENTER to "⏎",
        SpecialKey.BACKSPACE to "⌫",
        SpecialKey.DELETE to "Del",
        SpecialKey.UP to "↑",
        SpecialKey.DOWN to "↓",
        SpecialKey.LEFT to "←",
        SpecialKey.RIGHT to "→",
        SpecialKey.HOME to "Home",
        SpecialKey.END to "End",
        SpecialKey.PAGE_UP to "PgUp",
        SpecialKey.PAGE_DOWN to "PgDn",
    )

/** 一下的写法：修饰照 Ctrl · Alt · Shift 的先后在前，字母组合写大写（`Ctrl-B`）。 */
fun pressCap(p: KeyPress): String =
    when (p) {
        is KeyPress.Text -> p.text
        is KeyPress.Special -> withMods(p.mods, specialName(p.key))
        is KeyPress.Chord -> withMods(p.mods, if (p.char == " ") "Space" else p.char.uppercase())
    }

private fun withMods(
    mods: Set<KeyMod>,
    base: String,
) = (KeyMod.entries.filter { it in mods }.map(::modName) + base).joinToString("-")

/** 几下之间空一格；回车紧跟在不带空格的一段字后面时不空（`:w⏎`，而 `git status ⏎`）。 */
private fun inputCap(presses: List<KeyPress>): String =
    buildString {
        presses.forEachIndexed { i, p ->
            val prev = presses.getOrNull(i - 1)
            val glued = p == KeyPress.Special(SpecialKey.ENTER) && prev is KeyPress.Text && ' ' !in prev.text
            if (i > 0 && !glued) append(' ')
            append(pressCap(p))
        }
    }

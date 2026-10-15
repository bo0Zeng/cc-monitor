package com.ccmonitor.mobile.core.data.db

import androidx.room.Entity
import androidx.room.PrimaryKey
import androidx.room.TypeConverter

/*
 * 终端键条：几「组」、每组若干「按键」，一次显示一组。存在这部手机的库里（手机终端是 SSH 直连 PTY，送键不经那台核心）。
 *
 * 每颗按键三个触发（单击 · 双击 · 长按），各绑「送一串输入」「一个 App 动作」或不设（[KeyBinding] 为 null ＝ 自然行为：
 * 双击没设 ＝ 两次单击；长按没设 ＝ 能连发的键按住连发、其余等于单击一次）。修饰键（[TermKey.modifier]）不绑触发。
 * 组的先后只有一份（[KeyGroup.sortOrder]）：「切到下一组」照它轮转，切换组的列表也照它列。
 */

/** 一组按键。默认那三组的 id 固定（见 DefaultKeys），用户建的组 id 随机。 */
@Entity(tableName = "key_groups")
data class KeyGroup(
    @PrimaryKey val id: String,
    val name: String,
    val sortOrder: Int,
)

/**
 * 一颗按键。
 *
 * - [label]：键帽另写的字；null ＝ 照单击自动写。[icon]：键帽另选的图标名；null ＝ 不用图标。
 * - [modifier]：非空 ＝ 这是一颗修饰键（Ctrl / Alt / Shift），三个触发都是 null。
 * - [doubleTapMs] · [longPressMs]：这一颗另设的两个时长；null ＝ 跟总的那一份。
 * - [defaultId]：从哪颗默认按键来的（「还原默认」照它找回原样）；用户自建的 ＝ null。
 * - [unconverted]：升级那一次没转过来的几处（照实列给用户看）；改过或删掉之后就没了。
 */
@Entity(tableName = "term_keys")
data class TermKey(
    @PrimaryKey val id: String,
    val groupId: String,
    val sortOrder: Int,
    val label: String? = null,
    val icon: String? = null,
    val modifier: KeyMod? = null,
    val tap: KeyBinding? = null,
    val doubleTap: KeyBinding? = null,
    val longPress: KeyBinding? = null,
    val doubleTapMs: Int? = null,
    val longPressMs: Int? = null,
    val defaultId: String? = null,
    val unconverted: List<UnconvertedNote>? = null,
)

/** 修饰键。线上名（库里存的字）是小写英文。 */
enum class KeyMod(
    val wire: String,
) {
    CTRL("ctrl"),
    ALT("alt"),
    SHIFT("shift"),
    ;

    companion object {
        fun fromWire(s: String): KeyMod? = entries.firstOrNull { it.wire == s }
    }
}

/** 手机软键盘打不出的单颗键（录入时那排特殊键就是这一份）。线上名是库里存的字。 */
enum class SpecialKey(
    val wire: String,
) {
    ESC("esc"),
    TAB("tab"),
    ENTER("enter"),
    BACKSPACE("backspace"),
    DELETE("delete"),
    UP("up"),
    DOWN("down"),
    LEFT("left"),
    RIGHT("right"),
    HOME("home"),
    END("end"),
    PAGE_UP("pgup"),
    PAGE_DOWN("pgdn"),
    F1("f1"),
    F2("f2"),
    F3("f3"),
    F4("f4"),
    F5("f5"),
    F6("f6"),
    F7("f7"),
    F8("f8"),
    F9("f9"),
    F10("f10"),
    F11("f11"),
    F12("f12"),
    ;

    companion object {
        fun fromWire(s: String): SpecialKey? = entries.firstOrNull { it.wire == s }

        /** 长按没设时按住连发的那几颗（方向 · 删除 · 翻页）。 */
        val REPEATABLE: Set<SpecialKey> = setOf(UP, DOWN, LEFT, RIGHT, BACKSPACE, DELETE, PAGE_UP, PAGE_DOWN)
    }
}

/** 「送输入」里的一下。 */
sealed interface KeyPress {
    /** 照原样打出的一段字（软键盘打的字母数字）。 */
    data class Text(
        val text: String,
    ) : KeyPress

    /** 一颗特殊键，可带修饰（`Shift-Tab` · `Ctrl-End`）。 */
    data class Special(
        val key: SpecialKey,
        val mods: Set<KeyMod> = emptySet(),
    ) : KeyPress

    /** 带修饰的一个字（`Ctrl-B` · `Alt-x`）。[char] 是一个码点；[mods] 不为空（不带修饰的字是 [Text]）。 */
    data class Chord(
        val char: String,
        val mods: Set<KeyMod>,
    ) : KeyPress
}

/** App 动作。线上名是库里存的字。 */
enum class AppAction(
    val wire: String,
) {
    NEXT_GROUP("next_group"),

    /** 切到某一组：[KeyBinding.App.groupId] 存目标组的 id（不存组名，改名不断）。 */
    GOTO_GROUP("goto_group"),
    EDIT_KEYS("edit_keys"),
    PASTE("paste"),
    SHOW_TABS("show_tabs"),
    COMMAND_PALETTE("command_palette"),
    TMUX_SESSIONS("tmux_sessions"),
    SHOW_KEYBOARD("show_keyboard"),
    CAPTURE("capture"),
    PUSH_KEY("push_key"),
    ;

    companion object {
        fun fromWire(s: String): AppAction? = entries.firstOrNull { it.wire == s }

        /** 要那台有 tmux 才做得了的。 */
        val NEEDS_TMUX: Set<AppAction> = setOf(TMUX_SESSIONS, CAPTURE, PUSH_KEY)
    }
}

/** 一个触发绑的东西。不设 ＝ 这个触发那一格为 null。 */
sealed interface KeyBinding {
    data class Input(
        val presses: List<KeyPress>,
    ) : KeyBinding

    data class App(
        val action: AppAction,
        val groupId: String? = null,
    ) : KeyBinding
}

/** 长按没设时，这一下单击能不能按住连发：只送一颗可连发的特殊键（不带修饰），或只打一个字。 */
fun KeyBinding?.isRepeatable(): Boolean {
    val one = (this as? KeyBinding.Input)?.presses?.singleOrNull() ?: return false
    return when (one) {
        is KeyPress.Special -> one.mods.isEmpty() && one.key in SpecialKey.REPEATABLE
        is KeyPress.Text -> one.text.codePointCount(0, one.text.length) == 1
        is KeyPress.Chord -> false
    }
}

/** 升级那一次没转过来的一处：[issue] 是哪里没转过来，[original] 是当初填的原文（用户自己的东西，照写）。 */
data class UnconvertedNote(
    val issue: Issue,
    val original: String,
) {
    enum class Issue(
        val wire: String,
    ) {
        /** 单击的内容认不出 ⇒ 单击未设。 */
        TAP_UNPARSED("tap_unparsed"),

        /** 长按的内容认不出 ⇒ 长按未设。 */
        LONG_UNPARSED("long_unparsed"),

        /** 原来长按连发、这颗键不能连发 ⇒ 长按未设。 */
        LONG_NO_REPEAT("long_no_repeat"),
        ;

        companion object {
            fun fromWire(s: String): Issue? = entries.firstOrNull { it.wire == s }
        }
    }
}

/** 库里几格小 JSON ↔ 类型。读不懂的格当没设（那几格只由本端写）。 */
class KeyConverters {
    @TypeConverter
    fun bindingToText(b: KeyBinding?): String? = b?.let(KeyJson::encodeBinding)

    @TypeConverter
    fun textToBinding(s: String?): KeyBinding? = s?.let(KeyJson::decodeBinding)

    @TypeConverter
    fun modToText(m: KeyMod?): String? = m?.wire

    @TypeConverter
    fun textToMod(s: String?): KeyMod? = s?.let(KeyMod::fromWire)

    @TypeConverter
    fun notesToText(n: List<UnconvertedNote>?): String? = n?.takeIf { it.isNotEmpty() }?.let(KeyJson::encodeNotes)

    @TypeConverter
    fun textToNotes(s: String?): List<UnconvertedNote>? = s?.let(KeyJson::decodeNotes)?.takeIf { it.isNotEmpty() }
}

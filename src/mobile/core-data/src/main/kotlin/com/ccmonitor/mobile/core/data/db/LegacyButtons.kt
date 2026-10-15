package com.ccmonitor.mobile.core.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import com.ccmonitor.mobile.core.copy.copyText

/*
 * 第 17 版以前的按钮表（`custom_buttons`）：只给迁移用。
 *
 * 12→13、13→14 两跳当年往旧表里种过默认按键，这里留着那一份原样（那两跳照旧种，17→18 再把它们转过去）；
 * 17→18 把旧表的每一行照 [convertLegacyButtons] 转成组与按键，然后删旧表。
 */

/** 旧表里的一行（17→18 读出来的那几格）。 */
data class LegacyButton(
    val id: String,
    val hostId: String?,
    val label: String,
    val command: String,
    val sortOrder: Int,
    val ctrl: Boolean = false,
    val alt: Boolean = false,
    val shift: Boolean = false,
    val type: String = "command",
    val btnGroup: String = "main",
    val longPressMode: String = "none",
    val longPressCommand: String = "",
    val longPressThresholdMs: Int = 400,
)

/** 旧表时代的一台机器：按机器迁成的组照它的名字与先后。 */
data class LegacyHost(
    val id: String,
    val label: String,
    val sortOrder: Int,
)

private const val OLD_MAIN = "main"
private const val OLD_FN = "fn"
private const val OLD_ALT = "alt"
private const val OLD_VIM = "vim"

/** 旧工具条上那三颗内建（现在是固定工具条，不进按键数据）。 */
private val OLD_TOOLBAR_ACTIONS = setOf("show_tabs", "command_palette", "tmux_sessions")

/** 旧种子：12→13、13→14 种进旧表的那一份，原样（几个名字当年取自文案表）。 */
private val LEGACY_SEED: List<Array<String>> =
    listOf(
        arrayOf("ak-tb-tabs", "▤ tab", "builtin", "show_tabs", OLD_MAIN),
        arrayOf("ak-tb-cmd", copyText("mobDefaultButtons.label.command"), "builtin", "command_palette", OLD_MAIN),
        arrayOf("ak-tb-tmux", copyText("mobDefaultButtons.label.tmux"), "builtin", "tmux_sessions", OLD_MAIN),
        arrayOf("ak-fn-switch", SWITCH_KEY_LABEL, "switch", OLD_ALT, OLD_FN),
        arrayOf("ak-fn-ctrl", "Ctrl", "builtin", "toggle_ctrl", OLD_FN),
        arrayOf("ak-fn-tab", "Tab", "escape", "\\t", OLD_FN),
        arrayOf("ak-fn-esc", "Esc", "escape", "\\e", OLD_FN),
        arrayOf("ak-fn-up", "↑", "escape", "\\e[A", OLD_FN),
        arrayOf("ak-fn-down", "↓", "escape", "\\e[B", OLD_FN),
        arrayOf("ak-fn-left", "←", "escape", "\\e[D", OLD_FN),
        arrayOf("ak-fn-right", "→", "escape", "\\e[C", OLD_FN),
        arrayOf("ak-fn-paste", copyText("mobDefaultButtons.label.paste"), "builtin", "paste", OLD_FN),
        arrayOf("ak-alt-switch", SWITCH_KEY_LABEL, "switch", OLD_VIM, OLD_ALT),
        arrayOf("ak-alt-alt", "Alt", "builtin", "toggle_alt", OLD_ALT),
        arrayOf("ak-alt-ctrlend", "Ctrl+End", "escape", "\\e[1;5F", OLD_ALT),
        arrayOf("ak-alt-home", "Home", "escape", "\\e[H", OLD_ALT),
        arrayOf("ak-alt-end", "End", "escape", "\\e[F", OLD_ALT),
        arrayOf("ak-alt-pgup", "PgUp", "escape", "\\e[5~", OLD_ALT),
        arrayOf("ak-alt-pgdn", "PgDn", "escape", "\\e[6~", OLD_ALT),
        arrayOf("ak-alt-esc", "Esc", "escape", "\\e", OLD_ALT),
        arrayOf("ak-vim-switch", SWITCH_KEY_LABEL, "switch", OLD_FN, OLD_VIM),
        arrayOf("ak-vim-esc", "Esc", "escape", "\\e", OLD_VIM),
        arrayOf("ak-vim-i", "i", "escape", "i", OLD_VIM),
        arrayOf("ak-vim-colon", ":", "escape", ":", OLD_VIM),
        arrayOf("ak-vim-slash", "/", "escape", "/", OLD_VIM),
        arrayOf("ak-vim-w", ":w↵", "escape", ":w\\n", OLD_VIM),
        arrayOf("ak-vim-wq", ":wq↵", "escape", ":wq\\n", OLD_VIM),
        arrayOf("ak-vim-q", ":q!↵", "escape", ":q!\\n", OLD_VIM),
        arrayOf("ak-vim-dd", "dd", "escape", "dd", OLD_VIM),
        arrayOf("ak-vim-yy", "yy", "escape", "yy", OLD_VIM),
        arrayOf("ak-vim-p", "p", "escape", "p", OLD_VIM),
        arrayOf("ak-vim-u", "u", "escape", "u", OLD_VIM),
        arrayOf("ak-vim-gg", "gg", "escape", "gg", OLD_VIM),
        arrayOf("ak-vim-G", "G", "escape", "G", OLD_VIM),
        arrayOf("ak-vim-h", "h", "escape", "h", OLD_VIM),
        arrayOf("ak-vim-j", "j", "escape", "j", OLD_VIM),
        arrayOf("ak-vim-k", "k", "escape", "k", OLD_VIM),
        arrayOf("ak-vim-l", "l", "escape", "l", OLD_VIM),
        arrayOf("ak-vim-0", "0", "escape", "0", OLD_VIM),
        arrayOf("ak-vim-dollar", "$", "escape", "$", OLD_VIM),
    )

/** 旧种子 id → 当年种下的名字；17→18 据此认出「名字没改过的种子」（那是代码写的字，不是用户的）。 */
private val LEGACY_SEED_LABELS: Map<String, String> by lazy { legacySeedButtons().associate { it.id to it.label } }

/** 旧种子的那几行（全局、修饰与长按取旧表的列默认）。 */
internal fun legacySeedButtons(): List<LegacyButton> =
    LEGACY_SEED.mapIndexed { i, r ->
        LegacyButton(id = r[0], hostId = null, label = r[1], command = r[3], sortOrder = i, type = r[2], btnGroup = r[4])
    }

/** 12→13、13→14 往旧表里补种旧默认键：`INSERT OR IGNORE` 固定 id，不碰已有的行。 */
fun seedLegacyButtons(db: SupportSQLiteDatabase) {
    legacySeedButtons().forEach { b ->
        db.execSQL(
            "INSERT OR IGNORE INTO `custom_buttons` (`id`,`hostId`,`label`,`command`,`sortOrder`,`type`,`btnGroup`) " +
                "VALUES (?, NULL, ?, ?, ?, ?, ?)",
            arrayOf<Any?>(b.id, b.label, b.command, b.sortOrder, b.type, b.btnGroup),
        )
    }
}

/**
 * 旧按钮 → 新的组与按键。
 *
 * 组：fn · alt · vim 三组成「常用」「shell」「vim」（id 用默认那三组的），先后照 ⇌ 那一串（从 fn 起顺着每组第一颗 ⇌ 的目标走）；
 * 用户自建的组照原名，接在后面；main 组用户自己加的成「按钮栏」，再往后；只在某台显示的按键，按机器各成一组、组名是机器名，排在最后。
 * main 组那三颗工具条内建（标签页 · 命令 · tmux 会话）丢掉：工具条现在是固定的。机器已经删了的那几颗也丢掉（本来就显示不出来）。
 * 组名重了在后面加序号。没有按键的组不建。
 *
 * 按键：照 [convertLegacyButton]。组内先后照旧的 `sortOrder, id`。
 */
fun convertLegacyButtons(
    rows: List<LegacyButton>,
    hosts: List<LegacyHost>,
    systemLongPressMs: Int,
): Pair<List<KeyGroup>, List<TermKey>> {
    val kept = rows.filterNot { it.isOldToolbarBuiltin() }
    val global = kept.filter { it.hostId == null }
    val byOldGroup = global.filter { it.btnGroup != OLD_MAIN }.groupBy { it.btnGroup }
    val defaults = defaultKeyGroups().associate { it.id to it.name }
    val newIds = mapOf(OLD_FN to DEFAULT_GROUP_COMMON, OLD_ALT to DEFAULT_GROUP_SHELL, OLD_VIM to DEFAULT_GROUP_VIM)

    val planned = mutableListOf<Pair<KeyGroup, List<LegacyButton>>>()
    switchChain(byOldGroup).forEach { g ->
        val group = newIds[g]?.let { KeyGroup(it, defaults.getValue(it), 0) } ?: KeyGroup("kg-g-$g", g, 0)
        planned += group to byOldGroup.getValue(g)
    }
    global.filter { it.btnGroup == OLD_MAIN }.takeIf { it.isNotEmpty() }?.let {
        planned += KeyGroup("kg-bar", copyText("mobKeys.seed.bar"), 0) to it
    }
    val hostRows = kept.mapNotNull { b -> b.hostId?.let { it to b } }.groupBy({ it.first }, { it.second })
    hosts.sortedWith(compareBy({ it.sortOrder }, { it.label }, { it.id })).forEach { h ->
        hostRows[h.id]?.let { planned += KeyGroup("kg-host-${h.id}", h.label, 0) to it }
    }

    val taken = mutableSetOf<String>()
    val groups = mutableListOf<KeyGroup>()
    val keys = mutableListOf<TermKey>()
    planned.forEachIndexed { i, (g, members) ->
        val name = generateSequence(1) { it + 1 }.map { n -> if (n == 1) g.name else "${g.name} $n" }.first { it !in taken }
        taken += name
        groups += g.copy(name = name, sortOrder = i)
        members.sortedWith(ORDER).forEachIndexed { j, b -> keys += convertLegacyButton(b, g.id, j, systemLongPressMs) }
    }
    return groups to keys
}

private fun LegacyButton.isOldToolbarBuiltin() =
    btnGroup == OLD_MAIN && type.equals("builtin", true) && command.trim().lowercase() in OLD_TOOLBAR_ACTIONS

private val LegacyButton.hasMods: Boolean get() = ctrl || alt || shift

/** ⇌ 那一串：从 fn 起，顺着每组第一颗 ⇌ 的目标走；走不下去的再从剩下的里按名字挑一组接着走。 */
private fun switchChain(byOldGroup: Map<String, List<LegacyButton>>): List<String> {
    val chain = mutableListOf<String>()

    fun walk(start: String) {
        var g: String? = start
        while (g != null && g in byOldGroup && g !in chain) {
            chain += g
            g =
                byOldGroup
                    .getValue(g)
                    .sortedWith(ORDER)
                    .firstOrNull { it.type.equals("switch", true) }
                    ?.command
                    ?.trim()
        }
    }
    walk(OLD_FN)
    byOldGroup.keys.sorted().forEach(::walk)
    return chain
}

private val ORDER = compareBy<LegacyButton>({ it.sortOrder }, { it.id })

/**
 * 一颗旧按钮 → 一颗按键（放进 [groupId] 的第 [sortOrder] 位，id 照旧）。
 *
 * - 输入文字（command）：单击 ＝ 那段字 ＋ 回车；按一个键（keycode，带修饰）：认得出的换成特殊键或组合，非 ASCII 的字照原样打；
 *   转义（escape）：认得出的换成特殊键 · 组合 · 字，认不出的进「未能转换」、单击没设；
 *   内建（builtin）：同名 App 动作，Ctrl 粘性 · Alt 粘性成修饰键；切到按钮组（switch）：单击切到下一组、长按编辑按键。
 * - 长按单发：长按 ＝ 那段载荷照这颗原来的类型转；长按连发：能连发的键长按没设（本来就连发），不能连发的进「未能转换」、长按没设。
 * - 长按阈值和系统的不同才写成这颗的长按时长；连发间隔丢掉。
 * - 名字：没改过的旧种子名、或和自动写出来的一样 ⇒ 不另写（照单击自动写）；其余照旧。
 */
fun convertLegacyButton(
    b: LegacyButton,
    groupId: String,
    sortOrder: Int,
    systemLongPressMs: Int,
): TermKey {
    val notes = mutableListOf<UnconvertedNote>()
    val modifier = legacyModifier(b)
    val isSwitch = b.type.trim().lowercase() == "switch"
    val tap =
        when {
            modifier != null -> null
            isSwitch -> KeyBinding.App(AppAction.NEXT_GROUP)
            else -> convertPayload(b, b.command) ?: notes.miss(UnconvertedNote.Issue.TAP_UNPARSED, b, b.command)
        }
    val mode = b.longPressMode.trim().lowercase()
    val longPress = if (isSwitch) KeyBinding.App(AppAction.EDIT_KEYS) else legacyLongPress(b, mode, tap, modifier, notes)
    val longPressMs = b.longPressThresholdMs.takeIf { !isSwitch && mode in LONG_MODES && it != systemLongPressMs }
    val key = TermKey(id = b.id, groupId = groupId, sortOrder = sortOrder, modifier = modifier, tap = tap, longPress = longPress, longPressMs = longPressMs)
    val auto = key.cap()
    val label = b.label.takeUnless { it.isBlank() || it == auto || LEGACY_SEED_LABELS[b.id] == it }
    return key.copy(label = label, unconverted = notes.takeIf { it.isNotEmpty() })
}

private val LONG_MODES = setOf("discrete", "repeat")

/** 记下没转过来的一处，回 null（那个触发没设）。 */
private fun MutableList<UnconvertedNote>.miss(
    issue: UnconvertedNote.Issue,
    b: LegacyButton,
    raw: String,
): KeyBinding? {
    this += UnconvertedNote(issue, original(b, raw))
    return null
}

/** Ctrl 粘性 · Alt 粘性 ⇒ 修饰键。 */
private fun legacyModifier(b: LegacyButton): KeyMod? {
    if (b.type.trim().lowercase() != "builtin") return null
    return when (b.command.trim().lowercase()) {
        "toggle_ctrl" -> KeyMod.CTRL
        "toggle_alt" -> KeyMod.ALT
        else -> null
    }
}

/** 长按：单发 ⇒ 载荷照类型转；连发 ⇒ 能连发的没设、不能连发的记下；别的样式旧的也当没设。 */
private fun legacyLongPress(
    b: LegacyButton,
    mode: String,
    tap: KeyBinding?,
    modifier: KeyMod?,
    notes: MutableList<UnconvertedNote>,
): KeyBinding? =
    when (mode) {
        "discrete" ->
            convertPayload(b, b.longPressCommand)?.takeIf { modifier == null }
                ?: notes.miss(UnconvertedNote.Issue.LONG_UNPARSED, b, b.longPressCommand)
        "repeat" -> if (tap != null && !tap.isRepeatable()) notes.miss(UnconvertedNote.Issue.LONG_NO_REPEAT, b, b.command) else null
        else -> null
    }

/** 一段载荷照这颗旧按钮的类型转（单击与长按单发共用）；认不出 ⇒ null。 */
private fun convertPayload(
    b: LegacyButton,
    payload: String,
): KeyBinding? =
    when (b.type.trim().lowercase()) {
        "escape" -> legacyEscapePresses(payload)?.takeIf { it.isNotEmpty() }?.let { KeyBinding.Input(it) }
        "builtin" -> legacyBuiltinAction(payload)?.let { KeyBinding.App(it) }
        "switch" -> KeyBinding.App(AppAction.NEXT_GROUP)
        // command · keycode（以及认不得的类型，旧派发也当命令行）：有修饰按一个键，没修饰当命令行。
        else -> if (b.hasMods) legacyChord(payload, b) else commandLine(payload)
    }

private fun commandLine(s: String) =
    KeyBinding.Input(if (s.isEmpty()) listOf(KeyPress.Special(SpecialKey.ENTER)) else listOf(KeyPress.Text(s), KeyPress.Special(SpecialKey.ENTER)))

private fun legacyBuiltinAction(id: String): AppAction? =
    when (id.trim().lowercase()) {
        "show_tabs" -> AppAction.SHOW_TABS
        "command_palette" -> AppAction.COMMAND_PALETTE
        "tmux_sessions" -> AppAction.TMUX_SESSIONS
        "paste" -> AppAction.PASTE
        "capture" -> AppAction.CAPTURE
        "push_key" -> AppAction.PUSH_KEY
        "show_keyboard" -> AppAction.SHOW_KEYBOARD
        else -> null
    }

/** 按一个键：命名键（tab · enter · esc · space）照名字，其余取第一个字；非 ASCII 的字旧派发不加修饰 ⇒ 照原样打。 */
private fun legacyChord(
    command: String,
    b: LegacyButton,
): KeyBinding? {
    val mods =
        buildSet {
            if (b.ctrl) add(KeyMod.CTRL)
            if (b.alt) add(KeyMod.ALT)
            if (b.shift) add(KeyMod.SHIFT)
        }
    val press =
        when (command.trim().lowercase()) {
            "tab" -> KeyPress.Special(SpecialKey.TAB, mods)
            "enter", "return" -> KeyPress.Special(SpecialKey.ENTER, mods)
            "esc", "escape" -> KeyPress.Special(SpecialKey.ESC, mods)
            "space" -> KeyPress.Chord(" ", mods)
            "" -> return null
            else -> {
                val first = command.substring(0, command.offsetByCodePoints(0, 1))
                if (first.length == 1 && first[0].code < 0x80) KeyPress.Chord(first, mods) else KeyPress.Text(first)
            }
        }
    return KeyBinding.Input(listOf(press))
}

/** 「未能转换」里写的原文：带修饰的写成旧的 `Ctrl+X` 样子，其余照填的字。 */
private fun original(
    b: LegacyButton,
    payload: String,
): String =
    if (b.type.trim().lowercase() in setOf("command", "keycode") && b.hasMods) {
        buildString {
            if (b.ctrl) append("Ctrl+")
            if (b.alt) append("Alt+")
            if (b.shift) append("Shift+")
            append(payload)
        }
    } else {
        payload
    }

/** 旧转义写法 → 字节：`\e` · `\n` · `\r` · `\t` · `\\` · `\xNN`；认不得的 `\X` 反斜杠照字面。 */
private fun legacyEscapeBytes(seq: String): ByteArray {
    val out = java.io.ByteArrayOutputStream(seq.length)
    val simple = mapOf('e' to 0x1B, 'n' to 0x0A, 'r' to 0x0D, 't' to 0x09, '\\' to 0x5C)
    var i = 0
    while (i < seq.length) {
        val c = seq[i]
        if (c == '\\' && i + 1 < seq.length) {
            val n = seq[i + 1]
            val hex = if (n == 'x') seq.substring(i + 2, minOf(i + 4, seq.length)) else ""
            when {
                n in simple -> {
                    out.write(simple.getValue(n))
                    i += 2
                }
                hex.length == 2 && hex.toIntOrNull(16) != null -> {
                    out.write(hex.toInt(16))
                    i += 4
                }
                else -> {
                    out.write('\\'.code)
                    i += 1
                }
            }
        } else {
            val start = i
            while (i < seq.length && !(seq[i] == '\\' && i + 1 < seq.length)) i++
            out.write(seq.substring(start, i).toByteArray(Charsets.UTF_8))
        }
    }
    return out.toByteArray()
}

/** xterm 修饰参数（`1;5` 的 5）→ 修饰。 */
private fun xtermMods(p: Int?): Set<KeyMod>? {
    if (p == null || p == 1) return emptySet()
    val bits = p - 1
    if (bits !in 1..7) return null
    return buildSet {
        if (bits and 1 != 0) add(KeyMod.SHIFT)
        if (bits and 2 != 0) add(KeyMod.ALT)
        if (bits and 4 != 0) add(KeyMod.CTRL)
    }
}

private val CSI_LETTER = mapOf('A' to SpecialKey.UP, 'B' to SpecialKey.DOWN, 'C' to SpecialKey.RIGHT, 'D' to SpecialKey.LEFT, 'H' to SpecialKey.HOME, 'F' to SpecialKey.END)
private val CSI_TILDE =
    mapOf(
        1 to SpecialKey.HOME,
        3 to SpecialKey.DELETE,
        4 to SpecialKey.END,
        5 to SpecialKey.PAGE_UP,
        6 to SpecialKey.PAGE_DOWN,
        7 to SpecialKey.HOME,
        8 to SpecialKey.END,
        15 to SpecialKey.F5,
        17 to SpecialKey.F6,
        18 to SpecialKey.F7,
        19 to SpecialKey.F8,
        20 to SpecialKey.F9,
        21 to SpecialKey.F10,
        23 to SpecialKey.F11,
        24 to SpecialKey.F12,
    )
private val SS3 = mapOf('P' to SpecialKey.F1, 'Q' to SpecialKey.F2, 'R' to SpecialKey.F3, 'S' to SpecialKey.F4) + CSI_LETTER

/** 一段 CSI（`ESC [` 之后，[from] 指参数头）→ 一下与下一个位置；认不出 ⇒ null。 */
private fun csi(
    bytes: ByteArray,
    from: Int,
): Pair<KeyPress, Int>? {
    var j = from
    while (j < bytes.size && (bytes[j].toInt().toChar().isDigit() || bytes[j].toInt() == ';'.code)) j++
    if (j >= bytes.size) return null
    val params = String(bytes, from, j - from, Charsets.US_ASCII).split(';').filter { it.isNotEmpty() }.map { it.toInt() }
    return csiPress(params, bytes[j].toInt().toChar())?.let { it to j + 1 }
}

private fun csiPress(
    params: List<Int>,
    fin: Char,
): KeyPress? {
    if (fin == 'Z') return KeyPress.Special(SpecialKey.TAB, setOf(KeyMod.SHIFT)).takeIf { params.isEmpty() }
    val key =
        when (fin) {
            in CSI_LETTER -> CSI_LETTER.getValue(fin).takeIf { params.isEmpty() || params[0] == 1 }
            '~' -> params.firstOrNull()?.let { CSI_TILDE[it] }
            else -> null
        }
    val mods = xtermMods(params.getOrNull(1)).takeIf { params.size <= 2 }
    return if (key != null && mods != null) KeyPress.Special(key, mods) else null
}

/** 控制字节 → 一下（Tab · 回车 · 退格 · Ctrl-字）。 */
private fun controlPress(b: Int): KeyPress? =
    when (b) {
        0x09 -> KeyPress.Special(SpecialKey.TAB)
        0x0A, 0x0D -> KeyPress.Special(SpecialKey.ENTER)
        0x7F -> KeyPress.Special(SpecialKey.BACKSPACE)
        0x00 -> KeyPress.Chord("@", setOf(KeyMod.CTRL))
        in 0x01..0x1A -> KeyPress.Chord(('a' + (b - 1)).toString(), setOf(KeyMod.CTRL))
        in 0x1C..0x1F -> KeyPress.Chord(charArrayOf('\\', ']', '^', '_')[b - 0x1C].toString(), setOf(KeyMod.CTRL))
        else -> null
    }

/**
 * 旧转义按钮的内容 → 几下。认得：ESC（单独 · 后跟一个可打印字 ＝ Alt-字）· CSI / SS3 的方向 · Home · End · 翻页 · Del · F1–F12（带 xterm 修饰）·
 * `ESC [ Z`（Shift-Tab）· Tab · 回车（`\n` 与 `\r`）· 退格 · Ctrl-字 · 可打印的字（UTF-8）。别的一概认不出 ⇒ null。
 */
fun legacyEscapePresses(seq: String): List<KeyPress>? {
    val bytes = legacyEscapeBytes(seq)
    val out = mutableListOf<KeyPress>()
    var i = 0
    while (i < bytes.size) {
        val b = bytes[i].toInt() and 0xFF
        val (press, next) =
            when {
                b == 0x1B -> escPress(bytes, i)
                b < 0x20 || b == 0x7F -> controlPress(b)?.let { it to i + 1 }
                else -> textRun(bytes, i)
            } ?: return null
        val last = out.lastOrNull()
        if (press is KeyPress.Text && last is KeyPress.Text) out[out.lastIndex] = KeyPress.Text(last.text + press.text) else out += press
        i = next
    }
    return out
}

/** ESC 起的一下：CSI · SS3 · Alt-字 · 单独的 Esc。 */
private fun escPress(
    bytes: ByteArray,
    i: Int,
): Pair<KeyPress, Int>? {
    val n = if (i + 1 < bytes.size) bytes[i + 1].toInt() and 0xFF else -1
    val ss3 = if (n == 'O'.code && i + 2 < bytes.size) SS3[bytes[i + 2].toInt().toChar()] else null
    return when {
        n == '['.code -> csi(bytes, i + 2)
        ss3 != null -> KeyPress.Special(ss3) to i + 3
        n in 0x21..0x7E -> KeyPress.Chord(n.toChar().toString(), setOf(KeyMod.ALT)) to i + 2
        else -> KeyPress.Special(SpecialKey.ESC) to i + 1
    }
}

/** 一段可打印的字（UTF-8，错的字节 ⇒ null）。 */
private fun textRun(
    bytes: ByteArray,
    from: Int,
): Pair<KeyPress, Int>? {
    var i = from
    while (i < bytes.size && (bytes[i].toInt() and 0xFF).let { it >= 0x20 && it != 0x7F }) i++
    return decodeUtf8Strict(bytes, from, i)?.let { KeyPress.Text(it) to i }
}

private fun decodeUtf8Strict(
    bytes: ByteArray,
    from: Int,
    to: Int,
): String? =
    runCatching {
        Charsets.UTF_8
            .newDecoder()
            .onMalformedInput(java.nio.charset.CodingErrorAction.REPORT)
            .onUnmappableCharacter(java.nio.charset.CodingErrorAction.REPORT)
            .decode(java.nio.ByteBuffer.wrap(bytes, from, to - from))
            .toString()
    }.getOrNull()

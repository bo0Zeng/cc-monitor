package com.ccmonitor.mobile.core.data.db

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * 旧按钮 → 新的组与按键（17→18 迁移里那一步的纯函数部分；真库上跑一遍在设备上的 MigrationTest）。
 * 每类旧按钮各一条；组的先后 · 名字 · 丢掉哪些也各一条。
 */
class LegacyButtonsTest {
    private val sys = 400

    private fun one(
        b: LegacyButton,
        system: Int = sys,
    ) = convertLegacyButton(b, "g", 0, system)

    private fun btn(
        id: String,
        command: String,
        type: String = "command",
        group: String = "main",
        label: String = id,
        hostId: String? = null,
        sortOrder: Int = 0,
        ctrl: Boolean = false,
        alt: Boolean = false,
        shift: Boolean = false,
        longMode: String = "none",
        longCommand: String = "",
        longMs: Int = 400,
    ) = LegacyButton(id, hostId, label, command, sortOrder, ctrl, alt, shift, type, group, longMode, longCommand, longMs)

    private fun input(vararg p: KeyPress) = KeyBinding.Input(p.toList())

    private fun sp(
        k: SpecialKey,
        vararg m: KeyMod,
    ) = KeyPress.Special(k, m.toSet())

    private fun ch(
        c: String,
        vararg m: KeyMod,
    ) = KeyPress.Chord(c, m.toSet())

    private val enter = sp(SpecialKey.ENTER)

    // ---- 组 ----------------------------------------------------------------

    @Test
    fun theOldDefaultsBecomeCommonShellVimInSwitchChainOrderAndTheToolbarIsGone() {
        val (groups, keys) = convertLegacyButtons(legacySeedButtons(), emptyList(), sys)
        assertEquals(
            listOf(DEFAULT_GROUP_COMMON to "常用", DEFAULT_GROUP_SHELL to "shell", DEFAULT_GROUP_VIM to "vim"),
            groups.map { it.id to it.name },
        )
        assertEquals(listOf(0, 1, 2), groups.map { it.sortOrder })
        assertEquals("工具条那三颗内建不进按键", 0, keys.count { it.id.startsWith("ak-tb-") })
        assertEquals("旧种子里工具条那三颗之外都在", 37, keys.size)
        val common = keys.filter { it.groupId == DEFAULT_GROUP_COMMON }.sortedBy { it.sortOrder }
        assertEquals(
            listOf("ak-fn-switch", "ak-fn-ctrl", "ak-fn-tab", "ak-fn-esc", "ak-fn-up", "ak-fn-down", "ak-fn-left", "ak-fn-right", "ak-fn-paste"),
            common.map { it.id },
        )
        assertEquals(listOf(0, 1, 2, 3, 4, 5, 6, 7, 8), common.map { it.sortOrder })
        assertEquals("旧种子没改过的名字是代码写的，不另写", listOf<String?>(null), keys.map { it.label }.distinct())
        assertEquals("全都转过来了", listOf<List<UnconvertedNote>?>(null), keys.map { it.unconverted }.distinct())
        assertEquals("迁过来的不算默认按键", listOf<String?>(null), keys.map { it.defaultId }.distinct())
        val byId = keys.associateBy { it.id }
        assertEquals(KeyMod.CTRL, byId.getValue("ak-fn-ctrl").modifier)
        assertNull(byId.getValue("ak-fn-ctrl").tap)
        assertEquals(KeyMod.ALT, byId.getValue("ak-alt-alt").modifier)
        assertEquals(KeyBinding.App(AppAction.PASTE), byId.getValue("ak-fn-paste").tap)
        assertEquals(input(sp(SpecialKey.END, KeyMod.CTRL)), byId.getValue("ak-alt-ctrlend").tap)
        assertEquals("Ctrl-End", byId.getValue("ak-alt-ctrlend").cap())
        assertEquals(input(sp(SpecialKey.PAGE_UP)), byId.getValue("ak-alt-pgup").tap)
        assertEquals(input(KeyPress.Text(":wq"), enter), byId.getValue("ak-vim-wq").tap)
        assertEquals(input(KeyPress.Text("G")), byId.getValue("ak-vim-G").tap)
        listOf("ak-fn-switch", "ak-alt-switch", "ak-vim-switch").forEach {
            assertEquals(KeyBinding.App(AppAction.NEXT_GROUP), byId.getValue(it).tap)
            assertEquals("⇌ 原来固定的长按开编辑", KeyBinding.App(AppAction.EDIT_KEYS), byId.getValue(it).longPress)
        }
    }

    @Test
    fun userGroupsFollowTheChainThenTheBarThenOneGroupPerServerLast() {
        val rows =
            listOf(
                btn("s1", "alt", "switch", "fn"),
                btn("k1", "\\e", "escape", "fn", sortOrder = 1),
                btn("s2", "mine", "switch", "alt"),
                btn("s3", "fn", "switch", "mine"),
                btn("k2", "x", "escape", "mine", sortOrder = 1),
                btn("k3", "y", "escape", "zz"),
                btn("bar", "ls -la", label = "ls"),
                btn("tabs", "show_tabs", "builtin"),
                btn("mytmux", "tmux_sessions", "builtin", "fn", sortOrder = 2),
                btn("bpaste", "paste", "builtin", sortOrder = 1),
                btn("h1a", "make", hostId = "h1"),
                btn("h1b", "\\e[A", "escape", "fn", hostId = "h1", sortOrder = 1),
                btn("h2a", "top", hostId = "h2"),
                btn("gone", "rm", hostId = "deleted"),
            )
        val hosts = listOf(LegacyHost("h1", "devbox", 2), LegacyHost("h2", "shell", 1))
        val (groups, keys) = convertLegacyButtons(rows, hosts, sys)
        assertEquals(
            "⇌ 那一串 fn → alt → mine，zz 不在串里接在后面；按钮栏；机器按机器的先后、组名重了加序号",
            listOf("常用", "shell", "mine", "zz", "按钮栏", "shell 2", "devbox"),
            groups.map { it.name },
        )
        assertEquals(listOf("kg-common", "kg-shell", "kg-g-mine", "kg-g-zz", "kg-bar", "kg-host-h2", "kg-host-h1"), groups.map { it.id })
        val byId = keys.associateBy { it.id }
        assertEquals("工具条内建丢掉（只丢 main 组那几颗）", null, byId["tabs"])
        assertEquals("fn 组里自己放的 tmux 会话照迁", KeyBinding.App(AppAction.TMUX_SESSIONS), byId.getValue("mytmux").tap)
        assertEquals("kg-bar", byId.getValue("bpaste").groupId)
        assertEquals(input(KeyPress.Text("ls -la"), enter), byId.getValue("bar").tap)
        assertEquals("用户的名字照旧", "ls", byId.getValue("bar").label)
        assertEquals(listOf("h1a", "h1b"), keys.filter { it.groupId == "kg-host-h1" }.sortedBy { it.sortOrder }.map { it.id })
        assertEquals("机器删了的那几颗显示不出来，丢掉", null, byId["gone"])
    }

    // ---- 每类旧按钮 ---------------------------------------------------------

    @Test
    fun commandLinesBecomeTextPlusEnter() {
        assertEquals(input(KeyPress.Text("git status"), enter), one(btn("a", "git status")).tap)
        assertEquals("空命令 ＝ 只按回车（旧的只送换行）", input(enter), one(btn("a", "")).tap)
        assertEquals("keycode 类型没带修饰，旧派发也当命令行", input(KeyPress.Text("ls"), enter), one(btn("a", "ls", "keycode")).tap)
    }

    @Test
    fun chordsBecomeCombosOrSpecialKeys() {
        assertEquals(input(ch("c", KeyMod.CTRL)), one(btn("a", "c", "keycode", ctrl = true)).tap)
        assertEquals("旧库 10 版前的修饰行类型还是 command", input(ch("c", KeyMod.CTRL)), one(btn("a", "c", ctrl = true)).tap)
        assertEquals(input(sp(SpecialKey.TAB, KeyMod.SHIFT)), one(btn("a", "tab", "keycode", shift = true)).tap)
        assertEquals(input(sp(SpecialKey.ENTER, KeyMod.ALT)), one(btn("a", "Return", "keycode", alt = true)).tap)
        assertEquals(input(sp(SpecialKey.ESC, KeyMod.CTRL)), one(btn("a", "esc", "keycode", ctrl = true)).tap)
        assertEquals(input(ch(" ", KeyMod.CTRL)), one(btn("a", "space", "keycode", ctrl = true)).tap)
        assertEquals("只取第一个字", input(ch("x", KeyMod.ALT, KeyMod.SHIFT)), one(btn("a", "xyz", "keycode", alt = true, shift = true)).tap)
        assertEquals("非 ASCII 的字旧派发不加修饰 ⇒ 照原样打", input(KeyPress.Text("中")), one(btn("a", "中文", "keycode", ctrl = true)).tap)
        val empty = one(btn("a", "", "keycode", ctrl = true))
        assertNull(empty.tap)
        assertEquals(listOf(UnconvertedNote(UnconvertedNote.Issue.TAP_UNPARSED, "Ctrl+")), empty.unconverted)
    }

    @Test
    fun escapesThatAreRecognizedBecomeKeysAndTheRestAreListedAsUnconverted() {
        fun esc(s: String) = one(btn("a", s, "escape")).tap
        assertEquals(input(sp(SpecialKey.ESC)), esc("\\e"))
        assertEquals(input(sp(SpecialKey.ESC)), esc("\\x1b"))
        assertEquals(input(sp(SpecialKey.ESC), sp(SpecialKey.ESC)), esc("\\e\\e"))
        assertEquals(input(sp(SpecialKey.UP, KeyMod.SHIFT)), esc("\\e[1;2A"))
        assertEquals(input(sp(SpecialKey.LEFT, KeyMod.CTRL, KeyMod.ALT)), esc("\\e[1;7D"))
        assertEquals(input(sp(SpecialKey.HOME)), esc("\\e[1~"))
        assertEquals(input(sp(SpecialKey.DELETE)), esc("\\e[3~"))
        assertEquals(input(sp(SpecialKey.PAGE_DOWN, KeyMod.CTRL)), esc("\\e[6;5~"))
        assertEquals(input(sp(SpecialKey.F1)), esc("\\eOP"))
        assertEquals(input(sp(SpecialKey.UP)), esc("\\eOA"))
        assertEquals(input(sp(SpecialKey.F5)), esc("\\e[15~"))
        assertEquals(input(sp(SpecialKey.F12)), esc("\\e[24~"))
        assertEquals(input(sp(SpecialKey.TAB, KeyMod.SHIFT)), esc("\\e[Z"))
        assertEquals(input(ch("x", KeyMod.ALT)), esc("\\ex"))
        assertEquals(input(ch("c", KeyMod.CTRL)), esc("\\x03"))
        assertEquals(input(ch("@", KeyMod.CTRL)), esc("\\x00"))
        assertEquals(input(ch("]", KeyMod.CTRL)), esc("\\x1d"))
        assertEquals(input(sp(SpecialKey.BACKSPACE)), esc("\\x7f"))
        assertEquals(input(KeyPress.Text("a"), sp(SpecialKey.TAB), KeyPress.Text("b c")), esc("a\\tb c"))
        assertEquals(input(KeyPress.Text(":q!"), enter), esc(":q!\\r"))
        assertEquals("认不得的 \\X 反斜杠照字面", input(KeyPress.Text("\\q")), esc("\\q"))
        assertEquals(input(KeyPress.Text("é")), esc("\\xc3\\xa9"))

        listOf("\\e[?1049h", "\\e[2~", "\\e[1;9A", "\\e[", "\\xff", "").forEach { raw ->
            val k = one(btn("a", raw, "escape"))
            assertNull("「$raw」认不出 ⇒ 单击没设", k.tap)
            assertEquals(listOf(UnconvertedNote(UnconvertedNote.Issue.TAP_UNPARSED, raw)), k.unconverted)
        }
    }

    @Test
    fun builtinsBecomeAppActionsAndStickyModifiersBecomeModifierKeys() {
        mapOf(
            "show_tabs" to AppAction.SHOW_TABS,
            "command_palette" to AppAction.COMMAND_PALETTE,
            "tmux_sessions" to AppAction.TMUX_SESSIONS,
            "paste" to AppAction.PASTE,
            "capture" to AppAction.CAPTURE,
            "push_key" to AppAction.PUSH_KEY,
            "show_keyboard" to AppAction.SHOW_KEYBOARD,
        ).forEach { (old, new) -> assertEquals(KeyBinding.App(new), one(btn("a", old, "builtin", "fn")).tap) }
        val ctrl = one(btn("a", "toggle_ctrl", "builtin", "fn"))
        assertEquals(KeyMod.CTRL, ctrl.modifier)
        assertEquals(listOf(null, null, null), listOf(ctrl.tap, ctrl.doubleTap, ctrl.longPress))
        val odd = one(btn("a", "make_coffee", "builtin", "fn"))
        assertNull(odd.tap)
        assertEquals(listOf(UnconvertedNote(UnconvertedNote.Issue.TAP_UNPARSED, "make_coffee")), odd.unconverted)
    }

    @Test
    fun switchKeysGoToTheNextGroupAndLongPressOpensTheEditor() {
        val k = one(btn("a", "vim", "switch", "fn", label = "⇌", longMode = "discrete", longCommand = "x", longMs = 900))
        assertEquals(KeyBinding.App(AppAction.NEXT_GROUP), k.tap)
        assertEquals(KeyBinding.App(AppAction.EDIT_KEYS), k.longPress)
        assertNull("和自动写出来的一样 ⇒ 不另写", k.label)
        assertNull("⇌ 的长按原来不看存的阈值", k.longPressMs)
        assertEquals("用户改过的名字照旧", "next", one(btn("a", "vim", "switch", "fn", label = "next")).label)
    }

    @Test
    fun longPressOnceBecomesALongPressBindingInTheSameType() {
        val cmd = one(btn("a", "git status", longMode = "discrete", longCommand = "git log"))
        assertEquals(input(KeyPress.Text("git log"), enter), cmd.longPress)
        val esc = one(btn("a", "\\e", "escape", "fn", longMode = "Discrete", longCommand = "\\e\\e"))
        assertEquals(input(sp(SpecialKey.ESC), sp(SpecialKey.ESC)), esc.longPress)
        val chord = one(btn("a", "c", "keycode", ctrl = true, longMode = "discrete", longCommand = "d"))
        assertEquals("载荷照这颗原来的类型（带修饰）解释", input(ch("d", KeyMod.CTRL)), chord.longPress)
        val bad = one(btn("a", "\\e", "escape", "fn", longMode = "discrete", longCommand = "\\e[?25l"))
        assertEquals(input(sp(SpecialKey.ESC)), bad.tap)
        assertNull(bad.longPress)
        assertEquals(listOf(UnconvertedNote(UnconvertedNote.Issue.LONG_UNPARSED, "\\e[?25l")), bad.unconverted)
    }

    @Test
    fun longPressRepeatIsNaturalForRepeatableKeysAndListedForTheRest() {
        val left = one(btn("a", "\\e[D", "escape", "fn", longMode = "repeat"))
        assertNull(left.longPress)
        assertNull(left.unconverted)
        val j = one(btn("a", "j", "escape", "vim", longMode = "repeat"))
        assertNull("一个字也能连发", j.unconverted)
        val ls = one(btn("a", "ls", longMode = "repeat"))
        assertNull(ls.longPress)
        assertEquals(listOf(UnconvertedNote(UnconvertedNote.Issue.LONG_NO_REPEAT, "ls")), ls.unconverted)
        val unknown = one(btn("a", "\\e", "escape", "fn", longMode = "sideways"))
        assertNull("认不得的长按样式旧的当没设", unknown.unconverted)
    }

    @Test
    fun theThresholdIsKeptOnlyWhenItDiffersFromTheSystemAndOnlyWhenLongPressWasUsed() {
        assertEquals(600, one(btn("a", "\\e[D", "escape", longMode = "repeat", longMs = 600)).longPressMs)
        assertNull(one(btn("a", "\\e[D", "escape", longMode = "repeat", longMs = 400)).longPressMs)
        assertEquals("系统值是这台手机现读的", 400, one(btn("a", "\\e[D", "escape", longMode = "repeat", longMs = 400), system = 500).longPressMs)
        assertNull("没用长按 ⇒ 阈值没意义", one(btn("a", "ls", longMs = 900)).longPressMs)
    }

    @Test
    fun labelsAreKeptUnlessTheyAreTheOldSeedOrWhatWouldBeWrittenAnyway() {
        assertNull(one(btn("a", "\\e", "escape", label = "Esc")).label)
        assertEquals("ESC!", one(btn("ak-fn-esc", "\\e", "escape", "fn", label = "ESC!")).label)
        assertEquals("Ctrl+C", one(btn("a", "c", "keycode", ctrl = true, label = "Ctrl+C")).label)
        assertNull("空名字 ⇒ 照单击自动写", one(btn("a", "ls", label = " ")).label)
    }
}

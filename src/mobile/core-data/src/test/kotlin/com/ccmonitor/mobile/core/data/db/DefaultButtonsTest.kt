package com.ccmonitor.mobile.core.data.db

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 默认按键种子数据：main/fn/alt/vim 四组 40 条的结构、类型与命令字符串。
 * 命令字符串解码成字节由 core-terminal 那边测；迁移与 onCreate 真写入 SQLite 由 androidTest 在设备上验。
 */
class DefaultButtonsTest {
    @Test
    fun seedHasExpectedGroupsAndUniqueIds() {
        val seeds = DEFAULT_SEED_BUTTONS
        assertEquals("种子总数 40", 40, seeds.size)
        assertEquals("id 唯一", seeds.size, seeds.map { it.id }.toSet().size)
        assertTrue("无空 id/label/command", seeds.none { it.id.isBlank() || it.label.isBlank() || it.command.isBlank() })
        assertEquals("main 组 3", 3, seeds.count { it.group == "main" })
        assertEquals("fn 组 9", 9, seeds.count { it.group == "fn" })
        assertEquals("alt 组 8", 8, seeds.count { it.group == "alt" })
        assertEquals("vim 组 20", 20, seeds.count { it.group == "vim" })
        assertTrue("组仅 main/fn/alt/vim", seeds.all { it.group in setOf("main", "fn", "alt", "vim") })
    }

    /**
     * 组名常量的值也钉住：其余断言都用字面量做独立对照。
     * 这些值与迁移 SQL 的 `DEFAULT 'main'` 和已落库的行绑定，不能随意改名。
     */
    @Test
    fun buttonGroupConstantsHaveExpectedValues() {
        assertEquals("main", ButtonGroups.MAIN)
        assertEquals("fn", ButtonGroups.FN)
        assertEquals("alt", ButtonGroups.ALT)
        assertEquals("vim", ButtonGroups.VIM)
    }

    /** 组切换 `⇌`（switch 类型）形成 fn→alt→vim→fn 循环；标签统一 `⇌`、command=目标组。 */
    @Test
    fun switchSeedsFormFnAltVimCycle() {
        fun sw(id: String) = DEFAULT_SEED_BUTTONS.first { it.id == id }
        assertEquals("fn 的 ⇌ 切到 alt", "alt", sw("ak-fn-switch").command)
        assertEquals("alt 的 ⇌ 切到 vim", "vim", sw("ak-alt-switch").command)
        assertEquals("vim 的 ⇌ 切回 fn", "fn", sw("ak-vim-switch").command)
        listOf("ak-fn-switch", "ak-alt-switch", "ak-vim-switch").forEach {
            assertEquals("$it 是 switch 类型", "switch", sw(it).type)
            assertEquals("$it 标签统一 ⇌", "⇌", sw(it).label)
        }
    }

    /** Ctrl/Alt/粘贴是 builtin 按钮，command=toggle_ctrl/toggle_alt/paste。 */
    @Test
    fun stickyAndPasteAreBuiltins() {
        fun bt(id: String) = DEFAULT_SEED_BUTTONS.first { it.id == id }
        assertEquals("builtin" to "toggle_ctrl", bt("ak-fn-ctrl").type to bt("ak-fn-ctrl").command)
        assertEquals("builtin" to "toggle_alt", bt("ak-alt-alt").type to bt("ak-alt-alt").command)
        assertEquals("builtin" to "paste", bt("ak-fn-paste").type to bt("ak-fn-paste").command)
    }

    @Test
    fun fnAndAltEscapeKeysUseExpectedCommands() {
        fun cmd(id: String) = DEFAULT_SEED_BUTTONS.first { it.id == id }.command
        assertEquals("\\e", cmd("ak-fn-esc"))
        assertEquals("\\t", cmd("ak-fn-tab"))
        assertEquals("\\e[D", cmd("ak-fn-left"))
        assertEquals("\\e[A", cmd("ak-fn-up"))
        assertEquals("\\e[B", cmd("ak-fn-down"))
        assertEquals("\\e[C", cmd("ak-fn-right"))
        // alt 组：xterm 序列
        assertEquals("\\e[H", cmd("ak-alt-home"))
        assertEquals("\\e[F", cmd("ak-alt-end"))
        assertEquals("\\e[1;5F", cmd("ak-alt-ctrlend"))
        assertEquals("\\e[5~", cmd("ak-alt-pgup"))
        assertEquals("\\e[6~", cmd("ak-alt-pgdn"))
    }

    /** fn 组布局与顺序（⇌ 首、Ctrl、Tab/Esc/方向、粘贴末）。 */
    @Test
    fun fnGroupLayoutAndOrder() {
        val fn = DEFAULT_SEED_BUTTONS.filter { it.group == "fn" }
        assertEquals(listOf("⇌", "Ctrl", "Tab", "Esc", "↑", "↓", "←", "→", "粘贴"), fn.map { it.label })
    }

    /** alt 第二排布局与顺序。 */
    @Test
    fun altGroupLayoutAndOrder() {
        val alt = DEFAULT_SEED_BUTTONS.filter { it.group == "alt" }
        assertEquals(listOf("⇌", "Alt", "Ctrl+End", "Home", "End", "PgUp", "PgDn", "Esc"), alt.map { it.label })
    }

    /**
     * fn/alt 两组的方向/Tab/Esc 键是 `escape` 类型。纯数据表就能验，不需设备；
     * onCreate 真的被调、真写进 SQLite 仍由设备上的 `SeedOnCreateTest` 验。
     */
    @Test
    fun fnAndAltEscapeKeysAreEscapeTyped() {
        val expectedEscape =
            listOf(
                "ak-fn-tab",
                "ak-fn-esc",
                "ak-fn-up",
                "ak-fn-down",
                "ak-fn-left",
                "ak-fn-right",
                "ak-alt-ctrlend",
                "ak-alt-home",
                "ak-alt-end",
                "ak-alt-pgup",
                "ak-alt-pgdn",
                "ak-alt-esc",
            )
        val actual = expectedEscape.associateWith { id -> DEFAULT_SEED_BUTTONS.first { it.id == id }.type }
        assertEquals(
            "fn/alt 两组的方向/Tab/Esc 键必须是 `escape` 类型，否则派发走另一条分支，发出去的不是转义序列。",
            expectedEscape.associateWith { "escape" },
            actual,
        )
    }

    /**
     * vim 排 19 个 escape 键的 (label, command) 全表（不含组切换 `⇌`）。
     * 只比 label 的话，把 `ak-vim-h` 的 command 改成 `"l"` 也全绿，按 `h` 却发了别的字节。
     */
    @Test
    fun vimKeySeedsHaveExpectedLabelAndCommand() {
        val vim = DEFAULT_SEED_BUTTONS.filter { it.group == "vim" && it.type == "escape" }.map { it.label to it.command }
        assertEquals(
            listOf(
                "Esc" to "\\e",
                "i" to "i",
                ":" to ":",
                "/" to "/",
                ":w↵" to ":w\\n",
                ":wq↵" to ":wq\\n",
                ":q!↵" to ":q!\\n",
                "dd" to "dd",
                "yy" to "yy",
                "p" to "p",
                "u" to "u",
                "gg" to "gg",
                "G" to "G",
                "h" to "h",
                "j" to "j",
                "k" to "k",
                "l" to "l",
                "0" to "0",
                "\$" to "\$",
            ),
            vim,
        )
    }

    /**
     * 只有 `:w`/`:wq`/`:q!` 这类执行键带回车；导航/编辑键不带，否则误触即执行。
     * 判定在未解码的 command 字符串上做，所以穷举 `escapeSequenceBytes` 所有能产出 CR/LF 的写法。
     */
    @Test
    fun vimExecKeysCarryNewlineOthersDoNot() {
        val enterForms = Regex("""\\n|\\r|\\x0[aAdD]|\n|\r""")
        DEFAULT_SEED_BUTTONS.filter { it.group == "vim" }.forEach { b ->
            val carriesEnter = enterForms.containsMatchIn(b.command)
            if (b.label.endsWith("↵")) {
                assertTrue("${b.label} 应带回车", carriesEnter)
            } else {
                assertTrue("${b.label} 不应带回车（误触即执行）", !carriesEnter)
            }
        }
    }

    @Test
    fun vimEscapeKeysAreEscapeAndNewlineForEnterKeys() {
        val vim = DEFAULT_SEED_BUTTONS.filter { it.group == "vim" }
        assertTrue("vim escape 键全 escape 类型", vim.filter { it.id != "ak-vim-switch" }.all { it.type == "escape" })
        assertEquals("switch", vim.first { it.id == "ak-vim-switch" }.type)
        assertEquals("i", vim.first { it.id == "ak-vim-i" }.command)
        assertEquals(":w\\n", vim.first { it.id == "ak-vim-w" }.command) // ↵ = \n
        assertEquals(":wq\\n", vim.first { it.id == "ak-vim-wq" }.command)
        assertEquals(":q!\\n", vim.first { it.id == "ak-vim-q" }.command)
    }

    @Test
    fun toolbarSeedsAreBuiltinsWithKnownIds() {
        val tb = DEFAULT_SEED_BUTTONS.filter { it.group == "main" }
        assertTrue("工具条全 builtin", tb.all { it.type == "builtin" })
        assertEquals(setOf("show_tabs", "command_palette", "tmux_sessions"), tb.map { it.command }.toSet())
    }

    /** 三条工具条种子拿最低 sortOrder（0/1/2），在按钮条最左。 */
    @Test
    fun toolbarSeedsSortFirst() {
        assertEquals(listOf("ak-tb-tabs", "ak-tb-cmd", "ak-tb-tmux"), DEFAULT_SEED_BUTTONS.take(3).map { it.id })
    }

    /**
     * [defaultButtonEntities]（恢复默认走的实体路径）与 [DEFAULT_SEED_BUTTONS]（迁移/onCreate 的 raw SQL 路径）逐字一致。
     * sortOrder 是列表下标，与 `seedDefaultButtons` 的 `forEachIndexed` 一致。
     */
    @Test
    fun defaultButtonEntitiesMatchSeedTableExactly() {
        val ents = defaultButtonEntities()
        assertEquals("两条路径条数相同", DEFAULT_SEED_BUTTONS.size, ents.size)
        DEFAULT_SEED_BUTTONS.forEachIndexed { i, s ->
            val e = ents[i]
            assertEquals(s.id, e.id)
            assertEquals(s.label, e.label)
            assertEquals(s.command, e.command)
            assertEquals(s.type, e.type)
            assertEquals(s.group, e.btnGroup)
            assertEquals("sortOrder=下标", i, e.sortOrder)
            assertEquals("全局", null, e.hostId)
            assertTrue("无修饰", !e.ctrl && !e.alt && !e.shift)
        }
    }
}

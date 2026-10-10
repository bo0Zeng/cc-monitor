package com.ccmonitor.mobile.ui.session

import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.data.db.CustomButton
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/** 命令面板搜索过滤的纯函数（动作组装/UI 在 ModalBottomSheet，留给设备端；过滤逻辑在这里钉）。 */
class CommandPaletteTest {
    private val noop: () -> Unit = {}
    private val actions =
        listOf(
            // `run` 可空（null = 只读条目，聊天面的工具/MCP 用），所以具名传。
            // 终端面的每条动作都非空 ⇒ 恒可点。
            PaletteAction("模型", "opus", "/model opus", run = noop),
            PaletteAction("抓屏", "抓当前屏幕", null, run = noop),
            PaletteAction("自定义", "git", "git status", run = noop),
        )

    @Test fun blankReturnsAll() = assertEquals(3, filterPaletteActions(actions, "").size)

    @Test fun whitespaceQueryReturnsAll() = assertEquals(3, filterPaletteActions(actions, "   ").size)

    @Test fun matchesLabelCaseInsensitive() =
        assertEquals(listOf("opus"), filterPaletteActions(actions, "OP").map { it.label })

    @Test fun matchesGroup() =
        assertEquals(listOf("抓当前屏幕"), filterPaletteActions(actions, "抓屏").map { it.label })

    @Test fun matchesSubtitle() =
        assertEquals(listOf("git"), filterPaletteActions(actions, "status").map { it.label })

    @Test fun noMatchReturnsEmpty() =
        assertEquals(emptyList<String>(), filterPaletteActions(actions, "zzz").map { it.label })

    // === 段首匹配 ＋ 两个尺寸下界 ===============================
    //
    // 打 `re` 两个字母，裸 `contains` 会命中一堆，其中 `discord:configure` 与 `photo-capture`
    // 全是尾音节巧合；行高低于 Material3 的最小触控目标（48dp）就点不准；
    // 列表写死高度（如 `420.dp`）在高屏上白白浪费好几行。

    /** 尾音节巧合的那三条（名字原样，别改：它们就是判据本身）。 */
    private val realWorld =
        listOf(
            PaletteAction("技能", "reload-skills", null, run = noop),
            PaletteAction("扩展", "discord:configure", null, run = noop),
            PaletteAction("技能", "photo-capture", null, run = noop),
        )

    @Test
    fun segmentPrefixDropsTailSyllableCoincidences() =
        assertEquals(
            "打 `re` 只该中 reload-skills；另两条是尾音节巧合",
            listOf("reload-skills"),
            filterPaletteActions(realWorld, "re").map { it.label },
        )

    @Test
    fun segmentPrefixStillMatchesAfterSeparator() =
        assertEquals(
            "`capture` 是 `photo-capture` 的第二段 ⇒ 打 cap 该中（段首匹配 ≠ 只match整串开头）",
            listOf("photo-capture"),
            filterPaletteActions(realWorld, "cap").map { it.label },
        )

    @Test
    fun paletteRowMeetsMinimumTouchTarget() =
        assertTrue(
            "行高下界 $PALETTE_ROW_MIN_HEIGHT 低于 Material3 的最小触控目标 48dp",
            PALETTE_ROW_MIN_HEIGHT >= 48.dp,
        )

    @Test
    fun paletteListFractionLeavesRoomForSearchBox() {
        assertTrue(
            "列表占满整屏 ⇒ 搜索框被挤出 sheet（实得 $PALETTE_LIST_MAX_FRACTION）",
            PALETTE_LIST_MAX_FRACTION < 1.0f,
        )
        assertTrue(
            "列表上界太矮 ⇒ 重演写死 420dp 那个浪费（实得 $PALETTE_LIST_MAX_FRACTION）",
            PALETTE_LIST_MAX_FRACTION >= 0.5f,
        )
    }

    // === 命令面板里自定义按钮携修饰键（漏了的话 Ctrl+C 会误发 "c\n"）===

    @Test
    fun paletteCustomActionCarriesButtonWithModifiers() {
        val btn = CustomButton(id = "b", hostId = "h", label = "中断", command = "c", ctrl = true)
        var received: CustomButton? = null
        val acts =
            terminalPaletteActions(
                supportsTmux = false, // 只保留自定义动作，隔离
                customButtons = listOf(btn),
                onModel = {},
                onCapture = {},
                onShowTmux = {},
                onPushKey = {},
                onHistory = {},
                onCustom = { received = it },
            )
        val custom = acts.single { it.group == "自定义" }
        assertEquals("副标题应显示组合键而非裸键", "Ctrl+c", custom.subtitle)
        custom.run!!() // 触发（面板点击）：run 可空（null=只读），终端面的动作恒非空
        assertSame("面板动作应把整个按钮（含修饰键）交给 onCustom，供 customButtonBytes 编码", btn, received)
    }

    // === 命令面板「历史记录」动作（Claude 历史按项目分组 + resume）===

    @Test
    fun paletteHistoryActionPresentAndGatedByTmux() {
        var opened = false
        val withTmux =
            terminalPaletteActions(
                supportsTmux = true,
                customButtons = emptyList(),
                onModel = {},
                onCapture = {},
                onShowTmux = {},
                onPushKey = {},
                onHistory = { opened = true },
                onCustom = {},
            )
        val history = withTmux.single { it.group == "历史" }
        history.run!!() // run 可空；终端面的动作恒非空，!! 就是这条断言
        assertEquals("点「历史记录」应触发 onHistory", true, opened)
        val withoutTmux =
            terminalPaletteActions(
                supportsTmux = false, // resume 依赖 tmux/POSIX → 无 tmux 不显历史
                customButtons = emptyList(),
                onModel = {},
                onCapture = {},
                onShowTmux = {},
                onPushKey = {},
                onHistory = {},
                onCustom = {},
            )
        assertEquals("无 tmux 能力不该有历史动作", 0, withoutTmux.count { it.group == "历史" })
    }

    @Test
    fun customButtonSubtitleShowsChordOrPlainCommand() {
        assertEquals("git status", customButtonSubtitle(CustomButton("1", "h", "git", "git status")))
        assertEquals("Ctrl+c", customButtonSubtitle(CustomButton("2", "h", "中断", "c", ctrl = true)))
        assertEquals("Ctrl+Alt+Shift+x", customButtonSubtitle(CustomButton("3", "h", "x", "x", ctrl = true, alt = true, shift = true)))
    }
}

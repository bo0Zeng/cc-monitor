package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.data.db.CustomButton
import com.ccmonitor.mobile.core.terminal.KeyModifiers
import com.ccmonitor.mobile.core.terminal.customButtonBytes
import com.ccmonitor.mobile.core.terminal.escapeSequenceBytes
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** [dispatchButton] 四类型派发 + [ButtonBuiltins] 路由。command/keycode 与 customButtonBytes 逐字相同。 */
class ButtonDispatchTest {
    private fun btn(
        command: String,
        type: String,
        ctrl: Boolean = false,
        alt: Boolean = false,
        shift: Boolean = false,
    ) = CustomButton(id = "x", hostId = null, label = "L", command = command, type = type, ctrl = ctrl, alt = alt, shift = shift)

    private fun recordingBuiltins(
        sink: MutableList<String>,
        supportsTmux: Boolean = true, // 路由测默认 tmux 主机（不门控）；门控由 builtinsGateTmuxActions* 专测
    ) = ButtonBuiltins(
        supportsTmux = supportsTmux,
        onShowTabs = { sink += "show_tabs" },
        onCommandPalette = { sink += "command_palette" },
        onTmuxSessions = { sink += "tmux_sessions" },
        onPaste = { sink += "paste" },
        onShowHistory = { sink += "show_history" },
        onCapture = { sink += "capture" },
        onPushKey = { sink += "push_key" },
        onShowKeyboard = { sink += "show_keyboard" },
        onToggleCtrl = { sink += "toggle_ctrl" },
        onToggleAlt = { sink += "toggle_alt" },
    )

    private fun dispatch(button: CustomButton): Pair<ByteArray?, List<String>> {
        var sent: ByteArray? = null
        val sink = mutableListOf<String>()
        dispatchButton(button, { sent = it }, recordingBuiltins(sink))
        return sent to sink
    }

    @Test
    fun commandTypeSendsCommandLine() {
        val (sent, invoked) = dispatch(btn("ls -la", "command"))
        assertArrayEquals("命令行按钮发 command\\n", "ls -la\n".toByteArray(), sent)
        assertArrayEquals("与 customButtonBytes 逐字同", customButtonBytes("ls -la", KeyModifiers()), sent)
        assertEquals(emptyList<String>(), invoked)
    }

    @Test
    fun keycodeTypeSendsControlCode() {
        val (sent, invoked) = dispatch(btn("c", "keycode", ctrl = true))
        assertArrayEquals("Ctrl+C = 0x03（回归：与今日按键码逐字同）", byteArrayOf(0x03), sent)
        assertArrayEquals(customButtonBytes("c", KeyModifiers(ctrl = true)), sent)
        assertEquals(emptyList<String>(), invoked)
    }

    @Test
    fun escapeTypeSendsRawSequence() {
        val (sent, invoked) = dispatch(btn("\\e[A", "escape"))
        assertArrayEquals("转义 \\e[A → ESC [ A", escapeSequenceBytes("\\e[A"), sent)
        assertArrayEquals(byteArrayOf(0x1B, '['.code.toByte(), 'A'.code.toByte()), sent)
        assertEquals(emptyList<String>(), invoked)
    }

    @Test
    fun builtinTypeInvokesActionNoBytes() {
        val (sent, invoked) = dispatch(btn("show_tabs", "builtin"))
        assertNull("内建动作不出线字节", sent)
        assertEquals(listOf("show_tabs"), invoked)
    }

    /** switch 类型 → onSwitchGroup(command=目标组)，不出线字节、不碰 builtins。 */
    @Test
    fun switchTypeInvokesOnSwitchGroupNoBytes() {
        var sent: ByteArray? = null
        var switched: String? = null
        val sink = mutableListOf<String>()
        dispatchButton(btn("vim", "switch"), { sent = it }, recordingBuiltins(sink), onSwitchGroup = { switched = it })
        assertNull("组切换不出线字节", sent)
        assertEquals("切到 command 指定的目标组", "vim", switched)
        assertEquals("switch 不走 builtins", emptyList<String>(), sink)
    }

    /** switch 按钮经默认 dispatch（未注入 onSwitchGroup）= no-op，不出字节、不崩（命令面板等非切换场景）。 */
    @Test
    fun switchTypeDefaultOnSwitchGroupIsNoOp() {
        val (sent, invoked) = dispatch(btn("vim", "switch"))
        assertNull("默认 no-op 不出线字节", sent)
        assertEquals(emptyList<String>(), invoked)
    }

    /** 粘性修饰键点亮态——builtin toggle_ctrl/alt 绑定 ctrlActive/altActive，其余一律不点亮。 */
    @Test
    fun litStateBindsStickyModifiers() {
        assertTrue(litState(btn("toggle_ctrl", "builtin"), ctrlActive = true, altActive = false))
        assertFalse(litState(btn("toggle_ctrl", "builtin"), ctrlActive = false, altActive = false))
        assertTrue(litState(btn("toggle_alt", "builtin"), ctrlActive = false, altActive = true))
        assertFalse("非 builtin 一律不点亮", litState(btn("toggle_ctrl", "command"), ctrlActive = true, altActive = true))
        assertFalse("其它 builtin 动作不点亮", litState(btn("paste", "builtin"), ctrlActive = true, altActive = true))
    }

    @Test
    fun unknownTypeFallsBackToCommand() {
        // 前向兼容：未知 type → CustomButtonType.fromDb 降级 COMMAND → 走 customButtonBytes。
        val (sent, _) = dispatch(btn("echo hi", "future_type"))
        assertArrayEquals("echo hi\n".toByteArray(), sent)
    }

    @Test
    fun builtinsRouteEachAction() {
        val sink = mutableListOf<String>()
        val builtins = recordingBuiltins(sink)
        listOf(
            "show_tabs",
            "command_palette",
            "tmux_sessions",
            "paste",
            "show_history",
            "capture",
            "push_key",
            "show_keyboard",
            "toggle_ctrl",
            "toggle_alt",
        ).forEach { builtins.invoke(it) }
        assertEquals(
            listOf(
                "show_tabs",
                "command_palette",
                "tmux_sessions",
                "paste",
                "show_history",
                "capture",
                "push_key",
                "show_keyboard",
                "toggle_ctrl",
                "toggle_alt",
            ),
            sink,
        )
    }

    @Test
    fun builtinsUnknownActionIsNoOp() {
        val sink = mutableListOf<String>()
        recordingBuiltins(sink).invoke("no_such_action")
        assertEquals(emptyList<String>(), sink)
    }

    /** tmux 域 id 在非 tmux 主机上点击 no-op（门控在 invoke 里，读 TMUX_ONLY_BUILTINS 单一名单）。 */
    @Test
    fun builtinsGateTmuxActionsWhenNoTmux() {
        val sink = mutableListOf<String>()
        val builtins = recordingBuiltins(sink, supportsTmux = false)
        TMUX_ONLY_BUILTINS.forEach { builtins.invoke(it) }
        assertEquals("非 tmux 主机上 tmux 域动作全 no-op", emptyList<String>(), sink)
    }

    /** 非 tmux 域 id 与 tmux 主机上的 tmux id 都正常触发（门控不误伤）。 */
    @Test
    fun builtinsFireNonTmuxAlwaysAndTmuxWhenSupported() {
        val off = mutableListOf<String>()
        recordingBuiltins(off, supportsTmux = false).invoke("show_tabs")
        assertEquals("非 tmux 域动作与 tmux 无关、恒触发", listOf("show_tabs"), off)

        val on = mutableListOf<String>()
        recordingBuiltins(on, supportsTmux = true).invoke("tmux_sessions")
        assertEquals("tmux 主机上 tmux 域动作正常触发", listOf("tmux_sessions"), on)
    }
}

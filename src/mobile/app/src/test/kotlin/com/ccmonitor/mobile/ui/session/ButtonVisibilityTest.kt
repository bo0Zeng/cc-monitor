package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.data.db.CustomButton
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * `CustomButton.visibleOn(supportsTmux)`：工具条内建按钮的能力位门控。
 *
 * `▤ tab`/`命令`/`tmux 会话` 是 main 组的 builtin 种子；没有这道门，`tmux 会话` 会在非 tmux 主机上
 * 画出来、点了却是 no-op（`ButtonBuiltins` 构造方包了 tmux 守卫）。本函数管不渲染，那层 no-op 管点了无害，
 * 二者都要（任意组的任意按钮都能配成 tmux builtin）。
 */
class ButtonVisibilityTest {
    private fun btn(
        type: String,
        command: String,
    ) = CustomButton(id = "b", label = "L", command = command, type = type)

    @Test
    fun tmuxBuiltinsHiddenOnNonTmuxHost() {
        listOf("tmux_sessions", "show_history", "capture", "push_key").forEach { action ->
            assertFalse("$action 在非 tmux 主机应隐藏", btn("builtin", action).visibleOn(supportsTmux = false))
            assertTrue("$action 在 tmux 主机应显示", btn("builtin", action).visibleOn(supportsTmux = true))
        }
    }

    @Test
    fun nonTmuxBuiltinsAlwaysVisible() {
        listOf("show_tabs", "command_palette", "paste", "show_keyboard", "toggle_ctrl", "toggle_alt").forEach { action ->
            assertTrue("$action 与 tmux 无关，任何主机都该显示", btn("builtin", action).visibleOn(supportsTmux = false))
        }
    }

    @Test
    fun nonBuiltinTypesAlwaysVisible() {
        // 用户自己写的命令/按键码/转义——能不能跑由他自己负责，不替他门控（哪怕 command 恰好叫 tmux_sessions）。
        listOf("command", "keycode", "escape").forEach { type ->
            assertTrue(type, btn(type, "tmux_sessions").visibleOn(supportsTmux = false))
        }
    }

    @Test
    fun unknownBuiltinIdIsVisible() {
        // 前向兼容：与 ButtonBuiltins.invoke 的「未知 id → no-op」一致，不误杀未来版本新增的动作。
        assertTrue(btn("builtin", "some_future_action").visibleOn(supportsTmux = false))
    }

    @Test
    fun actionIdMatchIsTrimmedAndCaseInsensitive() {
        // 与 ButtonBuiltins.invoke 的 `trim().lowercase()` 归一化保持一致，否则「显示了但点不动」。
        assertFalse(btn("builtin", "  TMUX_Sessions ").visibleOn(supportsTmux = false))
    }
}

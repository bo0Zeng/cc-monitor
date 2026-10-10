package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.data.db.CustomButton
import com.ccmonitor.mobile.core.data.db.CustomButtonType
import com.ccmonitor.mobile.core.terminal.KeyModifiers
import com.ccmonitor.mobile.core.terminal.customButtonBytes
import com.ccmonitor.mobile.core.terminal.escapeSequenceBytes

/**
 * 内建动作束：`builtin` 类型的按钮经 [invoke] 触达终端 tab 已有的 UI 回调。
 * tmux 的点击门控也在 [invoke] 里：[supportsTmux]=false 时 [TMUX_ONLY_BUILTINS] 里的 id 直接 no-op。
 * 名单只在 [TMUX_ONLY_BUILTINS] 一处，[visibleOn]（管显示）与 [invoke]（管点击）都读它。
 */
class ButtonBuiltins(
    val supportsTmux: Boolean,
    val onShowTabs: () -> Unit,
    val onCommandPalette: () -> Unit,
    val onTmuxSessions: () -> Unit,
    val onPaste: () -> Unit,
    val onShowHistory: () -> Unit,
    val onCapture: () -> Unit,
    val onPushKey: () -> Unit,
    val onShowKeyboard: () -> Unit,
    val onToggleCtrl: () -> Unit,
    val onToggleAlt: () -> Unit,
) {
    /** 按 action id 触达对应动作。tmux 域 id 在非 tmux 主机上 no-op（任意组的按钮都可能配它）；未知 id 也 no-op。 */
    fun invoke(actionId: String) {
        val id = actionId.trim().lowercase()
        if (id in TMUX_ONLY_BUILTINS && !supportsTmux) return
        when (id) {
            "show_tabs" -> onShowTabs()
            "command_palette" -> onCommandPalette()
            "tmux_sessions" -> onTmuxSessions()
            "paste" -> onPaste()
            "show_history" -> onShowHistory()
            "capture" -> onCapture()
            "push_key" -> onPushKey()
            "show_keyboard" -> onShowKeyboard()
            "toggle_ctrl" -> onToggleCtrl()
            "toggle_alt" -> onToggleAlt()
            else -> {}
        }
    }
}

/**
 * 需要 tmux 能力位的内建动作 id，唯一定义处。同时管两件事：
 * - [ButtonBuiltins.invoke] 读它管点击：任意组的任意按钮都能配成 tmux builtin，非 tmux 主机上点了要无害；
 * - [visibleOn] 读它管显示：工具条内建按钮由 DB 渲染，非 tmux 主机上照样画出来就是按了没反应的死按钮。
 */
val TMUX_ONLY_BUILTINS = setOf("tmux_sessions", "show_history", "capture", "push_key")

/**
 * 本按钮在当前主机上是否该渲染。只有属于 [TMUX_ONLY_BUILTINS] 的 `builtin` 受门控；
 * command/keycode/escape 一律可见（自己写的命令能不能跑，由写的人负责）。
 * 未知 builtin id 也可见，与 [ButtonBuiltins.invoke] 对未知 id 的 no-op 一致。
 */
fun CustomButton.visibleOn(supportsTmux: Boolean): Boolean =
    supportsTmux ||
        CustomButtonType.fromDb(type) != CustomButtonType.BUILTIN ||
        command.trim().lowercase() !in TMUX_ONLY_BUILTINS

/** 内建动作 id → 中文标签（编辑器下拉选项 / 副标题）。 */
val BUILTIN_ACTIONS: List<Pair<String, String>> =
    listOf(
        "show_tabs" to "标签面板",
        "command_palette" to "命令面板",
        "tmux_sessions" to "tmux 会话",
        "paste" to "粘贴",
        "show_history" to "Claude 历史",
        "capture" to "抓屏",
        "push_key" to "推送公钥",
        "show_keyboard" to "软键盘",
        "toggle_ctrl" to "Ctrl 粘性",
        "toggle_alt" to "Alt 粘性",
    )

/**
 * 按 [CustomButton.type] 派发一次按钮触发（tap）。
 * - command/keycode → [customButtonBytes]：command 行无修饰，发 `command\n`；keycode 行有修饰，发按键码。
 * - escape → [escapeSequenceBytes]（原始转义序列）。builtin → [ButtonBuiltins.invoke]，不出线字节。
 * - switch → [onSwitchGroup]（`command` 是目标 btnGroup），不出线字节。默认 no-op；渲染层（`ExtraKeysRow`）
 *   注入真正的活动组 setter，非切换场景（如命令面板）用默认即可。
 */
fun dispatchButton(
    button: CustomButton,
    onSend: (ByteArray) -> Unit,
    builtins: ButtonBuiltins,
    onSwitchGroup: (String) -> Unit = {},
) {
    when (CustomButtonType.fromDb(button.type)) {
        CustomButtonType.COMMAND, CustomButtonType.KEYCODE ->
            onSend(customButtonBytes(button.command, KeyModifiers(ctrl = button.ctrl, alt = button.alt, shift = button.shift)))
        CustomButtonType.ESCAPE -> onSend(escapeSequenceBytes(button.command))
        CustomButtonType.BUILTIN -> builtins.invoke(button.command)
        CustomButtonType.SWITCH -> onSwitchGroup(button.command)
    }
}

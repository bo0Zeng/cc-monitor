package com.ccmonitor.mobile.core.terminal

import android.graphics.Typeface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.sp
import org.connectbot.terminal.SelectionController
import org.connectbot.terminal.Terminal
import org.connectbot.terminal.TerminalEmulator
import org.connectbot.terminal.TerminalEmulatorFactory

/*
 * core-terminal：termlib 的薄封装。termlib 的配置/构造只在这个模块（:app 不直接 new/配置 termlib）；
 * TerminalEmulator 与 SelectionController 这两个运行时句柄类型经 api(termlib) 暴露给 :app。
 */

/** TerminalPalette 的 16 ANSI 色 → termlib 期望的 ARGB IntArray。 */
fun TerminalPalette.toAnsiIntArray(): IntArray = IntArray(16) { ansi[it].toArgb() }

/**
 * 创建并记住一个按 [palette] 配色的 TerminalEmulator。
 * onKeyboardInput：用户键入产生的字节（真实场景写到 SSH channel）。
 */
@Composable
fun rememberTerminalEmulator(
    rows: Int = 24,
    cols: Int = 80,
    palette: TerminalPalette = OneHalfDarkPalette,
    onKeyboardInput: (ByteArray) -> Unit = {},
): TerminalEmulator =
    remember(palette) {
        TerminalEmulatorFactory
            .create(
                initialRows = rows,
                initialCols = cols,
                defaultForeground = palette.foreground,
                defaultBackground = palette.background,
                onKeyboardInput = onKeyboardInput,
            ).apply {
                applyColorScheme(
                    ansiColors = palette.toAnsiIntArray(),
                    defaultForeground = palette.foreground.toArgb(),
                    defaultBackground = palette.background.toArgb(),
                )
            }
    }

/**
 * 包 termlib [Terminal]，套用 [palette] 的前/背景色。等宽用系统 MONOSPACE。
 * 字号经 termlib 内建 pinch 缩放（[initialFontSize]/[minFontSize]/[maxFontSize]）。
 * pinch 结束会把缩放折算进真实字号并经 [onFontSizeChanged] 上报，:app 持久化后回喂 [initialFontSize]。
 */
@Composable
fun AtermTerminal(
    emulator: TerminalEmulator,
    modifier: Modifier = Modifier,
    palette: TerminalPalette = OneHalfDarkPalette,
    keyboardEnabled: Boolean = false,
    showSoftKeyboard: Boolean = true,
    initialFontSize: TextUnit = 13.sp,
    minFontSize: TextUnit = 7.sp,
    maxFontSize: TextUnit = 28.sp,
    // 焦点请求器：:app 拿它拉起键盘（requestFocus termlib 的 ImeInputView → 重新弹软键盘）。
    focusRequester: FocusRequester = remember { FocusRequester() },
    // termlib 的选区控制器（程序化 selectAll/copySelection），:app 拿来做"复制输出"。
    onSelectionControllerAvailable: ((SelectionController) -> Unit)? = null,
    // 捏合缩放落定的新字号（termlib 已 clamp 到 min/max），:app 更新 fontSize 状态以持久化。
    onFontSizeChanged: (TextUnit) -> Unit = {},
    // 终端空白区单击（termlib 手势状态机判为 tap：非 scroll/selection/zoom、无选区、非超链接）。
    // :app 接它做「点击终端唤起软键盘」。termlib 侧 tap 已 requestFocus（keyboardEnabled 时），
    // 但自定义 View 上仅请求焦点常弹不出 IME，:app 在这个回调里补 WindowInsetsController.show(ime())。
    onTerminalTap: () -> Unit = {},
) {
    Terminal(
        terminalEmulator = emulator,
        modifier = modifier,
        typeface = Typeface.MONOSPACE,
        initialFontSize = initialFontSize,
        minFontSize = minFontSize,
        maxFontSize = maxFontSize,
        backgroundColor = palette.background,
        foregroundColor = palette.foreground,
        keyboardEnabled = keyboardEnabled,
        showSoftKeyboard = showSoftKeyboard,
        focusRequester = focusRequester,
        onSelectionControllerAvailable = onSelectionControllerAvailable ?: {},
        onFontSizeChanged = onFontSizeChanged,
        onTerminalTap = onTerminalTap,
    )
}

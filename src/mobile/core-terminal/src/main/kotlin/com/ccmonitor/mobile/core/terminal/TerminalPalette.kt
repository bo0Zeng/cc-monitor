package com.ccmonitor.mobile.core.terminal

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color

/**
 * 终端画面的配色：底 · 默认字 · 光标 · 选区 ＋ ANSI 16 色。是数据，喂 termlib。
 * 值不住在这里：app 从桌面同一份令牌（`--term-*`）映射一份交进来（`ui/theme/TerminalTheme.kt`），终端引擎不依赖 core-ui。
 */
@Immutable
data class TerminalPalette(
    val foreground: Color,
    val background: Color,
    val cursor: Color,
    val selection: Color,
    val ansi: List<Color>, // 16 项：0-7 普通，8-15 高亮
)

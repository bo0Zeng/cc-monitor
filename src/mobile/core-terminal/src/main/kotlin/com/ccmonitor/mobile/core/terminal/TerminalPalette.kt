package com.ccmonitor.mobile.core.terminal

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color

/**
 * 终端 ANSI 16 色（One Half Dark）。是数据，喂 termlib，与 Material 主题分离。
 * 终端画布始终深色（终端工具惯例），不随 App light/dark 切换。
 * palette 是终端引擎的概念，所以放在 core-terminal，不依赖 core-ui。
 */
@Immutable
data class TerminalPalette(
    val foreground: Color,
    val background: Color,
    val cursor: Color,
    val selection: Color,
    val ansi: List<Color>, // 16 项：0-7 普通，8-15 高亮
)

/** One Half Dark 默认配色。值全部内联，palette 自足。 */
val OneHalfDarkPalette =
    TerminalPalette(
        foreground = Color(0xFFDCDFE4),
        background = Color(0xFF15181E), // 终端画布独立于 App 主题，不随其调整
        cursor = Color(0xFF5B9DF0),
        selection = Color(0x1F5B9DF0),
        ansi =
            listOf(
                Color(0xFF282C34),
                Color(0xFFE06C75),
                Color(0xFF98C379),
                Color(0xFFE5C07B),
                Color(0xFF61AFEF),
                Color(0xFFC678DD),
                Color(0xFF56B6C2),
                Color(0xFFDCDFE4),
                Color(0xFF5C6370),
                Color(0xFFE06C75),
                Color(0xFF98C379),
                Color(0xFFE5C07B),
                Color(0xFF61AFEF),
                Color(0xFFC678DD),
                Color(0xFF56B6C2),
                Color(0xFFDCDFE4),
            ),
    )

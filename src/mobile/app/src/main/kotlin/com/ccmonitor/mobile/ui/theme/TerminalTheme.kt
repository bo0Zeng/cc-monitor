package com.ccmonitor.mobile.ui.theme

import com.ccmonitor.mobile.core.terminal.TerminalPalette
import com.ccmonitor.mobile.core.ui.theme.ThemeTokens

/**
 * 终端画面的配色取桌面同一族令牌 `--term-*`（画面是内容色，不是界面色；后端给的 16 色名照这一族画）。
 * 光标 `--accent`、选区 `--selected-bg`。只做映射。
 */
val AppTerminalPalette =
    TerminalPalette(
        foreground = ThemeTokens.TermFg,
        background = ThemeTokens.TermBg,
        cursor = ThemeTokens.Accent,
        selection = ThemeTokens.SelectedBg,
        ansi =
            listOf(
                ThemeTokens.TermBlack,
                ThemeTokens.TermRed,
                ThemeTokens.TermGreen,
                ThemeTokens.TermYellow,
                ThemeTokens.TermBlue,
                ThemeTokens.TermMagenta,
                ThemeTokens.TermCyan,
                ThemeTokens.TermWhite,
                ThemeTokens.TermBrightBlack,
                ThemeTokens.TermBrightRed,
                ThemeTokens.TermBrightGreen,
                ThemeTokens.TermBrightYellow,
                ThemeTokens.TermBrightBlue,
                ThemeTokens.TermBrightMagenta,
                ThemeTokens.TermBrightCyan,
                ThemeTokens.TermBrightWhite,
            ),
    )

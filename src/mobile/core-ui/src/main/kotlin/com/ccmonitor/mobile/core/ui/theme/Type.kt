package com.ccmonitor.mobile.core.ui.theme

import androidx.compose.material3.Typography
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import com.ccmonitor.mobile.core.ui.R

/** 等宽：JetBrains Mono（打包）。终端 · 代码 · diff · 路径用。 */
val JetBrainsMono =
    FontFamily(
        Font(R.font.jetbrains_mono_regular, FontWeight.Normal),
        Font(R.font.jetbrains_mono_medium, FontWeight.Medium),
    )

/** 界面字与消息流正文：系统无衬线（Android ＝ Roboto ＋ Noto Sans CJK）；桌面的 Inter / Source Serif 不打包（主会话 10-11 定）。 */
val UiSans = FontFamily.Default

private val Strong = FontWeight(ThemeTokens.FontWeightStrong.toInt())

private fun ui(
    size: TextUnit,
    weight: FontWeight = FontWeight.Normal,
    lineHeight: Float = ThemeTokens.LineHeightUi,
) = TextStyle(fontFamily = UiSans, fontWeight = weight, fontSize = size, lineHeight = lineHeight.em)

/**
 * 字号六档一比一换成 sp（`--font-size-*`）：11 · 12 · 13（等宽）· 14 · 15（只给消息流）· 16（标题，600）；字重只有 400 / 600。
 * M3 每个槽都给，不留 M3 的 20 / 22 / 24 / 57 那些档。行高：控件 1.45 · 正文 1.55 · 消息流 1.65。
 */
val AppTypography =
    Typography(
        displayLarge = ui(ThemeTokens.FontSizeTitle, Strong),
        displayMedium = ui(ThemeTokens.FontSizeTitle, Strong),
        displaySmall = ui(ThemeTokens.FontSizeTitle, Strong),
        headlineLarge = ui(ThemeTokens.FontSizeTitle, Strong),
        headlineMedium = ui(ThemeTokens.FontSizeTitle, Strong),
        headlineSmall = ui(ThemeTokens.FontSizeTitle, Strong),
        titleLarge = ui(ThemeTokens.FontSizeTitle, Strong),
        titleMedium = ui(ThemeTokens.FontSizeTitle, Strong),
        titleSmall = ui(ThemeTokens.FontSizeBase, Strong),
        bodyLarge = ui(ThemeTokens.FontSizeProse, lineHeight = ThemeTokens.LineHeightProse),
        bodyMedium = ui(ThemeTokens.FontSizeBase, lineHeight = ThemeTokens.LineHeightBase),
        bodySmall = ui(ThemeTokens.FontSizeSmall),
        labelLarge = ui(ThemeTokens.FontSizeBase),
        labelMedium = ui(ThemeTokens.FontSizeSmall),
        labelSmall = ui(ThemeTokens.FontSizeXs),
    )

// 非 Material 槽的等宽样式
val monoBody =
    TextStyle(fontFamily = JetBrainsMono, fontWeight = FontWeight.Normal, fontSize = ThemeTokens.FontSizeMono, lineHeight = ThemeTokens.LineHeightMono.em)
val monoSmall =
    TextStyle(fontFamily = JetBrainsMono, fontWeight = FontWeight.Normal, fontSize = ThemeTokens.FontSizeSmall, lineHeight = ThemeTokens.LineHeightMono.em)

/** 终端画面字号是内容（用户可调），不进六档。 */
val terminalTextStyle = TextStyle(fontFamily = JetBrainsMono, fontWeight = FontWeight.Normal, fontSize = 14.sp)

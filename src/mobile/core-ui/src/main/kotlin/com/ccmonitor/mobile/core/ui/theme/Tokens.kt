package com.ccmonitor.mobile.core.ui.theme

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** 4dp 栅格间距。 */
@Immutable
data class Spacing(
    val xxs: Dp = 2.dp,
    val xs: Dp = 4.dp,
    val sm: Dp = 8.dp,
    val md: Dp = 12.dp,
    val lg: Dp = 16.dp,
    val xl: Dp = 24.dp,
    val xxl: Dp = 32.dp,
    val xxxl: Dp = 48.dp,
)

/**
 * Material3 ColorScheme 装不下的 token。
 * 经 [LocalAppTokens] 提供，用法：`LocalAppTokens.current.success`。
 */
@Immutable
data class AppTokens(
    val textFaint: Color,
    val success: Color,
    val warn: Color,
    val surfaceRaised: Color,
    val surfaceHigh: Color,
    val accentSoft: Color,
    // diff（实心左边框为主信号；bg 为辅）
    val diffAddBg: Color,
    val diffAddBorder: Color,
    val diffDelBg: Color,
    val diffDelBorder: Color,
    val spacing: Spacing = Spacing(),
)

val DarkAppTokens =
    AppTokens(
        textFaint = DarkTextFaint,
        success = DarkSuccess,
        warn = DarkWarn,
        surfaceRaised = DarkSurfaceRaised,
        surfaceHigh = DarkSurfaceHigh,
        accentSoft = DarkAccentSoft,
        diffAddBg = DarkDiffAddBg,
        diffAddBorder = DarkSuccess,
        diffDelBg = DarkDiffDelBg,
        diffDelBorder = DarkError,
    )

val LightAppTokens =
    AppTokens(
        textFaint = LightTextFaint,
        success = LightSuccess,
        warn = LightWarn,
        surfaceRaised = LightSurfaceRaised,
        surfaceHigh = LightSurfaceHigh,
        accentSoft = LightAccentSoft,
        diffAddBg = LightDiffAddBg,
        diffAddBorder = LightSuccess,
        diffDelBg = LightDiffDelBg,
        diffDelBorder = LightError,
    )

val LocalAppTokens = staticCompositionLocalOf { DarkAppTokens }

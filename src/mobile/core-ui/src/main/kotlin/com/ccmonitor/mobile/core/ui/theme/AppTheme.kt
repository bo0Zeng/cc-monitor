package com.ccmonitor.mobile.core.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider

// 不用 Material You 动态取色：终端工具要稳定调色板。
private val DarkScheme =
    darkColorScheme(
        primary = DarkAccent,
        onPrimary = DarkOnAccent,
        background = DarkBg,
        onBackground = DarkText,
        surface = DarkSurface,
        onSurface = DarkText,
        surfaceVariant = DarkSurfaceRaised,
        onSurfaceVariant = DarkText2,
        surfaceContainer = DarkSurfaceRaised,
        surfaceContainerHigh = DarkSurfaceHigh,
        outline = DarkOutline,
        outlineVariant = DarkOutlineSoft,
        error = DarkError,
    )

private val LightScheme =
    lightColorScheme(
        primary = LightAccent,
        onPrimary = LightOnAccent,
        background = LightBg,
        onBackground = LightText,
        surface = LightSurface,
        onSurface = LightText,
        surfaceVariant = LightSurfaceRaised,
        onSurfaceVariant = LightText2,
        surfaceContainer = LightSurfaceRaised,
        surfaceContainerHigh = LightSurfaceHigh,
        outline = LightOutline,
        outlineVariant = LightOutlineSoft,
        error = LightError,
    )

@Composable
fun AppTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    val scheme = if (darkTheme) DarkScheme else LightScheme
    val tokens = if (darkTheme) DarkAppTokens else LightAppTokens
    CompositionLocalProvider(LocalAppTokens provides tokens) {
        MaterialTheme(
            colorScheme = scheme,
            typography = AppTypography,
            content = content,
        )
    }
}

package com.ccmonitor.mobile.core.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider

/**
 * M3 的每个颜色槽取哪个令牌。只做映射，值全在 `tokens.css`。
 * 每个槽都显式给：漏一个就落回 M3 的默认紫（判据 `ThemeLedgerTest` 逐槽比 M3 默认值）。
 *
 * - `--accent` 不进 `primary`：M3 拿 primary 当 TextButton 的字色，accent 当字只有 3.7:1；它在 [AppTokens.accent]。
 * - 主按钮 · 悬浮按钮 ＝ `--accent-strong`（primary · primaryContainer）。
 * - `errorContainer` ＝ `--card`（错误条另画 3 dp `--error` 左条）。
 */
val AppColorScheme: ColorScheme =
    darkColorScheme(
        primary = ThemeTokens.AccentStrong,
        onPrimary = ThemeTokens.TextOnStrong,
        primaryContainer = ThemeTokens.AccentStrong,
        onPrimaryContainer = ThemeTokens.TextOnStrong,
        inversePrimary = ThemeTokens.Accent,
        secondary = ThemeTokens.Accent,
        onSecondary = ThemeTokens.TextOnStrong,
        secondaryContainer = ThemeTokens.SelectedBg,
        onSecondaryContainer = ThemeTokens.Text,
        tertiary = ThemeTokens.ColorLink,
        onTertiary = ThemeTokens.Bg2,
        tertiaryContainer = ThemeTokens.Card,
        onTertiaryContainer = ThemeTokens.Text,
        background = ThemeTokens.Bg,
        onBackground = ThemeTokens.Text,
        surface = ThemeTokens.Bg,
        onSurface = ThemeTokens.Text,
        surfaceVariant = ThemeTokens.Card,
        onSurfaceVariant = ThemeTokens.Text2,
        surfaceTint = ThemeTokens.Bg,
        inverseSurface = ThemeTokens.Card,
        inverseOnSurface = ThemeTokens.Text,
        error = ThemeTokens.Error,
        onError = ThemeTokens.TextOnStrong,
        errorContainer = ThemeTokens.Card,
        onErrorContainer = ThemeTokens.Text,
        outline = ThemeTokens.BorderMedium,
        outlineVariant = ThemeTokens.BorderSoft,
        scrim = ThemeTokens.OverlayBackdrop,
        surfaceBright = ThemeTokens.Card,
        surfaceContainer = ThemeTokens.Card,
        surfaceContainerHigh = ThemeTokens.Card,
        surfaceContainerHighest = ThemeTokens.Card,
        surfaceContainerLow = ThemeTokens.Bg,
        surfaceContainerLowest = ThemeTokens.Bg2,
        surfaceDim = ThemeTokens.Bg2,
    ).copy(
        // 「固定」那几族（不随深浅变的强调面）：手机只有深色，照同名的非固定槽给。
        primaryFixed = ThemeTokens.AccentStrong,
        primaryFixedDim = ThemeTokens.Accent,
        onPrimaryFixed = ThemeTokens.TextOnStrong,
        onPrimaryFixedVariant = ThemeTokens.TextOnStrong,
        secondaryFixed = ThemeTokens.Card,
        secondaryFixedDim = ThemeTokens.Bg2,
        onSecondaryFixed = ThemeTokens.Text,
        onSecondaryFixedVariant = ThemeTokens.Text2,
        tertiaryFixed = ThemeTokens.ColorLink,
        tertiaryFixedDim = ThemeTokens.ColorLink,
        onTertiaryFixed = ThemeTokens.Bg2,
        onTertiaryFixedVariant = ThemeTokens.Bg2,
    )

/** 圆角照桌面四档：M3 的 small / medium（按钮 · chip · 输入）取 m，large（卡 · 菜单）取 l，extraLarge（对话框 · 底部抽屉）取 xl。 */
val AppShapes =
    Shapes(
        extraSmall = RoundedCornerShape(ThemeTokens.RadiusS),
        small = RoundedCornerShape(ThemeTokens.RadiusM),
        medium = RoundedCornerShape(ThemeTokens.RadiusM),
        large = RoundedCornerShape(ThemeTokens.RadiusL),
        extraLarge = RoundedCornerShape(ThemeTokens.RadiusXl),
    )

/**
 * 固定深色：规范里只有深色令牌（`tokens.css` 没有浅色那一块），手机与桌面完全一致，不跟系统深浅走。
 * 以后加浅色：`tokens.css` 加一块浅色值、生成器多出一套，这里再按系统切换。
 * 不用 Material You 动态取色。
 */
@Composable
fun AppTheme(content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalAppTokens provides AppTokens()) {
        MaterialTheme(
            colorScheme = AppColorScheme,
            typography = AppTypography,
            shapes = AppShapes,
            content = content,
        )
    }
}

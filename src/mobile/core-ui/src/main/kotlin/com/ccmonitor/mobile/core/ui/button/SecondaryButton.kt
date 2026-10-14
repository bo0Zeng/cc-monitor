package com.ccmonitor.mobile.core.ui.button

import androidx.compose.foundation.layout.RowScope
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import com.ccmonitor.mobile.core.ui.theme.AppTokens
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens

/**
 * 次按钮的字色：常态 `--text`（`onSurface`），危险项（删除 · 结束 · 覆盖）`--error-text`。
 * 不用 M3 默认的 `primary`（`--accent-strong`）：它在 `--card` 上不到 3:1。
 */
fun secondaryContentColor(
    scheme: ColorScheme,
    tokens: AppTokens,
    danger: Boolean,
): Color = if (danger) tokens.errorText else scheme.onSurface

/**
 * 次按钮（只有字、没有底）：全手机唯一用 M3 `TextButton` 的地方，各屏只调它（判据 `SecondaryButtonTest`）。
 * 字色见 [secondaryContentColor]；主按钮（`--accent-strong` 底）另是 `Button`，不经这里。
 */
@Composable
fun SecondaryButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    danger: Boolean = false,
    content: @Composable RowScope.() -> Unit,
) {
    val color = secondaryContentColor(MaterialTheme.colorScheme, LocalAppTokens.current, danger)
    TextButton(
        onClick = onClick,
        modifier = modifier,
        enabled = enabled,
        colors = ButtonDefaults.textButtonColors(contentColor = color, disabledContentColor = LocalAppTokens.current.textFaint),
        content = content,
    )
}

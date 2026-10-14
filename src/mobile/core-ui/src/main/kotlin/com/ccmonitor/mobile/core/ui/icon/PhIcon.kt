package com.ccmonitor.mobile.core.ui.icon

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Button
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.unit.Dp
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.R
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.ThemeTokens
import com.ccmonitor.mobile.core.ui.theme.Touch

/** Phosphor regular 子集字体。 */
val PhosphorFont = FontFamily(Font(R.font.phosphor_regular))

/**
 * 一个 Phosphor 图标：字体里那个码位画成一个方块，颜色跟当前内容色走（`currentColor`，常态 `--text-2`、当前 `--accent` 由调用方给）。
 * [contentDescription] 为 null ⇒ 读屏跳过（旁边已有字时用）；非 null ⇒ 念那句（照文案表）。
 */
@Composable
fun PhIcon(
    icon: Ph,
    contentDescription: String?,
    modifier: Modifier = Modifier,
    size: Dp = Touch.iconBar,
    tint: Color = LocalContentColor.current,
) {
    val fontSize = with(LocalDensity.current) { size.toSp() }
    val sem =
        if (contentDescription == null) {
            Modifier.clearAndSetSemantics {}
        } else {
            Modifier.clearAndSetSemantics { this.contentDescription = contentDescription }
        }
    Box(modifier.size(size).then(sem), contentAlignment = Alignment.Center) {
        Text(
            text = icon.glyph,
            color = tint,
            style =
                TextStyle(
                    fontFamily = PhosphorFont,
                    fontSize = fontSize,
                    lineHeight = fontSize,
                    lineHeightStyle = LineHeightStyle(LineHeightStyle.Alignment.Center, LineHeightStyle.Trim.Both),
                ),
        )
    }
}

/** 图标按钮：命中区 48 dp（[Touch.target]），图标 20 dp；无障碍名必给（没有字陪着）。 */
@Composable
fun PhIconButton(
    icon: Ph,
    contentDescription: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    tint: Color = LocalContentColor.current,
) {
    IconButton(onClick = onClick, modifier = modifier.semantics { this.contentDescription = contentDescription }, enabled = enabled) {
        PhIcon(icon, contentDescription = null, tint = if (enabled) tint else tint.copy(alpha = ThemeTokens.StateDisabledOpacity))
    }
}

/** 二级页左上的返回：arrow-left 图标按钮（稿 `外观对齐` 第 13 节：「← 返回」字换图标），无障碍名「返回」。 */
@Composable
fun BackButton(
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    PhIconButton(Ph.ArrowLeft, copyText("mobKit.act.back"), onClick = onBack, modifier = modifier, tint = MaterialTheme.colorScheme.onSurfaceVariant)
}

/** 次按钮：行内 16 dp 图标 ＋ 字（「＋ 新建目录」这种，字照文案表、不带字符图标）。 */
@Composable
fun IconTextButton(
    icon: Ph,
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    TextButton(onClick = onClick, modifier = modifier, enabled = enabled) {
        PhIcon(icon, contentDescription = null, size = Touch.iconInline)
        Spacer(Modifier.width(LocalAppTokens.current.spacing.xs))
        Text(text)
    }
}

/** 主按钮（`--accent-strong` 底）：行内 16 dp 图标 ＋ 字。一屏最多一颗。 */
@Composable
fun PrimaryIconButton(
    icon: Ph,
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Button(onClick = onClick, modifier = modifier) {
        PhIcon(icon, contentDescription = null, size = Touch.iconInline)
        Spacer(Modifier.width(LocalAppTokens.current.spacing.xs))
        Text(text)
    }
}

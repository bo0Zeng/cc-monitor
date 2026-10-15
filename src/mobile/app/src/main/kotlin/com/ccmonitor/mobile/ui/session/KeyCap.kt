package com.ccmonitor.mobile.ui.session

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch

/** 键帽名里当图标用的那几个字符 → Phosphor。 */
private val KEY_GLYPHS =
    mapOf(
        "↑" to Ph.ArrowUp,
        "↓" to Ph.ArrowDown,
        "←" to Ph.ArrowLeft,
        "→" to Ph.ArrowRight,
        "⇌" to Ph.ArrowsLeftRight,
        "⏎" to Ph.ArrowElbowDownLeft,
        "⌫" to Ph.Backspace,
        "＋" to Ph.Plus,
        "+" to Ph.Plus,
    )

/**
 * 键帽的字是数据（另写的存在库里，没另写的照单击自动写）：整个是、或以「字符＋空格」开头的那几个图标字符，
 * 画成 Phosphor 图标 ＋ 余下的字；其余照字画。返回 null ＝ 没有图标。库里的名字不改（改了要迁移，读屏照旧念原名）。
 */
internal fun keyCapIcon(label: String): Pair<Ph, String>? {
    KEY_GLYPHS[label]?.let { return it to "" }
    val head = label.substringBefore(' ', missingDelimiterValue = "")
    val icon = KEY_GLYPHS[head] ?: return null
    return icon to label.substringAfter(' ').trim()
}

/** 键帽上的那一格：有图标画图标（行内 16 dp），再跟余下的字。 */
@Composable
internal fun KeyCapLabel(
    label: String,
    style: TextStyle,
    color: Color = LocalContentColor.current,
) {
    val icon = keyCapIcon(label)
    if (icon == null) {
        Text(text = label, style = style, color = color)
        return
    }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(LocalAppTokens.current.spacing.xs)) {
        PhIcon(icon.first, contentDescription = label, size = Touch.iconInline, tint = color)
        if (icon.second.isNotEmpty()) Text(text = icon.second, style = style, color = color)
    }
}

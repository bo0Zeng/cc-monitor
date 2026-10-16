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

/** 键帽可选的图标（选图标那一屏照这个先后列）。库里存的是 Phosphor 的图标名（[iconName]，`arrow-up` 这种），不存图。 */
internal val KEY_ICONS: List<Ph> =
    listOf(
        Ph.ArrowUp,
        Ph.ArrowDown,
        Ph.ArrowLeft,
        Ph.ArrowRight,
        Ph.ArrowsLeftRight,
        Ph.ArrowElbowDownLeft,
        Ph.Backspace,
        Ph.ArrowLineUp,
        Ph.ArrowLineDown,
        Ph.Tabs,
        Ph.Keyboard,
        Ph.Command,
        Ph.Clipboard,
        Ph.Copy,
        Ph.MagnifyingGlass,
        Ph.FloppyDisk,
        Ph.ArrowCounterClockwise,
        Ph.X,
        Ph.Check,
        Ph.Play,
        Ph.Stop,
        Ph.SignOut,
        Ph.TerminalWindow,
        Ph.GitBranch,
    )

/** Phosphor 的图标名（桌面 `@phosphor-icons/core` 同一套写法）：`ArrowElbowDownLeft` → `arrow-elbow-down-left`。 */
internal fun Ph.iconName(): String = name.replace(Regex("(?<=[a-z0-9])(?=[A-Z])"), "-").lowercase()

/** 库里存的图标名 → 图标；没选 · 不在 [KEY_ICONS] 里 ⇒ null（照字画）。 */
internal fun keyIcon(name: String?): Ph? = name?.let { n -> KEY_ICONS.firstOrNull { it.iconName() == n } }

/**
 * 键帽上的那一格。另选了图标（[icon]，库里的图标名）⇒ 只画那个图标，读屏念 [label]；
 * 没选 ⇒ [label] 里当图标用的字符照 [keyCapIcon] 画成图标（行内 16 dp），再跟余下的字。
 */
@Composable
internal fun KeyCapLabel(
    label: String,
    style: TextStyle,
    color: Color = LocalContentColor.current,
    icon: String? = null,
) {
    keyIcon(icon)?.let {
        PhIcon(it, contentDescription = label, size = Touch.iconInline, tint = color)
        return
    }
    val glyph = keyCapIcon(label)
    if (glyph == null) {
        Text(text = label, style = style, color = color)
        return
    }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(LocalAppTokens.current.spacing.xs)) {
        PhIcon(glyph.first, contentDescription = label, size = Touch.iconInline, tint = color)
        if (glyph.second.isNotEmpty()) Text(text = glyph.second, style = style, color = color)
    }
}

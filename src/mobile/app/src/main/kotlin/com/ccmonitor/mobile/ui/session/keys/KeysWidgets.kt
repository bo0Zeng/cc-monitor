package com.ccmonitor.mobile.ui.session.keys

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.PlainTooltip
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TooltipBox
import androidx.compose.material3.TooltipDefaults
import androidx.compose.material3.rememberTooltipState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.KeyPress
import com.ccmonitor.mobile.core.data.db.pressCap
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.ui.session.KeyCapLabel
import kotlinx.coroutines.launch

/** 编辑页里画的一颗小键帽（`--card` 底 ＋ 边，等宽小字）；字里的方向 · 回车这类画成图标。 */
@Composable
internal fun MiniCap(
    label: String,
    modifier: Modifier = Modifier,
    active: Boolean = false,
    onClick: (() -> Unit)? = null,
) {
    val tokens = LocalAppTokens.current
    val shape = RoundedCornerShape(tokens.radius.m)
    Box(
        modifier
            .clip(shape)
            .background(if (active) tokens.selectedBg else tokens.card)
            .border(1.dp, if (active) tokens.accent else MaterialTheme.colorScheme.outline, shape)
            .let { if (onClick != null) it.combinedClickable(onClick = onClick) else it }
            .defaultMinSize(minWidth = 36.dp, minHeight = 32.dp)
            .padding(horizontal = 8.dp, vertical = 6.dp),
        contentAlignment = Alignment.Center,
    ) {
        KeyCapLabel(label, style = monoSmall, color = MaterialTheme.colorScheme.onSurface)
    }
}

/** 录好的一串：组合 · 特殊键画成小键帽，打的字照原样。 */
@Composable
internal fun PressesLine(
    presses: List<KeyPress>,
    modifier: Modifier = Modifier,
) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(LocalAppTokens.current.spacing.xs), verticalAlignment = Alignment.CenterVertically) {
        presses.forEach { p ->
            if (p is KeyPress.Text) {
                Text(p.text, style = monoSmall, color = MaterialTheme.colorScheme.onSurface)
            } else {
                MiniCap(pressCap(p))
            }
        }
    }
}

/** 三段分段按钮（C23）：没设 · 送输入 · App 动作。 */
@Composable
internal fun BindSegments(
    selected: BindKind,
    onSelect: (BindKind) -> Unit,
    modifier: Modifier = Modifier,
) {
    val labels =
        listOf(
            BindKind.NONE to copyText("mobKeys.bind.none"),
            BindKind.INPUT to copyText("mobKeys.bind.input"),
            BindKind.ACTION to copyText("mobKeys.bind.action"),
        )
    Segments(labels, selected, onSelect, modifier)
}

/** 分段按钮：选中的那段 `--selected-bg` 底、下沿 accent。 */
@Composable
internal fun <T> Segments(
    options: List<Pair<T, String>>,
    selected: T,
    onSelect: (T) -> Unit,
    modifier: Modifier = Modifier,
) {
    val tokens = LocalAppTokens.current
    SingleChoiceSegmentedButtonRow(modifier.fillMaxWidth()) {
        options.forEachIndexed { i, (v, label) ->
            SegmentedButton(
                selected = v == selected,
                onClick = { onSelect(v) },
                shape = SegmentedButtonDefaults.itemShape(i, options.size),
                icon = {},
                colors =
                    SegmentedButtonDefaults.colors(
                        activeContainerColor = tokens.selectedBg,
                        activeContentColor = MaterialTheme.colorScheme.onSurface,
                        inactiveContainerColor = Color.Transparent,
                        inactiveContentColor = MaterialTheme.colorScheme.onSurfaceVariant,
                    ),
            ) { Text(label, style = MaterialTheme.typography.bodyMedium) }
        }
    }
}

/** 灰字一行（没设时的自然行为 · 说明）。 */
@Composable
internal fun HintLine(
    text: String,
    modifier: Modifier = Modifier,
) {
    Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = modifier)
}

/** 一行带图标的提示：[warn] ⇒ 琥珀色警告；否则红色报错（I10：放在出错的东西旁边）。 */
@Composable
internal fun NoteLine(
    text: String,
    warn: Boolean,
    modifier: Modifier = Modifier,
) {
    val tokens = LocalAppTokens.current
    val color = if (warn) tokens.warn else tokens.errorText
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs)) {
        PhIcon(if (warn) Ph.Warning else Ph.WarningCircle, contentDescription = null, size = Touch.iconInline, tint = color)
        Text(text, style = MaterialTheme.typography.bodySmall, color = if (warn) MaterialTheme.colorScheme.onSurface else color)
    }
}

/** 长按出说明的标签（手机上的悬停）：说明只在编辑页的标签上有。单击照常交给 [onClick]（标签在一行里时就是那一行的点）。 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalFoundationApi::class)
@Composable
internal fun TipLabel(
    text: String,
    tip: String?,
    style: TextStyle,
    modifier: Modifier = Modifier,
    onClick: () -> Unit = {},
) {
    if (tip == null) {
        Text(text, style = style, modifier = modifier)
        return
    }
    val state = rememberTooltipState()
    val scope = rememberCoroutineScope()
    TooltipBox(
        positionProvider = TooltipDefaults.rememberTooltipPositionProvider(),
        tooltip = { PlainTooltip { Text(tip) } },
        state = state,
        enableUserInput = false,
    ) {
        Text(text, style = style, modifier = modifier.combinedClickable(onClick = onClick, onLongClick = { scope.launch { state.show() } }))
    }
}

/** 一节的小标题（`--text-2`）。 */
@Composable
internal fun SectionLabel(
    text: String,
    modifier: Modifier = Modifier,
) {
    Text(text, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = modifier)
}

/** 一行「标签 …… 值 ›」，点了进下一层（时长 · 所在组这类）。 */
@Composable
internal fun ValueRow(
    label: @Composable () -> Unit,
    value: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    inset: Boolean = true,
) {
    val tokens = LocalAppTokens.current
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = Touch.rowSingle)
            .combinedClickable(onClick = onClick)
            .padding(horizontal = if (inset) tokens.spacing.lg else 0.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.weight(1f)) { label() }
        Text(value, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        PhIcon(Ph.CaretRight, contentDescription = null, size = Touch.iconInline, tint = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

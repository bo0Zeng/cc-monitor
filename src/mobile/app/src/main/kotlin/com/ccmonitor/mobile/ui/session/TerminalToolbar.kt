package com.ccmonitor.mobile.ui.session

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIconButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall

/**
 * 终端屏的工具条：App 自带的固定一行（标签页 · 命令 · tmux 会话），不是按键数据，删不掉也不用编。
 * tmux 会话只在那台有 tmux 时画。
 */
@Composable
fun TerminalToolbar(
    supportsTmux: Boolean,
    onShowTabs: () -> Unit,
    onCommandPalette: () -> Unit,
    onTmuxSessions: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val tokens = LocalAppTokens.current
    Row(
        modifier
            .fillMaxWidth()
            .background(tokens.bg2)
            .padding(horizontal = tokens.spacing.sm, vertical = tokens.spacing.xs),
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PhIconButton(Ph.Tabs, copyText("mobTabs.open.aria"), onClick = onShowTabs, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        ToolChip(copyText("mobDefaultButtons.label.command"), onCommandPalette)
        if (supportsTmux) ToolChip(copyText("mobDefaultButtons.label.tmux"), onTmuxSessions)
    }
}

/** 工具条上的一格：扁平（透明底 ＋ `--text-2` 字），与键条上的按键分开。 */
@Composable
private fun ToolChip(
    label: String,
    onClick: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Box(
        modifier =
            Modifier
                .clip(RoundedCornerShape(tokens.radius.m))
                .clickable(onClick = onClick)
                .defaultMinSize(minWidth = 44.dp, minHeight = 36.dp)
                .padding(horizontal = 12.dp, vertical = 8.dp),
        contentAlignment = Alignment.Center,
    ) {
        KeyCapLabel(label, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

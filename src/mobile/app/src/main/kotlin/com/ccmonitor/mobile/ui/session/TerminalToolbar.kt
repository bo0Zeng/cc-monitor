package com.ccmonitor.mobile.ui.session

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.KeyGroup
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.icon.PhIconButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.ui.session.keys.KeysSheet
import com.ccmonitor.mobile.ui.session.keys.SheetRow

/**
 * 终端屏的工具条：App 自带的固定一行（标签页 · 命令 · tmux 会话 · ⋯），不是按键数据，键条怎么改都碰不到它。
 * tmux 会话在那台没有 tmux 时灰掉。⋯ 拉起底部抽屉：「切换组 ›」「编辑按键」。
 */
@Composable
fun TerminalToolbar(
    supportsTmux: Boolean,
    onShowTabs: () -> Unit,
    onCommandPalette: () -> Unit,
    onTmuxSessions: () -> Unit,
    onMore: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val tokens = LocalAppTokens.current
    Row(
        modifier
            .fillMaxWidth()
            .background(tokens.bg2)
            .padding(start = tokens.spacing.sm),
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        ToolChip(Ph.Tabs, copyText("mobKeys.action.tabs"), onShowTabs)
        ToolChip(Ph.Command, copyText("mobDefaultButtons.label.command"), onCommandPalette)
        ToolChip(Ph.TerminalWindow, copyText("mobDefaultButtons.label.tmux"), onTmuxSessions, enabled = supportsTmux)
        Spacer(Modifier.weight(1f))
        PhIconButton(Ph.DotsThree, copyText("mobKeys.tool.more"), onClick = onMore, tint = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** 工具条上的一格：扁平（透明底 ＋ `--text-2` 图标与字），与键条上的按键分开。 */
@Composable
private fun ToolChip(
    icon: Ph,
    label: String,
    onClick: () -> Unit,
    enabled: Boolean = true,
) {
    val tokens = LocalAppTokens.current
    val color = if (enabled) MaterialTheme.colorScheme.onSurfaceVariant else tokens.textFaint
    Row(
        modifier =
            Modifier
                .clip(RoundedCornerShape(tokens.radius.m))
                .clickable(enabled = enabled, onClick = onClick)
                .defaultMinSize(minWidth = 44.dp, minHeight = 36.dp)
                .padding(horizontal = 8.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs),
    ) {
        PhIcon(icon, contentDescription = null, size = Touch.iconInline, tint = color)
        Text(label, style = MaterialTheme.typography.bodySmall, color = color)
    }
}

/**
 * ⋯ 抽屉：「切换组 ›」行尾写当前组名，下面「编辑按键」。点「切换组 ›」进第二层：全部组照先后列，行尾写颗数，当前组打勾；
 * 点一组立刻切过去、抽屉关上（切组不出 toast，键条换了就是结果）。一个组都没有时「切换组 ›」灰掉。
 */
@Composable
fun TerminalMoreSheet(
    groups: List<KeyGroup>,
    counts: Map<String, Int>,
    current: KeyGroup?,
    onSwitch: (String) -> Unit,
    onEditKeys: () -> Unit,
    onDismiss: () -> Unit,
) {
    var pick by remember { mutableStateOf(false) }
    if (pick) {
        KeysSheet(copyText("mobKeys.more.switchGroup"), null, onDismiss) {
            groups.forEach { g ->
                SheetRow(
                    null,
                    g.name,
                    onClick = {
                        onSwitch(g.id)
                        onDismiss()
                    },
                    trailing = copyText("mobKeys.group.count", "n" to (counts[g.id] ?: 0)),
                    checked = g.id == current?.id,
                )
            }
        }
        return
    }
    KeysSheet(null, null, onDismiss) {
        SheetRow(Ph.Rows, copyText("mobKeys.more.switchGroup"), onClick = { pick = true }, trailing = current?.name, more = true, enabled = groups.isNotEmpty())
        SheetRow(Ph.PencilSimple, copyText("mobKeys.action.editKeys"), onClick = {
            onDismiss()
            onEditKeys()
        })
    }
}

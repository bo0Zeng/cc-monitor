package com.ccmonitor.mobile.ui.chats

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.ListSort
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.sheet.ActionSheet
import com.ccmonitor.mobile.core.ui.sheet.SheetAction
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch

/**
 * 漏斗拉起的筛选（同桌面历史页「筛选」）：看法 · 机器（每台一勾，断着的写「离线」）· 时间 · 排序 · 显示已隐藏。
 * 改一项马上重问，抽屉不关。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FilterSheet(
    ui: ChatsUi,
    onFilters: ((Filters) -> Filters) -> Unit,
    onDismiss: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    val f = ui.filters
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = tokens.card, scrimColor = MaterialTheme.colorScheme.scrim, tonalElevation = 0.dp) {
        Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = tokens.spacing.lg).padding(bottom = tokens.spacing.lg)) {
            Text(copyText("history.page.filter"), style = MaterialTheme.typography.titleMedium, modifier = Modifier.padding(vertical = tokens.spacing.sm))
            Head(copyText("history.page.views"))
            Segments(
                listOf(ChatsView.TIME to copyText("history.page.byTime"), ChatsView.DIR to copyText("history.page.byProject")),
                f.view,
            ) { v -> onFilters { it.copy(view = v) } }
            Head(copyText("history.filter.machines"))
            ui.choices.forEach { c ->
                Row(
                    Modifier
                        .fillMaxWidth()
                        .heightIn(min = Touch.rowSetting)
                        .toggleable(value = c.checked, role = Role.Checkbox, onValueChange = { on -> onFilters { it.copy(unchecked = if (on) it.unchecked - c.id else it.unchecked + c.id) } }),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Checkbox(checked = c.checked, onCheckedChange = null)
                    Text(c.label, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f).padding(start = 8.dp))
                    if (c.offline) Text(copyText("history.filter.offline"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            Head(copyText("history.filter.time"))
            Segments(
                listOf(null to copyText("history.filter.timeAll"), WEEK to withinLabel(WEEK), MONTH to withinLabel(MONTH)),
                f.withinDays,
            ) { d -> onFilters { it.copy(withinDays = d) } }
            Head(copyText("history.filter.sort"))
            // 「最近活动」就是缺省：选它 ＝ 不带这一格（同桌面）。
            Segments(
                listOf(null to sortLabel(ListSort.ACTIVITY), ListSort.CREATED to sortLabel(ListSort.CREATED)),
                f.sort,
            ) { s -> onFilters { it.copy(sort = s) } }
            Row(
                Modifier
                    .fillMaxWidth()
                    .padding(top = tokens.spacing.sm)
                    .heightIn(min = Touch.rowSetting)
                    .toggleable(value = f.hidden, role = Role.Switch, onValueChange = { on -> onFilters { it.copy(hidden = on) } })
                    .testTag(ChatsTags.SHOW_HIDDEN),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(copyText("history.filter.showHidden"), style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
                Switch(checked = f.hidden, onCheckedChange = null)
            }
        }
    }
}

@Composable
private fun Head(text: String) {
    Text(text, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(top = 16.dp, bottom = 4.dp))
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun <T> Segments(
    choices: List<Pair<T, String>>,
    picked: T,
    onPick: (T) -> Unit,
) {
    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
        choices.forEachIndexed { i, (v, label) ->
            SegmentedButton(selected = picked == v, onClick = { onPick(v) }, shape = SegmentedButtonDefaults.itemShape(i, choices.size), icon = {}) { Text(label) }
        }
    }
}

/** 长按一行（手机的右键菜单）：「隐藏」/「取消隐藏」；隐藏的那一行标题底下是那台写好的那一句（为什么隐藏）。 */
@Composable
internal fun RowMenu(
    row: ChatRow,
    onHide: () -> Unit,
    onDismiss: () -> Unit,
) {
    val hidden = row.hidden != null
    ActionSheet(
        title = row.label,
        note = row.hidden,
        actions = listOf(SheetAction(if (hidden) Ph.Eye else Ph.EyeSlash, if (hidden) copyText("history.menu.unhide") else copyText("history.menu.hide"), onClick = onHide)),
        onDismiss = onDismiss,
    )
}

@file:Suppress("MatchingDeclarationName") // 底部抽屉与它的一项放一处

package com.ccmonitor.mobile.core.ui.sheet

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch

/** 底部抽屉里的一项：图标 ＋ 字（照文案表）；[danger] ⇒ 字与图标 `--error-text`（删除这类撤不回的，点了再弹确认框）。 */
data class SheetAction(
    val icon: Ph,
    val text: String,
    val danger: Boolean = false,
    val onClick: () -> Unit,
)

/**
 * 从底部拉起的一列动作（适配层：桌面的下拉 · 浮层菜单 · ⋯ 在手机上都从这里拉起，拇指够得着）。
 * `--card` 底、上两角 10（M3 extraLarge）、遮罩 `--overlay-backdrop`；下滑 · 点遮罩 · 返回键都关。点一项 ⇒ 先做、再关（那一项要接着弹确认框的，由调用方在关时留住）。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ActionSheet(
    title: String?,
    actions: List<SheetAction>,
    onDismiss: () -> Unit,
    /** 标题底下一句说明（那一项为什么是现在这样）；没有 ⇒ 不画。 */
    note: String? = null,
) {
    val tokens = LocalAppTokens.current
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor = tokens.card,
        scrimColor = MaterialTheme.colorScheme.scrim,
        tonalElevation = 0.dp,
    ) {
        Column(Modifier.fillMaxWidth().padding(bottom = tokens.spacing.lg)) {
            title?.let {
                Text(it, style = MaterialTheme.typography.titleMedium, modifier = Modifier.padding(horizontal = tokens.spacing.lg, vertical = tokens.spacing.sm))
            }
            note?.let {
                Text(
                    it,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(start = tokens.spacing.lg, end = tokens.spacing.lg, bottom = tokens.spacing.sm),
                )
            }
            actions.forEachIndexed { i, a ->
                if (i > 0) HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                val color = if (a.danger) tokens.errorText else MaterialTheme.colorScheme.onSurface
                Row(
                    Modifier
                        .fillMaxWidth()
                        .heightIn(min = Touch.rowSetting)
                        .clickable {
                            a.onClick()
                            onDismiss()
                        }.padding(horizontal = tokens.spacing.lg),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(tokens.spacing.md),
                ) {
                    PhIcon(a.icon, contentDescription = null, tint = if (a.danger) tokens.errorText else MaterialTheme.colorScheme.onSurfaceVariant)
                    Text(a.text, color = color, style = MaterialTheme.typography.bodyMedium)
                }
            }
        }
    }
}

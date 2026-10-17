package com.ccmonitor.mobile.ui.files

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import com.ccmonitor.mobile.core.claude.link.PickMenu
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIconButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens

/**
 * 选择模式的顶栏：✕ · 已选 {n} · 顶栏那一组 ·［⋯］里那一组。项与灰不灰全照核心那一份（[PickMenu]）；灰着的照样点得了，
 * 点了交 [onPick]、由屏上在底部说那一句为什么（手机没有悬停）。
 */
@Composable
internal fun PickBar(
    n: Int,
    menu: PickMenu,
    onClose: () -> Unit,
    onPick: (PickMenu.Entry) -> Unit,
) {
    val tokens = LocalAppTokens.current
    var more by remember { mutableStateOf(false) }
    Row(
        Modifier.fillMaxWidth().background(tokens.bg2).padding(horizontal = tokens.spacing.sm, vertical = tokens.spacing.xs),
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PhIconButton(Ph.X, copyText("mobKit.act.close"), onClick = onClose, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(copyText("mobFiles.picked", "n" to n), Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
        menu.bar.forEach { e -> SecondaryButton(onClick = { onPick(e) }) { Text(e.text, color = ink(e)) } }
        if (menu.more.isNotEmpty()) {
            Box {
                PhIconButton(Ph.DotsThree, copyText("rsFilewinChrome.command.more"), onClick = { more = true }, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                DropdownMenu(expanded = more, onDismissRequest = { more = false }) {
                    menu.more.forEach { e ->
                        DropdownMenuItem(
                            text = { Text(e.text, color = ink(e)) },
                            onClick = {
                                more = false
                                onPick(e)
                            },
                        )
                    }
                }
            }
        }
    }
}

/** 灰着的那一项淡下去（仍点得了：点了说为什么）。 */
@Composable
private fun ink(e: PickMenu.Entry): Color =
    MaterialTheme.colorScheme.onSurface.let { if (e.enabled) it else it.copy(alpha = LocalAppTokens.current.disabledAlpha) }
